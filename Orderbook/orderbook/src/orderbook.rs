//! # Orderbook Module
//!
//! This module provides a comprehensive implementation of an orderbook for managing limit and market orders in an exchange.
//!
//! ## Features
//! - **Order Types:** Supports [`OrderType`] variants such as GoodTillCancel, GoodForDay, FillAndKill, FillOrKill, and Market.
//! - **Bid/Ask Management:** Uses price levels and order queues for efficient bid/ask tracking.
//! - **Matching Engine:** Matches buy and sell orders, generating [`Trade`] records.
//! - **Order Modification & Cancellation:** Allows modification via [`OrderModify`] and cancellation by order ID.
//! - **Automatic Pruning:** GoodForDay orders are automatically pruned at market close.
//! - **Thread Safety:** All operations are thread-safe using `Arc<Mutex<_>>`.
//! - **Query Utilities:** Provides methods for querying orderbook state and trade history.
//! - **Extensibility & Testability:** Designed for easy extension and includes comprehensive unit tests.
//!
//! ## Main Types
//! - [`Orderbook`]: The main interface for interacting with the orderbook.
//! - [`Order`]: Represents an individual order.
//! - [`OrderType`]: Enum for order types.
//! - [`Side`]: Enum for order side (Buy/Sell).
//! - [`OrderModify`]: Structure for modifying existing orders.
//! - [`Trade`]: Structure representing a matched trade.
//! - [`OrderbookLevelInfos`]: Aggregated bid/ask level information.
//!
//! ## Example Usage
//!
//! ```rust
//! use orderbook::{Orderbook, Order, OrderType, Side};
//!
//! let ob = Orderbook::new(Default::default(), Default::default());
//! ob.add_order(Order::new(OrderType::GoodTillCancel, 1, Side::Buy, 100, 10));
//! ob.cancel_order(1);
//! ```
//!
//! ## Thread Safety
//! All public methods on [`Orderbook`] are thread-safe.
//!
//! ## See Also
//! - [`Orderbook`]
//! - [`Order`]
//! - [`OrderType`]
//! - [`OrderModify`]
//! - [`Trade`]
//! - [`OrderbookLevelInfos`]
//!

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
use log::{info, trace, warn, debug, error};



/// Represents the type of an order in the orderbook.
/// Determines how the order is handled regarding matching, cancellation, and expiry.
#[derive(Clone, Copy, PartialEq, Debug)]
pub enum OrderType {
    /// Persistent order until explicitly cancelled.
    GoodTillCancel, 
    /// Expires automatically at the end of the trading day.
    GoodForDay,
    /// Matches as much as possible immediately, cancels remainder.
    FillAndKill,
    /// Only executes if it can be fully filled immediately, otherwise cancels.
    FillOrKill,
    /// Executes at the best available price, does not specify a price.
    Market,
}


#[derive(Clone, Copy, PartialEq, Debug)]
pub enum Side {
    Buy,
    Sell,
}

/// Represents actions that can be performed on a price level's data in the orderbook.
#[derive(Clone, Copy, PartialEq, Debug)]
pub enum LevelDataAction {
    /// Add quantity and count to the level.
    Add,
    /// Remove quantity and count from the level.
    Remove,
    /// Match (reduce) quantity at the level.
    Match,
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

/// A single order tracked by the order book.
///
/// Tracks identity, side, price, and quantity lifecycle:
/// initial → remaining/filled, with a convenience flag `filled`.
#[derive(Debug)]
pub struct Order {
    /// Limit/market/GTC classification for matching behavior.
    order_type: OrderType,
    /// Unique identifier assigned by the client/system.
    order_id: OrderId,
    /// Buy or Sell.
    side: Side,
    /// Limit price. For market orders created via [`new_market`], this
    /// is initialized to a sentinel and may later be set by [`to_good_till_cancel`].
    price: Price,
    /// Quantity at creation time.
    initial_quantity: Quantity,
    /// Shares/contracts not yet executed.
    remaining_quantity: Quantity,
    /// Cumulative executed size.
    filled_quantity: Quantity,
    /// Convenience flag set when `remaining_quantity == 0`.
    filled: bool,
}

impl Order {
    /// Creates a new **limit** order wrapped in `Arc<Mutex<_>>`.
    ///
    /// # Parameters
    /// - `order_type`: Typically `OrderType::Limit` for this constructor.
    /// - `order_id`: Unique order identifier.
    /// - `side`: Buy or Sell.
    /// - `price`: Limit price.
    /// - `quantity`: Initial total quantity.
    ///
    /// # Returns
    /// A thread-safe handle to the newly created order.
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

