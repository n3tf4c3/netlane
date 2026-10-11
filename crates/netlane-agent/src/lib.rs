//! Session orchestration. A stopped process is not proof of successful cleanup.
pub mod default_connection;
use anyhow::{Result, ensure};
use netlane_core::{persistence::PolicyFile, planning::PlannedRule};
use netlane_windows::protocol::{self, Message, VERSION};
use std::{
    path::{Path, PathBuf},
    sync::{Arc, Mutex},
    time::Duration,
};
use tokio::sync::mpsc;

pub fn validate_start(path: &Path) -> Result<()> {
    ensure!(
        path.is_absolute() && path.is_file(),
        "Salve as regras em um arquivo local antes de iniciar."
    );
    let name = path.to_string_lossy();
    ensure!(
        !name.starts_with("\\\\"),
        "Caminho remoto/dispositivo recusado."
    );
    PolicyFile::new(path).load()?;
    Ok(())
}
#[derive(Clone, Debug, Default)]
pub struct SessionState {
    pub running: bool,
    pub busy: bool,
    pub cleanup_confirmed: bool,
    pub detail: String,
    pub rules: Vec<PlannedRule>,
}
enum Command {
    Start {
        policy_path: PathBuf,
        allow_flags: bool,
    },
    Stop,
}
pub struct Controller {
    sender: mpsc::UnboundedSender<Command>,
    state: Arc<Mutex<SessionState>>,
}
impl Controller {
    pub fn new(executable: PathBuf) -> Self {
        let (sender, receiver) = mpsc::unbounded_channel();
        let state = Arc::new(Mutex::new(SessionState {
            cleanup_confirmed: true,
            detail: "Sessão parada. O painel observa a rede sem elevação.".to_owned(),
            ..Default::default()
        }));
        let shared = state.clone();
        std::thread::spawn(move || {
            match tokio::runtime::Builder::new_current_thread()
                .enable_all()
                .build()
            {
                Ok(runtime) => runtime.block_on(controller_loop(executable, receiver, shared)),
                Err(e) => set_state(&shared, |s| {
                    s.detail = format!("Falha ao criar controlador: {e}")
                }),
            }
        });
        Self { sender, state }
    }
    pub fn state(&self) -> SessionState {
        self.state.lock().unwrap_or_else(|e| e.into_inner()).clone()
    }
    pub fn start(&self, path: PathBuf, allow_flags: bool) -> Result<()> {
        validate_start(&path)?;
        let mut state = self.state.lock().unwrap_or_else(|e| e.into_inner());
        ensure!(
            !state.running && !state.busy && state.cleanup_confirmed,
            "A sessão anterior precisa encerrar com limpeza confirmada."
        );
        state.busy = true;
        let sent = self
            .sender
            .send(Command::Start {
                policy_path: path,
                allow_flags,
            })
            .map_err(|_| anyhow::anyhow!("Controlador encerrado."));
        if sent.is_err() {
            state.busy = false;
        }
        sent
    }
    pub fn stop(&self) -> Result<()> {
        self.sender
            .send(Command::Stop)
            .map_err(|_| anyhow::anyhow!("Controlador encerrado."))
    }
}
impl Drop for Controller {
    fn drop(&mut self) {
        let _ = self.sender.send(Command::Stop);
    }
}
fn set_state(state: &Arc<Mutex<SessionState>>, change: impl FnOnce(&mut SessionState)) {
    change(&mut state.lock().unwrap_or_else(|e| e.into_inner()));
}
async fn controller_loop(
    executable: PathBuf,
    mut commands: mpsc::UnboundedReceiver<Command>,
    state: Arc<Mutex<SessionState>>,
) {
    while let Some(command) = commands.recv().await {
        if let Command::Start {
            policy_path,
            allow_flags,
        } = command
        {
            if !state
                .lock()
                .unwrap_or_else(|e| e.into_inner())
                .cleanup_confirmed
            {
                continue;
            }
            set_state(&state, |s| {
                s.busy = true;
                s.detail = "Aguardando autorização do Windows…".to_owned();
            });
            #[cfg(windows)]
            let result = control_session(
                &executable,
                &policy_path,
                allow_flags,
                &mut commands,
                &state,
            )
            .await;
            #[cfg(not(windows))]
            let result: Result<()> = Err(anyhow::anyhow!("Sessões WFP exigem Windows."));
            if let Err(e) = result {
                set_state(&state, |s| {
                    s.busy = false;
                    s.detail = format!("{e:#}");
                });
            }
        }
    }
}
#[cfg(windows)]
async fn control_session(
    executable: &Path,
    policy_path: &Path,
    allow_flags: bool,
    commands: &mut mpsc::UnboundedReceiver<Command>,
    state: &Arc<Mutex<SessionState>>,
) -> Result<()> {
    use netlane_windows::session;
    use tokio::time;
    let _lease = netlane_core::persistence::exclusive_file(
        &policy_path.with_extension("json.runtime.lock"),
    )?;
    let name = format!("{}{}", session::PREFIX, uuid::Uuid::new_v4().simple());
    let pipe = session::create_pipe(&name)?;
    let path = executable.to_owned();
    let launch_name = name.clone();
    let (process, pid) =
        tokio::task::spawn_blocking(move || session::launch_agent(&path, &launch_name)).await??;
    set_state(state, |s| {
        s.running = true;
        s.cleanup_confirmed = false;
    });
    let connected = async {
        let deadline = time::Instant::now() + Duration::from_secs(35);
        loop {
            time::timeout_at(deadline, pipe.connect()).await??;
            if session::verify_client(&pipe, pid).is_ok() {
                break;
            }
            pipe.disconnect()?;
        }
        Ok::<_, anyhow::Error>(())
    }
    .await;
    let result = if connected.is_ok() {
        exchange(pipe, pid, policy_path, allow_flags, commands, state).await
    } else {
        drop(pipe);
        connected
    };
    if let Err(error) = &result {
        set_state(state, |s| {
            s.detail = format!("{error:#}. Aguardando a saída do agente.")
        });
    }
    // Keep ownership of both the process handle and the lease until actual exit.
    // A deadline changes the diagnostic, never abandons a still-running child.
    wait_for_agent_exit(|| session::running(&process), commands, state).await;
    result?;
    let status = state.lock().unwrap_or_else(|e| e.into_inner());
    ensure!(
        !status.running && status.cleanup_confirmed,
        "Limpeza ou saída do agente não confirmada."
    );
    Ok(())
}

