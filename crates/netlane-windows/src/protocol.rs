use anyhow::{Result, ensure};
use netlane_core::planning::PlannedRule;
use serde::{Deserialize, Serialize};
use tokio::io::{AsyncRead, AsyncReadExt, AsyncWrite, AsyncWriteExt};
pub const MAX_MESSAGE_BYTES: usize = 65_536;
pub const VERSION: u32 = 1;
// A dedicated reader keeps partially received frames alive when timers win select!.
// Recreating read_exact futures in select! would corrupt the stream on cancellation.
pub struct Inbox {
    receiver: tokio::sync::mpsc::Receiver<Result<Message>>,
    task: tokio::task::JoinHandle<()>,
}
impl Inbox {
    pub fn new(mut reader: impl AsyncRead + Unpin + Send + 'static) -> Self {
        let (sender, receiver) = tokio::sync::mpsc::channel(16);
        let task = tokio::spawn(async move {
            loop {
                let message = receive(&mut reader).await;
                let failed = message.is_err();
                if sender.send(message).await.is_err() || failed {
                    break;
                }
            }
        });
        Self { receiver, task }
    }
    pub async fn receive(&mut self) -> Result<Message> {
        self.receiver
            .recv()
            .await
            .ok_or_else(|| anyhow::anyhow!("Canal de controle encerrado."))?
    }
}
impl Drop for Inbox {
    fn drop(&mut self) {
        self.task.abort();
    }
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(tag = "kind")]
pub enum Message {
    DefaultHello {
        version: u32,
    },
    DefaultApply {
        request: netlane_core::default_connection::Request,
    },
    DefaultResult {
        result: netlane_core::default_connection::OperationResult,
    },
    DefaultAck,
    DefaultFinished {
        accepted: bool,
    },
    Hello {
        version: u32,
    },
    Start {
        policy_path: String,
        allow_temporary_route_policies: bool,
    },
    Ping,
    Stop,
    Snapshot {
        process_id: u32,
        policy_path: String,
        revision: Option<String>,
        rules: Vec<PlannedRule>,
        detail: String,
    },
    Error {
        detail: String,
    },
    Stopped {
        success: bool,
        detail: String,
    },
}
pub async fn send(writer: &mut (impl AsyncWrite + Unpin), message: &Message) -> Result<()> {
    let bytes = serde_json::to_vec(message)?;
    ensure!(bytes.len() <= MAX_MESSAGE_BYTES, "Mensagem excede 64 KiB.");
    // If a write times out, the caller closes the session rather than reusing a partial frame.
    tokio::time::timeout(std::time::Duration::from_secs(5), async {
        writer
            .write_all(&(bytes.len() as u32).to_le_bytes())
            .await?;
        writer.write_all(&bytes).await?;
        writer.flush().await
    })
    .await??;
    Ok(())
}
pub async fn receive(reader: &mut (impl AsyncRead + Unpin)) -> Result<Message> {
    let length = reader.read_u32_le().await? as usize;
    ensure!(
        length > 0 && length <= MAX_MESSAGE_BYTES,
        "Tamanho inválido no canal de controle."
    );
    let mut bytes = vec![0; length];
    reader.read_exact(&mut bytes).await?;
    Ok(serde_json::from_slice(&bytes)?)
}
#[cfg(test)]
mod tests {
    use super::*;
    #[tokio::test]
    async fn timer_cancellation_keeps_partial_frame_intact() {
        let (mut writer, reader) = tokio::io::duplex(1024);
        let mut inbox = Inbox::new(reader);
        let bytes = serde_json::to_vec(&Message::Hello { version: VERSION }).unwrap();
        let header = (bytes.len() as u32).to_le_bytes();
        writer.write_all(&header[..2]).await.unwrap();
        assert!(
            tokio::time::timeout(std::time::Duration::from_millis(5), inbox.receive())
                .await
                .is_err()
        );
        writer.write_all(&header[2..]).await.unwrap();
        writer.write_all(&bytes[..3]).await.unwrap();
        assert!(
            tokio::time::timeout(std::time::Duration::from_millis(5), inbox.receive())
                .await
                .is_err()
        );
        writer.write_all(&bytes[3..]).await.unwrap();
        assert!(matches!(
            inbox.receive().await.unwrap(),
            Message::Hello { version: VERSION }
        ));
    }
    #[tokio::test]
    async fn frame_roundtrip_and_bounds() {
        let (mut a, mut b) = tokio::io::duplex(1024);
        send(&mut a, &Message::Hello { version: VERSION })
            .await
            .unwrap();
        assert!(matches!(
            receive(&mut b).await.unwrap(),
            Message::Hello { version: 1 }
        ));
        a.write_all(&65537u32.to_le_bytes()).await.unwrap();
        assert!(receive(&mut b).await.is_err());
    }
    #[tokio::test]
    async fn disconnect_mid_frame_is_not_success() {
        let (mut a, mut b) = tokio::io::duplex(1024);
        a.write_all(&100u32.to_le_bytes()).await.unwrap();
        a.write_all(b"{}").await.unwrap();
        drop(a);
        assert!(receive(&mut b).await.is_err());
    }
}
