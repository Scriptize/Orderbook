#![allow(unused)]
use std::{
    rc::Rc,
    cell::RefCell,
    collections::{BTreeMap, HashMap},
    thread::{self, JoinHandle},
    sync::{Arc, Mutex, Condvar},
    sync::atomic::{AtomicBool, Ordering},
    time::{Duration, SystemTime, UNIX_EPOCH}
};
use chrono::{Local, NaiveDateTime, TimeDelta, DateTime, Timelike};

#[derive(Clone, Copy, PartialEq, Debug)]
pub enum OrderType {
    GoodTillCancel,
    GoodForDay,
    FillAndKill,
    FillOrKill,
    Market,
}

#[derive(Clone, Copy, PartialEq, Debug)]
pub enum Side {
    Buy,
    Sell,
}

#[derive(Clone, Copy, PartialEq, Debug)]
enum LevelDataAction {
    Add,
    Remove,
    Match
}

type Price = i32;
type Quantity = u32;
type OrderId = u32;
#[derive(Debug)]
pub struct LevelInfo {
    pub price: Price,
    pub quantity: Quantity,
}

type LevelInfos = Vec<LevelInfo>;
#[derive(Debug)]
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
#[derive(Debug)]
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
    //new pointer to order; will be used most of the time
    pub fn new(
        order_type: OrderType,
        order_id: OrderId,
        side: Side,
        price: Price,
        quantity: Quantity,
    ) -> Arc<Mutex<Self>> {
        Arc::new(Mutex::new(Self{
            order_type,
            order_id,
            side,
            price,
            initial_quantity: quantity,
            remaining_quantity: quantity,
            filled_quantity: 0,
            filled: false,
        }))
    }

    pub fn new_market(
        order_id: OrderId,
        side: Side,
        quantity: Quantity, 
    ) -> Arc<Mutex<Self>> {
        // Use an obviously invalid price for market orders, e.g., i32::MIN
        Self::new(
            OrderType::Market,
            order_id,
            side,
            i32::MIN,
            quantity
        )
    }

    pub fn to_good_till_cancel(&mut self, price: Price) -> Result<(), String> {
        match self.get_order_type(){
            OrderType::Market => {
                self.price = price;
                self.order_type = OrderType::GoodTillCancel;
                Ok(())
            }
            _ => return Err("Order cannot have its price adjusted, only market orders can.".to_string()),
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
            println!("Filling {} by {}", self.order_id, quantity);
            println!("Before: rem={}, filled={}", self.remaining_quantity, self.filled_quantity);   
            Ok(())
        } else {
            Err("Order cannot be filled for more than it's remaining quantity.".to_string())
        }
    }

    
}

type OrderPointer = Arc<Mutex<Order>>;
type OrderPointers = Vec<OrderPointer>;
#[derive(Debug)]
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
        Order::new(
            order_type,
            self.get_order_id(),
            self.get_side(),
            self.get_price(),
            self.get_quantity(),
        )
    }
}

#[derive(Debug, Clone, Copy)]
pub struct TradeInfo {
    pub order_id: OrderId,
    pub price: Price,
    pub quantity: Quantity,
}
#[derive(Debug)]
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
#[derive(Debug)]
struct OrderEntry {
    order: OrderPointer,
    location: usize,
    side: Side,
    price: Price,
}

#[derive(Debug)]
struct LevelData{
    pub quantity: Quantity,
    pub count: Quantity,
}


#[derive(Debug)]
pub struct Orderbook {
    inner: Arc<Mutex<InnerOrderbook>>,
}

impl Orderbook {
    pub fn new(bids: BTreeMap<Price, OrderPointers>, asks: BTreeMap<Price, OrderPointers>) -> Self {
        let inner = InnerOrderbook::new(bids, asks);
        Self {
            inner: Arc::new(Mutex::new(inner)),
        }
    }

    pub fn build(bids: BTreeMap<Price, OrderPointers>, asks: BTreeMap<Price, OrderPointers>, test_mode: bool) -> Self {
        let mut book = Self::new(bids, asks);
        let inner = Arc::clone(&book.inner);
        let handle = thread::spawn(move || {
            let mut ob = inner.lock().unwrap();
            ob.prune_gfd_orders(test_mode);
        });
        book.inner.lock().unwrap().orders_prune_thread = Some(handle);
        book
    }

