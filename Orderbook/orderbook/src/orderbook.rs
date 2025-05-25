use std::{
    rc::Rc,
    cell::RefCell,
    collections::{BTreeMap, HashMap}
};

#[derive(Clone, Copy, PartialEq, Debug)]
pub enum OrderType {
    GoodTillCancel,
    FillAndKill,
}

#[derive(Clone, Copy, PartialEq, Debug)]
pub enum Side {
    Buy,
    Sell,
}

type Price = i32;
type Quantity = u32;
type OrderId = u32;

pub struct LevelInfo {
    pub price: Price,
    pub quantity: Quantity,
}

type LevelInfos = Vec<LevelInfo>;

pub struct OrderbookLevelInfos {
    bid_infos: LevelInfos,
    ask_infos: LevelInfos,
}

impl OrderbookLevelInfos {
    pub fn new(bids: LevelInfos, asks: LevelInfos) -> Self {
        Self { bid_infos: bids, ask_infos: asks }
    }
    pub const fn get_bids(&self) -> &LevelInfos {
        &self.bid_infos
    }
    pub const fn get_asks(&self) -> &LevelInfos {
        &self.ask_infos
    }
}

pub struct Order {
    order_type: OrderType,
    order_id: OrderId,
    side: Side,
    price: Price,
    initial_quantity: Quantity,
    remaining_quantity: Quantity,
    filled_quantity: Quantity,
    filled: bool,
}

impl Order {
    pub fn new(
        order_type: OrderType,
        order_id: OrderId,
        side: Side,
        price: Price,
        quantity: Quantity,
    ) -> Self {
        Self {
            order_type,
            order_id,
            side,
            price,
            initial_quantity: quantity,
            remaining_quantity: quantity,
            filled_quantity: 0,
            filled: false,
        }
    }

    pub const fn get_order_id(&self) -> OrderId {
        self.order_id
    }
    pub const fn get_side(&self) -> Side {
        self.side
    }
    pub const fn get_price(&self) -> Price {
        self.price
    }
    pub const fn get_order_type(&self) -> OrderType {
        self.order_type
    }
    pub const fn get_initial_quantity(&self) -> Quantity {
        self.initial_quantity
    }
    pub const fn get_remaining_quantity(&self) -> Quantity {
        self.remaining_quantity
    }
    pub const fn get_filled_quantity(&self) -> Quantity {
        self.filled_quantity
    }
    pub const fn is_filled(&self) -> bool {
        self.filled
    }

    pub fn fill(&mut self, quantity: Quantity) -> Result<(), String> {
        if quantity <= self.remaining_quantity {
            self.remaining_quantity -= quantity;
            self.filled_quantity += quantity;
            if self.remaining_quantity == 0 {
                self.filled = true;
            }
            Ok(())
        } else {
            Err("Order cannot be filled for more than it's remaining quantity.".to_string())
        }
    }
}

type OrderPointer = Rc<RefCell<Order>>;
type OrderPointers = Vec<OrderPointer>;

pub struct OrderModify {
    order_id: OrderId,
    price: Price,
    side: Side,
    quantity: Quantity,
}

impl OrderModify {
    pub fn new(order_id: OrderId, side: Side, price: Price, quantity: Quantity) -> Self {
        Self {
            order_id,
            side,
            price,
            quantity,
        }
    }

    pub const fn get_order_id(&self) -> OrderId {
        self.order_id
    }
    pub const fn get_side(&self) -> Side {
        self.side
    }
    pub const fn get_price(&self) -> Price {
        self.price
    }
    pub const fn get_quantity(&self) -> Quantity {
        self.quantity
    }

    pub fn to_order_pointer(&self, order_type: OrderType) -> OrderPointer {
        Rc::new(RefCell::new(Order::new(
            order_type,
            self.get_order_id(),
            self.get_side(),
            self.get_price(),
            self.get_quantity(),
        )))
    }
}

