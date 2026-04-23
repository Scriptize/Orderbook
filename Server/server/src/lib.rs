// server/src/lib.rs

use std::sync::{
    atomic::{AtomicUsize, Ordering},
    Arc, Mutex,
};

use exchange::{Event, Exchange};
use futures_util::{SinkExt, StreamExt};
use tokio::net::{TcpListener, TcpStream};
use tokio::sync::broadcast;
use tokio_tungstenite::accept_async;
use tungstenite::protocol::Message;

pub struct ExchangeServer {
    pub event_tx: broadcast::Sender<Event>,
    pub client_count: Arc<AtomicUsize>,
    pub exchange: Arc<Mutex<Exchange>>,
}

impl ExchangeServer {
    pub fn new(exchange: Arc<Mutex<Exchange>>) -> Self {
        let (tx, _) = broadcast::channel(10_000);

        Self {
            event_tx: tx,
            client_count: Arc::new(AtomicUsize::new(0)),
            exchange,
        }
    }

    pub async fn start(self, addr: &str) {
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
                Some(Ok(_)) => {}
                Some(Err(_)) | None => break false,
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

        let snapshot_msg = self.snapshot_message();
        if write.send(Message::Text(snapshot_msg.into())).await.is_err() {
            self.client_count.fetch_sub(1, Ordering::SeqCst);
            println!(
                "client disconnected after snapshot send, total = {}",
                self.client_count.load(Ordering::SeqCst)
            );
            return;
        }

        loop {
            tokio::select! {
                recv = rx.recv() => {
                    match recv {
                        Ok(event) => {
                            let event_msg = serde_json::json!({
                                "type": "event",
                                "data": event
                            }).to_string();

                            if write.send(Message::Text(event_msg.into())).await.is_err() {
                                break;
                            }

                            let snapshot_msg = self.snapshot_message();
                            if write.send(Message::Text(snapshot_msg.into())).await.is_err() {
                                break;
                            }
                        }
                        Err(broadcast::error::RecvError::Lagged(n)) => {
                            println!("client lagged by {} messages", n);

                            let snapshot_msg = self.snapshot_message();
                            if write.send(Message::Text(snapshot_msg.into())).await.is_err() {
                                break;
                            }
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

    fn snapshot_message(&self) -> String {
        let snapshot = {
            let mut exchange = self.exchange.lock().unwrap();
            exchange.get_snapshot()
        };

        serde_json::json!({
            "type": "snapshot",
            "data": snapshot
        })
        .to_string()
    }
}

impl Clone for ExchangeServer {
    fn clone(&self) -> Self {
        Self {
            event_tx: self.event_tx.clone(),
            client_count: self.client_count.clone(),
            exchange: self.exchange.clone(),
        }
    }
}