    pub fn add_order(&self, order: OrderPointer) -> Trades {
        self.inner.lock().unwrap().add_order(order)
    }

    pub fn cancel_order(&self, order_id: OrderId) {
        self.inner.lock().unwrap().cancel_order(order_id)
    }

    pub fn modify_order(&self, order: OrderModify) -> Trades {
        self.inner.lock().unwrap().modify_order(order)
    }

    pub fn size(&self) -> usize {
        self.inner.lock().unwrap().size()
    }

    pub fn get_order_infos(&self) -> OrderbookLevelInfos {
        self.inner.lock().unwrap().get_order_infos()
    }
}

#[derive(Debug)]
pub struct InnerOrderbook {
    data: HashMap<Price, LevelData>,
    bids: BTreeMap<Price, OrderPointers>,
    asks: BTreeMap<Price, OrderPointers>,
    orders: HashMap<OrderId, OrderEntry>,
    orders_prune_thread: Option<JoinHandle<()>>,
    shutdown_condition_variable: Condvar,
    shutdown: AtomicBool,
}

impl InnerOrderbook {
    pub fn new(bids: BTreeMap<Price, OrderPointers>, asks: BTreeMap<Price, OrderPointers>) -> Self {
        Self {
            bids,
            asks,
            orders: HashMap::new(),
            orders_prune_thread: None,
            shutdown_condition_variable: Condvar::new(),
            shutdown: AtomicBool::new(false),
            data: HashMap::new(),
        }
    }

    pub fn size(&self) -> usize {
        self.orders.len()
    }

    pub fn get_order_infos(&self) -> OrderbookLevelInfos {
        let mut bid_infos: LevelInfos = Vec::with_capacity(self.orders.len());
        let mut ask_infos: LevelInfos = Vec::with_capacity(self.orders.len());

        let create_level_infos = |price: Price, orders: &OrderPointers| {
            let total_quantity = orders.iter().fold(0, |sum, order| {
                sum + order.lock().unwrap().get_remaining_quantity()
            });
            LevelInfo { price, quantity: total_quantity }
        };

        for (price, orders) in &self.bids {
            bid_infos.push(create_level_infos(*price, orders));
        }

        for (price, orders) in &self.asks {
            ask_infos.push(create_level_infos(*price, orders));
        }

        OrderbookLevelInfos { bid_infos, ask_infos }
    }

    pub fn add_order(&mut self, order: OrderPointer) -> Trades {
        {
            let mut ord = order.lock().unwrap();
            if self.orders.contains_key(&ord.get_order_id()){
                return vec![];
            }

            if ord.get_order_type() == OrderType::Market {
                let result = match ord.get_side() {
                    Side::Buy if !self.asks.is_empty() => {
                        let (worst_ask, _) = self.asks.iter().next_back().unwrap();
                        ord.to_good_till_cancel(*worst_ask)
                    }
                    Side::Sell if !self.bids.is_empty() => {
                        let (worst_bid, _) = self.bids.iter().next().unwrap();
                        ord.to_good_till_cancel(*worst_bid)
                    }
                    _ => return vec![],
                };
                if result.is_err() {
                    return vec![];
                }
            }

            let order_type = ord.get_order_type();
            let side = ord.get_side();
            let price = ord.get_price();
            let initial_quantity = ord.get_initial_quantity();

            if order_type == OrderType::FillAndKill && !self.can_match(side, price) {
                return vec![];
            }

            if order_type == OrderType::FillOrKill && !self.can_fully_fill(side, price, initial_quantity) {
                return vec![];
            }

            let mut index: usize = 0;
            if side == Side::Buy {
                let orders = &mut self.bids.entry(price).or_default();
                orders.push(order.clone());
                index = orders.len() - 1;
            } else {
                let orders = &mut self.asks.entry(price).or_default();
                orders.push(order.clone());
                index = orders.len() - 1;
            }

            let order_id = ord.get_order_id();
            self.orders.insert(order_id, OrderEntry {order: order.clone(), location: index, side, price,});
        }
        self.on_order_added(order.clone());
        self.match_orders()
    }