#[derive(Clone, Copy)]
pub struct TradeInfo {
    pub order_id: OrderId,
    pub price: Price,
    pub quantity: Quantity,
}

pub struct Trade{
    bid_trade: TradeInfo,
    ask_trade: TradeInfo,
}

impl Trade{
    pub fn new(bid_trade: TradeInfo, ask_trade: TradeInfo) -> Self{
        Self{
            bid_trade,
            ask_trade,
        }
    }

    pub const fn get_bid_trade(&self) -> TradeInfo {
        self.bid_trade
    }

    pub const fn get_ask_trade(&self) -> TradeInfo {
        self.ask_trade
    }
}

type Trades = Vec<Trade>;

///////////////////////////////////////

struct OrderEntry{
    order: OrderPointer,
    location: usize,
}

pub struct Orderbook{
    bids: BTreeMap<Price, OrderPointers>,
    asks: BTreeMap<Price, OrderPointers>,
    orders: HashMap<OrderId, OrderEntry>,
}

impl Orderbook{
    pub fn new(
        bids: BTreeMap<Price,OrderPointers>, 
        asks: BTreeMap<Price, OrderPointers>) -> Self
        {
            Self{
                bids,
                asks,
                orders: HashMap::new(),
            }
        }

    pub fn size(&self) -> usize {
        self.orders.len()
    }

    pub fn get_order_infos(&self) -> OrderbookLevelInfos{
        let mut bid_infos: LevelInfos = Vec::with_capacity(self.orders.len());
        let mut ask_infos: LevelInfos = Vec::with_capacity(self.orders.len());

        let create_level_infos = |price: Price , orders: OrderPointers|{
            let total_quantity = orders.iter().fold(0, |running_sum, order|{
                running_sum + order.borrow().get_remaining_quantity()
            });

            LevelInfo{
                price,
                quantity: total_quantity,
            }
        };

        for (price, orders) in &self.bids{
            bid_infos.push(create_level_infos(*price, orders.clone()));
        }

        for (price, orders) in &self.asks{
            ask_infos.push(create_level_infos(*price, orders.clone()));
        }

        OrderbookLevelInfos{
            bid_infos,
            ask_infos,
        }
    }

    pub fn add_order(&mut self, order: OrderPointer) -> Trades{
        //check if order exist
        if self.orders.contains_key(&order.borrow().get_order_id()){
            return vec![];
        }
        //check if order is a FillAndKill that can't match
        if order.borrow().get_order_type() == OrderType::FillAndKill && !self.can_match(order.borrow().get_side(), order.borrow().get_price()){
            return vec![];
        }

        let mut index: usize = 0;

        if order.borrow().get_side() == Side::Buy{
            let orders = &mut self.bids.entry(order.borrow().get_price()).or_default();
            orders.push(order.clone());
            index = orders.len() - 1;
        } else {
            let orders = &mut self.asks.entry(order.borrow().get_price()).or_default();
            orders.push(order.clone());
            index = orders.len() - 1;
        }

        let order_id = order.borrow().get_order_id();

        self.orders.insert(order_id, OrderEntry { order, location: index });
        self.match_orders()
    }

    pub fn cancel_order(&mut self, order_id : OrderId){
        if !self.orders.contains_key(&order_id){
            return;
        }

        let entry = &self.orders[&order_id];
        let order = entry.order.clone();
        let index = entry.location;

        self.orders.remove(&order_id);

        if order.borrow().get_side() == Side::Sell{
            let price = order.borrow().get_price();
            let orders = self.asks.get_mut(&price).unwrap();
            orders.remove(index);
            if orders.is_empty(){
                self.asks.remove(&price);
            }
        } else {
            let price = order.borrow().get_price();
            let orders = self.bids.get_mut(&price).unwrap();
            orders.remove(index);
            if orders.is_empty(){
                self.bids.remove(&price);
            }
        }
    }

