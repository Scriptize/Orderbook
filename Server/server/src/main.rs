use exchange::NewOrderRequest;
use exchange::Command;
use server::ExchangeServer;
use orderbook::OrderType;
use orderbook::Side;


#[tokio::main]
async fn main() {
    let localhost = "127.0.0.1:8080";
    let (mut exchange, rx) = ExchangeServer::new();


    exchange.start(localhost, rx).await;
    

    for _ in 0..100 {
        let oreq = NewOrderRequest::new(
            OrderType::GoodTillCancel,
            Side::Buy,
            100,
            100).unwrap();

        let ocmd = Command::NewOrder(oreq); 

        let events = exchange.process(ocmd);

        for event in events {
            exchange.publish_event(event).await;
        }

        let snapshot = exchange.get_snapshot();
        exchange.publish_event(Event::Snapshot(snapshot)).await;
        tokio::time::sleep(tokio::time::Duration::from_millis(500)).await;
        
    }

    
}