    pub fn cancel_order(&mut self, order_id: OrderId) {
        if let Some(entry) = self.orders.remove(&order_id) {
            let OrderEntry { order, location, side, price } = entry;

            let maybe_queue = match side {
                Side::Buy => self.bids.get_mut(&price),
                Side::Sell => self.asks.get_mut(&price),
            };

            if let Some(queue) = maybe_queue {
                let last_index = queue.len() - 1;
                queue.swap_remove(location);

                if location < queue.len() {
                    let moved_order = &queue[location];
                    let moved_id = moved_order.lock().unwrap().get_order_id();
                    if let Some(moved_entry) = self.orders.get_mut(&moved_id) {
                        moved_entry.location = location;
                    }
                }

                if queue.is_empty() {
                    match side {
                        Side::Buy => { self.bids.remove(&price); }
                        Side::Sell => { self.asks.remove(&price); }
                    }
                }
            }

            self.on_order_cancelled(order.clone());
        }
    }


    pub fn modify_order(&mut self, order: OrderModify) -> Trades {
        let order_type = self.orders.get(&order.get_order_id())
            .map(|entry| entry.order.lock().unwrap().get_order_type());

        if order_type.is_none() {
            return vec![];
        }

        self.cancel_order(order.get_order_id());
        self.add_order(order.to_order_pointer(order_type.unwrap()))
    }
    fn update_level_data(&mut self, price: Price, quantity: Quantity, action: LevelDataAction) {
        let data = self.data.entry(price).or_insert(LevelData { quantity: 0, count: 0 });

        match action {
            LevelDataAction::Remove => {
                data.count -= 1;
                data.quantity -= quantity;
            },
            LevelDataAction::Add => {
                data.count += 1;
                data.quantity += quantity;
            },
            LevelDataAction::Match => {
                data.quantity -= quantity;
            },
        }

        if data.count == 0 {
            self.data.remove(&price);
        }
    }
    fn on_order_cancelled(&mut self, order: OrderPointer){
        let ord = order.lock().unwrap();
        self.update_level_data(ord.get_price(), ord.get_initial_quantity(), LevelDataAction::Remove)
    }
    fn on_order_added(&mut self, order: OrderPointer) {
        let ord = order.lock().unwrap();
        self.update_level_data(ord.get_price(), ord.get_initial_quantity(), LevelDataAction::Add)
    }
    fn on_order_matched(&mut self, price: Price, quantity: Quantity, is_fully_filled: bool) {
        let action = if is_fully_filled {
            LevelDataAction::Remove
        } else {
            LevelDataAction::Match
        };
        self.update_level_data(price, quantity, action);
    }

    fn can_match(&mut self, side: Side, price: Price) -> bool {
        match side {
            Side::Buy => self.asks.first_key_value().map_or(false, |(ask, _)| price >= *ask),
            Side::Sell => self.bids.first_key_value().map_or(false, |(bid, _)| price <= *bid),
        }
    }

    fn can_fully_fill(&mut self, side: Side, price: Price, mut quantity: Quantity) -> bool {

        if !self.can_match(side, price){
            return false
        }

        let threshold: Option<Price> = None;

        // Since bids or asks are guaranteed to be non-empty, unwrap directly.
        let threshold = Some(
            if side == Side::Buy {
            *self.asks.iter().next().unwrap().0
            } else {
            *self.bids.iter().next_back().unwrap().0
            }
        );

        for (level_price, level_data) in self.data.iter() {
            if let Some(threshold_price) = threshold {
                let outside_bounds = match side {
                    Side::Buy => threshold_price > *level_price,
                    Side::Sell => threshold_price < *level_price,
                };
                if outside_bounds {
                    continue;
                }
            }

            if (side == Side::Buy && *level_price > price) || (side == Side::Sell && *level_price < price){
                continue;
            }

            if quantity <= level_data.quantity{
                return true
            }

            quantity -= level_data.quantity

        }
        return false
    }

    fn remove_order_from_book(&mut self, order_id: OrderId, price: Price, side: Side) {
        // Remove from orders map and get the entry (contains location)
        if let Some(entry) = self.orders.remove(&order_id) {
            let book = match side {
                Side::Buy => &mut self.bids,
                Side::Sell => &mut self.asks,
            };

            if let Some(queue) = book.get_mut(&price) {
                let idx = entry.location;
                let last_idx = queue.len() - 1;
                queue.swap_remove(idx);
                // If we swapped with another order, update its location in orders map
                if idx < queue.len() {
                    let swapped_order_id = queue[idx].lock().unwrap().get_order_id();
                    if let Some(swapped_entry) = self.orders.get_mut(&swapped_order_id) {
                        swapped_entry.location = idx;
                    }
                }
                if queue.is_empty() {
                    book.remove(&price);
                }
            }
        }
    }

