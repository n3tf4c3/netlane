//! Codex App Server oficial: autenticação ChatGPT e propostas sem ferramentas
//! locais. Não lê/copia tokens nem usa a configuração do Codex pessoal.
//! Adaptado de termIAX: Copyright (c) 2026 Paulo Gomes. Todos os direitos reservados.
use std::{
    collections::{HashMap, HashSet},
    path::{Path, PathBuf},
    process::Stdio,
    time::{Duration, Instant},
};

use serde_json::{Value, json};
use tokio::{
    io::{AsyncBufRead, AsyncBufReadExt, AsyncReadExt, AsyncWrite, AsyncWriteExt, BufReader},
    process::Child,
    sync::mpsc,
};

use crate::{
    AcpError, AgentHandle, Command, Event, ModelChoice, ModelOption, context::SYSTEM_PROMPT,
};

pub const CODEX_VERSION: &str = "0.162.0";
const VALIDATED_VERSIONS: [&str; 1] = [CODEX_VERSION];

fn validated_version(version: &str) -> bool {
    VALIDATED_VERSIONS.contains(&version)
}
const MODEL: &str = "gpt-6.1-sol";
const EFFORT: &str = "medium";
const MAX_LINE: usize = 256 * 1024;
const MAX_PROMPT: usize = 32 * 1024;
const MAX_ANSWER: usize = 128 * 1024;
const PROFILE: &str = r#"forced_login_method="chatgpt"
cli_auth_credentials_store="keyring"
sandbox_mode="read-only"
approval_policy="never"
web_search="disabled"
project_doc_max_bytes=0
include_environment_context=false
history.persistence="none"
notify=[]
features.shell_tool=false
features.unified_exec=false
features.view_image=false
features.apps=false
features.plugins=false
features.hooks=false
features.memories=false
features.multi_agent=false
features.code_mode_host=false
features.browser_use=false
features.computer_use=false
features.image_generation=false
features.skill_mcp_dependency_install=false
features.skip_host_skill_discovery=true
features.skill_search=false
features.tool_suggest=false
features.remote_plugin=false
features.recommended_plugins=false
features.workspace_dependencies=false
features.daemon_auto_start=false
analytics.enabled=false
feedback.enabled=false
"#;

#[derive(Clone, Debug)]
pub struct CodexInfo {
    pub executable: PathBuf,
    pub version: String,
}
impl CodexInfo {
    fn label(&self) -> String {
        format!("Codex CLI {} · login ChatGPT", self.version)
    }
}
pub fn start(
    info: &CodexInfo,
    home: PathBuf,
    workdir: PathBuf,
    events: mpsc::Sender<Event>,
) -> Result<AgentHandle, AcpError> {
    crate::codex::validate_executable(&info.executable)
        .map_err(|e| AcpError::Start(e.to_string()))?;
    if !home.is_absolute() || !workdir.is_absolute() {
        return Err(AcpError::Start(
            "Perfil e diretório da ponte precisam ser absolutos.".into(),
        ));
    }
    if !validated_version(&info.version) {
        return Err(AcpError::Start("Versão do Codex não validada.".into()));
    }
    let (home, workdir) = prepare(&home, &workdir)?;
    let (commands, rx) = mpsc::channel(8);
    let info = info.clone();
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .map_err(|e| AcpError::Start(e.to_string()))?;
    std::thread::Builder::new()
        .name("netlane-codex".into())
        .spawn(move || {
            let result = runtime.block_on(run(&info, &home, &workdir, rx, events.clone()));
            let message = if result.is_ok() {
                "Conexão Codex encerrada."
            } else {
                "Conexão Codex encerrada após erro ou prazo. A causa não foi confirmada; confira o estado antes de reconectar."
            };
            let _ = events.blocking_send(Event::Closed {
                message: message.into(),
            });
        })
        .map_err(|e| AcpError::Start(e.to_string()))?;
    Ok(AgentHandle { commands })
}

fn prepare(home: &Path, workdir: &Path) -> Result<(PathBuf, PathBuf), AcpError> {
    std::fs::create_dir_all(home)
        .map_err(|_| AcpError::Start("Não foi possível preparar o Codex.".into()))?;
    std::fs::create_dir_all(workdir)
        .map_err(|_| AcpError::Start("Não foi possível preparar a conversa.".into()))?;
    let home = std::fs::canonicalize(home)
        .map_err(|_| AcpError::Start("Pasta do Codex indisponível.".into()))?;
    let workdir = std::fs::canonicalize(workdir)
        .map_err(|_| AcpError::Start("Pasta da conversa indisponível.".into()))?;
    let catalog = home.join("proposal-model.json");
    // Um catálogo local documentado remove também apply_patch, que não é
    // desligado por shell_tool=false. Não se trata de uma instrução ao modelo.
    std::fs::write(&catalog, include_str!("proposal-model.json"))
        .map_err(|_| AcpError::Start("Não foi possível preparar o modelo.".into()))?;
    let config = format!(
        "{PROFILE}\nmodel_catalog_json={}\n",
        json!(catalog.to_string_lossy())
    );
    std::fs::write(home.join("config.toml"), config)
        .map_err(|_| AcpError::Start("Não foi possível preparar a configuração.".into()))?;
    Ok((home, workdir))
}