async fn wait_for_agent_exit(
    running: impl Fn() -> bool,
    commands: &mut mpsc::UnboundedReceiver<Command>,
    state: &Arc<Mutex<SessionState>>,
) {
    let deadline = tokio::time::Instant::now() + Duration::from_secs(20);
    let mut warned = false;
    let mut commands_open = true;
    while running() {
        set_state(state, |s| {
            s.running = true;
            s.busy = true;
        });
        if !warned && tokio::time::Instant::now() >= deadline {
            warned = true;
            set_state(state, |s| {
                s.detail = "O agente ainda está encerrando. O controlador mantém a sessão sob acompanhamento; limpeza ainda precisa de confirmação.".into()
            });
        }
        tokio::select! {
            _ = tokio::time::sleep(Duration::from_millis(50)) => {},
            command = commands.recv(), if commands_open => match command {
                Some(Command::Stop) => set_state(state, |s| s.detail = "Parada recebida. O canal foi encerrado; aguardando a saída do agente, sem forçar término ou presumir limpeza.".into()),
                Some(Command::Start { .. }) => set_state(state, |s| s.detail = "A nova sessão precisa aguardar a saída e a limpeza confirmada da anterior.".into()),
                None => commands_open = false,
            }
        }
    }
    set_state(state, |s| {
        s.running = false;
        s.busy = false;
    });
}
async fn exchange<S>(
    pipe: S,
    pid: u32,
    policy_path: &Path,
    allow_flags: bool,
    commands: &mut mpsc::UnboundedReceiver<Command>,
    state: &Arc<Mutex<SessionState>>,
) -> Result<()>
where
    S: tokio::io::AsyncRead + tokio::io::AsyncWrite + Unpin + Send + 'static,
{
    use tokio::{io, time};
    let deadline = time::Instant::now() + Duration::from_secs(15);
    let (mut reader, mut writer) = io::split(pipe);
    ensure!(
        matches!(
            time::timeout_at(deadline, protocol::receive(&mut reader)).await??,
            Message::Hello { version: VERSION }
        ),
        "Agente incompatível."
    );
    protocol::send(
        &mut writer,
        &Message::Start {
            policy_path: policy_path.to_string_lossy().into_owned(),
            allow_temporary_route_policies: allow_flags,
        },
    )
    .await?;
    let mut inbox = protocol::Inbox::new(reader);
    let mut ping = time::interval(Duration::from_secs(3));
    let mut exiting = false;
    let mut stop_deadline = None;
    let mut last_response = time::Instant::now();
    loop {
        let write_result = tokio::select! {
            biased;
            message=inbox.receive()=> {
                last_response = time::Instant::now();
                if accept_response(message?,pid,policy_path,state)? {return Ok(());}
                Ok(())
            }
            command=commands.recv(),if !exiting=> {
                if matches!(command,None|Some(Command::Stop)) {
                    exiting=true;stop_deadline=Some(time::Instant::now()+Duration::from_secs(30));
                    set_state(state,|s|{s.busy=true;s.detail="Encerrando o motor e restaurando opções temporárias…".to_owned();});
                    protocol::send(&mut writer,&Message::Stop).await
                } else {Ok(())}
            }
            _=ping.tick()=> {
                if stop_deadline.is_some_and(|t|time::Instant::now()>t){anyhow::bail!("O agente não encerrou no prazo; limpeza não confirmada.");}
                ensure!(last_response.elapsed()<Duration::from_secs(35),"O agente não respondeu; limpeza não confirmada.");
                protocol::send(&mut writer,&Message::Ping).await
            }
        };
        if let Err(write_error) = write_result {
            // The agent can exit immediately after writing Stopped. Drain the independent
            // reader before treating a concurrent failed ping/Stop as missing cleanup.
            let final_receipt = time::timeout(Duration::from_secs(3), async {
                loop {
                    if accept_response(inbox.receive().await?, pid, policy_path, state)? {
                        return Ok::<_, anyhow::Error>(());
                    }
                }
            })
            .await;
            return match final_receipt {
                Ok(Ok(())) => Ok(()),
                _ => Err(write_error.context("Canal encerrado sem confirmação de limpeza.")),
            };
        }
    }
}