    fn match_orders(&mut self) -> Trades {
        let mut trades = Vec::with_capacity(self.orders.len());

        loop {
            if self.bids.is_empty() || self.asks.is_empty() {
                break;
            }

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

            let bid_order_ptr = bids.get(0).cloned();
            let ask_order_ptr = asks.get(0).cloned();

            let (bid_order_ptr, ask_order_ptr) = match (bid_order_ptr, ask_order_ptr) {
                (Some(b), Some(a)) => (b, a),
                _ => break,
            };

            let (bid_filled, ask_filled, bid_id, ask_id, trade_quantity, final_bid_price, final_ask_price, bid_type, ask_type);
            {
                let mut bid = bid_order_ptr.lock().unwrap();
                let mut ask = ask_order_ptr.lock().unwrap();

                trade_quantity = bid.get_remaining_quantity().min(ask.get_remaining_quantity());

                // If nothing to match, break or handle F&K
                if trade_quantity == 0 {
                    break;
                }

                bid.fill(trade_quantity).ok();
                ask.fill(trade_quantity).ok();

                bid_filled = bid.is_filled();
                ask_filled = ask.is_filled();

                bid_id = bid.get_order_id();
                ask_id = ask.get_order_id();

                final_bid_price = bid.get_price();
                final_ask_price = ask.get_price();

                bid_type = bid.get_order_type();
                ask_type = ask.get_order_type();
            }

            trades.push(Trade::new(
                TradeInfo { order_id: bid_id, price: final_bid_price, quantity: trade_quantity },
                TradeInfo { order_id: ask_id, price: final_ask_price, quantity: trade_quantity },
            ));

            self.on_order_matched(final_bid_price, trade_quantity, bid_filled);
            self.on_order_matched(final_ask_price, trade_quantity, ask_filled);

            // Fully filled orders
            if bid_filled {
                self.remove_order_from_book(bid_id, final_bid_price, Side::Buy);
            }

            if ask_filled {
                self.remove_order_from_book(ask_id, final_ask_price, Side::Sell);
            }

            // Remove partially filled F&K orders (should not persist)
            if !bid_filled && bid_type == OrderType::FillAndKill {
                self.remove_order_from_book(bid_id, final_bid_price, Side::Buy);
            }

            if !ask_filled && ask_type == OrderType::FillAndKill {
                self.remove_order_from_book(ask_id, final_ask_price, Side::Sell);
            }
        }

        trades
    }


    

