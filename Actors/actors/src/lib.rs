use exchange::{CancelOrderRequest, Command, Event, NewOrderRequest};
use orderbook::*;
use rand::Rng;

type OrderId = u32;
type ActorId = u32;

pub trait Actor: Send {
    fn step(&mut self, book: OrderbookLevelInfos) -> Vec<Command>;
    fn on_event(&mut self, event: &Event);
}

/* ========================= MARKET MAKER ========================= */

pub struct MarketMaker {
    actor_id: ActorId,
    bid_order_id: Option<OrderId>,
    ask_order_id: Option<OrderId>,
    inventory: i32,
    spread: i32,
}

impl Actor for MarketMaker {
    fn step(&mut self, book: OrderbookLevelInfos) -> Vec<Command> {
        let bids = book.get_bids();
        let asks = book.get_asks();

        let best_bid = bids.iter().map(|b| b.price).max();
        let best_ask = asks.iter().map(|a| a.price).min();

        let mid = match (best_bid, best_ask) {
            (Some(b), Some(a)) => b + (a - b) / 2,
            _ => 1000,
        };

        let skew = -self.inventory / 10;
        let spread = self.spread.max(1);

        let mut bid_price = mid - spread + skew;
        let mut ask_price = mid + spread + skew;

        if bid_price >= ask_price {
            bid_price = mid - 1;
            ask_price = mid + 1;
        }

        let size = (150 - self.inventory.abs()).max(50) as u32;

        let mut cmds = Vec::new();

        // cancel old
        if let Some(id) = self.bid_order_id {
            cmds.push(Command::Cancel(CancelOrderRequest {
                actor_id: self.actor_id,
                order_id: id,
            }));
        }

        if let Some(id) = self.ask_order_id {
            cmds.push(Command::Cancel(CancelOrderRequest {
                actor_id: self.actor_id,
                order_id: id,
            }));
        }

        // new bid
        cmds.push(Command::NewOrder(
            NewOrderRequest::new(
                self.actor_id,
                OrderType::GoodTillCancel,
                Side::Buy,
                bid_price.max(1),
                size,
            )
            .unwrap(),
        ));

        // new ask
        cmds.push(Command::NewOrder(
            NewOrderRequest::new(
                self.actor_id,
                OrderType::GoodTillCancel,
                Side::Sell,
                ask_price.max(1),
                size,
            )
            .unwrap(),
        ));

        cmds
    }

    fn on_event(&mut self, event: &Event) {
        match event {
            Event::OrderAccepted(order_id, actor_id) if *actor_id == self.actor_id => {
                if self.bid_order_id.is_none() {
                    self.bid_order_id = Some(*order_id);
                } else {
                    self.ask_order_id = Some(*order_id);
                }
            }

            Event::OrderRemoved(order_id, actor_id) if *actor_id == self.actor_id => {
                if Some(*order_id) == self.bid_order_id {
                    self.bid_order_id = None;
                }
                if Some(*order_id) == self.ask_order_id {
                    self.ask_order_id = None;
                }
            }

            Event::TradeExecuted {
                bidder_id,
                asker_id,
                quantity,
                ..
            } => {
                if *bidder_id == self.actor_id {
                    self.inventory += *quantity as i32;
                }
                if *asker_id == self.actor_id {
                    self.inventory -= *quantity as i32;
                }
            }

            _ => {}
        }
    }
}

/* ========================= NOISE TRADER ========================= */

pub struct NoiseTrader {
    actor_id: ActorId,
}

impl Actor for NoiseTrader {
    fn step(&mut self, book: OrderbookLevelInfos) -> Vec<Command> {
        let mut rng = rand::thread_rng();

        let bids = book.get_bids();
        let asks = book.get_asks();

        if bids.is_empty() || asks.is_empty() {
            return vec![];
        }

        let best_bid = bids.iter().map(|b| b.price).max().unwrap();
        let best_ask = asks.iter().map(|a| a.price).min().unwrap();

        let mid = best_bid + (best_ask - best_bid) / 2;

        let price = (mid + rng.gen_range(-3..=3)).max(1);
        let qty = rng.gen_range(1..50);

        let side = if rng.gen_bool(0.5) {
            Side::Buy
        } else {
            Side::Sell
        };

        vec![Command::NewOrder(
            NewOrderRequest::new(self.actor_id, OrderType::GoodTillCancel, side, price, qty)
                .unwrap(),
        )]
    }

    fn on_event(&mut self, _event: &Event) {}
}

