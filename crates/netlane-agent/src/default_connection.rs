//! A one-shot command, independent from WFP. Recovery paths never cross the UAC boundary.
use anyhow::{Result, ensure};
use netlane_core::default_connection::{self as domain, OperationResult, Request, Settings};
use netlane_windows::protocol::{self, Message};
use std::{path::Path, time::Duration};
const VERSION: u32 = 1;

/// Called only after the unelevated controller durably saves its recovery journal.
#[cfg(windows)]
pub fn execute(executable: &Path, request: Request) -> Result<OperationResult> {
    tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()?
        .block_on(control(executable, request))
}
#[cfg(windows)]
async fn control(executable: &Path, request: Request) -> Result<OperationResult> {
    use netlane_windows::session;
    use tokio::time;
    let name = format!("{}{}", session::PREFIX, uuid::Uuid::new_v4().simple());
    let pipe = session::create_pipe(&name)?;
    let path = executable.to_owned();
    let launch_name = name.clone();
    let (process, pid) =
        tokio::task::spawn_blocking(move || session::launch_default_agent(&path, &launch_name))
            .await??;
    let result = async {
        let deadline = time::Instant::now() + Duration::from_secs(35);
        loop {
            time::timeout_at(deadline, pipe.connect()).await??;
            if session::verify_client(&pipe, pid).is_ok() {
                break;
            }
            pipe.disconnect()?;
        }
        exchange(pipe, &request).await
    }
    .await;
    // The channel is now closed. Never release the UI operation lease while a writer
    // or rollback may still be running. A stuck CIM provider keeps the operation busy.
    while session::running(&process) {
        time::sleep(Duration::from_millis(50)).await;
    }
    let result = result?;
    ensure!(
        !result.success || session::exit_code(&process)? == 0,
        "O componente encerrou sem confirmar a alteração. Releia a prioridade."
    );
    Ok(result)
}
async fn exchange<S: tokio::io::AsyncRead + tokio::io::AsyncWrite + Unpin>(
    mut pipe: S,
    request: &Request,
) -> Result<OperationResult> {
    tokio::time::timeout(Duration::from_secs(90), async {
        ensure!(
            matches!(
                protocol::receive(&mut pipe).await?,
                Message::DefaultHello { version: VERSION }
            ),
            "Componente de conexão padrão incompatível."
        );
        protocol::send(
            &mut pipe,
            &Message::DefaultApply {
                request: request.clone(),
            },
        )
        .await?;
        let result = match protocol::receive(&mut pipe).await? {
            Message::DefaultResult { result } => result,
            Message::Error { detail } => anyhow::bail!("{detail}"),
            _ => anyhow::bail!("A alteração não foi confirmada."),
        };
        result.snapshot.validate()?;
        if result.success {
            request.verify_after(&result.snapshot)?;
        }
        protocol::send(&mut pipe, &Message::DefaultAck).await?;
        ensure!(
            matches!(
                protocol::receive(&mut pipe).await?,
                Message::DefaultFinished { accepted: true }
            ),
            "Componente encerrou sem confirmar recebimento. Recuperação preservada."
        );
        Ok(result)
    })
    .await?
}
/// Generic exchange permits failure/disconnect tests without launching a process or touching Windows.
async fn serve<S: tokio::io::AsyncRead + tokio::io::AsyncWrite + Unpin>(
    mut pipe: S,
    settings: &mut impl Settings,
    alive: impl Fn() -> bool,
) -> Result<()> {
    protocol::send(&mut pipe, &Message::DefaultHello { version: VERSION }).await?;
    let Message::DefaultApply { request } =
        tokio::time::timeout(Duration::from_secs(15), protocol::receive(&mut pipe)).await??
    else {
        anyhow::bail!("Pedido de conexão padrão inválido.");
    };
    let mut receipt = match domain::apply(settings, &request, &alive) {
        Ok(receipt) => receipt,
        Err(e) => {
            let _ = protocol::send(
                &mut pipe,
                &Message::Error {
                    detail: e.to_string(),
                },
            )
            .await;
            return Err(e);
        }
    };
    let acknowledged: Result<()> = async {
        protocol::send(
            &mut pipe,
            &Message::DefaultResult {
                result: receipt.result.clone(),
            },
        )
        .await?;
        ensure!(
            matches!(
                tokio::time::timeout(Duration::from_secs(15), protocol::receive(&mut pipe))
                    .await??,
                Message::DefaultAck
            ) && alive(),
            "Confirmação de recebimento inválida ou janela encerrada."
        );
        Ok(())
    }
    .await;
    if let Err(e) = acknowledged {
        if receipt.result.success {
            receipt.revert(settings);
        }
        return Err(e);
    }
    protocol::send(&mut pipe, &Message::DefaultFinished { accepted: true }).await?;
    // Once Ack was accepted, loss of the final message leaves the durable journal
    // pending for a reread/restore, rather than undoing a confirmed user operation.
    ensure!(
        receipt.result.success || receipt.result.rollback_confirmed,
        "Restauração incompleta: {}",
        receipt.result.detail
    );
    Ok(())
}
#[cfg(windows)]
pub async fn run(name: &str, owner_pid: u32, owner_start: u64) -> Result<()> {
    use netlane_windows::{default_connection::WindowsSettings, session};
    session::require_elevation()?;
    let owner = session::open_owner(owner_pid, owner_start)?;
    let pipe = session::connect_pipe(name)?;
    session::verify_server(&pipe, owner_pid)?;
    // Same lease as the .NET implementation, excluding concurrent changes by either version.
    let _lease = session::create_pipe("NetLane.DefaultConnection.v1")?;
    let began = std::time::Instant::now();
    serve(pipe, &mut WindowsSettings, || {
        session::running(&owner) && began.elapsed() < Duration::from_secs(80)
    })
    .await
}

