// server/src/lib.rs

use std::sync::{
    atomic::{AtomicUsize, Ordering},
    Arc,
};

use exchange::Event;
use futures_util::{SinkExt, StreamExt};
use tokio::net::{TcpListener, TcpStream};
use tokio::sync::broadcast;
use tokio_tungstenite::accept_async;
use tungstenite::protocol::Message;

#[derive(Clone)]
pub struct ExchangeServer {
    pub event_tx: broadcast::Sender<Event>,
    pub client_count: Arc<AtomicUsize>,
}

impl ExchangeServer {
    pub fn new(event_tx: broadcast::Sender<Event>) -> Self {
        Self {
            event_tx,
            client_count: Arc::new(AtomicUsize::new(0)),
        }
    }

    pub async fn run(self, addr: &str) {
        let listener = TcpListener::bind(addr).await.unwrap();

        println!("WebSocket server listening on {}", addr);

        loop {
            let (stream, _) = listener.accept().await.unwrap();
            let server = self.clone();

            tokio::spawn(async move {
                server.handle_connection(stream).await;
            });
        }
    }

    async fn handle_connection(&self, stream: TcpStream) {
        let ws_stream = accept_async(stream).await.unwrap();

        self.client_count.fetch_add(1, Ordering::SeqCst);

        println!(
            "client connected, total = {}",
            self.client_count.load(Ordering::SeqCst)
        );

        let (mut write, mut read) = ws_stream.split();

        let subscribed = loop {
            match read.next().await {
                Some(Ok(Message::Text(text))) if text == "subscribe" => break true,
                Some(Ok(Message::Close(_))) | None => break false,
                Some(Ok(_)) => {}
                Some(Err(_)) => break false,
            }
        };

        if !subscribed {
            self.client_count.fetch_sub(1, Ordering::SeqCst);

            println!(
                "client disconnected before subscribe, total = {}",
                self.client_count.load(Ordering::SeqCst)
            );

            return;
        }

        let mut rx = self.event_tx.subscribe();

        loop {
            tokio::select! {
                recv = rx.recv() => {
                    match recv {
                        Ok(event) => {
                            let event_msg = serde_json::json!({
                                "type": "event",
                                "data": event
                            })
                            .to_string();

                            if write.send(Message::Text(event_msg)).await.is_err() {
                                break;
                            }
                        }

                        Err(broadcast::error::RecvError::Lagged(n)) => {
                            println!("client lagged by {} messages", n);
                        }

                        Err(broadcast::error::RecvError::Closed) => {
                            break;
                        }
                    }
                }

                incoming = read.next() => {
                    match incoming {
                        Some(Ok(Message::Close(_))) | None | Some(Err(_)) => break,
                        _ => {}
                    }
                }
            }
        }

        self.client_count.fetch_sub(1, Ordering::SeqCst);

        println!(
            "client disconnected, total = {}",
            self.client_count.load(Ordering::SeqCst)
        );
    }
}
