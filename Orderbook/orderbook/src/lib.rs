#![allow(unused)]
use chrono::{DateTime, Local, NaiveDateTime, TimeDelta, Timelike};
use log::{debug, error, info, trace, warn};
use serde::{Deserialize, Serialize};
use std::fmt;
use std::{
    cell::RefCell,
    collections::{btree_map::Entry, BTreeMap, HashMap},
    rc::Rc,
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc, Condvar, Mutex,
    },
    thread::{self, JoinHandle},
    time::{Duration, Instant, SystemTime, UNIX_EPOCH},
};

/// Represents the type of an order in the orderbook.
/// Determines how the order is handled regarding matching, cancellation, and expiry.
#[derive(Clone, Copy, PartialEq, Debug, Serialize)]
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

impl std::fmt::Display for OrderType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            OrderType::GoodTillCancel => write!(f, "GTC"),
            OrderType::GoodForDay => write!(f, "GFD"),
            OrderType::FillAndKill => write!(f, "F&K"),
            OrderType::FillOrKill => write!(f, "FOK"),
            OrderType::Market => write!(f, "Market"),
        }
    }
}

#[derive(Clone, Copy, PartialEq, Debug, Serialize)]
pub enum Side {
    Buy,
    Sell,
}

impl std::fmt::Display for Side {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Side::Buy => write!(f, "BUY"),
            Side::Sell => write!(f, "SELL"),
        }
    }
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
type ActorId = u32;

#[derive(Debug, Clone, Serialize)]
pub struct LevelInfo {
    pub price: Price,
    pub quantity: Quantity,
}

type LevelInfos = Vec<LevelInfo>;
#[derive(Debug, Serialize, Clone)]
pub struct OrderbookLevelInfos {
    bid_infos: LevelInfos,
    ask_infos: LevelInfos,
}

