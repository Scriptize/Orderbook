use std::{os::windows::process, process::Command};

use orderbook::OrderbookLevelInfos;
use tokio::sync::mpsc;

use exchange::*;
use tokio::{
    net::{TcpListener, TcpStream},
    sync::broadcast,
};
use tokio_tungstenite::accept_async;
use tungstenite::protocol::Message;
use futures_util::{StreamExt, SinkExt};

pub struct ExchangeServer {
    exchange: Exchange,
    event_tx: mpsc::Sender<Event>,
}

impl ExchangeServer {
    pub fn new() -> (Self, mpsc::Receiver<Event>) {
        let (tx, rx) = mpsc::channel(1000);
        (
            Self {
                exchange: Exchange::new(),
                event_tx: tx,
            },
            rx,
        )
    }

    pub async fn start(&self, addr: &str, mut rx: mpsc::Receiver<Event>) {
        println!("Server started!");
        let listener = TcpListener::bind(addr).await.unwrap();

        let (stream, _) = listener.accept().await.unwrap();

        self.handle_connection(stream, &mut rx).await;
    }

    async fn handle_connection(&self, stream: TcpStream, rx: &mut mpsc::Receiver<Event>) {
        let ws_stream = accept_async(stream).await.unwrap();
        let (mut write, _) = ws_stream.split();

        

        while let Some(event) = rx.recv().await {
            let msg = serde_json::to_string(&event).unwrap();

            if write.send(Message::Text(msg.into())).await.is_err() {
                break;
            }
        }
    }

    pub fn process(& mut self, cmd: exchange::Command) -> Vec<Event> {
        self.exchange.process(cmd)
    }

    pub async fn publish_event(&self, event: Event) {
        let _ = self.event_tx.send(event).await;
    }

    pub fn get_snapshot(&self) -> OrderbookLevelInfos {
        self.exchange.get_snapshot()
    }
}