    /// Creates a new **market** order wrapped in `Arc<Mutex<_>>`.
    ///
    /// Initializes `price` to a sentinel (e.g., `i32::MIN`) since market
    /// orders are price-less until optionally converted via [`to_good_till_cancel`].
    pub fn new_market(
        order_id: OrderId,
        side: Side,
        quantity: Quantity, 
    ) -> Arc<Mutex<Self>> {
        Self::new(
            OrderType::Market,
            order_id,
            side,
            i32::MIN,
            quantity
        )
    }

    /// Converts a **market** order into **good-till-cancel** with a concrete limit `price`.
    ///
    /// # Errors
    /// Returns an error if the order is not currently `OrderType::Market`.
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

    /// Returns the order's unique identifier.
    pub const fn get_order_id(&self) -> OrderId {
        self.order_id
    }

    /// Returns the order side.
    pub const fn get_side(&self) -> Side {
        self.side
    }

    /// Returns the current limit price.
    pub const fn get_price(&self) -> Price {
        self.price
    }

    /// Returns the current order type.
    pub const fn get_order_type(&self) -> OrderType {
        self.order_type
    }

    /// Returns the initial quantity at creation.
    pub const fn get_initial_quantity(&self) -> Quantity {
        self.initial_quantity
    }

    /// Returns the currently remaining (unfilled) quantity.
    pub const fn get_remaining_quantity(&self) -> Quantity {
        self.remaining_quantity
    }

    /// Returns the cumulative filled quantity.
    pub const fn get_filled_quantity(&self) -> Quantity {
        self.filled_quantity
    }

    /// Indicates whether the order is fully filled.
    pub const fn is_filled(&self) -> bool {
        self.filled
    }

    /// Applies a partial or full fill to the order.
    ///
    /// Decrements `remaining_quantity` and increments `filled_quantity`.
    /// Sets `filled = true` when `remaining_quantity` reaches zero.
    ///
    /// # Errors
    /// Returns an error if `quantity` exceeds the current `remaining_quantity`.
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

type OrderPointer = Arc<Mutex<Order>>;
type OrderPointers = Vec<OrderPointer>;

/// Represents a request to modify an existing order.
///
/// `OrderModify` holds the new parameters (price, side, quantity) to
/// be applied to an existing order identified by `order_id`.
#[derive(Debug)]
pub struct OrderModify {
    /// Unique identifier of the order to be modified.
    order_id: OrderId,
    /// New price for the order.
    price: Price,
    /// New side (buy or sell) for the order.
    side: Side,
    /// New total quantity for the order.
    quantity: Quantity,
}

impl OrderModify {
    /// Creates a new `OrderModify` request.
    ///
    /// # Parameters
    /// - `order_id`: The unique ID of the order to modify.
    /// - `side`: The updated order side.
    /// - `price`: The updated price.
    /// - `quantity`: The updated total quantity.
    pub fn new(order_id: OrderId, side: Side, price: Price, quantity: Quantity) -> Self {
        Self {
            order_id,
            side,
            price,
            quantity,
        }
    }

    /// Returns the order ID targeted by this modification.
    pub const fn get_order_id(&self) -> OrderId {
        self.order_id
    }

    /// Returns the updated side.
    pub const fn get_side(&self) -> Side {
        self.side
    }

    /// Returns the updated price.
    pub const fn get_price(&self) -> Price {
        self.price
    }

    /// Returns the updated quantity.
    pub const fn get_quantity(&self) -> Quantity {
        self.quantity
    }