struct Rpc {
    child: Option<Child>,
    input: Box<dyn AsyncWrite + Unpin + Send>,
    output: Box<dyn AsyncBufRead + Unpin + Send>,
    buffer: Vec<u8>,
    next_id: u64,
}
impl Rpc {
    async fn launch(info: &CodexInfo, home: &Path, workdir: &Path) -> Result<Self, ()> {
        let mut version = native_command(info, home, workdir);
        version.arg("--version").stdout(Stdio::piped());
        let version = tokio::time::timeout(Duration::from_secs(5), version.output())
            .await
            .map_err(|_| ())?
            .map_err(|_| ())?;
        if !version.status.success()
            || String::from_utf8_lossy(&version.stdout).trim()
                != format!("codex-cli {}", info.version)
        {
            return Err(());
        }
        let mut cmd = native_command(info, home, workdir);
        cmd.args(["--strict-config", "app-server", "--listen", "stdio://"])
            .stdin(Stdio::piped())
            .stdout(Stdio::piped());
        let mut child = cmd.spawn().map_err(|_| ())?;
        let input = child.stdin.take().ok_or(())?;
        let output = BufReader::new(child.stdout.take().ok_or(())?);
        Ok(Self {
            child: Some(child),
            input: Box::new(input),
            output: Box::new(output),
            buffer: Vec::new(),
            next_id: 0,
        })
    }
    async fn write(&mut self, value: Value) -> Result<(), ()> {
        let mut bytes = serde_json::to_vec(&value).map_err(|_| ())?;
        bytes.push(b'\n');
        tokio::time::timeout(Duration::from_secs(5), self.input.write_all(&bytes))
            .await
            .map_err(|_| ())?
            .map_err(|_| ())
    }
    async fn close(&mut self) -> Result<(), ()> {
        if let Some(mut child) = self.child.take() {
            let kill = child.start_kill();
            let exit = tokio::time::timeout(Duration::from_secs(5), child.wait()).await;
            if kill.is_err() || !matches!(exit, Ok(Ok(_))) {
                return Err(());
            }
        }
        Ok(())
    }
    async fn send(&mut self, method: &str, params: Value) -> Result<u64, ()> {
        if !matches!(
            method,
            "initialize"
                | "account/read"
                | "account/login/start"
                | "account/login/cancel"
                | "account/logout"
                | "config/read"
                | "model/list"
                | "mcpServerStatus/list"
                | "thread/start"
                | "thread/unsubscribe"
                | "turn/start"
                | "turn/interrupt"
        ) {
            return Err(());
        }
        self.next_id += 1;
        let id = self.next_id;
        self.write(json!({"id":id,"method":method,"params":params}))
            .await?;
        Ok(id)
    }
    async fn read(&mut self) -> Result<Value, ()> {
        read_frame(&mut self.output, &mut self.buffer).await
    }
    async fn request(&mut self, method: &str, params: Value) -> Result<Value, ()> {
        let id = self.send(method, params).await?;
        tokio::time::timeout(Duration::from_secs(20), async {
            loop {
                let msg = self.read().await?;
                if msg["id"] == id {
                    return msg.get("result").cloned().ok_or(());
                }
                if msg.get("id").is_some() {
                    self.deny(&msg).await?;
                }
            }
        })
        .await
        .map_err(|_| ())?
    }
    async fn deny(&mut self, msg: &Value) -> Result<(), ()> {
        self.write(json!({"id":msg["id"],"error":{"code":-32601,"message":"Capacidade indisponível no NetLane"}})).await
    }
}

fn native_command(info: &CodexInfo, home: &Path, workdir: &Path) -> tokio::process::Command {
    let mut cmd = tokio::process::Command::new(&info.executable);
    cmd.current_dir(workdir)
        .env_clear()
        .env("CODEX_HOME", home)
        .env("RUST_LOG", "off")
        .stdin(Stdio::null())
        .stderr(Stdio::null())
        .kill_on_drop(true);
    // Sem chaves de API, gateway, perfil, logs ou configuração de adaptador
    // herdados. Retém somente o necessário ao SO e ao login nativo.
    for name in [
        "SystemRoot",
        "WINDIR",
        "USERPROFILE",
        "LOCALAPPDATA",
        "APPDATA",
        "PATH",
        "TEMP",
        "TMP",
        "ProgramFiles",
        "CommonProgramFiles",
        "ALLUSERSPROFILE",
        "HOMEDRIVE",
        "HOMEPATH",
        "HOME",
    ] {
        if let Some(value) = std::env::var_os(name) {
            cmd.env(name, value);
        }
    }
    #[cfg(windows)]
    cmd.creation_flags(0x0800_0000);
    cmd
}

enum Pending {
    Thread(u64, String, ModelChoice),
    Turn(u64),
}
struct Active {
    conversation: u64,
    started: Instant,
    cancelled: bool,
    turn: Option<String>,
    bytes: usize,
    cancel_started: Option<Instant>,
}

impl Active {
    fn bind_turn(&mut self, id: &Value) -> Result<(), ()> {
        let id = id
            .as_str()
            .filter(|id| !id.is_empty() && id.len() <= 128)
            .ok_or(())?;
        if self.turn.as_ref().is_some_and(|current| current != id) {
            return Err(());
        }
        self.turn = Some(id.into());
        Ok(())
    }

    fn matches_turn(&self, id: &Value) -> bool {
        self.turn
            .as_ref()
            .is_some_and(|current| id.as_str() == Some(current.as_str()))
    }
}

fn thread_params(workdir: &Path, model: &str) -> Value {
    json!({"model":model,"modelProvider":"openai","cwd":workdir,"approvalPolicy":"never","sandbox":"read-only","ephemeral":true,"baseInstructions":SYSTEM_PROMPT,"serviceName":"netlane"})
}

