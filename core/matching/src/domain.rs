use chrono::{DateTime, Utc};
use exchange_oms::{OrderSide, TimeInForce};
use ordered_float::OrderedFloat;
use serde::{Deserialize, Serialize};
use std::cmp::Reverse;
use std::collections::{BTreeMap, VecDeque};
use uuid::Uuid;

// ============================================================================
// TRADE (OUTPUT OF MATCHING)
// ============================================================================

/// Trade represents a matched execution between two orders.
///
/// CRITICAL: This is the atomic unit of execution.
/// Either fully recorded or not recorded at all.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct Trade {
    /// Unique trade identifier
    pub trade_id: Uuid,
    /// Instrument being traded
    pub instrument_id: String,
    /// Order that took liquidity (aggressor)
    pub taker_order_id: Uuid,
    /// Order that provided liquidity (resting)
    pub maker_order_id: Uuid,
    /// Buyer user ID
    pub buyer_id: Uuid,
    /// Seller user ID
    pub seller_id: Uuid,
    /// Execution price (ALWAYS the maker's price)
    pub price: f64,
    /// Number of contracts traded
    pub quantity: u32,
    /// Which side was the aggressor
    pub aggressor_side: OrderSide,
    /// Sequence number (for deterministic ordering)
    pub sequence: u64,
    /// When trade occurred
    pub timestamp: DateTime<Utc>,
}

impl Trade {
    pub fn new(
        instrument_id: String,
        taker_order_id: Uuid,
        maker_order_id: Uuid,
        buyer_id: Uuid,
        seller_id: Uuid,
        price: f64,
        quantity: u32,
        aggressor_side: OrderSide,
        sequence: u64,
    ) -> Self {
        Self {
            trade_id: Uuid::new_v4(),
            instrument_id,
            taker_order_id,
            maker_order_id,
            buyer_id,
            seller_id,
            price,
            quantity,
            aggressor_side,
            sequence,
            timestamp: Utc::now(),
        }
    }
}

// ============================================================================
// BOOK ORDER (Simplified order for the matching engine)
// ============================================================================

/// Order in the matching engine's order book.
///
/// This is a simplified view - the full Order lives in OMS.
/// Matching engine only needs what's required for price-time priority.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BookOrder {
    /// Order ID
    pub order_id: Uuid,
    /// User who placed order
    pub user_id: Uuid,
    /// Instrument this order belongs to
    pub instrument_id: String,
    /// Buy or Sell
    pub side: OrderSide,
    /// Price (for limit orders)
    pub price: f64,
    /// Remaining quantity to fill
    pub quantity: u32,
    /// Sequence number (determines time priority)
    pub sequence: u64,
    /// Time-in-force
    pub time_in_force: TimeInForce,
}

impl BookOrder {
    /// Create a BookOrder from an OMS Order
    pub fn from_oms_order(order: &exchange_oms::Order, sequence: u64) -> Self {
        Self {
            order_id: order.order_id,
            user_id: order.user_id,
            instrument_id: order.instrument_id.clone(),
            side: order.side,
            price: order.price.unwrap_or(0.0),
            quantity: order.remaining_quantity(),
            sequence,
            time_in_force: order.time_in_force,
        }
    }

    /// Reduce quantity after partial fill
    pub fn fill(&mut self, qty: u32) {
        self.quantity = self.quantity.saturating_sub(qty);
    }

    /// Check if order is completely filled
    pub fn is_filled(&self) -> bool {
        self.quantity == 0
    }
}

// ============================================================================
// ORDER BOOK
// ============================================================================

/// Order book for a single instrument.
///
/// CRITICAL PROPERTIES:
/// 1. Bids sorted descending (highest price first)
/// 2. Asks sorted ascending (lowest price first)
/// 3. Each price level is FIFO queue
/// 4. Deterministic iteration order (BTreeMap guarantees this)
#[derive(Debug, Clone)]
pub struct OrderBook {
    /// Instrument this book is for
    pub instrument_id: String,
    /// Buy orders: price (descending via Reverse) -> FIFO queue
    pub bids: BTreeMap<Reverse<OrderedFloat<f64>>, VecDeque<BookOrder>>,
    /// Sell orders: price (ascending) -> FIFO queue
    pub asks: BTreeMap<OrderedFloat<f64>, VecDeque<BookOrder>>,
    /// Sequence counter for this book
    pub sequence: u64,
}

impl OrderBook {
    pub fn new(instrument_id: String) -> Self {
        Self {
            instrument_id,
            bids: BTreeMap::new(),
            asks: BTreeMap::new(),
            sequence: 0,
        }
    }

    /// Get best bid price (highest buy)
    pub fn best_bid(&self) -> Option<f64> {
        self.bids.keys().next().map(|k| k.0.into_inner())
    }

    /// Get best ask price (lowest sell)
    pub fn best_ask(&self) -> Option<f64> {
        self.asks.keys().next().map(|k| k.into_inner())
    }

    /// Get spread (ask - bid)
    pub fn spread(&self) -> Option<f64> {
        match (self.best_bid(), self.best_ask()) {
            (Some(bid), Some(ask)) => Some(ask - bid),
            _ => None,
        }
    }