    /// Converts this modification into a fresh [`Order`] instance wrapped in `OrderPointer`.
    ///
    /// This is typically used when re-inserting the modified order into the order book.
    ///
    /// # Parameters
    /// - `order_type`: The desired type for the new order (e.g., `OrderType::Limit`).
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

/// Represents one side of a trade (either bid or ask).
///
/// `TradeInfo` contains the order ID, execution price, and executed
/// quantity for a single participant in a matched trade.
#[derive(Debug, Clone, Copy)]
pub struct TradeInfo {
    /// Identifier of the order participating in the trade.
    pub order_id: OrderId,
    /// Execution price for this side of the trade.
    pub price: Price,
    /// Executed quantity for this side of the trade.
    pub quantity: Quantity,
}

/// Represents an executed trade in the order book.
///
/// A `Trade` pairs the buy-side (`bid_trade`) and sell-side (`ask_trade`)
/// information that resulted in a match.
#[derive(Debug)]
pub struct Trade {
    /// Information about the bid (buy) side of the trade.
    bid_trade: TradeInfo,
    /// Information about the ask (sell) side of the trade.
    ask_trade: TradeInfo,
}

impl Trade {
    /// Creates a new `Trade` from the given bid and ask trade information.
    ///
    /// # Parameters
    /// - `bid_trade`: Information about the buy side of the trade.
    /// - `ask_trade`: Information about the sell side of the trade.
    pub fn new(bid_trade: TradeInfo, ask_trade: TradeInfo) -> Self {
        Self {
            bid_trade,
            ask_trade,
        }
    }

    /// Returns the `TradeInfo` for the bid (buy) side.
    pub const fn get_bid_trade(&self) -> TradeInfo {
        self.bid_trade
    }

    /// Returns the `TradeInfo` for the ask (sell) side.
    pub const fn get_ask_trade(&self) -> TradeInfo {
        self.ask_trade
    }
}


type Trades = Vec<Trade>;


/// Internal record used to track an order’s position in the order book.
///
/// `OrderEntry` stores a pointer to the order itself along with its
/// cached location index, side, and price for quick lookup and updates.
#[derive(Debug)]
struct OrderEntry {
    /// Shared, mutable pointer to the underlying order.
    order: OrderPointer,
    /// Cached index of the order’s position in its side’s queue.
    location: usize,
    /// Side (buy or sell) of the order.
    side: Side,
    /// Price of the order.
    price: Price,
}


/// Aggregated data for a single price level in the order book.
///
/// `LevelData` tracks the total quantity and the number of individual
/// orders at a given price level.
#[derive(Debug)]
struct LevelData {
    /// Total aggregated quantity at this price level.
    pub quantity: Quantity,
    /// Number of distinct orders at this price level.
    pub count: Quantity,
}



/// Thread-safe public interface to the order book.
///
/// `Orderbook` is the *outer* type in the **inner–outer locking pattern**:
/// - The **outer** type (`Orderbook`) is a thin, `pub` façade that holds
///   an `Arc<Mutex<InnerOrderbook>>`, making it safe to clone and share
///   across threads.
/// - The **inner** type (`InnerOrderbook`) contains all mutable state
///   (orders, price levels, trades, etc.) and is *not* `pub`, ensuring
///   that all mutation goes through controlled API methods on `Orderbook`.
///
/// # Locking Pattern
/// This design allows:
/// - Multiple owners of the `Orderbook` (via `Arc`) to share the same state.
/// - Synchronization (via `Mutex`) so that only one thread can mutate the
///   `InnerOrderbook` at a time.
/// - Encapsulation: callers never manipulate `InnerOrderbook` directly,
///   reducing the risk of inconsistent state or broken invariants.
///
/// # Example
/// ```
/// let book = Orderbook::new();
/// book.add_order(my_order); // Internally locks `inner`
/// ```
#[derive(Debug)]
pub struct Orderbook {
    /// Shared, mutex-protected inner order book state (private to enforce encapsulation).
    inner: Arc<Mutex<InnerOrderbook>>,
}

impl Orderbook {
    /// Creates a new `Orderbook` with pre-populated bid/ask maps.
    ///
    /// The returned outer `Orderbook` wraps an `InnerOrderbook` in `Arc<Mutex<_>>`
    /// so the book can be shared safely across threads.
    ///
    /// # Parameters
    /// - `bids`: Map of price → queue of orders on the bid side.
    /// - `asks`: Map of price → queue of orders on the ask side.
    pub fn new(bids: BTreeMap<Price, OrderPointers>, asks: BTreeMap<Price, OrderPointers>) -> Self {
        let inner = InnerOrderbook::new(bids, asks);
        Self {
            inner: Arc::new(Mutex::new(inner)),
        }
    }