async fn begin_turn(
    rpc: &mut Rpc,
    thread: &str,
    conversation: u64,
    text: String,
    choice: &ModelChoice,
    pending: &mut HashMap<u64, (Pending, Instant)>,
    active: &mut HashMap<String, Active>,
) -> Result<(), ()> {
    let id = rpc.send("turn/start", json!({"threadId":thread,"input":[{"type":"text","text":text,"text_elements":[]}],"model":choice.model,"effort":choice.effort,"approvalPolicy":"never"})).await?;
    pending.insert(id, (Pending::Turn(conversation), Instant::now()));
    active.insert(
        thread.into(),
        Active {
            conversation,
            started: Instant::now(),
            cancelled: false,
            turn: None,
            bytes: 0,
            cancel_started: None,
        },
    );
    Ok(())
}

async fn bootstrap(info: &CodexInfo, home: &Path, workdir: &Path) -> Result<(Rpc, Value), ()> {
    let mut rpc = Rpc::launch(info, home, workdir).await?;
    let result=async {
    let init = rpc.request("initialize", json!({"clientInfo":{"name":"netlane","version":env!("CARGO_PKG_VERSION")},"capabilities":{"experimentalApi":false}})).await?;
    if !init.is_object() {
        return Err(());
    }
    rpc.write(json!({"method":"initialized","params":{}}))
        .await?;
    let config = rpc
        .request("config/read", json!({"includeLayers":false}))
        .await?;
    if !safe_config(&config["config"], home) {
        return Err(());
    }
    let mcp = rpc
        .request("mcpServerStatus/list", json!({"limit":100}))
        .await?;
    if !mcp["data"].as_array().is_some_and(|data| data.is_empty())
        || mcp
            .get("nextCursor")
            .is_some_and(|cursor| !cursor.is_null())
    {
        return Err(());
    }
    let account = rpc
        .request("account/read", json!({"refreshToken":false}))
        .await?;
        Ok(account)
    }.await;
    match result {
        Ok(account) => Ok((rpc, account)),
        Err(()) => {
            let _ = rpc.close().await;
            Err(())
        }
    }
}

/// Usa a API oficial, confirma a ausência da conta e reinicia o processo para
/// descartar threads efêmeras, requisições e notificações da conta anterior.
async fn logout(rpc: &mut Rpc, info: &CodexInfo, home: &Path, workdir: &Path) -> Result<(), ()> {
    rpc.request("account/logout", json!({})).await?;
    let account = rpc
        .request("account/read", json!({"refreshToken":false}))
        .await?;
    if !account.get("account").is_some_and(Value::is_null) {
        return Err(());
    }
    rpc.close().await?;
    let (fresh, account) = bootstrap(info, home, workdir).await?;
    if !account.get("account").is_some_and(Value::is_null) {
        return Err(());
    }
    *rpc = fresh;
    Ok(())
}

