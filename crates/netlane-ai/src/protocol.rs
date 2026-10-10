//! Bounded JSONL. No command, file, MCP, login or inference RPC can be sent here.
use anyhow::{Context, Result, bail, ensure};
use serde_json::{Value, json};
use tokio::io::{AsyncBufRead, AsyncBufReadExt, AsyncWrite, AsyncWriteExt};
pub const MAX_FRAME_BYTES: usize = 1024 * 1024;
pub struct JsonLines<R> {
    reader: R,
    partial: Vec<u8>,
}
impl<R: AsyncBufRead + Unpin> JsonLines<R> {
    pub fn new(reader: R) -> Self {
        Self {
            reader,
            partial: vec![],
        }
    }
    /// Partial bytes live in the reader so cancelling this future cannot lose framing.
    pub async fn next(&mut self) -> Result<Value> {
        loop {
            let chunk = self.reader.fill_buf().await?;
            ensure!(
                !chunk.is_empty(),
                "Codex encerrou a conexão antes da resposta."
            );
            let newline = chunk.iter().position(|b| *b == b'\n');
            let take = newline.map_or(chunk.len(), |i| i + 1);
            ensure!(
                self.partial.len() + take <= MAX_FRAME_BYTES,
                "Resposta Codex excede 1 MiB."
            );
            self.partial.extend_from_slice(&chunk[..take]);
            self.reader.consume(take);
            if newline.is_some() {
                let bytes = std::mem::take(&mut self.partial);
                return serde_json::from_slice(&bytes).context("Codex enviou um frame inválido");
            }
        }
    }
}
pub struct Rpc<R, W> {
    lines: JsonLines<R>,
    writer: W,
    next_id: u64,
}
impl<R: AsyncBufRead + Unpin, W: AsyncWrite + Unpin> Rpc<R, W> {
    pub fn new(reader: R, writer: W) -> Self {
        Self {
            lines: JsonLines::new(reader),
            writer,
            next_id: 0,
        }
    }
    async fn write(&mut self, value: &Value) -> Result<()> {
        let mut data = serde_json::to_vec(value)?;
        ensure!(
            data.len() < MAX_FRAME_BYTES,
            "Solicitação Codex excede o limite."
        );
        data.push(b'\n');
        self.writer.write_all(&data).await?;
        self.writer.flush().await?;
        Ok(())
    }
    pub async fn initialized(&mut self) -> Result<()> {
        self.write(&json!({"method":"initialized"})).await
    }
    pub async fn call(&mut self, method: &str, params: Value) -> Result<Value> {
        ensure!(
            matches!(
                method,
                "initialize"
                    | "account/read"
                    | "model/list"
                    | "config/read"
                    | "mcpServerStatus/list"
            ),
            "Método fora da ponte de metadados."
        );
        self.next_id += 1;
        let id = self.next_id;
        self.write(&json!({"id":id,"method":method,"params":params}))
            .await?;
        for _ in 0..256 {
            let frame = self.lines.next().await?;
            if frame.get("method").is_some() {
                if let Some(request_id) = frame.get("id") {
                    self.write(&json!({"id":request_id,"error":{"code":-32601,"message":"NetLane não executa solicitações do servidor."}})).await?;
                    bail!("Codex solicitou uma operação fora da consulta de metadados.");
                }
                continue;
            }
            ensure!(
                frame.get("id").and_then(Value::as_u64) == Some(id),
                "Resposta Codex não corresponde à solicitação."
            );
            if let Some(error) = frame.get("error") {
                bail!(
                    "Codex recusou a consulta (código {}).",
                    error.get("code").and_then(Value::as_i64).unwrap_or(-1)
                );
            }
            return frame
                .get("result")
                .cloned()
                .context("Resposta Codex sem resultado");
        }
        bail!("Notificações Codex excederam o limite.")
    }
}
