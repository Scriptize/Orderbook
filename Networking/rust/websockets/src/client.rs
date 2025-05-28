use tokio_tungstenite::connect_async;
use futures_util::{SinkExt, StreamExt};
use url::Url;

#[tokio::main]
async fn main() {
    let url = Url::parse("ws://127.0.0.1:9001").unwrap();
    let (mut ws_stream, _) = connect_async(url).await.unwrap();
    println!("Connected to WebSocket server");

    ws_stream.send("Hello from client".into()).await.unwrap();

    if let Some(msg) = ws_stream.next().await {
        let msg = msg.unwrap();
        println!("Received from server: {}", msg);
    }
}