async fn run(
    info: &CodexInfo,
    home: &Path,
    workdir: &Path,
    mut commands: mpsc::Receiver<Command>,
    events: mpsc::Sender<Event>,
) -> Result<(), ()> {
    let (mut rpc, account) = bootstrap(info, home, workdir).await?;
    let result = serve(
        &mut rpc,
        account,
        info,
        home,
        workdir,
        &mut commands,
        &events,
    )
    .await;
    rpc.close().await?;
    result
}
async fn serve(
    rpc: &mut Rpc,
    account: Value,
    info: &CodexInfo,
    home: &Path,
    workdir: &Path,
    commands: &mut mpsc::Receiver<Command>,
    events: &mpsc::Sender<Event>,
) -> Result<(), ()> {
    let mut authenticated = account["account"]["type"] == "chatgpt";
    let mut plan = plan_label(&account).to_string();
    let mut options = models_for(&account);
    let mut choice = resolve(
        &options,
        &ModelChoice {
            model: MODEL.into(),
            effort: EFFORT.into(),
        },
    );
    events
        .send(if authenticated {
            Event::Ready {
                agent: format!("{} · ChatGPT {}", info.label(), plan),
            }
        } else {
            Event::LoginRequired
        })
        .await
        .map_err(|_| ())?;
    if authenticated {
        events
            .send(Event::Models {
                options: options.clone(),
                selected: choice.clone(),
            })
            .await
            .map_err(|_| ())?;
    }
    let mut ids = HashMap::<u64, String>::new();
    let mut pending = HashMap::new();
    let mut active = HashMap::<String, Active>::new();
    let mut cancelled = HashSet::new();
    let mut login_pending: Option<(String, Instant)> = None;
    let mut tick = tokio::time::interval(Duration::from_secs(1));
    loop {
        tokio::select! {
            command = commands.recv() => {
                let Some(command) = command else { break };
                match command {
                    Command::Login if !authenticated && login_pending.is_none() => {
                        let login = rpc.request("account/login/start", json!({"type":"chatgpt"})).await?;
                        let url = login["authUrl"].as_str().filter(|url| valid_login_url(url)).ok_or(())?;
                        if login["type"] != "chatgpt" { return Err(()) }
                        let id = login["loginId"].as_str().filter(|id| !id.is_empty() && id.len() <= 128).ok_or(())?;
                        login_pending = Some((id.into(), Instant::now()));
                        events.send(Event::LoginUrl { url: url.into() }).await.map_err(|_| ())?;
                    }
                    Command::Login => {},
                    Command::CancelLogin => {
                        if let Some((id, _)) = login_pending.take() {
                            rpc.request("account/login/cancel", json!({"loginId":id})).await?;
                            events.send(Event::LoginRequired).await.map_err(|_| ())?;
                        }
                    }
                    Command::Logout => {
                        if let Some((id, _)) = login_pending.take() {
                            rpc.request("account/login/cancel", json!({"loginId":id})).await?;
                        }
                        logout(rpc, info, home, workdir).await?;
                        authenticated = false;
                        plan = "(plano não informado)".into();
                        ids.clear();
                        pending.clear();
                        active.clear();
                        cancelled.clear();
                        events.send(Event::LoggedOut).await.map_err(|_| ())?;
                    }
                    command @ (Command::Prompt { .. } | Command::PromptWithModel { .. }) => {
                        let (conversation, text, turn_choice) = match command {
                            Command::Prompt { conversation, text } => (conversation, text, choice.clone()),
                            Command::PromptWithModel { conversation, text, model } => (conversation, text, model),
                            _ => unreachable!(),
                        };
                        if !authenticated || text.len() > MAX_PROMPT || pending.len() + active.len() >= 8 || active.values().any(|a| a.conversation == conversation) || pending.values().any(|(p, _)| matches!(p, Pending::Thread(c, _, _) | Pending::Turn(c) if *c == conversation)) {
                            events.send(Event::Failed { conversation: Some(conversation), message: "Entre com ChatGPT ou aguarde a resposta atual.".into() }).await.map_err(|_| ())?; continue;
                        }
                        if resolve(&options, &turn_choice) != turn_choice {
                            events.send(Event::Failed { conversation: Some(conversation), message: "O modelo ou esforço solicitado não está disponível. Confirme a seleção antes de enviar.".into() }).await.map_err(|_| ())?; continue;
                        }
                        cancelled.remove(&conversation);
                        if let Some(thread) = ids.get(&conversation) {
                            begin_turn(rpc, thread, conversation, text, &turn_choice, &mut pending, &mut active).await?;
                        } else {
                            let id = rpc.send("thread/start", thread_params(workdir, &turn_choice.model)).await?;
                            pending.insert(id, (Pending::Thread(conversation, text, turn_choice), Instant::now()));
                        }
                    }
                    Command::SetModel(wanted) => {
                        choice = resolve(&options, &wanted);
                        if authenticated { events.send(Event::Models { options: options.clone(), selected: choice.clone() }).await.map_err(|_| ())?; }
                    }
                    Command::Reset { conversation } => {
                        if active.values().any(|a|a.conversation==conversation) {continue;}
                        if let Some(thread)=ids.remove(&conversation) {rpc.send("thread/unsubscribe",json!({"threadId":thread})).await?;}
                        cancelled.remove(&conversation);
                    }
                    Command::Cancel { conversation } => {
                        cancelled.insert(conversation);
                        for (thread, state) in &mut active {
                            if state.conversation == conversation {
                                state.cancelled = true;
                                state.cancel_started=Some(Instant::now());
                                if let Some(turn) = &state.turn { rpc.send("turn/interrupt", json!({"threadId":thread,"turnId":turn})).await?; }
                            }
                        }
                    }
                }
            }
            result = rpc.read() => {
                let msg = result?;
                if msg.get("id").is_some() && msg.get("method").is_some() { rpc.deny(&msg).await?; continue; }
                if let Some(id) = msg["id"].as_u64() {
                    let Some((request, _)) = pending.remove(&id) else { continue };
                    let Some(result) = msg.get("result") else {
                        let conversation = match request { Pending::Thread(c, _, _) | Pending::Turn(c) => c };
                        active.retain(|_, a| a.conversation != conversation);
                        events.send(Event::Failed { conversation: Some(conversation), message: failure_message(&msg["error"], &plan) }).await.map_err(|_| ())?; continue;
                    };
                    match request {
                        Pending::Thread(conversation, text, turn_choice) => {
                            if result["thread"]["ephemeral"] != true { return Err(()) }
                            let thread = result["thread"]["id"].as_str().filter(|id| !id.is_empty()&&id.len() <= 128&&!id.chars().any(char::is_control)).ok_or(())?;
                            ids.insert(conversation, thread.into());
                            if !cancelled.contains(&conversation) { begin_turn(rpc, thread, conversation, text, &turn_choice, &mut pending, &mut active).await?; }
                            else {events.send(Event::Done {conversation,note:Some("Cancelado antes de iniciar a análise no provedor.".into())}).await.map_err(|_|())?;}
                        }
                        Pending::Turn(conversation) => {
                            if let Some(thread) = ids.get(&conversation) && let Some(state) = active.get_mut(thread) {
                                state.bind_turn(&result["turn"]["id"])?;
                                if state.cancelled && let Some(turn) = &state.turn { rpc.send("turn/interrupt", json!({"threadId":thread,"turnId":turn})).await?; }
                            }
                        }
                    }
                    continue;
                }
                let params = &msg["params"];
                let method = msg["method"].as_str().unwrap_or("");
                if method == "account/login/completed" {
                    if !login_pending.as_ref().is_some_and(|(id, _)| params["loginId"] == id.as_str()) { continue }
                    login_pending = None;
                    let account = rpc.request("account/read", json!({"refreshToken":false})).await?;
                    authenticated = params["success"] == true && account["account"]["type"] == "chatgpt";
                    plan = plan_label(&account).to_string();
                    options = models_for(&account);
                    choice = resolve(&options, &choice);
                    events.send(if authenticated { Event::Ready { agent: format!("{} · ChatGPT {}", info.label(), plan) } } else { Event::LoginRequired }).await.map_err(|_| ())?;
                    if authenticated { events.send(Event::Models { options: options.clone(), selected: choice.clone() }).await.map_err(|_| ())?; }
                    continue;
                }
                let Some(thread) = params["threadId"].as_str() else { continue };
                let Some(state) = active.get_mut(thread) else { continue };
                let conversation = state.conversation;
                if method == "turn/started" {
                    if state.turn.is_none() { state.bind_turn(&params["turn"]["id"])?; }
                    continue;
                }
                if method == "item/started" && forbidden_item(params["item"]["type"].as_str().unwrap_or("")) {
                    events.send(Event::Blocked { conversation, what: "capacidade local indisponível".into() }).await.map_err(|_| ())?; return Err(());
                }
                if method == "item/agentMessage/delta" && !state.cancelled && state.matches_turn(&params["turnId"]) {
                    let delta = params["delta"].as_str().ok_or(())?;
                    state.bytes = state.bytes.saturating_add(delta.len());
                    if state.bytes > MAX_ANSWER { return Err(()) }
                    events.send(Event::Text { conversation, text: delta.into() }).await.map_err(|_| ())?;
                } else if method == "turn/completed" && state.matches_turn(&params["turn"]["id"]) {
                    let state = active.remove(thread).ok_or(())?;
                    if state.cancelled {
                        events.send(Event::Done {conversation,note:Some("Turno encerrado após pedido de interrupção.".into())}).await.map_err(|_|())?;
                    } else {
                        let failed = params["turn"]["status"] == "failed";
                        events.send(Event::Done { conversation, note: failed.then(|| failure_message(&params["turn"]["error"], &plan)) }).await.map_err(|_| ())?;
                    }
                }
            }
            _ = tick.tick() => {
                if login_pending.as_ref().is_some_and(|(_, since)| since.elapsed() > Duration::from_secs(300)) {
                    let (id, _) = login_pending.take().ok_or(())?;
                    rpc.request("account/login/cancel", json!({"loginId":id})).await?;
                    events.send(Event::LoginRequired).await.map_err(|_| ())?;
                }
                if pending.values().any(|(_, since)| since.elapsed() > Duration::from_secs(20)) || active.values().any(|a| a.started.elapsed() > Duration::from_secs(180)||a.cancel_started.is_some_and(|since|since.elapsed()>Duration::from_secs(10))) { return Err(()) }
            }
        }
    }
    Ok(())
}