    fn prune_gfd_orders(&mut self, test_mode: bool) {
        let end_hour = 16;
        println!("end_hour: {}", end_hour);

        loop {
            println!("Started Loop!");
            let now = SystemTime::now();
            let now_duration = now.duration_since(UNIX_EPOCH).unwrap();
            println!("now_duration: {:?}", now_duration);
            let now_secs = now_duration.as_secs() as i64;
            println!("now_secs: {}", now_secs);
            
            let now_parts = DateTime::from_timestamp(now_secs, 0).unwrap();
            println!("now_parts: {:?}", now_parts);
            let mut date = now_parts.date_naive();
            println!("date: {}", date);
            let hour = now_parts.hour();
            println!("hour: {}", hour);

            println!("Comparing hours!");
            println!("Current hour is {}, end hour is {}", hour, end_hour);
            if hour >= end_hour {
                date = date.succ_opt().unwrap(); // move to next day
                println!("Moved to next day, new date: {}", date);
            }

            let next_cutoff = date.and_hms_opt(end_hour, 0, 0).unwrap();
            println!("next_cutoff: {}", next_cutoff);
            let cutoff_ts = UNIX_EPOCH + Duration::from_secs(next_cutoff.and_utc().timestamp() as u64);
            println!("cutoff_ts: {:?}", cutoff_ts);
            let now_system_time = SystemTime::now();
            println!("now_system_time: {:?}", now_system_time);
            
            println!("Finding wait duration");
            let wait_duration = cutoff_ts
                .duration_since(now_system_time)
                .unwrap_or(Duration::from_secs(0)) + Duration::from_millis(100);
            println!("wait_duration: {:?}", wait_duration);

            // Use a dummy mutex for waiting on the condition variable.
            let dummy_mutex = Mutex::new(());
            let guard = dummy_mutex.lock().unwrap();
            let (guard, result) = self.shutdown_condition_variable
                .wait_timeout(guard, wait_duration)
                .unwrap();
            
            println!("result.timed_out(): {}", result.timed_out());
            println!("self.shutdown: {}", self.shutdown.load(Ordering::Acquire));
            
            println!("DEBUG: About to check shutdown condition");
            if self.shutdown.load(Ordering::Acquire) {
                println!("Shutdown requested, exiting prune_gfd_orders.");
                return;
            }

            println!("DEBUG: About to check timeout condition");
            if !result.timed_out() {
                println!("Woke up early (not timed out), skipping pruning.");
                continue;
            }

            println!("DEBUG: About to start pruning logic");
            
            // Pruning logic
            println!("Pruning Orders!");
            let mut order_ids = vec![];

            println!("DEBUG: About to iterate over orders");
            for (order_id, entry) in &self.orders {
                println!("DEBUG: Checking order {}", order_id);
                let order = entry.order.lock().unwrap();
                println!("DEBUG: Order type: {:?}", order.get_order_type());
                if order.get_order_type() == OrderType::GoodForDay {
                    println!("DEBUG: Adding GFD order {} to cancellation list", order_id);
                    order_ids.push(*order_id);
                }
            }
            
            println!("Found {} GFD orders to cancel", order_ids.len());

            for id in order_ids {
                println!("Canceling order with id: {}", id);
                self.cancel_order(id);
            }
            
            println!("Orders left: {}", self.orders.len());

            if test_mode{
                println!("Finished pruning! test mode on");
                break;
            }
        }
    }
}
impl Drop for InnerOrderbook {
    fn drop(&mut self) {
        self.shutdown.store(true, Ordering::Release);
        self.shutdown_condition_variable.notify_one();
        if let Some(handle) = self.orders_prune_thread.take() {
            handle.join().expect("Failed to join orders_prune_thread");
        }
    }
}
        


/// Tests:

//Each test implicitly assumes a working match_orders() functionality
#[cfg(test)]
mod test {
    use super::*;

    #[test]
    fn test_orderbook_new(){
        let orderbook = Orderbook::new(BTreeMap::new(), BTreeMap::new());
        assert_eq!(orderbook.size(), 0)
    }

    #[test]
    fn test_orderbook_add_order(){
        let mut orderbook = Orderbook::new(BTreeMap::new(), BTreeMap::new());
        orderbook.add_order(Order::new(OrderType::GoodTillCancel, 1, Side::Buy, 100, 10));
        orderbook.add_order(Order::new(OrderType::GoodTillCancel, 2, Side::Buy, 100, 10));
        orderbook.add_order(Order::new(OrderType::GoodTillCancel, 3, Side::Buy, 100, 10));
        
        assert_eq!(orderbook.size(), 3);
    }

    #[test]
    fn test_orderbook_cancel_order(){
        let mut orderbook = Orderbook::new(BTreeMap::new(), BTreeMap::new());

        orderbook.add_order(Order::new(OrderType::GoodTillCancel, 1, Side::Buy, 100, 10));
        orderbook.add_order(Order::new(OrderType::GoodTillCancel, 2, Side::Buy, 100, 10));
        orderbook.add_order(Order::new(OrderType::GoodTillCancel, 3, Side::Buy, 100, 10));
        orderbook.cancel_order(1);
        orderbook.cancel_order(2);
        orderbook.cancel_order(3);

        assert_eq!(orderbook.size(), 0);
    }

    #[test]
    fn test_order_modify_order(){
        let mut orderbook = Orderbook::new(BTreeMap::new(),BTreeMap::new());
        orderbook.add_order(Order::new(OrderType::GoodTillCancel, 1, Side::Buy, 100, 10));
        orderbook.add_order(Order::new(OrderType::GoodTillCancel, 2, Side::Buy, 100, 10));
    

        //create modification
        let order_mod = OrderModify::new(2, Side::Sell, 100, 10);

        //should match and fill order with id 1
        orderbook.modify_order(order_mod);
        assert_eq!(orderbook.size(), 0);
        

    }

