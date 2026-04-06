use tokio::sync::mpsc;

use exchange::*;
use tokio::{
    net::{TcpListener, TcpStream},
};
use tokio_tungstenite::accept_async;
use tungstenite::protocol::Message;
use futures_util::{StreamExt, SinkExt};


pub struct ExchangeServer {
    pub event_tx: mpsc::Sender<Event>,
}

impl ExchangeServer {
    pub fn new() -> (Self, mpsc::Receiver<Event>) {
        let (tx, rx) = mpsc::channel(1000);
        (
            Self {
                event_tx: tx,
            },
            rx,
        )
    }

    pub async fn start(&self, addr: &str, mut rx: mpsc::Receiver<Event>) {
        println!("Server started!");
        let listener = TcpListener::bind(addr).await.unwrap();

        let (stream, _) = listener.accept().await.unwrap();

        self.handle_connection(stream, rx).await;
    }

    async fn handle_connection(&self, stream: TcpStream, mut rx: mpsc::Receiver<Event>) {
        let ws_stream = accept_async(stream).await.unwrap();
        let (mut write, mut read) = ws_stream.split();

        tokio::spawn(async move {
            while let Some(msg) = read.next().await {
                if msg.is_err() {
                    break;
                }
            }
        });

        while let Some(event) = rx.recv().await {
            let msg = serde_json::to_string(&event).unwrap();

            if write.send(Message::Text(msg.into())).await.is_err() {
                break;
            }
        }
    }


}