impl OrderbookLevelInfos {
    pub fn new(bids: LevelInfos, asks: LevelInfos) -> Self {
        Self {
            bid_infos: bids,
            ask_infos: asks,
        }
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
    ///Order Owner
    actor_id: ActorId,
    /// Limit/market/GTC classification for matching behavior.
    order_type: OrderType,
    /// Unique identifier assigned by the client/system.
    order_id: OrderId,
    /// Buy or Sell.
    side: Side,
    /// Limit price. For market orders created via [`Order::new_market`], this
    /// is initialized to a sentinel and may later be set by [`Order::to_good_till_cancel`].
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
    /// Creates a new **limit** order
    ///
    /// # Parameters
    /// - `order_type`: Typically `OrderType::Limit` for this constructor.
    /// - `order_id`: Unique order identifier.
    /// - `side`: Buy or Sell.
    /// - `price`: Limit price.
    /// - `quantity`: Initial total quantity.
    ///
    /// # Returns
    /// A newly created order.
    pub fn new(
        actor_id: ActorId,
        order_type: OrderType,
        order_id: OrderId,
        side: Side,
        price: Price,
        quantity: Quantity,
    ) -> Result<Self, OrderbookError> {
        if quantity == 0 {
            return Err(OrderbookError::InvalidQuantity);
        }

        if price <= 0 {
            return Err(OrderbookError::InvalidPrice);
        }

        Ok(Self {
            actor_id,
            order_type,
            order_id,
            side,
            price,
            initial_quantity: quantity,
            remaining_quantity: quantity,
            filled_quantity: 0,
            filled: false,
        })
    }

    /// Initializes `price` to a sentinel (e.g., `i32::MIN`) since market
    /// orders are price-less until optionally converted via [`Order::to_good_till_cancel`].
    pub fn new_market(
        actor_id: ActorId,
        order_id: OrderId,
        side: Side,
        quantity: Quantity,
    ) -> Result<Self, OrderbookError> {
        Self::new(
            actor_id,
            OrderType::Market,
            order_id,
            side,
            i32::MIN,
            quantity,
        )
    }

    /// Converts a **market** order into **good-till-cancel** with a concrete limit `price`.
    ///
    /// # Errors
    /// Returns an error if the order is not currently `OrderType::Market`.
    pub fn to_good_till_cancel(&mut self, price: Price) -> Result<(), String> {
        match self.get_order_type() {
            OrderType::Market => {
                self.price = price;
                self.order_type = OrderType::GoodTillCancel;
                Ok(())
            }
            _ => Err("Order cannot have its price adjusted, only market orders can.".to_string()),
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

type OrderIds = Vec<OrderId>;

/// Represents a request to modify an existing order.
///
/// `OrderModify` holds the new parameters (price, side, quantity) to
/// be applied to an existing order identified by `order_id`.
#[derive(Debug)]
pub struct OrderModify {
    ///Order Owner
    actor_id: ActorId,
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
    pub fn new(
        actor_id: ActorId,
        order_id: OrderId,
        side: Side,
        price: Price,
        quantity: Quantity,
    ) -> Self {
        Self {
            actor_id,
            order_id,
            side,
            price,
            quantity,
        }
    }

    pub const fn get_actor_id(&self) -> ActorId {
        self.actor_id
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

    /// This is typically used when re-inserting the modified order into the order book.
    ///
    /// # Parameters
    /// - `order_type`: The desired type for the new order (e.g., `OrderType::Limit`).
    pub fn to_order(&self, order_type: OrderType) -> Result<Order, OrderbookError> {
        Order::new(
            self.get_actor_id(),
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
    trade_price: Price,
    trade_quantity: Quantity,
    /// Information about the bid (buy) side of the trade.
    bid_trade: TradeInfo,
    /// Information about the ask (sell) side of the trade.
    ask_trade: TradeInfo,
    bidder_id: ActorId,
    asker_id: ActorId,
}

impl Trade {
    /// Creates a new `Trade` from the given bid and ask trade information.
    ///
    /// # Parameters
    /// - `bid_trade`: Information about the buy side of the trade.
    /// - `ask_trade`: Information about the sell side of the trade.
    pub fn new(
        trade_price: Price,
        trade_quantity: Quantity,
        bid_trade: TradeInfo,
        ask_trade: TradeInfo,
        bidder_id: ActorId,
        asker_id: ActorId,
    ) -> Self {
        Self {
            trade_price,
            trade_quantity,
            bid_trade,
            ask_trade,
            bidder_id,
            asker_id,
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

    pub const fn get_trade_price(&self) -> Price {
        self.trade_price
    }

    pub const fn get_trade_quantity(&self) -> Quantity {
        self.trade_quantity
    }

    pub const fn get_bidder_id(&self) -> ActorId {
        self.bidder_id
    }

    pub const fn get_asker_id(&self) -> ActorId {
        self.asker_id
    }
}

type Trades = Vec<Trade>;

pub struct MatchResult {
    trades: Trades,
    filled_orders: Vec<(OrderId, ActorId)>,
}

impl MatchResult {
    pub fn empty() -> Self {
        Self {
            trades: vec![],
            filled_orders: vec![],
        }
    }

    pub fn get_trades(&self) -> &Trades {
        &self.trades
    }

    pub fn get_filled_orders(&self) -> &Vec<(OrderId, ActorId)> {
        &self.filled_orders
    }
}

/// Internal record used to track an order’s position in the order book.
///
/// `OrderEntry` stores a pointer to the order itself along with its
/// cached location index, side, and price for quick lookup and updates.
#[derive(Debug)]
struct OrderEntry {
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

#[derive(Debug)]
pub enum OrderbookError {
    DuplicateOrderId(OrderId),
    InvalidPrice,
    InvalidQuantity,
    OrderUnmodifyable(OrderId),
    OrderNotFound(OrderId),
    MarketOrderNoLiquidity,
    FillOrKillNotFillable,
    FillAndKillNotMatchable,
}

impl fmt::Display for OrderbookError {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "Orderbook Error:")
    }
}
impl std::error::Error for OrderbookError {}

#[derive(Debug)]
pub struct Orderbook {
    /// Aggregated per-level stats used for FOK checks and level reporting.
    data: HashMap<Price, LevelData>,
    /// Bid book: price → FIFO of orders (best bid = highest price).
    bids: BTreeMap<Price, OrderIds>,
    /// Ask book: price → FIFO of orders (best ask = lowest price).
    asks: BTreeMap<Price, OrderIds>,
    /// Fast lookup: order id → (pointer + cached location/side/price).
    orders: HashMap<OrderId, Order>,

    order_index: HashMap<OrderId, OrderEntry>,
}

impl Default for Orderbook {
    fn default() -> Self {
        Self::new()
    }
}

impl Orderbook {
    pub fn new() -> Self {
        Self {
            bids: BTreeMap::new(),
            asks: BTreeMap::new(),
            orders: HashMap::new(),
            data: HashMap::new(),
            order_index: HashMap::new(),
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

        let create_level_infos = |price: Price, order_ids: &OrderIds| {
            let total_quantity = order_ids.iter().fold(0, |sum, id| {
                let order = self.orders.get(id).unwrap();
                sum + order.get_remaining_quantity()
            });
            LevelInfo {
                price,
                quantity: total_quantity,
            }
        };

        for (price, orders) in &self.bids {
            bid_infos.push(create_level_infos(*price, orders));
        }

        for (price, orders) in &self.asks {
            ask_infos.push(create_level_infos(*price, orders));
        }

        OrderbookLevelInfos {
            bid_infos,
            ask_infos,
        }
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
    pub fn add_order(&mut self, mut order: Order) -> Result<MatchResult, OrderbookError> {
        let order_id = order.get_order_id();

        if self.orders.contains_key(&order_id) {
            warn!("Order {} already exists", order_id);
            return Err(OrderbookError::DuplicateOrderId(order_id));
        }

        let side = order.get_side();
        let order_type = order.get_order_type();

        // --- Market → GTC ---
        if order_type == OrderType::Market {
            let result = match side {
                Side::Buy if !self.asks.is_empty() => {
                    let (worst_ask, _) = self.asks.iter().next_back().unwrap();
                    order.to_good_till_cancel(*worst_ask)
                }
                Side::Sell if !self.bids.is_empty() => {
                    let (worst_bid, _) = self.bids.iter().next().unwrap();
                    order.to_good_till_cancel(*worst_bid)
                }
                _ => return Err(OrderbookError::MarketOrderNoLiquidity),
            };
        }

        let price = order.get_price();
        let quantity = order.get_initial_quantity();

        // --- F&K ---
        if order_type == OrderType::FillAndKill && !self.can_match(side, price) {
            return Err(OrderbookError::FillAndKillNotMatchable);
        }

        // --- FOK ---
        if order_type == OrderType::FillOrKill && !self.can_fully_fill(side, price, quantity) {
            return Err(OrderbookError::FillOrKillNotFillable);
        }

        // --- INSERT ---
        self.orders.insert(order_id, order);

        let index = if side == Side::Buy {
            let queue = self.bids.entry(price).or_default();
            queue.push(order_id);
            queue.len() - 1
        } else {
            let queue = self.asks.entry(price).or_default();
            queue.push(order_id);
            queue.len() - 1
        };

        self.order_index.insert(
            order_id,
            OrderEntry {
                location: index,
                side,
                price,
            },
        );

        debug!(
            "Added {}#{} for {}/{} @ {} ({})",
            side, order_id, quantity, quantity, price, order_type
        );

        self.on_order_added(order_id);

        Ok(self.match_orders(side))
    }

    /// Cancels (removes) an order by ID, repairing queues and indices as needed.
    pub fn cancel_order(&mut self, order_id: OrderId) -> Result<(), OrderbookError> {
        let entry = match self.order_index.get(&order_id) {
            Some(e) => e,
            None => {
                warn!("Cannot cancel non existent order {}", order_id);
                return Err(OrderbookError::OrderNotFound(order_id));
            }
        };

        let price = entry.price;
        let side = entry.side;

        self.on_order_cancelled(order_id);
        self.remove_order_from_book(order_id, price, side);

        info!(
            "Cancelled Order#{} at price {} side {:?}",
            order_id, price, side
        );
        Ok(())
    }

    /// Modifies an existing order by canceling and re-adding with new parameters.
    ///
    /// If the new order crosses, matching may occur immediately.
    ///
    /// # Returns
    /// Any `Trades` produced by re-insertion.
    pub fn modify_order(&mut self, ordermod: OrderModify) -> Result<MatchResult, OrderbookError> {
        let order_id = ordermod.get_order_id();

        let order_type = match self.orders.get(&order_id) {
            Some(o) => o.get_order_type(),
            None => {
                warn!("Cannot modify non-existent order {}", order_id);
                return Err(OrderbookError::OrderNotFound(order_id));
            }
        };

        info!(
            "Modifying order_id {} to price {} qty {} side {:?}",
            order_id,
            ordermod.get_price(),
            ordermod.get_quantity(),
            ordermod.get_side()
        );
        self.cancel_order(order_id);
        self.add_order(ordermod.to_order(order_type)?)
    }

    /// Updates per-level aggregates after adds/matches/cancels.
    fn update_level_data(&mut self, price: Price, quantity: Quantity, action: LevelDataAction) {
        let data = self.data.entry(price).or_insert(LevelData {
            quantity: 0,
            count: 0,
        });

        match action {
            LevelDataAction::Remove => {
                data.count -= 1;
                data.quantity -= quantity;
            }
            LevelDataAction::Add => {
                data.count += 1;
                data.quantity += quantity;
            }
            LevelDataAction::Match => {
                data.quantity -= quantity;
            }
        }

        if data.count == 0 {
            self.data.remove(&price);
        }
    }

    /// Hook invoked on successful cancel; updates aggregates.
    fn on_order_cancelled(&mut self, order_id: OrderId) {
        if let Some(entry) = self.orders.get(&order_id) {
            self.update_level_data(
                entry.price,
                entry.remaining_quantity,
                LevelDataAction::Remove,
            )
        }
    }

    /// Hook invoked on successful add; updates aggregates.
    fn on_order_added(&mut self, order_id: OrderId) {
        if let Some(entry) = self.orders.get(&order_id) {
            self.update_level_data(entry.price, entry.remaining_quantity, LevelDataAction::Add)
        }
    }

    /// Hook invoked on each match; decrements or removes level aggregates.
    fn on_order_matched(&mut self, price: Price, quantity: Quantity, is_fully_filled: bool) {
        let action = if is_fully_filled {
            LevelDataAction::Remove
        } else {
            LevelDataAction::Match
        };
        self.update_level_data(price, quantity, action);
    }

    /// Returns `true` if a new order on `side` at `price` would cross the book.
    fn can_match(&mut self, side: Side, price: Price) -> bool {
        match side {
            Side::Buy => self
                .asks
                .first_key_value()
                .is_some_and(|(ask, _)| price >= *ask),
            Side::Sell => self
                .bids
                .last_key_value()
                .is_some_and(|(bid, _)| price <= *bid),
        }
    }

    /// Returns `true` if a new order can be **fully** filled immediately at/within the book.
    ///
    /// Used by FOK validation; walks level aggregates inside the crossable range.
    fn can_fully_fill(&self, side: Side, price: Price, mut quantity: Quantity) -> bool {
        match side {
            Side::Buy => {
                // walk asks from lowest → highest
                for (ask_price, queue) in &self.asks {
                    if *ask_price > price {
                        break;
                    }

                    for order_id in queue {
                        let order = self.orders.get(order_id).unwrap();
                        let available = order.get_remaining_quantity();

                        if quantity <= available {
                            return true;
                        }

                        quantity -= available;
                    }
                }
            }

            Side::Sell => {
                // walk bids from highest → lowest
                for (bid_price, queue) in self.bids.iter().rev() {
                    if *bid_price < price {
                        break;
                    }

                    for order_id in queue {
                        let order = self.orders.get(order_id).unwrap();
                        let available = order.get_remaining_quantity();

                        if quantity <= available {
                            return true;
                        }

                        quantity -= available;
                    }
                }
            }
        }

        false
    }

    /// Removes an order from the side/price queue and fixes indices/maps.
    fn remove_order_from_book(&mut self, order_id: OrderId, price: Price, side: Side) {
        let entry = match self.order_index.remove(&order_id) {
            Some(e) => e,
            None => return,
        };

        let queue = match side {
            Side::Buy => self.bids.get_mut(&price),
            Side::Sell => self.asks.get_mut(&price),
        };

        if let Some(queue) = queue {
            let idx = entry.location;

            queue.remove(idx);

            // update indicies, remove() shifts left

            for (i, _) in queue.iter().enumerate().skip(idx) {
                let moved_id = queue[i];

                if let Some(e) = self.order_index.get_mut(&moved_id) {
                    e.location = i
                }
            }

            if queue.is_empty() {
                match side {
                    Side::Buy => {
                        self.bids.remove(&price);
                    }
                    Side::Sell => {
                        self.asks.remove(&price);
                    }
                }
            }
        }

        self.orders.remove(&order_id);

        trace!(
            "Removed Order#{} from book at price {} side {:?}",
            order_id,
            price,
            side
        );
    }

    /// Central matching loop.
    ///
    /// While best bid ≥ best ask, match head-of-queue orders at those prices,
    /// create `Trade`s, update aggregates, and remove/repair queues for fully
    /// filled and partially filled F&K orders.
    fn match_orders(&mut self, incoming_side: Side) -> MatchResult {
        let mut trades = Vec::new();
        let mut filled_orders = Vec::new();

        loop {
            if self.bids.is_empty() || self.asks.is_empty() {
                break;
            }

            //best prices
            let bid_price = *self.bids.keys().next_back().unwrap();
            let ask_price = *self.asks.keys().next().unwrap();

            if bid_price < ask_price {
                break;
            }

            //front of queues
            let bid_id = self.bids.get(&bid_price).unwrap()[0];
            let ask_id = self.asks.get(&ask_price).unwrap()[0];

            // --- READ ONLY FIRST ---
            let (bid_remaining, ask_remaining, bid_type, ask_type, bid_actor_id, ask_actor_id);

            {
                let mut bid = self.orders.get(&bid_id).unwrap();
                let mut ask = self.orders.get(&ask_id).unwrap();

                bid_remaining = bid.get_remaining_quantity();
                ask_remaining = ask.get_remaining_quantity();

                bid_type = bid.get_order_type();
                ask_type = ask.get_order_type();

                bid_actor_id = bid.actor_id;
                ask_actor_id = ask.actor_id;
            }

            let trade_quantity = bid_remaining.min(ask_remaining);
            if trade_quantity == 0 {
                break;
            }

            // --- APPLY FILLS (separately to avoid borrow conflict) ---

            let mut bid_filled = false;
            let mut ask_filled = false;

            {
                let bid = self.orders.get_mut(&bid_id).unwrap();
                bid.fill(trade_quantity).ok();
                bid_filled = bid.is_filled()
            }

            {
                let ask = self.orders.get_mut(&ask_id).unwrap();
                ask.fill(trade_quantity).ok();
                ask_filled = ask.is_filled()
            }

            let trade_price = match incoming_side {
                Side::Buy => ask_price,
                Side::Sell => bid_price,
            };

            trades.push(Trade::new(
                trade_price,
                trade_quantity,
                TradeInfo {
                    order_id: bid_id,
                    price: bid_price,
                    quantity: trade_quantity,
                },
                TradeInfo {
                    order_id: ask_id,
                    price: ask_price,
                    quantity: trade_quantity,
                },
                bid_actor_id,
                ask_actor_id,
            ));

            self.on_order_matched(bid_price, trade_quantity, bid_filled);
            self.on_order_matched(ask_price, trade_quantity, ask_filled);
            info!(
                "Matched Bid Order #{} and Ask Order #{} @ price {} qty {}",
                bid_id, ask_id, trade_price, trade_quantity
            );

            // --- REMOVE FILLED ---
            if bid_filled {
                filled_orders.push((bid_id, bid_actor_id));
                info!("Bid Order #{} filled; removing from book...", bid_id);
                self.remove_order_from_book(bid_id, bid_price, Side::Buy);
            }

            if ask_filled {
                filled_orders.push((ask_id, ask_actor_id));
                info!("Ask Order #{} filled; removing from book...", ask_id);
                self.remove_order_from_book(ask_id, ask_price, Side::Sell);
            }

            // --- F&K CLEANUP
            if !bid_filled && bid_type == OrderType::FillAndKill {
                info!("Removing partially filled F&K bid order_id {}", bid_id);
                self.remove_order_from_book(bid_id, bid_price, Side::Buy);
            }

            if !ask_filled && ask_type == OrderType::FillAndKill {
                info!("Removing partially filled F&K ask order_id {}", ask_id);
                self.remove_order_from_book(ask_id, ask_price, Side::Sell);
            }
        }
        MatchResult {
            trades,
            filled_orders,
        }
    }

    // pub fn prune_expired(&mut self) {
    //     info!("Pruning Expired Orders...");
    //     let now = Instant::now();
    //     let orders = &self.orders;
    //     let mut expired_ids = Vec::new();

    //     for (order_id, order) in orders {
    //         if let Some(expiry) = order.expiration {
    //             if expiry <= now {
    //                 expired_ids.push(*order_id);

    //             }
    //         }
    //     }

    //     //remove expired orders
    //     for id in expired_ids {
    //         self.cancel_order(id);
    //         warn!("Pruning order with id {}", id);
    //     }

    // }
}
/// Tests:
//Each test implicitly assumes a working match_orders() functionality
#[cfg(test)]
mod test {
    use super::*;

    #[test]
    fn test_orderbook_new() {
        let orderbook = Orderbook::new();
        assert_eq!(orderbook.size(), 0)
    }

    #[test]
    fn test_orderbook_add_order() -> Result<(), Box<dyn std::error::Error>> {
        let mut orderbook = Orderbook::new();
        orderbook.add_order(Order::new(
            0,
            OrderType::GoodTillCancel,
            1,
            Side::Buy,
            100,
            10,
        )?);
        orderbook.add_order(Order::new(
            0,
            OrderType::GoodTillCancel,
            2,
            Side::Buy,
            100,
            10,
        )?);
        orderbook.add_order(Order::new(
            0,
            OrderType::GoodTillCancel,
            3,
            Side::Buy,
            100,
            10,
        )?);

        assert_eq!(orderbook.size(), 3);
        Ok(())
    }

    #[test]
    fn test_orderbook_cancel_order() -> Result<(), Box<dyn std::error::Error>> {
        let mut orderbook = Orderbook::new();

        orderbook.add_order(Order::new(
            0,
            OrderType::GoodTillCancel,
            1,
            Side::Buy,
            100,
            10,
        )?);
        orderbook.add_order(Order::new(
            0,
            OrderType::GoodTillCancel,
            2,
            Side::Buy,
            100,
            10,
        )?);
        orderbook.add_order(Order::new(
            0,
            OrderType::GoodTillCancel,
            3,
            Side::Buy,
            100,
            10,
        )?);
        orderbook.cancel_order(1);
        orderbook.cancel_order(2);
        orderbook.cancel_order(3);

        assert_eq!(orderbook.size(), 0);
        Ok(())
    }

    #[test]
    fn test_order_modify_order() -> Result<(), Box<dyn std::error::Error>> {
        let mut orderbook = Orderbook::new();
        orderbook.add_order(Order::new(
            0,
            OrderType::GoodTillCancel,
            1,
            Side::Buy,
            100,
            10,
        )?);
        orderbook.add_order(Order::new(
            0,
            OrderType::GoodTillCancel,
            2,
            Side::Buy,
            100,
            10,
        )?);

        //create modification
        let order_mod = OrderModify::new(0, 2, Side::Sell, 100, 10);

        //should match and fill order with id 1
        orderbook.modify_order(order_mod);
        assert_eq!(orderbook.size(), 0);
        Ok(())
    }

    #[test]
    fn test_orderbook_will_cancel_fnk() -> Result<(), Box<dyn std::error::Error>> {
        let mut orderbook = Orderbook::new();

        // match should completely fill
        orderbook.add_order(Order::new(
            0,
            OrderType::GoodTillCancel,
            2,
            Side::Sell,
            100,
            10,
        )?);
        orderbook.add_order(Order::new(
            0,
            OrderType::FillAndKill,
            1,
            Side::Buy,
            100,
            10,
        )?);

        //Unmatched F&K (should cancel)
        orderbook.add_order(Order::new(
            0,
            OrderType::GoodTillCancel,
            3,
            Side::Buy,
            250,
            5,
        )?);
        orderbook.add_order(Order::new(
            0,
            OrderType::FillAndKill,
            4,
            Side::Buy,
            100,
            10,
        )?);

        assert_eq!(orderbook.size(), 1);
        Ok(())
    }

    #[test]
    fn test_orderbook_will_cancel_fok() -> Result<(), Box<dyn std::error::Error>> {
        let mut orderbook = Orderbook::new();

        // Add a sell order with quantity less than the FOK buy order
        orderbook.add_order(Order::new(
            0,
            OrderType::GoodTillCancel,
            1,
            Side::Sell,
            100,
            5,
        )?);

        // Try to add a FOK buy order that requires more quantity than available (should not be added)
        orderbook.add_order(Order::new(0, OrderType::FillOrKill, 2, Side::Buy, 100, 10)?);
        assert_eq!(orderbook.size(), 1);

        // Now add enough sell quantity to fill the FOK order
        orderbook.add_order(Order::new(
            0,
            OrderType::GoodTillCancel,
            3,
            Side::Sell,
            100,
            10,
        )?);

        // Add a FOK buy order that can be fully filled (should match and remove both)
        orderbook.add_order(Order::new(0, OrderType::FillOrKill, 4, Side::Buy, 100, 10)?);
        println!("{:#?}", orderbook);
        assert_eq!(orderbook.size(), 1);
        Ok(())
    }

    #[test]
    fn test_orderbook_wont_match() -> Result<(), Box<dyn std::error::Error>> {
        let mut ob1 = Orderbook::new();
        let mut ob2 = Orderbook::new();

        //Same side
        ob1.add_order(Order::new(
            0,
            OrderType::GoodTillCancel,
            1,
            Side::Buy,
            1,
            1,
        )?);
        ob1.add_order(Order::new(
            0,
            OrderType::GoodTillCancel,
            2,
            Side::Buy,
            1,
            1,
        )?);

        //Ask higher than bid
        ob2.add_order(Order::new(
            0,
            OrderType::GoodTillCancel,
            1,
            Side::Buy,
            1,
            1,
        )?);
        ob2.add_order(Order::new(
            0,
            OrderType::GoodTillCancel,
            2,
            Side::Sell,
            2,
            1,
        )?);

        assert_eq!(ob1.size(), ob2.size());
        Ok(())
    }

    #[test]
    fn test_orderbook_can_match() -> Result<(), Box<dyn std::error::Error>> {
        //bid -> highest price buyer willing to pay
        let mut bids: BTreeMap<Price, OrderIds> = BTreeMap::new();
        //ask -> lowest price seller willing to accept
        let mut asks: BTreeMap<Price, OrderIds> = BTreeMap::new();

        bids.insert(90, vec![0, 1, 2]);
        bids.insert(100, vec![0, 1, 2]);

        asks.insert(100, vec![0, 1, 2]);
        asks.insert(95, vec![0, 1, 2]);

        let price: Price = 95;
        //-----Fix: Use the last bid (highest price) -------
        //sold if highest price willing to buy >= lowest price willing to sell
        let mut sold = bids.last_key_value().is_some_and(|(bid, _)| price <= *bid);
        assert_eq!(sold, true);

        //-----Replicate Original Bug -------
        // sold = bids.first_key_value().is_some_and(|(bid, _)| price <= *bid);
        // assert_eq!(sold, true);

        Ok(())
    }

    // #[test]
    // fn test_add_market_order() -> Result<(), Box<dyn std::error::Error>> {
    //     let mut ob = Orderbook::new();
    //     println!("Created orderbook!");

    //     ob.add_order(Order::new(
    //         0,
    //         OrderType::GoodTillCancel,
    //         1,
    //         Side::Buy,
    //         100,
    //         10,
    //     )?);
    //     ob.add_order(Order::new(
    //         0,
    //         OrderType::GoodTillCancel,
    //         2,
    //         Side::Buy,
    //         150,
    //         10,
    //     )?);
    //     // No orders can match
    //     ob.add_order(Order::new(
    //         0,
    //         OrderType::GoodTillCancel,
    //         3,
    //         Side::Sell,
    //         200,
    //         10,
    //     )?);
    //     ob.add_order(Order::new(
    //         0,
    //         OrderType::GoodTillCancel,
    //         4,
    //         Side::Sell,
    //         300,
    //         10,
    //     )?);
    //     println!("Added incompatible orders!");
    //     // Will match worst sell order (300); asks should be left with 1
    //     ob.add_order(Order::new_market(0, 5, Side::Buy, 10)?);
    //     println!("Added market order!");
    //     let level_infos = ob.get_order_infos();
    //     let asks = level_infos.get_asks();

    //     assert_eq!(asks.len(), 1);
    //     Ok(())
    // }

    // #[test]
    // fn test_good_for_day_pruning() {
    //     let mut ob = Orderbook::new();

    //     let mut order1 = Order::new(0, OrderType::GoodForDay, 1, Side::Buy, 100, 10);
    //     let mut order2 = Order::new(0, OrderType::GoodForDay, 2, Side::Buy, 100, 10);
    //     let mut order3 = Order::new(0, OrderType::GoodForDay, 3, Side::Buy, 100, 10);

    //     order1.set_expiry(Instant::now());
    //     order2.set_expiry(Instant::now());
    //     order3.set_expiry(Instant::now());

    //     ob.add_order(order1);
    //     ob.add_order(order2);
    //     ob.add_order(order3);

    //     // directly call prune
    //     ob.prune_expired();

    //     assert_eq!(ob.size(), 0);
    // }
}