fn accept_response(
    message: Message,
    pid: u32,
    policy_path: &Path,
    state: &Arc<Mutex<SessionState>>,
) -> Result<bool> {
    match message {
        Message::Snapshot {
            process_id,
            policy_path: reported,
            rules,
            detail,
            ..
        } => {
            ensure!(
                process_id == pid && reported.eq_ignore_ascii_case(&policy_path.to_string_lossy()),
                "Retorno não corresponde à sessão iniciada."
            );
            set_state(state, |s| {
                s.busy = false;
                s.detail = detail;
                s.rules = rules;
            });
        }
        Message::Error { detail } => set_state(state, |s| {
            s.busy = false;
            s.detail = detail;
        }),
        Message::Stopped { success, detail } => {
            set_state(state, |s| {
                s.cleanup_confirmed = success;
                s.detail = detail;
                s.rules.clear();
            });
            return Ok(true);
        }
        _ => anyhow::bail!("Retorno inesperado do agente."),
    }
    Ok(false)
}

#[cfg(windows)]
pub async fn run_session(name: &str, owner_pid: u32, owner_start: u64) -> Result<()> {
    use netlane_core::{
        engine::RoutingEngine,
        planning::{expand_related, plan},
    };
    use netlane_windows::{
        flags::{TemporaryFlags, WindowsRoutePolicySettings},
        monitor, session,
        wfp::NativePolicySession,
    };
    use tokio::{io, time};
    let owner = session::open_owner(owner_pid, owner_start)?;
    let pipe = session::connect_pipe(name)?;
    session::verify_server(&pipe, owner_pid)?;
    let (mut reader, mut writer) = io::split(pipe);
    protocol::send(&mut writer, &Message::Hello { version: VERSION }).await?;
    let request = time::timeout(Duration::from_secs(15), protocol::receive(&mut reader)).await??;
    let Message::Start {
        policy_path,
        allow_temporary_route_policies,
    } = request
    else {
        anyhow::bail!("Pedido inicial inválido.");
    };
    let path = PathBuf::from(&policy_path);
    let mut global_lease = None;
    let mut flags = TemporaryFlags::new(WindowsRoutePolicySettings);
    let mut engine = None;
    let mut inbox = protocol::Inbox::new(reader);
    let result: Result<()> = async {
        validate_start(&path)?;
        global_lease = Some(session::create_pipe("NetLane.RoutingSession.v1")?);
        flags.enable(allow_temporary_route_policies)?;
        engine=Some(RoutingEngine::new(NativePolicySession::open()?));
        let mut tick=time::interval(Duration::from_secs(10)); let mut owner_tick=time::interval(Duration::from_secs(1));
        let mut last_ping=time::Instant::now();
        loop {
            tokio::select! {
                message=inbox.receive()=> {match message? {Message::Ping=>last_ping=time::Instant::now(),Message::Stop=>break,_=>anyhow::bail!("Comando de sessão inválido.")}}
                _=owner_tick.tick()=> {if !session::running(&owner) || last_ping.elapsed()>Duration::from_secs(12){break;}}
                _=tick.tick()=> {
                    let loaded=PolicyFile::new(&path).load()?; let snapshot=monitor::snapshot()?;
                    let expanded=expand_related(&loaded.policies,|p|Path::new(p).is_file()); let mut rules=plan(&expanded,&snapshot.adapters);
                    for rule in &mut rules {if rule.mode!=netlane_core::RouteMode::Automatic && rule.executable.as_deref().is_none_or(|p|!Path::new(p).is_absolute() || !Path::new(p).is_file()){rule.mode=netlane_core::RouteMode::Automatic;rule.detail="Executável ausente; política retirada.".to_owned();}}
                    engine.as_mut().expect("engine initialized").apply(&rules)?;
                    protocol::send(&mut writer,&Message::Snapshot{process_id:std::process::id(),policy_path:policy_path.clone(),revision:loaded.revision,rules,detail:"Políticas aceitas pelo Windows. Tráfego ainda não verificado.".to_owned()}).await?;
                }
            }
        } Ok(())
    }.await;
    let mut errors = Vec::new();
    if let Some(mut engine) = engine
        && let Err(e) = engine.close()
    {
        errors.push(format!("WFP: {e:#}"));
    }
    errors.extend(flags.restore());
    if let Err(e) = &result {
        let _ = protocol::send(
            &mut writer,
            &Message::Error {
                detail: format!("{e:#}"),
            },
        )
        .await;
    }
    let success = errors.is_empty();
    let detail = if success {
        "Sessão encerrada. Opções temporárias restauradas.".to_owned()
    } else {
        format!("Limpeza incompleta: {}", errors.join(" | "))
    };
    let _ = time::timeout(
        Duration::from_secs(3),
        protocol::send(&mut writer, &Message::Stopped { success, detail }),
    )
    .await;
    drop(global_lease);
    ensure!(success, "Limpeza incompleta.");
    result
}

