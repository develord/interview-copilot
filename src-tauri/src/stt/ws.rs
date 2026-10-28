//! WebSocket connection loop shared by every provider. Provider specifics live in a `Codec`,
//! which is pure (JSON in, events out) and unit-tested without a network.

use std::time::Duration;

use futures_util::{SinkExt, StreamExt};
use tokio::sync::mpsc;
use tokio_tungstenite::tungstenite::client::IntoClientRequest;
use tokio_tungstenite::tungstenite::http::{HeaderName, HeaderValue};
use tokio_tungstenite::tungstenite::protocol::CloseFrame;
use tokio_tungstenite::tungstenite::{Error as WsError, Message};

use super::provider::{AudioMsg, EventSender, ProviderEvent};
use super::types::{SttError, SttErrorCode};

/// After the audio ends, how long to wait for the provider's last transcripts.
const FINISH_TIMEOUT: Duration = Duration::from_secs(3);
const CONNECT_TIMEOUT: Duration = Duration::from_secs(10);

#[derive(Debug)]
pub enum CodecOutput {
    Event(ProviderEvent),
    Send(Message),
    /// The provider confirmed the end of the session.
    Finished,
}

pub trait Codec: Send {
    /// Messages to send right after the WebSocket opens (configuration).
    fn on_open(&mut self) -> Vec<Message>;
    fn on_text(&mut self, text: &str) -> Result<Vec<CodecOutput>, SttError>;
    fn force_endpoint(&self) -> Message;
    /// Message that ends the stream gracefully.
    fn finish(&self) -> Message;
    /// Maps a server close frame to an error (heuristics are provider-specific).
    fn classify_close(&self, frame: Option<&CloseFrame>) -> SttError;
}

