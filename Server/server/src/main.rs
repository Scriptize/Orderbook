// src/main.rs

use std::sync::{Arc, Mutex};

use exchange::{Command, Exchange, NewOrderRequest};
use orderbook::{OrderType, Side};
use server::ExchangeServer;
use rand::{Rng, SeedableRng};
use rand::rngs::SmallRng;
use actors::{Actor, MarketMaker, NoiseTrader, Taker, InformedTrader, LiquiditySweeper};
use tokio::time::{Duration, Interval, interval};

#[tokio::main]
async fn main() {

    let mut actors: Vec<Box<dyn Actor>> = vec![
    // Box::new(MarketMaker::new(1)),
    // Box::new(MarketMaker::new(2)),
    // Box::new(MarketMaker::new(3)),
    // Box::new(MarketMaker::new(4)),
    // Box::new(MarketMaker::new(5)),
    // Box::new(NoiseTrader::new(6)),
    // Box::new(Taker::new(7)),
    // Box::new(InformedTrader::new(8)),
    // Box::new(LiquiditySweeper::new(9)),
    // Box::new(NoiseTrader::new(10)),
    // Box::new(NoiseTrader::new(11)),
    // Box::new(NoiseTrader::new(12)),
    // Box::new(NoiseTrader::new(13)),
    // Box::new(NoiseTrader::new(14)),
    // Box::new(NoiseTrader::new(15)),
    // Box::new(NoiseTrader::new(16)),
    // Box::new(NoiseTrader::new(17)),
    // Box::new(NoiseTrader::new(18)),
    // Box::new(NoiseTrader::new(19)),
    ];
    
    let localhost = "127.0.0.1:9001";

    let exchange = Arc::new(Mutex::new(Exchange::new(Duration::from_secs(60))));

    let (tx, _) = tokio::sync::broadcast::channel::<exchange::Event>(10_000);

    let server = ExchangeServer::new(tx.clone());
    let server_count = server.client_count.clone();

    tokio::spawn(async move {
        server.run(localhost).await;
    });

    while server_count.load(std::sync::atomic::Ordering::SeqCst) == 0 {
        tokio::time::sleep(tokio::time::Duration::from_millis(100)).await;
    }

    //exchange task
    tokio::spawn({
        let pruning_exchange = exchange.clone();
        let tx = tx.clone();
        let day_length = Duration::from_secs(60);

        async move {
            let mut ticker = interval(day_length);


            loop {
                ticker.tick().await;
                let events = {
                    let mut ex = pruning_exchange.lock().unwrap();
                    println!("Pruning!!!");
                    ex.prune_expired_orders().unwrap()
                };
                

                for event in events {
                    let _ = tx.send(event);
                }
            }
        }

        
    });
    
    
    tokio::spawn({
        let exchange = exchange.clone();
        let tx = tx.clone();

        {
            let mut ex = exchange.lock().unwrap();

            ex.process(Command::NewOrder(
                NewOrderRequest::new(0, OrderType::GoodForDay, Side::Buy, 999, 100).unwrap()
            )).unwrap();

            ex.process(Command::NewOrder(
                NewOrderRequest::new(0, OrderType::GoodForDay, Side::Buy, 1001, 100).unwrap()
            )).unwrap();
        }

        async move {
            loop {
                // 1. snapshot
                let snapshot = {
                    let mut ex = exchange.lock().unwrap();
                    ex.get_snapshot()
                };

                // 2. collect cmds
                let mut all_cmds = Vec::new();
                for actor in actors.iter_mut() {
                    //randomisj
                    if rand::thread_rng().gen_bool(0.7) {
                        all_cmds.extend(actor.step(snapshot.clone()));
                    }
                    // let cmds = actor.step(snapshot.clone());
                    // all_cmds.extend(cmds);
                }

                // 3. process commands through exchange
                let mut all_events = Vec::new();
                {
                    let mut ex = exchange.lock().unwrap();
                    for cmd in all_cmds {
                        let events = ex.process(cmd).unwrap();
                        all_events.extend(events);
                    }
                }

                // 4. feed events back to actors
                for event in &all_events {
                    for actor in actors.iter_mut() {
                        actor.on_event(event);
                    }
                }

                // 5. send to websocket clients
                for event in all_events {
                    let _ = tx.send(event);
                }

                // pacing
                tokio::time::sleep(tokio::time::Duration::from_millis(350)).await;
            }
        }
    });

    println!("seeded orderbook, server still running");
    tokio::signal::ctrl_c().await.unwrap();
}