/* ========================= TAKER ========================= */

pub struct Taker {
    actor_id: ActorId,
}

impl Actor for Taker {
    fn step(&mut self, book: OrderbookLevelInfos) -> Vec<Command> {
        let mut rng = rand::thread_rng();

        let bids = book.get_bids();
        let asks = book.get_asks();

        if bids.is_empty() || asks.is_empty() {
            return vec![];
        }

        let best_bid = bids.iter().map(|b| b.price).max().unwrap();
        let best_ask = asks.iter().map(|a| a.price).min().unwrap();

        let qty = rng.gen_range(10..100);

        if rng.gen_bool(0.5) {
            vec![Command::NewOrder(
                NewOrderRequest::new(self.actor_id, OrderType::Market, Side::Buy, best_ask, qty)
                    .unwrap(),
            )]
        } else {
            vec![Command::NewOrder(
                NewOrderRequest::new(self.actor_id, OrderType::Market, Side::Sell, best_bid, qty)
                    .unwrap(),
            )]
        }
    }

    fn on_event(&mut self, _event: &Event) {}
}

/* ========================= INFORMED TRADER ========================= */

pub struct InformedTrader {
    actor_id: ActorId,
    last_mid: Option<i32>,
}

impl Actor for InformedTrader {
    fn step(&mut self, book: OrderbookLevelInfos) -> Vec<Command> {
        let bids = book.get_bids();
        let asks = book.get_asks();

        if bids.is_empty() || asks.is_empty() {
            return vec![];
        }

        let best_bid = bids.iter().map(|b| b.price).max().unwrap();
        let best_ask = asks.iter().map(|a| a.price).min().unwrap();

        let mid = best_bid + (best_ask - best_bid) / 2;

        let mut cmds = Vec::new();

        if let Some(last) = self.last_mid {
            if mid > last {
                cmds.push(Command::NewOrder(
                    NewOrderRequest::new(self.actor_id, OrderType::Market, Side::Buy, best_ask, 50)
                        .unwrap(),
                ));
            } else if mid < last {
                cmds.push(Command::NewOrder(
                    NewOrderRequest::new(
                        self.actor_id,
                        OrderType::Market,
                        Side::Sell,
                        best_bid,
                        50,
                    )
                    .unwrap(),
                ));
            }
        }

        self.last_mid = Some(mid);

        cmds
    }

    fn on_event(&mut self, _event: &Event) {}
}

/* ========================= LIQUIDITY SWEEPER ========================= */

pub struct LiquiditySweeper {
    actor_id: ActorId,
}

impl Actor for LiquiditySweeper {
    fn step(&mut self, book: OrderbookLevelInfos) -> Vec<Command> {
        let mut rng = rand::thread_rng();

        if !rng.gen_bool(0.1) {
            return vec![];
        }

        let bids = book.get_bids();
        let asks = book.get_asks();

        if bids.is_empty() || asks.is_empty() {
            return vec![];
        }

        let best_bid = bids.iter().map(|b| b.price).max().unwrap();
        let best_ask = asks.iter().map(|a| a.price).min().unwrap();

        let qty = rng.gen_range(50..150);

        if rng.gen_bool(0.5) {
            vec![Command::NewOrder(
                NewOrderRequest::new(self.actor_id, OrderType::Market, Side::Buy, best_ask, qty)
                    .unwrap(),
            )]
        } else {
            vec![Command::NewOrder(
                NewOrderRequest::new(self.actor_id, OrderType::Market, Side::Sell, best_bid, qty)
                    .unwrap(),
            )]
        }
    }

    fn on_event(&mut self, _event: &Event) {}
}

/* ========================= CONSTRUCTORS ========================= */

impl MarketMaker {
    pub fn new(actor_id: ActorId) -> Self {
        Self {
            actor_id,
            bid_order_id: None,
            ask_order_id: None,
            inventory: 0,
            spread: 2,
        }
    }
}

impl NoiseTrader {
    pub fn new(actor_id: ActorId) -> Self {
        Self { actor_id }
    }
}
impl Taker {
    pub fn new(actor_id: ActorId) -> Self {
        Self { actor_id }
    }
}
impl InformedTrader {
    pub fn new(actor_id: ActorId) -> Self {
        Self {
            actor_id,
            last_mid: None,
        }
    }
}
impl LiquiditySweeper {
    pub fn new(actor_id: ActorId) -> Self {
        Self { actor_id }
    }
}