pub async fn run(
    endpoint: &str,
    headers: &[(&'static str, String)],
    codec: &mut dyn Codec,
    audio: &mut mpsc::Receiver<AudioMsg>,
    events: &EventSender,
) -> Result<(), SttError> {
    let mut request = endpoint
        .into_client_request()
        .map_err(|e| SttError::new(SttErrorCode::Protocol, format!("bad endpoint: {e}")))?;
    for (name, value) in headers {
        let value = HeaderValue::from_str(value).map_err(|_| {
            SttError::new(
                SttErrorCode::Unauthorized,
                "API key contains invalid characters",
            )
        })?;
        request
            .headers_mut()
            .insert(HeaderName::from_static(name), value);
    }

    let (socket, _response) =
        tokio::time::timeout(CONNECT_TIMEOUT, tokio_tungstenite::connect_async(request))
            .await
            .map_err(|_| SttError::new(SttErrorCode::Network, "connection timed out"))?
            .map_err(classify_connect_error)?;
    let (mut sink, mut stream) = socket.split();

    for message in codec.on_open() {
        sink.send(message).await.map_err(network)?;
    }

    let mut finishing = false;
    loop {
        tokio::select! {
            outgoing = audio.recv(), if !finishing => match outgoing {
                Some(AudioMsg::Chunk(bytes)) => sink.send(Message::Binary(bytes.into())).await.map_err(network)?,
                Some(AudioMsg::ForceEndpoint) => sink.send(codec.force_endpoint()).await.map_err(network)?,
                None => {
                    finishing = true;
                    sink.send(codec.finish()).await.map_err(network)?;
                }
            },
            incoming = stream.next() => match incoming {
                Some(Ok(Message::Text(text))) => {
                    for output in codec.on_text(text.as_str())? {
                        match output {
                            CodecOutput::Event(event) => {
                                let _ = events.send(event);
                            }
                            CodecOutput::Send(message) => sink.send(message).await.map_err(network)?,
                            CodecOutput::Finished => {
                                // Close cleanly so the provider releases the session at once.
                                let _ = sink.close().await;
                                return Ok(());
                            }
                        }
                    }
                }
                Some(Ok(Message::Close(frame))) => {
                    return if finishing { Ok(()) } else { Err(codec.classify_close(frame.as_ref())) };
                }
                Some(Ok(_)) => {}
                Some(Err(error)) => return if finishing { Ok(()) } else { Err(network(error)) },
                None => {
                    return if finishing {
                        Ok(())
                    } else {
                        Err(SttError::new(SttErrorCode::Network, "connection closed by the server"))
                    };
                }
            },
            _ = tokio::time::sleep(FINISH_TIMEOUT), if finishing => {
                let _ = sink.close().await;
                return Ok(());
            }
        }
    }
}

fn network(error: WsError) -> SttError {
    SttError::new(SttErrorCode::Network, error.to_string())
}

/// HTTP status at the WebSocket upgrade tells most failures apart.
pub fn classify_connect_error(error: WsError) -> SttError {
    match error {
        WsError::Http(response) => {
            let status = response.status().as_u16();
            let code = match status {
                401 | 403 => SttErrorCode::Unauthorized,
                402 => SttErrorCode::Quota,
                408 | 429 => SttErrorCode::Network,
                500..=599 => SttErrorCode::Provider,
                _ => SttErrorCode::Protocol,
            };
            SttError::new(code, format!("HTTP {status} during connection"))
        }
        other => network(other),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tokio::net::TcpListener;

    struct EchoCodec;

    impl Codec for EchoCodec {
        fn on_open(&mut self) -> Vec<Message> {
            vec![Message::Text("hello".into())]
        }
        fn on_text(&mut self, text: &str) -> Result<Vec<CodecOutput>, SttError> {
            Ok(match text {
                "done" => vec![CodecOutput::Finished],
                other => vec![CodecOutput::Event(ProviderEvent::Partial {
                    turn: 0,
                    text: other.into(),
                    start_ms: 0,
                    end_ms: 0,
                })],
            })
        }
        fn force_endpoint(&self) -> Message {
            Message::Text("force".into())
        }
        fn finish(&self) -> Message {
            Message::Text("finish".into())
        }
        fn classify_close(&self, _frame: Option<&CloseFrame>) -> SttError {
            SttError::new(SttErrorCode::Unauthorized, "closed")
        }
    }

    /// One-connection fake server: records what the client sent, replies on cue.
    async fn fake_server(listener: TcpListener) -> Vec<String> {
        let (tcp, _) = listener.accept().await.unwrap();
        let mut ws = tokio_tungstenite::accept_async(tcp).await.unwrap();
        let mut seen = Vec::new();
        while let Some(Ok(message)) = ws.next().await {
            match message {
                Message::Text(t) => {
                    seen.push(t.to_string());
                    match t.as_str() {
                        "hello" => ws.send(Message::Text("ready".into())).await.unwrap(),
                        "force" => ws.send(Message::Text("forced".into())).await.unwrap(),
                        "finish" => {
                            ws.send(Message::Text("done".into())).await.unwrap();
                            break;
                        }
                        _ => {}
                    }
                }
                Message::Binary(b) => seen.push(format!("audio:{}", b.len())),
                _ => {}
            }
        }
        seen
    }

    #[tokio::test]
    async fn streams_audio_relays_events_and_finishes_gracefully() {
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let url = format!("ws://{}", listener.local_addr().unwrap());
        let server = tokio::spawn(fake_server(listener));

        let (audio_tx, mut audio_rx) = mpsc::channel(8);
        let (events_tx, mut events_rx) = mpsc::unbounded_channel();
        audio_tx
            .send(AudioMsg::Chunk(vec![0; 2_560]))
            .await
            .unwrap();
        audio_tx.send(AudioMsg::ForceEndpoint).await.unwrap();
        drop(audio_tx);

        let result = run(
            &url,
            &[("authorization", "k".into())],
            &mut EchoCodec,
            &mut audio_rx,
            &events_tx,
        )
        .await;
        assert_eq!(result, Ok(()));
        assert_eq!(
            server.await.unwrap(),
            vec!["hello", "audio:2560", "force", "finish"]
        );

        let mut texts = Vec::new();
        while let Ok(ProviderEvent::Partial { text, .. }) = events_rx.try_recv() {
            texts.push(text);
        }
        assert!(texts.contains(&"ready".to_string()));
    }

    #[tokio::test]
    async fn an_unexpected_close_is_classified_by_the_codec() {
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let url = format!("ws://{}", listener.local_addr().unwrap());
        tokio::spawn(async move {
            let (tcp, _) = listener.accept().await.unwrap();
            let mut ws = tokio_tungstenite::accept_async(tcp).await.unwrap();
            let _ = ws.close(None).await;
        });
        let (_audio_tx, mut audio_rx) = mpsc::channel(8);
        let (events_tx, _events_rx) = mpsc::unbounded_channel();

        let error = run(&url, &[], &mut EchoCodec, &mut audio_rx, &events_tx)
            .await
            .unwrap_err();
        assert_eq!(error.code, SttErrorCode::Unauthorized);
    }

    #[tokio::test]
    async fn refused_upgrades_map_to_stable_codes() {
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let url = format!("ws://{}", listener.local_addr().unwrap());
        tokio::spawn(async move {
            use tokio::io::{AsyncReadExt, AsyncWriteExt};
            let (mut tcp, _) = listener.accept().await.unwrap();
            let mut buf = [0u8; 1024];
            let _ = tcp.read(&mut buf).await;
            let _ = tcp
                .write_all(b"HTTP/1.1 401 Unauthorized\r\ncontent-length: 0\r\n\r\n")
                .await;
        });
        let (_audio_tx, mut audio_rx) = mpsc::channel(8);
        let (events_tx, _events_rx) = mpsc::unbounded_channel();

        let error = run(&url, &[], &mut EchoCodec, &mut audio_rx, &events_tx)
            .await
            .unwrap_err();
        assert_eq!(error.code, SttErrorCode::Unauthorized);
        assert!(!error.is_recoverable());
    }
}