/// O buffer sobrevive ao cancelamento pelo select: uma notificação parcial
/// não pode ser perdida ao receber um comando ou o timer de prazo.
async fn read_frame<R: tokio::io::AsyncBufRead + Unpin>(
    output: &mut R,
    buffer: &mut Vec<u8>,
) -> Result<Value, ()> {
    let remaining = (MAX_LINE + 1).checked_sub(buffer.len()).ok_or(())?;
    output
        .take(remaining as u64)
        .read_until(b'\n', buffer)
        .await
        .map_err(|_| ())?;
    if buffer.is_empty() || buffer.len() > MAX_LINE || buffer.last() != Some(&b'\n') {
        return Err(());
    }
    let result = serde_json::from_slice(buffer).map_err(|_| ());
    buffer.clear();
    result
}

pub fn valid_login_url(url: &str) -> bool {
    url.len() <= 8192
        && url.starts_with("https://auth.openai.com/oauth/authorize?")
        && !url.chars().any(char::is_control)
}

fn forbidden_item(kind: &str) -> bool {
    !matches!(
        kind,
        "agentMessage" | "userMessage" | "reasoning" | "plan" | "contextCompaction"
    )
}

/// Somente rótulos públicos e conhecidos; não exibe e-mail, ID ou tokens.
fn plan_label(account: &Value) -> &'static str {
    match account["account"]["planType"].as_str() {
        Some("free") => "Free",
        Some("go") => "Go",
        Some("plus") => "Plus",
        Some("pro" | "prolite" | "promax") => "Pro",
        Some(
            "team" | "business" | "self_serve_business_prolite" | "self_serve_business_usage_based",
        ) => "Business",
        Some(
            "enterprise" | "ent26" | "enterprise_cbp_automation" | "enterprise_cbp_usage_based",
        ) => "Enterprise",
        Some("edu" | "edu_plus" | "edu_pro") => "Edu",
        _ => "(plano não informado)",
    }
}

/// Modelos do catálogo local, todos sem ferramentas. Ultra fica de fora: ele
/// usa subagentes, desligados no perfil.
fn catalog() -> Vec<ModelOption> {
    let catalog: Value =
        serde_json::from_str(include_str!("proposal-model.json")).unwrap_or_default();
    catalog["models"]
        .as_array()
        .into_iter()
        .flatten()
        .filter_map(|m| {
            Some(ModelOption {
                id: m["slug"].as_str()?.into(),
                label: m["display_name"].as_str()?.into(),
                efforts: m["supported_reasoning_levels"]
                    .as_array()?
                    .iter()
                    .filter_map(|level| level["effort"].as_str().map(Into::into))
                    .collect(),
            })
        })
        .collect()
}

fn models_for(_account: &Value) -> Vec<ModelOption> {
    catalog()
}

/// Mantém o modelo e o esforço pedidos quando a conta pode usá-los.
fn resolve(options: &[ModelOption], wanted: &ModelChoice) -> ModelChoice {
    match options
        .iter()
        .find(|m| m.id == wanted.model)
        .or(options.first())
    {
        Some(m) => ModelChoice {
            model: m.id.clone(),
            effort: if m.efforts.contains(&wanted.effort) {
                wanted.effort.clone()
            } else {
                EFFORT.into()
            },
        },
        None => ModelChoice {
            model: MODEL.into(),
            effort: EFFORT.into(),
        },
    }
}

