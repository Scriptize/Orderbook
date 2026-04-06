use exchange::NewOrderRequest;
use exchange::Command;
use server::ExchangeServer;
use orderbook::OrderType;
use exchange::Exchange;
use orderbook::Side;
use exchange::Event;


#[tokio::main]
async fn main() {
    let localhost = "127.0.0.1:9001";

    let (server, rx) = ExchangeServer::new();

    // server runs independently (NO LOCK)
    let tx = server.event_tx.clone();

    tokio::spawn(async move {
        server.start(localhost, rx).await;
    });

    let mut exchange = Exchange::new();
    tokio::time::sleep(tokio::time::Duration::from_secs(5)).await;
    for i in 0..100 {
        let order_type = if i % 3 == 0 {
            OrderType::GoodTillCancel
        } else {
            OrderType::FillOrKill
        };

        let side = if i % 2 == 0 { Side::Buy } else { Side::Sell };

        match i % 4 {
            0 => {
                // New order
                let oreq = NewOrderRequest::new(order_type, side, 100, 100 + i as u32)
                    .unwrap();
                let ocmd = Command::NewOrder(oreq);
                let events = exchange.process(ocmd);
                for event in events {
                    let _ = tx.send(event).await;
                }
            }
            1 => {
                // Cancel order
                let ocmd = Command::Cancel(i as u32);
                let events = exchange.process(ocmd);
                for event in events {
                    let _ = tx.send(event).await;
                }
            }
            _ => {
                // New order with varied parameters
                let oreq = NewOrderRequest::new(order_type, side, 50 + i as i32, 100)
                    .unwrap();
                let ocmd = Command::NewOrder(oreq);
                let events = exchange.process(ocmd);
                for event in events {
                    let _ = tx.send(event).await;
                }
            }
        }

        let snapshot = exchange.get_snapshot();
        let _ = tx.send(Event::Snapshot(snapshot)).await;

        tokio::time::sleep(tokio::time::Duration::from_millis(500)).await;
    }
}
