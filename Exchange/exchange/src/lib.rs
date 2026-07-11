use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::fmt;
use std::time::{Duration, Instant};

use orderbook::*;

type OrderId = u32;
type Price = i32;
type Quantity = u32;
type ActorId = u32;

#[derive(Serialize, Clone, Debug)]
pub enum Event {
    OrderAccepted(OrderId, ActorId),
    OrderRejected(RequestError, ActorId),
    OrderModified(OrderId, OrderId, ActorId),
    Snapshot(OrderbookLevelInfos),
    CancellationFailure(OrderId, RequestError),
    TradeExecuted {
        taker_id: OrderId,
        maker_id: OrderId,
        aggresor_side: Side,
        price: Price,
        quantity: Quantity,
        bidder_id: ActorId,
        asker_id: ActorId,
    },
    OrderRemoved(OrderId, ActorId),
}

impl PartialEq for Event {
    fn eq(&self, other: &Self) -> bool {
        match (self, other) {
            (Event::OrderRemoved(a_id, a_actor), Event::OrderRemoved(b_id, b_actor)) => {
                a_id == b_id && a_actor == b_actor
            }
            _ => false,
        }
    }
}

#[derive(Serialize, Clone, Copy, Debug)]
pub enum RequestError {
    InvalidQuantity,
    InvalidPrice,
    InvalidOrder,
}

impl fmt::Display for RequestError {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        match self {
            RequestError::InvalidQuantity => write!(f, "Quantity must be > 0"),
            RequestError::InvalidPrice => write!(f, "Price must be > 0 for limit orders"),
            RequestError::InvalidOrder => write!(f, "Order does not exist in book"),
        }
    }
}

impl std::error::Error for RequestError {}

pub struct NewOrderRequest {
    actor_id: ActorId,
    order_type: OrderType,
    side: Side,
    price: Price,
    quantity: Quantity,
}

impl NewOrderRequest {
    pub fn new(
        actor_id: ActorId,
        order_type: OrderType,
        side: Side,
        price: Price,
        quantity: Quantity,
    ) -> Result<Self, RequestError> {
        if quantity == 0 {
            return Err(RequestError::InvalidQuantity);
        }

        if order_type != OrderType::Market && price <= 0 {
            return Err(RequestError::InvalidPrice);
        }

        Ok(Self {
            actor_id,
            order_type,
            side,
            price,
            quantity,
        })
    }
}

pub struct ModifyOrderRequest {
    id: OrderId,
    actor_id: ActorId,
    order_type: OrderType,
    side: Side,
    price: Price,
    quantity: Quantity,
}

pub struct CancelOrderRequest {
    pub actor_id: ActorId,
    pub order_id: OrderId,
}

pub enum Command {
    NewOrder(NewOrderRequest),
    Cancel(CancelOrderRequest),
    Modify(ModifyOrderRequest),
}

pub struct Exchange {
    orderbook: Orderbook,
    next_order_id: OrderId,
    expiry_map: BTreeMap<Instant, Vec<(ActorId, OrderId)>>,
    day_length: Duration,
    exchange_start: Instant,
}

impl Exchange {
    pub fn new(day_length: Duration) -> Self {
        Self {
            orderbook: Orderbook::new(),
            next_order_id: 1,
            expiry_map: BTreeMap::new(),
            day_length: day_length,
            exchange_start: Instant::now(),
        }
    }