#[cfg(test)]
mod tests {
    use super::*;
    use tokio::io::DuplexStream;

    #[tokio::test(start_paused = true)]
    async fn late_exit_is_supervised_after_deadline_and_accepts_stop_without_claiming_cleanup() {
        use std::sync::atomic::{AtomicBool, Ordering};
        let alive = Arc::new(AtomicBool::new(true));
        let running = alive.clone();
        let (sender, mut receiver) = mpsc::unbounded_channel();
        let state = Arc::new(Mutex::new(SessionState {
            running: true,
            cleanup_confirmed: false,
            ..Default::default()
        }));
        let shared = state.clone();
        let watcher = tokio::spawn(async move {
            wait_for_agent_exit(|| running.load(Ordering::Relaxed), &mut receiver, &shared).await;
        });
        tokio::task::yield_now().await;
        tokio::time::advance(Duration::from_secs(21)).await;
        tokio::task::yield_now().await;
        assert!(!watcher.is_finished());
        {
            let status = state.lock().unwrap();
            assert!(status.running && status.busy);
        }
        sender.send(Command::Stop).unwrap();
        tokio::task::yield_now().await;
        assert!(state.lock().unwrap().detail.contains("Parada recebida"));
        drop(sender);
        alive.store(false, Ordering::Relaxed);
        tokio::time::advance(Duration::from_millis(100)).await;
        watcher.await.unwrap();
        let status = state.lock().unwrap();
        assert!(!status.running && !status.busy && !status.cleanup_confirmed);
    }