    /// Builds an `Orderbook` and launches a background pruning thread.
    ///
    /// Spawns a thread that locks the inner book and prunes Good-For-Day (GFD) orders.
    /// This demonstrates the inner–outer pattern: public API here, mutation inside the lock.
    ///
    /// # Parameters
    /// - `bids`: Initial bid levels (price → order queue).
    /// - `asks`: Initial ask levels (price → order queue).
    /// - `test_mode`: If `true`, enables test-friendly pruning behavior.
    ///
    /// # Notes
    /// - Stores the join handle in `orders_prune_thread` for lifecycle management.
    /// - Locking uses `Mutex::lock().unwrap()`, which will **panic** if the mutex is poisoned.
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

    /// Adds an order to the book and attempts to match it.
    ///
    /// Internally locks the inner book, inserts the order, and runs matching logic.
    ///
    /// # Parameters
    /// - `order`: Shared pointer to the order to add.
    ///
    /// # Returns
    /// Any `Trades` generated by matching against the opposite side.
    pub fn add_order(&self, order: OrderPointer) -> Trades {
        self.inner.lock().unwrap().add_order(order)
    }

    /// Cancels an order by ID.
    ///
    /// Internally locks the inner book and removes or marks the order as canceled.
    ///
    /// # Parameters
    /// - `order_id`: Identifier of the order to cancel.
    pub fn cancel_order(&self, order_id: OrderId) {
        self.inner.lock().unwrap().cancel_order(order_id)
    }

    /// Modifies an existing order using an `OrderModify` request.
    ///
    /// Internally locks the inner book, applies changes, and may requeue the order.
    ///
    /// # Parameters
    /// - `order`: Modification descriptor (new price/side/quantity).
    ///
    /// # Returns
    /// Any `Trades` generated if the modification triggers matching.
    pub fn modify_order(&self, order: OrderModify) -> Trades {
        self.inner.lock().unwrap().modify_order(order)
    }

    /// Returns the total number of live orders in the book.
    ///
    /// Locks the inner book to compute the value.
    pub fn size(&self) -> usize {
        self.inner.lock().unwrap().size()
    }

    /// Returns aggregated level information (depth) for both sides.
    ///
    /// Locks the inner book and collects `OrderbookLevelInfos`, which includes
    /// per-price totals and counts for bids and asks.
    pub fn get_order_infos(&self) -> OrderbookLevelInfos {
        self.inner.lock().unwrap().get_order_infos()
    }
}


/// Core, single-threaded state and matching engine for the order book.
///
/// `InnerOrderbook` is the *inner* part of the inner–outer locking pattern:
/// external callers interact with a public `Orderbook` wrapper that holds
/// an `Arc<Mutex<InnerOrderbook>>`. All mutation happens by locking this
/// inner structure, preserving invariants such as price–time priority.
///
/// # Responsibilities
/// - Maintain bid/ask books (`BTreeMap<Price, OrderPointers>`) ordered by price.
/// - Track per-price aggregates in `data` (quantity, count).
/// - Map `OrderId` → `OrderEntry` to quickly locate and update an order.
/// - Provide matching (`match_orders`) and administrative flows (add/modify/cancel).
/// - Optionally run/coordinate a pruning thread for GFD orders via a join handle.
///
/// # Concurrency & Lifecycle
/// - The pruning worker (if any) is owned via `orders_prune_thread`.
/// - `shutdown` and `shutdown_condition_variable` coordinate graceful stop for pruning.
///   `Drop` sets `shutdown = true`, notifies the condvar, and joins the worker.
#[derive(Debug)]
pub struct InnerOrderbook {
    /// Aggregated per-level stats used for FOK checks and level reporting.
    data: HashMap<Price, LevelData>,
    /// Bid book: price → FIFO of orders (best bid = highest price).
    bids: BTreeMap<Price, OrderPointers>,
    /// Ask book: price → FIFO of orders (best ask = lowest price).
    asks: BTreeMap<Price, OrderPointers>,
    /// Fast lookup: order id → (pointer + cached location/side/price).
    orders: HashMap<OrderId, OrderEntry>,
    /// Optional background thread that prunes Good-For-Day orders at cutoff.
    orders_prune_thread: Option<JoinHandle<()>>,
    /// Used by the pruning loop to wait until cutoff or shutdown.
    shutdown_condition_variable: Condvar,
    /// Signals pruning loop to exit when dropping the book.
    shutdown: AtomicBool,
}

impl InnerOrderbook {
    /// Constructs a new inner order book from initial bid/ask maps.
    ///
    /// Typically called by the outer `Orderbook` and wrapped in `Arc<Mutex<...>>`.
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