    fn handle_new_order(&mut self, request: NewOrderRequest) -> Result<Vec<Event>, OrderbookError> {
        let mut events = Vec::new();

        let id = self.next_order_id;
        self.next_order_id += 1;

        let new_order = Order::new(
            request.actor_id,
            request.order_type,
            id,
            request.side,
            request.price,
            request.quantity,
        );

        if new_order.is_ok() && request.order_type == OrderType::GoodForDay {
            let now = Instant::now();
            // grouping expiry within 10ms
            let bucket = Duration::from_millis(10);
            let expiry = now + self.day_length;
            let since_start = expiry.duration_since(self.exchange_start);

            // convert to nanos cause Duration doesn't support modulo
            let bucket_ns = bucket.as_nanos();
            let since_ns = since_start.as_nanos();
            let bucketed_ns = since_ns - (since_ns % bucket_ns);

            let bucketed = Duration::from_nanos(bucketed_ns as u64);
            let bucketed_expiry = self.exchange_start + bucketed;

            self.expiry_map
                .entry(bucketed_expiry)
                .or_default()
                .push((request.actor_id, id));
        }

        events.push(Event::OrderAccepted(id, request.actor_id));

        let matching_result = self.orderbook.add_order(new_order?)?;

        let trades = matching_result.get_trades();
        let filled_orders = matching_result.get_filled_orders();

        for trade in trades {
            let ask_id = trade.get_ask_trade().order_id;
            let bid_id = trade.get_bid_trade().order_id;
            let trade_price = trade.get_trade_price();
            let trade_quantity = trade.get_trade_quantity();
            let bidder_id = trade.get_bidder_id();
            let asker_id = trade.get_asker_id();

            events.push(Event::TradeExecuted {
                maker_id: if bid_id == id { ask_id } else { bid_id },
                taker_id: id,
                aggresor_side: if id == bid_id { Side::Buy } else { Side::Sell },
                price: trade_price,
                quantity: trade_quantity,
                bidder_id,
                asker_id,
            });
        }

        for (filled_id, actor_id) in filled_orders {
            events.push(Event::OrderRemoved(*filled_id, *actor_id));
        }

        Ok(events)
    }

    fn handle_cancel_order(
        &mut self,
        request: CancelOrderRequest,
    ) -> Result<Vec<Event>, OrderbookError> {
        let mut events = Vec::new();
        let actor_id = request.actor_id;
        let order_id = request.order_id;

        if self.orderbook.cancel_order(order_id).is_ok() {
            events.push(Event::OrderRemoved(order_id, actor_id));
        } else {
            events.push(Event::CancellationFailure(
                order_id,
                RequestError::InvalidOrder,
            ));
        }

        Ok(events)
    }

    fn handle_modify_order(
        &mut self,
        request: ModifyOrderRequest,
    ) -> Result<Vec<Event>, OrderbookError> {
        let mut events = Vec::new();

        match NewOrderRequest::new(
            request.actor_id,
            request.order_type,
            request.side,
            request.price,
            request.quantity,
        ) {
            Ok(req) => {
                events.extend(self.handle_cancel_order(CancelOrderRequest {
                    actor_id: request.actor_id,
                    order_id: request.id,
                })?);

                if let Some(Event::CancellationFailure(_, RequestError::InvalidOrder)) =
                    events.last()
                {
                    return Ok(events);
                }

                events.extend(self.handle_new_order(req)?);
                events.push(Event::OrderModified(
                    request.id,
                    self.next_order_id - 1,
                    request.actor_id,
                ));
            }
            Err(e) => {
                events.push(Event::OrderRejected(e, request.actor_id));
            }
        }

        Ok(events)
    }

    pub fn process(&mut self, cmd: Command) -> Result<Vec<Event>, OrderbookError> {
        match cmd {
            Command::NewOrder(request) => self.handle_new_order(request),
            Command::Cancel(id) => self.handle_cancel_order(id),
            Command::Modify(request) => self.handle_modify_order(request),
        }
    }

    pub fn get_snapshot(&mut self) -> OrderbookLevelInfos {
        self.orderbook.get_order_infos()
    }

    pub fn prune_expired_orders(&mut self) -> Result<Vec<Event>, OrderbookError> {
        // get current time,
        // while top of map is a key representing a time in the past
        // loop thru ids and send cancel requests

        let mut events = Vec::new();

        while let Some(entry) = self.expiry_map.first_entry() {
            let expire_time = *entry.key();
            let now = Instant::now();

            if expire_time > now {
                break;
            }

            let (_expire_time, order_info) = entry.remove_entry();

            for (actor_id, order_id) in order_info {
                events.extend(self.handle_cancel_order(CancelOrderRequest { actor_id, order_id })?)
            }
        }
        Ok(events)
    }
}
#[cfg(test)]
mod tests {
    use super::*;

    fn create_exchange() -> Exchange {
        Exchange::new(Duration::from_mins(5)) // Prod day length will be around 5
    }

    #[test]
    fn test_new_order_increments_order_id() -> Result<(), OrderbookError> {
        let mut exchange = create_exchange();
        let request =
            NewOrderRequest::new(1, OrderType::GoodTillCancel, Side::Buy, 100, 10).unwrap();
        let events = exchange.handle_new_order(request)?;
        assert_eq!(exchange.next_order_id, 2);
        assert!(matches!(events[0], Event::OrderAccepted(1, _)));
        Ok(())
    }

