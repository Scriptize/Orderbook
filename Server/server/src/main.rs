// src/main.rs

use std::sync::{Arc, Mutex};

use exchange::{Command, Exchange, NewOrderRequest};
use orderbook::{OrderType, Side};
use server::ExchangeServer;
use rand::{Rng, SeedableRng};
use rand::rngs::SmallRng;

#[tokio::main]
async fn main() {
    let localhost = "127.0.0.1:9001";

    let exchange = Arc::new(Mutex::new(Exchange::new()));
    let server = ExchangeServer::new(exchange.clone());

    let tx = server.event_tx.clone();
    let server_count = server.client_count.clone();

    tokio::spawn(async move {
        server.start(localhost).await;
    });

    while server_count.load(std::sync::atomic::Ordering::SeqCst) == 0 {
        tokio::time::sleep(tokio::time::Duration::from_millis(100)).await;
    }
    
    
    tokio::spawn({
        let exchange = exchange.clone();
        let tx = tx.clone();

        async move {
            use rand::Rng;

            let mut rng = SmallRng::from_entropy();;
            let mid = 1000;
            let levels = 50;

            for _ in 0..1000 {
                let is_bid = rng.gen_bool(0.5);

                let level = rng.gen_range(0..levels);
                let quantity = ((level + 1) * 10) as u32;

                let price = if is_bid {
                    mid - level as i32
                } else {
                    mid + 1 + level as i32
                };

                let side = if is_bid { Side::Buy } else { Side::Sell };

                let events = {
                    let mut ex = exchange.lock().unwrap();
                    let req = NewOrderRequest::new(
                        OrderType::GoodTillCancel,
                        side,
                        price,
                        quantity,
                    ).unwrap();

                    ex.process(Command::NewOrder(req))
                };

                for event in events {
                    let _ = tx.send(event);
                }

                let delay = rng.gen_range(5..30);
                tokio::time::sleep(tokio::time::Duration::from_millis(delay)).await;
            }
        }
    });

    println!("seeded orderbook, server still running");
    tokio::signal::ctrl_c().await.unwrap();
}