    /// Get total bid quantity at a price level
    pub fn bid_quantity_at(&self, price: f64) -> u32 {
        self.bids
            .get(&Reverse(OrderedFloat(price)))
            .map(|orders| orders.iter().map(|o| o.quantity).sum())
            .unwrap_or(0)
    }

    /// Get total ask quantity at a price level
    pub fn ask_quantity_at(&self, price: f64) -> u32 {
        self.asks
            .get(&OrderedFloat(price))
            .map(|orders| orders.iter().map(|o| o.quantity).sum())
            .unwrap_or(0)
    }

    /// Total number of bid orders across all price levels
    pub fn bid_order_count(&self) -> usize {
        self.bids.values().map(|q| q.len()).sum()
    }

    /// Total number of ask orders across all price levels
    pub fn ask_order_count(&self) -> usize {
        self.asks.values().map(|q| q.len()).sum()
    }

    /// Insert order into the book
    pub fn insert_order(&mut self, order: BookOrder) {
        match order.side {
            OrderSide::Buy => {
                self.bids
                    .entry(Reverse(OrderedFloat(order.price)))
                    .or_default()
                    .push_back(order);
            }
            OrderSide::Sell => {
                self.asks
                    .entry(OrderedFloat(order.price))
                    .or_default()
                    .push_back(order);
            }
        }
    }

    /// Remove an order by ID (for cancellations)
    pub fn remove_order(&mut self, order_id: Uuid) -> Option<BookOrder> {
        // Search bids
        for queue in self.bids.values_mut() {
            if let Some(pos) = queue.iter().position(|o| o.order_id == order_id) {
                let removed = queue.remove(pos);
                return removed;
            }
        }

        // Search asks
        for queue in self.asks.values_mut() {
            if let Some(pos) = queue.iter().position(|o| o.order_id == order_id) {
                let removed = queue.remove(pos);
                return removed;
            }
        }

        None
    }

    /// Clean up empty price levels
    pub fn cleanup_empty_levels(&mut self) {
        self.bids.retain(|_, queue| !queue.is_empty());
        self.asks.retain(|_, queue| !queue.is_empty());
    }

    /// Calculate total available ask quantity at or below a given price
    /// (used for FOK pre-check on buy orders)
    pub fn available_ask_quantity_at_or_below(&self, max_price: f64) -> u32 {
        self.asks
            .iter()
            .take_while(|(price, _)| price.into_inner() <= max_price)
            .flat_map(|(_, orders)| orders.iter())
            .map(|o| o.quantity)
            .sum()
    }

    /// Calculate total available bid quantity at or above a given price
    /// (used for FOK pre-check on sell orders)
    pub fn available_bid_quantity_at_or_above(&self, min_price: f64) -> u32 {
        self.bids
            .iter()
            .take_while(|(price, _)| price.0.into_inner() >= min_price)
            .flat_map(|(_, orders)| orders.iter())
            .map(|o| o.quantity)
            .sum()
    }
}

// ============================================================================
// MATCH RESULT
// ============================================================================

/// Result of a matching operation
#[derive(Debug, Clone)]
pub struct MatchResult {
    /// Trades generated during matching
    pub trades: Vec<Trade>,
    /// Remaining order (if not fully filled)
    pub remaining_order: Option<BookOrder>,
    /// Whether the remainder was inserted into the book
    pub inserted: bool,
}

impl MatchResult {
    pub fn no_match(order: BookOrder, inserted: bool) -> Self {
        Self {
            trades: vec![],
            remaining_order: Some(order),
            inserted,
        }
    }

    pub fn fully_matched(trades: Vec<Trade>) -> Self {
        Self {
            trades,
            remaining_order: None,
            inserted: false,
        }
    }

    pub fn partial_match(trades: Vec<Trade>, remaining: BookOrder, inserted: bool) -> Self {
        Self {
            trades,
            remaining_order: Some(remaining),
            inserted,
        }
    }

    pub fn cancelled(order: BookOrder) -> Self {
        Self {
            trades: vec![],
            remaining_order: Some(order),
            inserted: false,
        }
    }
}

// ============================================================================
// ORDER BOOK SNAPSHOT (for market data)
// ============================================================================

/// Aggregated price level for market data
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct PriceLevel {
    pub price: f64,
    pub quantity: u32,
    pub order_count: usize,
}

/// Order book snapshot for market data feeds
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OrderBookSnapshot {
    pub instrument_id: String,
    pub bids: Vec<PriceLevel>,
    pub asks: Vec<PriceLevel>,
    pub sequence: u64,
    pub timestamp: DateTime<Utc>,
}

impl OrderBookSnapshot {
    /// Create a snapshot from a live order book
    pub fn from_book(book: &OrderBook) -> Self {
        let bids: Vec<PriceLevel> = book
            .bids
            .iter()
            .map(|(price, orders)| PriceLevel {
                price: price.0.into_inner(),
                quantity: orders.iter().map(|o| o.quantity).sum(),
                order_count: orders.len(),
            })
            .collect();

        let asks: Vec<PriceLevel> = book
            .asks
            .iter()
            .map(|(price, orders)| PriceLevel {
                price: price.into_inner(),
                quantity: orders.iter().map(|o| o.quantity).sum(),
                order_count: orders.len(),
            })
            .collect();

        Self {
            instrument_id: book.instrument_id.clone(),
            bids,
            asks,
            sequence: book.sequence,
            timestamp: Utc::now(),
        }
    }
}
