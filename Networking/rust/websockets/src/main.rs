use tokio::net::TcpListener;
use tokio_tungstenite::accept_async;
use futures_util::{StreamExt, SinkExt};

#[tokio::main]
async fn main() {
    let listener = TcpListener::bind("127.0.0.1:9001").await.unwrap();
    println!("WebSocket server listening on 127.0.0.1:9001");

    while let Ok((stream, _)) = listener.accept().await {
        tokio::spawn(async move {
            let mut ws_stream = accept_async(stream).await.unwrap();
            println!("New WebSocket connection");

            while let Some(result) = ws_stream.next().await {
                match result {
                    Ok(msg) => {
                        println!("Received: {:?}", msg);
                        if msg.is_text() || msg.is_binary() {
                            ws_stream.send(msg).await.unwrap();
                        }
                    }
                    Err(e) => {
                        eprintln!("WebSocket error: {}", e);
                        break;
                    }
                }
            }
        });
    }
}
