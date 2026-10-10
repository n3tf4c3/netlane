use netlane_ai::protocol::*;
use serde_json::{Value, json};
use std::time::Duration;
use tokio::io::{AsyncReadExt, AsyncWriteExt, BufReader, duplex, split};
#[tokio::test]
async fn partial_frames_survive_cancellation() {
    let (client, mut server) = duplex(64);
    let mut lines = JsonLines::new(BufReader::new(client));
    server.write_all(b"{\"id\":").await.unwrap();
    assert!(
        tokio::time::timeout(Duration::from_millis(5), lines.next())
            .await
            .is_err()
    );
    server.write_all(b"1}\n").await.unwrap();
    assert_eq!(lines.next().await.unwrap()["id"], 1);
}
#[tokio::test]
async fn frame_size_truncation_and_invalid_json_fail_closed() {
    let mut lines = JsonLines::new(BufReader::new(&b"{\"id\":1}"[..]));
    assert!(lines.next().await.is_err());
    let mut lines = JsonLines::new(BufReader::new(&b"invalid\n"[..]));
    assert!(lines.next().await.is_err());
    let data = vec![b'x'; MAX_FRAME_BYTES + 1];
    let mut lines = JsonLines::new(BufReader::new(data.as_slice()));
    assert!(lines.next().await.is_err());
}
#[tokio::test]
async fn tool_requests_are_rejected_and_not_run() {
    let (client, server) = duplex(4096);
    let (r, w) = split(client);
    let (server_reader, mut wserver) = split(server);
    let worker = tokio::spawn(async move {
        let mut input = JsonLines::new(BufReader::new(server_reader));
        assert_eq!(input.next().await.unwrap()["method"], "account/read");
        wserver
            .write_all(b"{\"id\":\"tool-1\",\"method\":\"item/tool/call\",\"params\":{}}\n")
            .await
            .unwrap();
        let refusal = input.next().await.unwrap();
        assert_eq!(refusal["id"], "tool-1");
        assert_eq!(refusal["error"]["code"], -32601);
    });
    let mut rpc = Rpc::new(BufReader::new(r), w);
    assert!(
        rpc.call("account/read", json!({"refreshToken":false}))
            .await
            .is_err()
    );
    worker.await.unwrap();
}
#[tokio::test]
async fn unsafe_methods_are_refused_before_any_write() {
    let (client, mut server) = duplex(4096);
    let (r, w) = split(client);
    let mut rpc = Rpc::new(BufReader::new(r), w);
    for method in [
        "turn/start",
        "command/exec",
        "fs/readFile",
        "account/login/start",
        "mcpServer/tool/call",
    ] {
        assert!(rpc.call(method, json!({})).await.is_err());
    }
    assert!(
        tokio::time::timeout(Duration::from_millis(5), server.read(&mut [0u8; 1]))
            .await
            .is_err()
    );
}
#[tokio::test]
async fn response_ids_and_errors_are_not_mistaken_for_success() {
    for response in [
        json!({"id":999,"result":{}}),
        json!({"id":1,"error":{"code":401,"message":"secret should not be exposed"}}),
    ] {
        let (client, server) = duplex(4096);
        let (r, w) = split(client);
        let (server_reader, mut sw) = split(server);
        let task = tokio::spawn(async move {
            let mut lines = JsonLines::new(BufReader::new(server_reader));
            lines.next().await.unwrap();
            let mut data = serde_json::to_vec(&response).unwrap();
            data.push(b'\n');
            sw.write_all(&data).await.unwrap();
        });
        let mut rpc = Rpc::new(BufReader::new(r), w);
        let err = rpc
            .call("model/list", Value::Null)
            .await
            .unwrap_err()
            .to_string();
        assert!(!err.contains("secret"));
        task.await.unwrap();
    }
}