    pub fn modify_order(&mut self, order: OrderModify) -> Trades{
        if !self.orders.contains_key(&order.get_order_id()){
            return vec![];
        }

        let order_type = if let Some(entry) = self.orders.get(&order.get_order_id()) {
            entry.order.borrow().get_order_type()
        } else {
            return vec![];
        };
        self.cancel_order(order.get_order_id());

        self.add_order(order.to_order_pointer(order_type))
    }

    fn can_match(&self, side: Side, price: Price) -> bool{
        match side{
            Side::Buy => {
                if self.asks.is_empty(){
                    return false;
                }
                let (best_ask, _) = self.asks.iter().next().unwrap();
                return price >= *best_ask;
            }

            Side::Sell => {
                if self.bids.is_empty(){
                    return false;
                }
                let (best_bid, _) = self.bids.iter().next().unwrap();
                return price <= *best_bid;
            }
        }
    }

    fn match_orders(&mut self) -> Trades {
        let mut trades: Trades = Vec::with_capacity(self.orders.len());

        loop {
            if self.bids.is_empty() || self.asks.is_empty() {
                break;
            }

            // Get best bid and ask (highest bid, lowest ask)
            let (bid_price, bids) = match self.bids.iter_mut().next_back() {
                Some((p, b)) => (*p, b),
                None => break,
            };
            let (ask_price, asks) = match self.asks.iter_mut().next() {
                Some((p, a)) => (*p, a),
                None => break,
            };

            if bid_price < ask_price {
                break;
            }

            // Always match the first order at each price level
            let bid_order_ptr = &bids[0];
            let ask_order_ptr = &asks[0];

            let mut bid = bid_order_ptr.borrow_mut();
            let mut ask = ask_order_ptr.borrow_mut();

            let trade_quantity = bid.get_remaining_quantity().min(ask.get_remaining_quantity());

            // Fill both orders
            bid.fill(trade_quantity).ok();
            ask.fill(trade_quantity).ok();

            // Prepare trade info
            trades.push(Trade::new(
                TradeInfo {
                    order_id: bid.get_order_id(),
                    price: bid.get_price(),
                    quantity: trade_quantity,
                },
                TradeInfo {
                    order_id: ask.get_order_id(),
                    price: ask.get_price(),
                    quantity: trade_quantity,
                },
            ));

            // Remove filled orders from book and orders map
            let mut remove_bid = false;
            let mut remove_ask = false;
            if bid.is_filled() {
                let bid_id = bid.get_order_id();
                remove_bid = true;
                self.orders.remove(&bid_id);
            }
            if ask.is_filled() {
                let ask_id = ask.get_order_id();
                remove_ask = true;
                self.orders.remove(&ask_id);
            }
            drop(bid);
            drop(ask);

            if remove_bid {
                bids.remove(0);
                if bids.is_empty() {
                    self.bids.remove(&bid_price);
                }
            }
            if remove_ask {
                asks.remove(0);
                if asks.is_empty() {
                    self.asks.remove(&ask_price);
                }
            }

            // Handle FillAndKill orders that remain unmatched
            if !self.bids.is_empty() {
                let (_, bids) = self.bids.iter().next_back().unwrap();
                let order = &bids[0];
                if order.borrow().get_order_type() == OrderType::FillAndKill {
                    let order_id = order.borrow().get_order_id();
                    self.cancel_order(order_id);
                }
            }
            if !self.asks.is_empty() {
                let (_, asks) = self.asks.iter().next().unwrap();
                let order = &asks[0];
                if order.borrow().get_order_type() == OrderType::FillAndKill {
                    let order_id = order.borrow().get_order_id();
                    self.cancel_order(order_id);
                }
            }

            // If either side is empty, break
            if self.bids.is_empty() || self.asks.is_empty() {
                break;
            }
        }
        trades
    }
}
        


/// Tests:

#[cfg(test)]
mod test {
    use super::*;

    #[test]
    fn test_new_orderbook(){
        let mut orderbook = Orderbook::new(BTreeMap::new(), BTreeMap::new());
        assert_eq!(orderbook.size(), 0)
    }

    

}