    #[test]
    fn test_orderbook_will_cancel_fnk(){
        let mut orderbook = Orderbook::new(BTreeMap::new(),BTreeMap::new());

        // match should completely fill
        orderbook.add_order(Order::new(OrderType::GoodTillCancel, 2, Side::Sell, 100, 10));
        orderbook.add_order(Order::new(OrderType::FillAndKill, 1, Side::Buy, 100, 10));
        
        
        //Unmatched F&K (should cancel)
        orderbook.add_order(Order::new(OrderType::GoodTillCancel, 3, Side:: Buy, 250, 5));
        orderbook.add_order(Order::new(OrderType::FillAndKill, 4, Side::Buy, 100, 10));

        assert_eq!(orderbook.size(), 1);
    }

    #[test]
    fn test_orderbook_will_cancel_fok(){
        let mut orderbook = Orderbook::new(BTreeMap::new(), BTreeMap::new());

        // Add a sell order with quantity less than the FOK buy order
        orderbook.add_order(Order::new(OrderType::GoodTillCancel, 1, Side::Sell, 100, 5));

        // Try to add a FOK buy order that requires more quantity than available (should not be added)
        orderbook.add_order(Order::new(OrderType::FillOrKill, 2, Side::Buy, 100, 10));
        assert_eq!(orderbook.size(), 1);

        // Now add enough sell quantity to fill the FOK order
        orderbook.add_order(Order::new(OrderType::GoodTillCancel, 3, Side::Sell, 100, 10));

        // Add a FOK buy order that can be fully filled (should match and remove both)
        orderbook.add_order(Order::new(OrderType::FillOrKill, 4, Side::Buy, 100, 10));
        println!("{:#?}", orderbook);
        assert_eq!(orderbook.size(), 1);
    }

    #[test]
    fn test_orderbook_wont_match(){
        let mut ob1 = Orderbook::new(BTreeMap::new(),BTreeMap::new());
        let mut ob2 = Orderbook::new(BTreeMap::new(),BTreeMap::new());
        

        //Same side
        ob1.add_order(Order::new(OrderType::GoodTillCancel, 1, Side::Buy, 1, 1));
        ob1.add_order(Order::new(OrderType::GoodTillCancel, 2, Side::Buy, 1, 1));

        //Ask higher than bid
        ob2.add_order(Order::new(OrderType::GoodTillCancel, 1, Side::Buy, 1, 1));
        ob2.add_order(Order::new(OrderType::GoodTillCancel, 2, Side::Sell, 2, 1));
        
        assert_eq!(ob1.size(), ob2.size());

    }

    #[test]
    fn test_add_market_order(){
        let mut ob = Orderbook::new(BTreeMap::new(),BTreeMap::new());
        println!("Created orderbook!");

        ob.add_order(Order::new(OrderType::GoodTillCancel, 1, Side::Buy, 100, 10));
        ob.add_order(Order::new(OrderType::GoodTillCancel, 2, Side::Buy, 150, 10));
        // No orders can match
        ob.add_order(Order::new(OrderType::GoodTillCancel, 3, Side::Sell, 200, 10));
        ob.add_order(Order::new(OrderType::GoodTillCancel, 4, Side::Sell, 300, 10));
        println!("Added incompatible orders!");
        // Will match worst sell order (300); asks should be left with 1 
        ob.add_order(Order::new_market(5, Side::Buy, 10));
        println!("Added market order!");
        let level_infos = ob.get_order_infos();
        let asks = level_infos.get_asks();

        assert_eq!(asks.len(), 1);

    }

    #[test]
    fn test_good_for_day_pruning() {
        use chrono::Local;
        let now = Local::now();
        let minute = now.minute();
        let second = now.second();
        let hour = now.hour();

        let ob = Orderbook::build(BTreeMap::new(), BTreeMap::new(), true);
        ob.add_order(Order::new(OrderType::GoodForDay, 1, Side::Buy, 100, 10));
        ob.add_order(Order::new(OrderType::GoodForDay, 2, Side::Sell, 200, 10));
        ob.add_order(Order::new(OrderType::GoodTillCancel, 3, Side::Sell, 1000, 10));

        // Find time until next hour
        let secs_until_next_hour = (59 - minute) * 60 + (60 - second);
        if secs_until_next_hour > 180 {
            // More than 3 minutes until next hour, pruning won't happen, just check size is 2
            assert_eq!(ob.size(), 3);
        } else {
            // Within 3 minutes of next hour, pruning may happen soon
            thread::sleep(std::time::Duration::from_millis(200)); // Give prune thread time to run
            assert_eq!(ob.size(), 1);
        }
    }
}