    /// Returns the count of live orders tracked by the book.
    pub fn size(&self) -> usize {
        self.orders.len()
    }

    /// Produces aggregated depth (level infos) for bids and asks.
    ///
    /// Each level contains `(price, total_remaining_quantity)` gathered from the queues.
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

    /// Inserts an order into the book, possibly converting it and/or matching immediately.
    ///
    /// - Rejects duplicate `order_id`.
    /// - Converts `Market` to `GoodTillCancel` at a worst-opposite price if the book is non-empty.
    /// - Enforces `FillAndKill` (must be matchable now) and `FillOrKill` (must be fully fillable now).
    /// - Appends to the correct side/price queue, updates indices, emits aggregates,
    ///   and runs the matching loop.
    ///
    /// # Returns
    /// A vector of `Trade` records generated by matching.
    pub fn add_order(&mut self, order: OrderPointer) -> Trades {
        {
            let mut ord = order.lock().unwrap();
            if self.orders.contains_key(&ord.get_order_id()){
                warn!("InnerOrderbook: Order with id {} already exists, skipping add.", ord.get_order_id());
                return vec![];
            }

            // Convert Market → GTC at a price that ensures immediate consideration, if possible.
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
                    warn!("InnerOrderbook: Failed to convert market order to GTC: {:?}", result);
                    return vec![];
                }
            }

            let order_type = ord.get_order_type();
            let side = ord.get_side();
            let price = ord.get_price();
            let initial_quantity = ord.get_initial_quantity();
            let order_id = ord.get_order_id();

            // F&K: must be crossable *now*
            if order_type == OrderType::FillAndKill && !self.can_match(side, price) {
                info!("F&K Order#{} cannot match, not adding.", order_id);
                return vec![];
            }

            // FOK: must be fully fillable at current book
            if order_type == OrderType::FillOrKill && !self.can_fully_fill(side, price, initial_quantity) {
                info!("FOK Order#{} cannot be fully filled, not adding.", order_id);
                return vec![];
            }

            // Insert to side/price queue and remember location
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
            let str_side = match side{
                Side::Buy => "BUY",
                Side::Sell => "SELL"
            };
            let order_id = ord.get_order_id();
            info!("Added {}#{} for {}/{} @ {} ({:?})", str_side, order_id, initial_quantity, initial_quantity, price, order_type);
            self.orders.insert(order_id, OrderEntry {order: order.clone(), location: index, side, price,});
        }
        self.on_order_added(order.clone());
        let trades = self.match_orders();
        if !trades.is_empty() {
            // info!("InnerOrderbook: Trades occurred after add: {:?}", trades);
        }
        trades
    }

    /// Cancels (removes) an order by ID, repairing queues and indices as needed.
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

                // If we swapped-in another order, update its cached index
                if location < queue.len() {
                    let moved_order = &queue[location];
                    let moved_id = moved_order.lock().unwrap().get_order_id();
                    if let Some(moved_entry) = self.orders.get_mut(&moved_id) {
                        moved_entry.location = location;
                    }
                }