/// Classifica o erro em memória; nunca retorna nem registra payload bruto,
/// detalhes remotos, URLs, autenticação ou o texto da conversa.
fn failure_message(error: &Value, plan: &str) -> String {
    let info = &error["codexErrorInfo"];
    let kind = info.as_str().unwrap_or("");
    let status = [
        "httpConnectionFailed",
        "responseStreamConnectionFailed",
        "responseStreamDisconnected",
        "responseTooManyFailedAttempts",
    ]
    .iter()
    .find_map(|key| info[key]["httpStatusCode"].as_u64());
    let message = error["message"].as_str().unwrap_or("").to_ascii_lowercase();
    if kind == "unauthorized"
        || status == Some(401)
        || message.contains("unauthorized")
        || message.contains("authentication")
    {
        "O login ChatGPT não foi aceito. Entre novamente com a conta correta.".into()
    } else if kind == "usageLimitExceeded"
        || kind == "rateLimitExceeded"
        || status == Some(429)
        || message.contains("usage limit")
        || message.contains("rate limit")
        || message.contains("quota")
    {
        "O Codex informou que o limite de uso da conta foi atingido. Confira o uso e o horário de renovação no ChatGPT.".into()
    } else if message.contains("model")
        && (message.contains("not supported")
            || message.contains("not found")
            || message.contains("does not exist")
            || message.contains("access"))
    {
        format!(
            "Login ChatGPT confirmado · plano {plan}. O modelo selecionado não está disponível para essa conta. Confira a conta e a compatibilidade do plano com Codex CLI."
        )
    } else if kind == "contextWindowExceeded" {
        "A conversa excedeu o contexto aceito pelo modelo. Inicie uma nova conversa.".into()
    } else if status.is_some_and(|status| status >= 500) || kind == "serverOverloaded" {
        "O serviço do Codex está indisponível no momento. Tente novamente mais tarde.".into()
    } else if message.contains("connection") || message.contains("network") || info.is_object() {
        "A conexão com o serviço do Codex falhou. Confira a rede e tente novamente.".into()
    } else {
        "O Codex não concluiu a resposta. A causa não foi identificada; isso não confirma um limite da assinatura.".into()
    }
}

/// Uma política de máquina pode sobrepor o arquivo: confira o resultado
/// carregado pelo servidor antes de disponibilizar qualquer conversa.
fn safe_config(config: &Value, home: &Path) -> bool {
    config["forced_login_method"] == "chatgpt"
        && config["cli_auth_credentials_store"] == "keyring"
        && config["approval_policy"] == "never"
        && config["sandbox_mode"] == "read-only"
        && config["web_search"] == "disabled"
        && config["history"]["persistence"] == "none"
        && config["model_catalog_json"].as_str().is_some_and(|path| {
            std::fs::canonicalize(path).ok().as_ref() == Some(&home.join("proposal-model.json"))
        })
        && [
            "shell_tool",
            "unified_exec",
            "view_image",
            "apps",
            "plugins",
            "hooks",
            "memories",
            "multi_agent",
            "code_mode_host",
            "browser_use",
            "computer_use",
            "image_generation",
            "skill_mcp_dependency_install",
            "skill_search",
            "tool_suggest",
            "remote_plugin",
            "recommended_plugins",
            "workspace_dependencies",
            "daemon_auto_start",
        ]
        .iter()
        .all(|feature| config["features"][feature] == false)
        && config["features"]["skip_host_skill_discovery"] == true
        && config["project_doc_max_bytes"] == 0
        && config["include_environment_context"] == false
        && config["notify"] == json!([])
        && config.get("mcp_servers").is_none_or(|servers| {
            servers.is_null()
                || servers
                    .as_object()
                    .is_some_and(|servers| servers.is_empty())
        })
}

