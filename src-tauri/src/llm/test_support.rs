//! Test helpers: Messages API stream bodies and a local fake API server.

use std::sync::{Arc, Mutex};

use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpListener;

/// One SSE event in the Messages API wire format.
pub fn sse(data: serde_json::Value) -> String {
    format!(
        "event: {}\ndata: {data}\n\n",
        data["type"].as_str().unwrap()
    )
}

/// A whole streamed response: a thinking block (as on Opus), a ping, then `text` cut into
/// deltas of `piece` characters.
pub fn streamed_text(text: &str, piece: usize) -> String {
    let mut out = sse(serde_json::json!({
        "type": "message_start",
        "message": { "id": "msg_1", "type": "message", "role": "assistant",
            "model": "claude-haiku-5-5", "content": [],
            "usage": { "input_tokens": 210, "cache_read_input_tokens": 1400,
                "cache_creation_input_tokens": 0, "output_tokens": 1 } }
    }));
    out += &sse(
        serde_json::json!({ "type": "content_block_start", "index": 0,
        "content_block": { "type": "thinking", "thinking": "" } }),
    );
    out += &sse(serde_json::json!({ "type": "content_block_stop", "index": 0 }));
    out += &sse(serde_json::json!({ "type": "ping" }));
    out += &sse(
        serde_json::json!({ "type": "content_block_start", "index": 1,
        "content_block": { "type": "text", "text": "" } }),
    );
    let chars: Vec<char> = text.chars().collect();
    for chunk in chars.chunks(piece.max(1)) {
        let chunk: String = chunk.iter().collect();
        out += &sse(
            serde_json::json!({ "type": "content_block_delta", "index": 1,
            "delta": { "type": "text_delta", "text": chunk } }),
        );
    }
    out += &sse(serde_json::json!({ "type": "content_block_stop", "index": 1 }));
    out += &sse(serde_json::json!({ "type": "message_delta",
        "delta": { "stop_reason": "end_turn" }, "usage": { "output_tokens": 64 } }));
    out += &sse(serde_json::json!({ "type": "message_stop" }));
    out
}

/// Minimal HTTP/1.1 server: answers each connection with the next canned response and
/// records the raw requests.
pub async fn fake_api(responses: Vec<(u16, String)>) -> (String, Arc<Mutex<Vec<String>>>) {
    serve(responses, None).await
}

/// Like `fake_api`, but each response body stops halfway for `pause` (a stream in progress).
pub async fn fake_api_paused(
    responses: Vec<(u16, String)>,
    pause: std::time::Duration,
) -> (String, Arc<Mutex<Vec<String>>>) {
    serve(responses, Some(pause)).await
}

async fn serve(
    responses: Vec<(u16, String)>,
    pause: Option<std::time::Duration>,
) -> (String, Arc<Mutex<Vec<String>>>) {
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let url = format!("http://{}/v1/messages", listener.local_addr().unwrap());
    let seen = Arc::new(Mutex::new(Vec::new()));
    let log = seen.clone();
    tokio::spawn(async move {
        for (status, body) in responses {
            let (mut tcp, _) = listener.accept().await.unwrap();
            let mut raw = Vec::new();
            let mut buf = [0u8; 8192];
            loop {
                let n = tcp.read(&mut buf).await.unwrap();
                raw.extend_from_slice(&buf[..n]);
                let text = String::from_utf8_lossy(&raw).to_string();
                if let Some(end) = text.find("\r\n\r\n") {
                    let length = text[..end]
                        .lines()
                        .find_map(|l| {
                            l.to_lowercase()
                                .strip_prefix("content-length:")
                                .map(|v| v.trim().parse::<usize>().unwrap())
                        })
                        .unwrap_or(0);
                    if raw.len() >= end + 4 + length {
                        break;
                    }
                }
                if n == 0 {
                    break;
                }
            }
            log.lock()
                .unwrap()
                .push(String::from_utf8_lossy(&raw).to_string());
            let kind = if status == 200 {
                "text/event-stream"
            } else {
                "application/json"
            };
            let reply = format!(
                "HTTP/1.1 {status} X\r\ncontent-type: {kind}\r\ncontent-length: {}\r\nconnection: close\r\n\r\n{body}",
                body.len()
            );
            let bytes = reply.as_bytes();
            match pause {
                Some(pause) => {
                    let half = bytes.len() - body.len() / 2;
                    tcp.write_all(&bytes[..half]).await.unwrap();
                    tcp.flush().await.unwrap();
                    tokio::time::sleep(pause).await;
                    tcp.write_all(&bytes[half..]).await.unwrap();
                }
                None => tcp.write_all(bytes).await.unwrap(),
            }
        }
    });
    (url, seen)
}