    #[test]
    fn test_cancel_existing_order() -> Result<(), OrderbookError> {
        let mut exchange = create_exchange();
        let request =
            NewOrderRequest::new(1, OrderType::GoodTillCancel, Side::Buy, 100, 10).unwrap();
        exchange.handle_new_order(request)?;

        let events = exchange.handle_cancel_order(CancelOrderRequest {
            actor_id: 1,
            order_id: 1,
        })?;
        assert!(matches!(events[0], Event::OrderRemoved(1, _)));
        Ok(())
    }

    #[test]
    fn test_cancel_nonexistent_order() -> Result<(), OrderbookError> {
        let mut exchange = create_exchange();
        let events = exchange.handle_cancel_order(CancelOrderRequest {
            actor_id: 1,
            order_id: 999,
        })?;
        assert!(matches!(
            events[0],
            Event::CancellationFailure(999, RequestError::InvalidOrder)
        ));
        Ok(())
    }

    #[test]
    fn test_process_new_order_command() -> Result<(), OrderbookError> {
        let mut exchange = create_exchange();
        let request =
            NewOrderRequest::new(1, OrderType::GoodTillCancel, Side::Sell, 100, 5).unwrap();
        let cmd = Command::NewOrder(request);
        let events = exchange.process(cmd)?;
        assert!(matches!(events[0], Event::OrderAccepted(1, _)));
        Ok(())
    }

    #[test]
    fn test_process_cancel_command() -> Result<(), OrderbookError> {
        let mut exchange = create_exchange();
        let request =
            NewOrderRequest::new(1, OrderType::GoodTillCancel, Side::Buy, 100, 10).unwrap();
        exchange.handle_new_order(request)?;

        let cmd = Command::Cancel(CancelOrderRequest {
            actor_id: 1,
            order_id: 1,
        });
        let events = exchange.process(cmd)?;
        assert!(matches!(events[0], Event::OrderRemoved(1, _)));
        Ok(())
    }

    #[test]
    fn test_modify_order_cancels_and_creates_new() -> Result<(), OrderbookError> {
        let mut exchange = create_exchange();
        let orig_request =
            NewOrderRequest::new(1, OrderType::GoodTillCancel, Side::Buy, 100, 10).unwrap();
        exchange.handle_new_order(orig_request)?;

        let modify_req = ModifyOrderRequest {
            id: 1,
            actor_id: 1,
            order_type: OrderType::GoodTillCancel,
            side: Side::Sell,
            price: 150,
            quantity: 5,
        };
        let events = exchange.handle_modify_order(modify_req)?;
        assert!(matches!(events[0], Event::OrderRemoved(1, _)));
        assert!(matches!(events[1], Event::OrderAccepted(2, _)));
        Ok(())
    }

    #[test]
    fn test_modify_order_with_invalid_request() -> Result<(), OrderbookError> {
        let mut exchange = create_exchange();
        let orig_request =
            NewOrderRequest::new(1, OrderType::GoodTillCancel, Side::Buy, 100, 10).unwrap();
        exchange.handle_new_order(orig_request)?;

        let modify_req = ModifyOrderRequest {
            id: 1,
            actor_id: 1,
            order_type: OrderType::GoodTillCancel,
            side: Side::Buy,
            price: 0,
            quantity: 10,
        };
        let events = exchange.handle_modify_order(modify_req)?;
        assert!(matches!(
            events[0],
            Event::OrderRejected(RequestError::InvalidPrice, _)
        ));
        Ok(())
    }

    #[test]
    fn test_eod_pruning() -> Result<(), OrderbookError> {
        let mut exchange = Exchange::new(Duration::from_nanos(1));

        for _ in 0..5 {
            let request =
                NewOrderRequest::new(1, OrderType::GoodForDay, Side::Buy, 100, 10).unwrap();
            let command = Command::NewOrder(request);
            let _ = exchange.process(command)?;
        }

        let prune_events = exchange.prune_expired_orders()?;

        assert_eq!(prune_events[0], Event::OrderRemoved(1, 1));
        assert_eq!(prune_events[1], Event::OrderRemoved(2, 1));
        assert_eq!(prune_events[2], Event::OrderRemoved(3, 1));
        assert_eq!(prune_events[3], Event::OrderRemoved(4, 1));
        assert_eq!(prune_events[4], Event::OrderRemoved(5, 1));
        assert_eq!(exchange.orderbook.size(), 0);

        Ok(())
    }
}