#[cfg(test)]
mod tests {
    use super::*;
    use netlane_core::default_connection::{Interface, Kind, Metric, Route, Snapshot};
    struct Fake(Snapshot);
    impl Settings for Fake {
        fn read(&mut self) -> Result<Snapshot> {
            Ok(self.0.clone())
        }
        fn write(&mut self, m: &Metric) -> Result<()> {
            let i = self.0.interfaces.iter_mut().find(|i| i.id == m.id).unwrap();
            i.automatic_metric = m.automatic_metric;
            i.metric = if m.automatic_metric { 25 } else { m.metric };
            i.saved_metric = (!m.automatic_metric).then_some(m.metric);
            self.0.routes[0].interface_metric = i.metric;
            Ok(())
        }
    }
    fn initial() -> Snapshot {
        let i = Interface {
            id: uuid::Uuid::from_u128(1),
            name: "Ethernet".into(),
            kind: Kind::Cable,
            index: 1,
            connected: true,
            automatic_metric: true,
            metric: 25,
            default_route_metric: Some(0),
            saved_metric: None,
        };
        Snapshot {
            interfaces: vec![i],
            routes: vec![Route {
                index: 1,
                name: "Ethernet".into(),
                interface_metric: 25,
                route_metric: 0,
            }],
        }
    }
    fn request(s: &Snapshot) -> Request {
        Request {
            expected: s.clone(),
            changes: domain::plan(s, s.interfaces[0].id, &[]).unwrap(),
            preferred_interface: Some(s.interfaces[0].id),
        }
    }
    #[tokio::test]
    async fn missing_final_receipt_after_ack_is_not_success() {
        let (ui, mut agent) = tokio::io::duplex(65_536);
        let server = tokio::spawn(async move {
            protocol::send(&mut agent, &Message::DefaultHello { version: VERSION })
                .await
                .unwrap();
            protocol::receive(&mut agent).await.unwrap();
            let mut fake = Fake(initial());
            let receipt = domain::apply(&mut fake, &request(&initial()), || true).unwrap();
            protocol::send(
                &mut agent,
                &Message::DefaultResult {
                    result: receipt.result,
                },
            )
            .await
            .unwrap();
            assert!(matches!(
                protocol::receive(&mut agent).await.unwrap(),
                Message::DefaultAck
            ));
            // Disconnect before the final receipt.
        });
        assert!(exchange(ui, &request(&initial())).await.is_err());
        server.await.unwrap();
    }
    #[tokio::test]
    async fn owner_gone_before_ack_causes_rollback_even_if_ui_sent_ack() {
        let (ui, agent) = tokio::io::duplex(65_536);
        let server = tokio::spawn(async move {
            let calls = std::sync::atomic::AtomicUsize::new(0);
            let mut fake = Fake(initial());
            assert!(
                serve(agent, &mut fake, || calls
                    .fetch_add(1, std::sync::atomic::Ordering::Relaxed)
                    < 3)
                .await
                .is_err()
            );
            fake.0
        });
        assert!(exchange(ui, &request(&initial())).await.is_err());
        assert!(server.await.unwrap().interfaces[0].automatic_metric);
    }
    #[tokio::test]
    async fn authenticated_exchange_requires_final_receipt() {
        let before = initial();
        let (a, b) = tokio::io::duplex(65_536);
        let server = tokio::spawn(async move {
            let mut fake = Fake(initial());
            serve(b, &mut fake, || true).await.unwrap();
            fake.0
        });
        let response = exchange(a, &request(&before)).await.unwrap();
        assert!(response.success);
        assert_eq!(server.await.unwrap().interfaces[0].metric, 5);
    }
    #[tokio::test]
    async fn disconnect_without_ack_restores_original() {
        let (mut ui, agent) = tokio::io::duplex(65_536);
        let server = tokio::spawn(async move {
            let mut fake = Fake(initial());
            assert!(serve(agent, &mut fake, || true).await.is_err());
            fake.0
        });
        assert!(matches!(
            protocol::receive(&mut ui).await.unwrap(),
            Message::DefaultHello { .. }
        ));
        protocol::send(
            &mut ui,
            &Message::DefaultApply {
                request: request(&initial()),
            },
        )
        .await
        .unwrap();
        assert!(matches!(
            protocol::receive(&mut ui).await.unwrap(),
            Message::DefaultResult { .. }
        ));
        drop(ui);
        let after = server.await.unwrap();
        assert!(after.interfaces[0].automatic_metric);
        assert_eq!(after.interfaces[0].metric, 25);
    }
    #[tokio::test]
    async fn old_component_and_false_result_are_not_acknowledged() {
        for old in [true, false] {
            let (ui, mut agent) = tokio::io::duplex(65_536);
            let server = tokio::spawn(async move {
                protocol::send(
                    &mut agent,
                    &if old {
                        Message::Hello { version: 1 }
                    } else {
                        Message::DefaultHello { version: VERSION }
                    },
                )
                .await
                .unwrap();
                if !old {
                    protocol::receive(&mut agent).await.unwrap();
                    protocol::send(
                        &mut agent,
                        &Message::DefaultResult {
                            result: OperationResult {
                                snapshot: initial(),
                                success: true,
                                detail: "false success".into(),
                                rollback_confirmed: false,
                            },
                        },
                    )
                    .await
                    .unwrap();
                }
                assert!(protocol::receive(&mut agent).await.is_err());
            });
            assert!(exchange(ui, &request(&initial())).await.is_err());
            server.await.unwrap();
        }
    }
}