                // Clean up empty price level
                if queue.is_empty() {
                    match side {
                        Side::Buy => { self.bids.remove(&price); }
                        Side::Sell => { self.asks.remove(&price); }
                    }
                }
            }
            
            info!("Cancelled Order#{} at price {} side {:?}", order_id, price, side);
            self.on_order_cancelled(order.clone());
        } else {
            warn!("InnerOrderbook: Tried to cancel non-existent order_id {}", order_id);
        }
    }

    /// Modifies an existing order by canceling and re-adding with new parameters.
    ///
    /// If the new order crosses, matching may occur immediately.
    ///
    /// # Returns
    /// Any `Trades` produced by re-insertion.
    pub fn modify_order(&mut self, order: OrderModify) -> Trades {
        let order_type = self.orders.get(&order.get_order_id())
            .map(|entry| entry.order.lock().unwrap().get_order_type());

        if order_type.is_none() {
            warn!("InnerOrderbook: Tried to modify non-existent order_id {}", order.get_order_id());
            return vec![];
        }

        info!("InnerOrderbook: Modifying order_id {} to price {} qty {} side {:?}", order.get_order_id(), order.get_price(), order.get_quantity(), order.get_side());
        self.cancel_order(order.get_order_id());
        let trades = self.add_order(order.to_order_pointer(order_type.unwrap()));
        if !trades.is_empty() {
            info!("InnerOrderbook: Trades occurred after modify: {:?}", trades);
        }
        trades
    }

    /// Updates per-level aggregates after adds/matches/cancels.
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

    /// Hook invoked on successful cancel; updates aggregates.
    fn on_order_cancelled(&mut self, order: OrderPointer){
        let ord = order.lock().unwrap();
        self.update_level_data(ord.get_price(), ord.get_initial_quantity(), LevelDataAction::Remove)
    }

    /// Hook invoked on successful add; updates aggregates.
    fn on_order_added(&mut self, order: OrderPointer) {
        let ord = order.lock().unwrap();
        self.update_level_data(ord.get_price(), ord.get_initial_quantity(), LevelDataAction::Add)
    }

    /// Hook invoked on each match; decrements or removes level aggregates.
    fn on_order_matched(&mut self, price: Price, quantity: Quantity, is_fully_filled: bool) {
        let action = if is_fully_filled {
            LevelDataAction::Remove
        } else {
            LevelDataAction::Match
        };
        debug!("Order matched @ price {} qty {} fully_filled {}", price, quantity, is_fully_filled);
        self.update_level_data(price, quantity, action);
    }

    /// Returns `true` if a new order on `side` at `price` would cross the book.
    fn can_match(&mut self, side: Side, price: Price) -> bool {
        match side {
            Side::Buy => self.asks.first_key_value().map_or(false, |(ask, _)| price >= *ask),
            Side::Sell => self.bids.first_key_value().map_or(false, |(bid, _)| price <= *bid),
        }
    }

    /// Returns `true` if a new order can be **fully** filled immediately at/within the book.
    ///
    /// Used by FOK validation; walks level aggregates inside the crossable range.
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

    /// Removes an order from the side/price queue and fixes indices/maps.
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
            trace!("Removed Order#{} from book at price {} side {:?}", order_id, price, side);
        }
    }

    /// Central matching loop.
    ///
    /// While best bid ≥ best ask, match head-of-queue orders at those prices,
    /// create `Trade`s, update aggregates, and remove/repair queues for fully
    /// filled and partially filled F&K orders.
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

                info!("Matching bid order_id {} and ask order_id {} for quantity {}", bid.get_order_id(), ask.get_order_id(), trade_quantity);

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
                info!("Removing partially filled F&K bid order_id {}", bid_id);
                self.remove_order_from_book(bid_id, final_bid_price, Side::Buy);
            }

            if !ask_filled && ask_type == OrderType::FillAndKill {
                info!("Removing partially filled F&K ask order_id {}", ask_id);
                self.remove_order_from_book(ask_id, final_ask_price, Side::Sell);
            }
        }
        trades
    }

    /// Background loop that cancels Good-For-Day orders at a daily cutoff.
    ///
    /// Computes the next cutoff (local `end_hour`), waits on a condition variable
    /// until either the timeout or `shutdown` is signaled, and on timeout
    /// cancels all `GoodForDay` orders. When `test_mode` is `true`, performs
    /// a single prune cycle then exits (useful for tests).
    fn prune_gfd_orders(&mut self, test_mode: bool) {
        let end_hour = 16;
        info!("end_hour: {}", end_hour);

        loop {
            info!("Started Loop!");
            let now = SystemTime::now();
            let now_duration = now.duration_since(UNIX_EPOCH).unwrap();
            debug!("now_duration: {:?}", now_duration);
            let now_secs = now_duration.as_secs() as i64;
            debug!("now_secs: {}", now_secs);
            
            let now_parts = DateTime::from_timestamp(now_secs, 0).unwrap();
            debug!("now_parts: {:?}", now_parts);
            let mut date = now_parts.date_naive();
            debug!("date: {}", date);
            let hour = now_parts.hour();
            debug!("hour: {}", hour);

            debug!("Comparing hours!");
            debug!("Current hour is {}, end hour is {}", hour, end_hour);
            if hour >= end_hour {
                date = date.succ_opt().unwrap(); // move to next day
                debug!("Moved to next day, new date: {}", date);
            }

            let next_cutoff = date.and_hms_opt(end_hour, 0, 0).unwrap();
            debug!("next_cutoff: {}", next_cutoff);
            let cutoff_ts = UNIX_EPOCH + Duration::from_secs(next_cutoff.and_utc().timestamp() as u64);
            debug!("cutoff_ts: {:?}", cutoff_ts);
            let now_system_time = SystemTime::now();
            debug!("now_system_time: {:?}", now_system_time);
            
            debug!("Finding wait duration");
            let wait_duration = cutoff_ts
                .duration_since(now_system_time)
                .unwrap_or(Duration::from_secs(0)) + Duration::from_millis(100);
            debug!("wait_duration: {:?}", wait_duration);

            // Use a dummy mutex for waiting on the condition variable.
            let dummy_mutex = Mutex::new(());
            let guard = dummy_mutex.lock().unwrap();
            let (guard, result) = self.shutdown_condition_variable
                .wait_timeout(guard, wait_duration)
                .unwrap();
            
            debug!("result.timed_out(): {}", result.timed_out());
            debug!("self.shutdown: {}", self.shutdown.load(Ordering::Acquire));
            
            debug!("DEBUG: About to check shutdown condition");
            if self.shutdown.load(Ordering::Acquire) {
                info!("Shutdown requested, exiting prune_gfd_orders.");
                return;
            }

            debug!("DEBUG: About to check timeout condition");
            if !result.timed_out() {
                info!("Woke up early (not timed out), skipping pruning.");
                continue;
            }

            debug!("DEBUG: About to start pruning logic");
            
            // Pruning logic
            info!("Pruning Orders!");
            let mut order_ids = vec![];

            debug!("DEBUG: About to iterate over orders");
            for (order_id, entry) in &self.orders {
                debug!("DEBUG: Checking order {}", order_id);
                let order = entry.order.lock().unwrap();
                debug!("DEBUG: Order type: {:?}", order.get_order_type());
                if order.get_order_type() == OrderType::GoodForDay {
                    info!("DEBUG: Adding GFD order {} to cancellation list", order_id);
                    order_ids.push(*order_id);
                }
            }
            
            info!("Found {} GFD orders to cancel", order_ids.len());

            for id in order_ids {
                info!("Canceling order with id: {}", id);
                self.cancel_order(id);
            }
            
            info!("Orders left: {}", self.orders.len());

            if test_mode{
                info!("Finished pruning! test mode on");
                break;
            }
        }
    }
}

impl Drop for InnerOrderbook {
    /// Ensures the pruning thread is stopped cleanly on drop.
    ///
    /// Sets `shutdown = true`, notifies the condition variable to wake the
    /// pruner if it is sleeping, and joins the thread handle if present.
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