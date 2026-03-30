use orderbook::*;

type OrderId = u32;
enum Event {
    OrderAdded(OrderId),
    Trade {
        price: u32,
        quantity: u32 
    },
    OrderRemoved(OrderId)
}

enum Command{
    NewOrder(Order),
    Cancel(OrderId)
}

struct Exchange {
    orderbook: Orderbook
}

impl Exchange {
    impl Exchange {
    pub fn process(&mut self, cmd: Command) -> Vec<Event> {
        match cmd {
            Command::NewOrder(order) => {
                todo!();
                // return events
            }
            Command::Cancel(id) => {
                todo!();
                // ...
            }
        }
    }
}
}