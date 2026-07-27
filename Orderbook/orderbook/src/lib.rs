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

#[derive(Debug, Default)]
struct PriceLevel {
    pub order_ids: Vec<OrderId>,
    pub total_quantity: Quantity,
    pub order_count: usize,
}

impl PriceLevel {
    fn push(&mut self, order_id: OrderId, quantity: Quantity) -> usize {
        self.order_ids.push(order_id);
        self.total_quantity += quantity;
        self.order_count = self.order_ids.len();
        self.order_ids.len() - 1
    }

    fn remove(&mut self, order_index: usize, quantity: Quantity) {
        self.order_ids.remove(order_index);
        self.total_quantity -= quantity;
        self.order_count = self.order_ids.len();
    }

    fn apply_fill(&mut self, quantity: Quantity) {
        self.total_quantity -= quantity;
    }
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
    /// Bid book: price → FIFO of orders (best bid = highest price).
    bids: BTreeMap<Price, PriceLevel>,
    /// Ask book: price → FIFO of orders (best ask = lowest price).
    asks: BTreeMap<Price, PriceLevel>,
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

        for (price, level) in &self.bids {
            bid_infos.push(LevelInfo {
                price: *price,
                quantity: level.total_quantity,
            });
        }

        for (price, level) in &self.asks {
            ask_infos.push(LevelInfo {
                price: *price,
                quantity: level.total_quantity,
            });
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
            let level = self.bids.entry(price).or_default();
            level.push(order_id, quantity)
        } else {
            let level = self.asks.entry(price).or_default();
            level.push(order_id, quantity)
        };

        self.order_index.insert(
            order_id,
            OrderEntry {
                location: index,
                side,
                price,
            },
        );

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
    fn can_fully_fill(&self, side: Side, price: Price, quantity: Quantity) -> bool {
        let mut available: Quantity = 0;

        match side {
            Side::Buy => {
                for (ask_price, level) in &self.asks {
                    if *ask_price > price {
                        break;
                    }

                    available += level.total_quantity;

                    if available >= quantity {
                        return true;
                    }
                }
            }

            Side::Sell => {
                for (bid_price, level) in self.bids.iter().rev() {
                    if *bid_price < price {
                        break;
                    }

                    available += level.total_quantity;

                    if available >= quantity {
                        return true;
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

        let remaining_quantity = self.orders.get(&order_id).unwrap().get_remaining_quantity();

        let level = match side {
            Side::Buy => self.bids.get_mut(&price),
            Side::Sell => self.asks.get_mut(&price),
        };

        if let Some(level) = level {
            let idx = entry.location;

            level.remove(idx, remaining_quantity);

            // update indicies, remove() shifts left

            for i in idx..level.order_ids.len() {
                let moved_id = level.order_ids[i];

                if let Some(e) = self.order_index.get_mut(&moved_id) {
                    e.location = i
                }
            }

            if level.order_ids.is_empty() {
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
            let bid_id = self.bids.get(&bid_price).unwrap().order_ids[0];
            let ask_id = self.asks.get(&ask_price).unwrap().order_ids[0];

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
            //update aggregates
            {
                let bid_level = self.bids.get_mut(&bid_price).unwrap();
                bid_level.apply_fill(trade_quantity);
            }

            {
                let ask_level = self.asks.get_mut(&ask_price).unwrap();
                ask_level.apply_fill(trade_quantity);
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
}

/// Tests:
//Each test implicitly assumes a working match_orders() functionality
#[cfg(test)]
mod tests;
