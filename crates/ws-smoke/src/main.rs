use futures_util::{SinkExt, StreamExt};
use tokio::time::{Duration, timeout};
use tokio_tungstenite::connect_async;

#[tokio::main]
async fn main() {
    let url = std::env::args()
        .nth(1)
        .unwrap_or_else(|| "ws://127.0.0.1:8082/ws/telemetry".to_string());
    eprintln!("Connecting to {url}");
    let (ws_stream, _) = connect_async(url).await.expect("WS connect failed");
    let (mut write, mut read) = ws_stream.split();

    // Optional: send a ping
    let _ = write
        .send(tokio_tungstenite::tungstenite::Message::Ping(
            tokio_tungstenite::tungstenite::Bytes::new(),
        ))
        .await;

    // Wait up to 5s for one telemetry message, then print and exit
    match timeout(Duration::from_secs(5), read.next()).await {
        Ok(Some(Ok(msg))) => match msg {
            tokio_tungstenite::tungstenite::Message::Text(t) => {
                println!("WS message: {t}");
            }
            other => {
                println!("WS non-text message: {other:?}");
            }
        },
        Ok(Some(Err(e))) => {
            eprintln!("WS receive error: {e}");
            std::process::exit(2);
        }
        Ok(None) => {
            eprintln!("WS closed by server");
            std::process::exit(3);
        }
        Err(_) => {
            eprintln!("Timeout waiting for WS telemetry");
            std::process::exit(4);
        }
    }
}