    async fn exchange_fixture(response: Message) -> (Result<()>, SessionState) {
        let (client, mut agent) = tokio::io::duplex(4096);
        let (commands, mut receiver) = mpsc::unbounded_channel();
        let shared = Arc::new(Mutex::new(SessionState {
            running: true,
            cleanup_confirmed: false,
            ..Default::default()
        }));
        let fake = tokio::spawn(async move {
            protocol::send(&mut agent, &Message::Hello { version: VERSION })
                .await
                .unwrap();
            assert!(matches!(
                protocol::receive(&mut agent).await.unwrap(),
                Message::Start {
                    allow_temporary_route_policies: false,
                    ..
                }
            ));
            commands.send(Command::Stop).unwrap();
            loop {
                if matches!(protocol::receive(&mut agent).await.unwrap(), Message::Stop) {
                    break;
                }
            }
            protocol::send(&mut agent, &response).await.unwrap();
        });
        let result = exchange(
            client,
            42,
            Path::new("C:\\Fixtures\\rules.json"),
            false,
            &mut receiver,
            &shared,
        )
        .await;
        fake.await.unwrap();
        let state = shared.lock().unwrap().clone();
        (result, state)
    }

    #[tokio::test]
    async fn stopped_receipt_is_required_and_failure_never_confirms_cleanup() {
        for success in [true, false] {
            let (result, state) = exchange_fixture(Message::Stopped {
                success,
                detail: "fixture".into(),
            })
            .await;
            assert!(result.is_ok());
            assert_eq!(state.cleanup_confirmed, success);
        }
        let (result, state) = exchange_fixture(Message::Error {
            detail: "Disconnected before cleanup".into(),
        })
        .await;
        assert!(result.is_err());
        assert!(!state.cleanup_confirmed);
    }

    #[tokio::test]
    async fn snapshot_from_another_process_or_profile_is_rejected() {
        for (process_id, policy_path) in [
            (43, "C:\\Fixtures\\rules.json"),
            (42, "C:\\Other\\rules.json"),
        ] {
            let (result, state) = exchange_fixture(Message::Snapshot {
                process_id,
                policy_path: policy_path.into(),
                revision: None,
                rules: vec![],
                detail: "fixture".into(),
            })
            .await;
            assert!(result.is_err());
            assert!(!state.cleanup_confirmed);
        }
    }

    #[tokio::test]
    async fn unknown_protocol_version_is_rejected_before_start() {
        let (client, mut agent): (DuplexStream, DuplexStream) = tokio::io::duplex(1024);
        let (_sender, mut commands) = mpsc::unbounded_channel();
        let state = Arc::new(Mutex::new(SessionState::default()));
        protocol::send(
            &mut agent,
            &Message::Hello {
                version: VERSION + 1,
            },
        )
        .await
        .unwrap();
        assert!(
            exchange(
                client,
                42,
                Path::new("C:\\Fixtures\\rules.json"),
                false,
                &mut commands,
                &state
            )
            .await
            .is_err()
        );
        assert!(!state.lock().unwrap().cleanup_confirmed);
    }

    #[tokio::test(start_paused = true)]
    async fn silent_agent_times_out_without_a_cleanup_receipt() {
        let (client, mut agent) = tokio::io::duplex(4096);
        let (_sender, mut commands) = mpsc::unbounded_channel();
        let state = Arc::new(Mutex::new(SessionState::default()));
        let fake = tokio::spawn(async move {
            protocol::send(&mut agent, &Message::Hello { version: VERSION })
                .await
                .unwrap();
            assert!(matches!(
                protocol::receive(&mut agent).await.unwrap(),
                Message::Start { .. }
            ));
            // The pipe remains open and accepts pings but reports no status.
            while protocol::receive(&mut agent).await.is_ok() {}
        });
        let result = exchange(
            client,
            42,
            Path::new("C:\\Fixtures\\rules.json"),
            false,
            &mut commands,
            &state,
        )
        .await;
        assert!(result.is_err());
        assert!(!state.lock().unwrap().cleanup_confirmed);
        fake.await.unwrap();
    }

    #[test]
    fn repeated_start_is_refused_before_a_launch_can_be_queued() {
        let folder = std::env::temp_dir().join(format!("netlane-start-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir(&folder).unwrap();
        let file = folder.join("rules.json");
        std::fs::write(&file, "[]").unwrap();
        let (sender, mut receiver) = mpsc::unbounded_channel();
        let state = Arc::new(Mutex::new(SessionState {
            cleanup_confirmed: true,
            ..Default::default()
        }));
        let controller = Controller { sender, state };
        controller.start(file.clone(), false).unwrap();
        assert!(controller.start(file.clone(), false).is_err());
        assert!(matches!(
            receiver.try_recv().unwrap(),
            Command::Start {
                allow_flags: false,
                ..
            }
        ));
        assert!(receiver.try_recv().is_err());
        std::fs::remove_file(file).unwrap();
        std::fs::remove_dir(folder).unwrap();
    }
}