/// Opt-in compatibility proof. Fresh profile, ephemeral thread, no login or turn.
pub fn verify_profile(info: &CodexInfo, home: PathBuf, workdir: PathBuf) -> Result<(), AcpError> {
    crate::codex::validate_executable(&info.executable)
        .map_err(|e| AcpError::Start(e.to_string()))?;
    if !validated_version(&info.version) || !home.is_absolute() || !workdir.is_absolute() {
        return Err(AcpError::Start(
            "Versão ou perfil de prova inválidos.".into(),
        ));
    }
    let (home, workdir) = prepare(&home, &workdir)?;
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .map_err(|e| AcpError::Start(e.to_string()))?;
    runtime.block_on(async {
        let (mut rpc, account) = bootstrap(info, &home, &workdir)
            .await
            .map_err(|_| AcpError::Start("Configuração isolada não confirmada.".into()))?;
        let result = async {
            if !account["account"].is_null() {
                return Err(());
            }
            let thread = rpc
                .request("thread/start", thread_params(&workdir, MODEL))
                .await?;
            if thread["thread"]["ephemeral"] != true || thread["model"] != MODEL {
                return Err(());
            }
            Ok(())
        }
        .await;
        if rpc.close().await.is_err() || result.is_err() {
            return Err(AcpError::Start(
                "Thread efêmera ou encerramento não confirmados.".into(),
            ));
        }
        if home.join("auth.json").exists() || home.join("sessions").exists() {
            return Err(AcpError::Start(
                "A prova deixou autenticação ou sessões no disco.".into(),
            ));
        }
        Ok(())
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::protocol::JsonLines;
    use tokio::io::{DuplexStream, duplex, split};

    async fn write_json(writer: &mut (impl AsyncWrite + Unpin), value: Value) {
        let mut bytes = serde_json::to_vec(&value).unwrap();
        bytes.push(b'\n');
        writer.write_all(&bytes).await.unwrap();
    }
    fn mock(client: DuplexStream) -> Rpc {
        let (r, w) = split(client);
        Rpc {
            child: None,
            input: Box::new(w),
            output: Box::new(BufReader::new(r)),
            buffer: Vec::new(),
            next_id: 0,
        }
    }
    async fn receive(events: &mut mpsc::Receiver<Event>) -> Event {
        tokio::time::timeout(Duration::from_secs(2), events.recv())
            .await
            .unwrap()
            .unwrap()
    }
    #[test]
    fn proposal_catalog_and_thread_contract_have_no_tools_or_persistent_history() {
        let catalog: Value = serde_json::from_str(include_str!("proposal-model.json")).unwrap();
        for model in catalog["models"].as_array().unwrap() {
            assert_eq!(model["shell_type"], "disabled");
            assert!(model["apply_patch_tool_type"].is_null());
            assert_eq!(model["experimental_supported_tools"], json!([]));
            assert_eq!(model["include_skills_usage_instructions"], false);
            assert_eq!(model["include_plugin_usage_instructions"], false);
            assert_eq!(model["node_repl_disabled"], true);
        }
        let thread = thread_params(Path::new("empty-room"), MODEL);
        assert_eq!(thread["ephemeral"], true);
        assert_eq!(thread["approvalPolicy"], "never");
        assert_eq!(thread["sandbox"], "read-only");
        assert!(
            thread["baseInstructions"]
                .as_str()
                .unwrap()
                .contains("capturas")
        );
        assert!(validated_version("0.162.0"));
        assert!(!validated_version("0.163.0"));
        assert!(!valid_login_url(
            "https://auth.openai.com.evil.invalid/oauth/authorize?"
        ));
    }
    #[tokio::test]
    async fn partial_frames_survive_timer_cancellation_and_unsafe_rpc_is_not_written() {
        let (client, mut peer) = duplex(128);
        let mut reader = BufReader::new(client);
        let mut buffer = vec![];
        peer.write_all(b"{\"id\":").await.unwrap();
        assert!(
            tokio::time::timeout(
                Duration::from_millis(5),
                read_frame(&mut reader, &mut buffer)
            )
            .await
            .is_err()
        );
        peer.write_all(b"1}\n").await.unwrap();
        assert_eq!(read_frame(&mut reader, &mut buffer).await.unwrap()["id"], 1);
        let (client, mut peer) = duplex(128);
        let mut rpc = mock(client);
        for method in ["command/exec", "fs/readFile", "mcpServer/tool/call"] {
            assert!(rpc.send(method, json!({})).await.is_err());
        }
        assert!(
            tokio::time::timeout(Duration::from_millis(5), peer.read(&mut [0u8; 1]))
                .await
                .is_err()
        );
        let oversized = vec![b'x'; MAX_LINE + 1];
        let mut reader = BufReader::new(oversized.as_slice());
        assert!(read_frame(&mut reader, &mut vec![]).await.is_err());
    }
    #[tokio::test]
    async fn conversation_ignores_foreign_deltas_and_cancel_waits_for_completion() {
        let (client, peer) = duplex(16 * 1024);
        let mut rpc = mock(client);
        let (commands, mut command_rx) = mpsc::channel(8);
        let (events, mut receiver) = mpsc::channel(64);
        let (interrupted_tx, interrupted_rx) = tokio::sync::oneshot::channel();
        let (complete_tx, complete_rx) = tokio::sync::oneshot::channel();
        let (release_tx, release_rx) = tokio::sync::oneshot::channel();
        let engine = tokio::spawn(async move {
            serve(
                &mut rpc,
                json!({"account":{"type":"chatgpt","planType":"plus"}}),
                &CodexInfo {
                    executable: "never-run.exe".into(),
                    version: CODEX_VERSION.into(),
                },
                Path::new("fixture-home"),
                Path::new("fixture-room"),
                &mut command_rx,
                &events,
            )
            .await
        });
        let server = tokio::spawn(async move {
            let (r, mut w) = split(peer);
            let mut lines = JsonLines::new(BufReader::new(r));
            let request = lines.next().await.unwrap();
            assert_eq!(request["method"], "thread/start");
            assert_eq!(request["params"]["ephemeral"], true);
            write_json(&mut w,json!({"id":request["id"],"result":{"thread":{"id":"thread-one","ephemeral":true}}})).await;
            let request = lines.next().await.unwrap();
            assert_eq!(request["method"], "turn/start");
            assert_eq!(
                request["params"]["input"][0]["text"],
                "selected metadata only"
            );
            write_json(
                &mut w,
                json!({"id":request["id"],"result":{"turn":{"id":"turn-one"}}}),
            )
            .await;
            for (thread, turn, text) in [
                ("another-thread", "turn-one", "foreign-thread"),
                ("thread-one", "old-turn", "foreign-turn"),
                ("thread-one", "turn-one", "first delta"),
            ] {
                write_json(&mut w,json!({"method":"item/agentMessage/delta","params":{"threadId":thread,"turnId":turn,"delta":text}})).await;
            }
            let request = lines.next().await.unwrap();
            assert_eq!(request["method"], "turn/interrupt");
            assert_eq!(request["params"]["turnId"], "turn-one");
            write_json(&mut w, json!({"id":request["id"],"result":{}})).await;
            interrupted_tx.send(()).unwrap();
            complete_rx.await.unwrap();
            write_json(&mut w,json!({"method":"item/agentMessage/delta","params":{"threadId":"thread-one","turnId":"turn-one","delta":"late after cancellation"}})).await;
            write_json(&mut w,json!({"method":"turn/completed","params":{"threadId":"thread-one","turn":{"id":"turn-one","status":"interrupted"}}})).await;
            release_rx.await.unwrap();
        });
        commands
            .send(Command::Prompt {
                conversation: 7,
                text: "selected metadata only".into(),
            })
            .await
            .unwrap();
        loop {
            if let Event::Text { conversation, text } = receive(&mut receiver).await {
                assert_eq!(conversation, 7);
                assert_eq!(text, "first delta");
                break;
            }
        }
        commands
            .send(Command::Cancel { conversation: 7 })
            .await
            .unwrap();
        interrupted_rx.await.unwrap();
        assert!(matches!(
            receiver.try_recv(),
            Err(mpsc::error::TryRecvError::Empty)
        ));
        complete_tx.send(()).unwrap();
        assert!(matches!(
            receive(&mut receiver).await,
            Event::Done {
                conversation: 7,
                note: Some(_)
            }
        ));
        drop(commands);
        assert!(engine.await.unwrap().is_ok());
        release_tx.send(()).unwrap();
        server.await.unwrap();
    }
    #[tokio::test]
    async fn model_is_frozen_while_thread_start_is_pending() {
        let (client, peer) = duplex(16 * 1024);
        let mut rpc = mock(client);
        let (commands, mut command_rx) = mpsc::channel(8);
        let (events, mut receiver) = mpsc::channel(64);
        let (thread_tx, thread_rx) = tokio::sync::oneshot::channel();
        let (continue_tx, continue_rx) = tokio::sync::oneshot::channel();
        let (turn_tx, turn_rx) = tokio::sync::oneshot::channel();
        let (release_tx, release_rx) = tokio::sync::oneshot::channel();
        let options = catalog();
        assert!(options.len() > 1);
        let original = ModelChoice {
            model: options[0].id.clone(),
            effort: EFFORT.into(),
        };
        let alternate = ModelChoice {
            model: options[1].id.clone(),
            effort: EFFORT.into(),
        };
        let expected = original.clone();
        let engine = tokio::spawn(async move {
            serve(
                &mut rpc,
                json!({"account":{"type":"chatgpt"}}),
                &CodexInfo {
                    executable: "never-run.exe".into(),
                    version: CODEX_VERSION.into(),
                },
                Path::new("fixture-home"),
                Path::new("fixture-room"),
                &mut command_rx,
                &events,
            )
            .await
        });
        let server = tokio::spawn(async move {
            let (r, mut w) = split(peer);
            let mut lines = JsonLines::new(BufReader::new(r));
            let thread = lines.next().await.unwrap();
            assert_eq!(thread["method"], "thread/start");
            assert_eq!(thread["params"]["model"], expected.model);
            thread_tx.send(()).unwrap();
            continue_rx.await.unwrap();
            write_json(&mut w, json!({"id":thread["id"],"result":{"thread":{"id":"thread-frozen","ephemeral":true}}})).await;
            let turn = lines.next().await.unwrap();
            assert_eq!(turn["method"], "turn/start");
            assert_eq!(turn["params"]["model"], expected.model);
            assert_eq!(turn["params"]["effort"], expected.effort);
            turn_tx.send(()).unwrap();
            release_rx.await.unwrap();
        });
        commands
            .send(Command::PromptWithModel {
                conversation: 19,
                text: "reviewed metadata".into(),
                model: original,
            })
            .await
            .unwrap();
        thread_rx.await.unwrap();
        commands
            .send(Command::SetModel(alternate.clone()))
            .await
            .unwrap();
        loop {
            if let Event::Models { selected, .. } = receive(&mut receiver).await
                && selected == alternate
            {
                break;
            }
        }
        continue_tx.send(()).unwrap();
        turn_rx.await.unwrap();
        drop(commands);
        assert!(engine.await.unwrap().is_ok());
        release_tx.send(()).unwrap();
        server.await.unwrap();
    }
    #[tokio::test]
    async fn server_tool_requests_are_denied_and_tool_items_stop_the_conversation() {
        let (client, peer) = duplex(16 * 1024);
        let mut rpc = mock(client);
        let (commands, mut command_rx) = mpsc::channel(8);
        let (events, mut receiver) = mpsc::channel(64);
        let engine = tokio::spawn(async move {
            serve(
                &mut rpc,
                json!({"account":{"type":"chatgpt"}}),
                &CodexInfo {
                    executable: "never-run.exe".into(),
                    version: CODEX_VERSION.into(),
                },
                Path::new("fixture-home"),
                Path::new("fixture-room"),
                &mut command_rx,
                &events,
            )
            .await
        });
        let server = tokio::spawn(async move {
            let (r, mut w) = split(peer);
            let mut lines = JsonLines::new(BufReader::new(r));
            let thread = lines.next().await.unwrap();
            write_json(
                &mut w,
                json!({"id":thread["id"],"result":{"thread":{"id":"thread-one","ephemeral":true}}}),
            )
            .await;
            let turn = lines.next().await.unwrap();
            write_json(
                &mut w,
                json!({"id":turn["id"],"result":{"turn":{"id":"turn-one"}}}),
            )
            .await;
            write_json(&mut w,json!({"id":"request-tool","method":"item/tool/call","params":{"name":"read_private_file"}})).await;
            let denial = lines.next().await.unwrap();
            assert_eq!(denial["id"], "request-tool");
            assert_eq!(denial["error"]["code"], -32601);
            write_json(&mut w,json!({"method":"item/started","params":{"threadId":"thread-one","item":{"type":"fileChange"}}})).await;
        });
        commands
            .send(Command::Prompt {
                conversation: 8,
                text: "fixture".into(),
            })
            .await
            .unwrap();
        loop {
            if let Event::Blocked { conversation, .. } = receive(&mut receiver).await {
                assert_eq!(conversation, 8);
                break;
            }
        }
        assert!(engine.await.unwrap().is_err());
        server.await.unwrap();
    }
}
