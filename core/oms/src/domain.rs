use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::fmt;
use uuid::Uuid;

// ============================================================================
// ORDER TYPES
// ============================================================================

/// Type of order
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Hash)]
pub enum OrderType {
    /// Limit order - execute at specified price or better
    Limit,
    // Market order - execute at best available price (future)
    // Market,
}

impl fmt::Display for OrderType {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            OrderType::Limit => write!(f, "LIMIT"),
        }
    }
}

/// Time-in-force specifies order lifetime behavior
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Hash)]
pub enum TimeInForce {
    /// Good Till Cancelled - stay in book until filled or cancelled
    GTC,
    /// Immediate or Cancel - fill what you can now, cancel rest
    IOC,
    /// Fill or Kill - fill completely now or cancel all
    FOK,
}

impl fmt::Display for TimeInForce {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            TimeInForce::GTC => write!(f, "GTC"),
            TimeInForce::IOC => write!(f, "IOC"),
            TimeInForce::FOK => write!(f, "FOK"),
        }
    }
}

/// Order side - buy or sell
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Hash)]
pub enum OrderSide {
    /// Buy order (go long)
    Buy,
    /// Sell order (go short)
    Sell,
}

impl fmt::Display for OrderSide {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            OrderSide::Buy => write!(f, "BUY"),
            OrderSide::Sell => write!(f, "SELL"),
        }
    }
}

impl OrderSide {
    /// Get the opposite side
    pub fn opposite(&self) -> Self {
        match self {
            OrderSide::Buy => OrderSide::Sell,
            OrderSide::Sell => OrderSide::Buy,
        }
    }
}

// ============================================================================
// ORDER STATUS (STATE MACHINE)
// ============================================================================

/// Order status - tracks order lifecycle
///
/// Valid transitions:
/// - PendingRisk -> Open (after risk approval)
/// - PendingRisk -> Rejected (risk rejection)
/// - Open -> PartiallyFilled (partial match)
/// - Open -> Filled (full match)
/// - Open -> Cancelled (user cancellation)
/// - Open -> Expired (time-based)
/// - PartiallyFilled -> Filled (remaining filled)
/// - PartiallyFilled -> Cancelled (user cancels partial)
/// - PartiallyFilled -> Expired (time-based)
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Hash)]
pub enum OrderStatus {
    /// Waiting for risk engine approval
    PendingRisk,
    /// Active in order book, awaiting match
    Open,
    /// Some contracts filled, remainder still open
    PartiallyFilled,
    /// All contracts filled, order complete
    Filled,
    /// Order cancelled before completion
    Cancelled,
    /// Order rejected by risk engine
    Rejected,
    /// Order expired (time-based)
    Expired,
}

impl OrderStatus {
    /// Check if transition to new status is valid
    pub fn can_transition_to(&self, new_status: OrderStatus) -> bool {
        use OrderStatus::*;

        matches!(
            (self, new_status),
            // From PendingRisk
            (PendingRisk, Open)
                | (PendingRisk, Rejected)
                // From Open
                | (Open, PartiallyFilled)
                | (Open, Filled)
                | (Open, Cancelled)
                | (Open, Expired)
                // From PartiallyFilled
                | (PartiallyFilled, Filled)
                | (PartiallyFilled, Cancelled)
                | (PartiallyFilled, Expired)
        )
    }

    /// Check if order is in terminal state
    pub fn is_terminal(&self) -> bool {
        matches!(
            self,
            OrderStatus::Filled
                | OrderStatus::Cancelled
                | OrderStatus::Rejected
                | OrderStatus::Expired
        )
    }

    /// Check if order is active (can be matched)
    pub fn is_active(&self) -> bool {
        matches!(self, OrderStatus::Open | OrderStatus::PartiallyFilled)
    }
}

impl fmt::Display for OrderStatus {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            OrderStatus::PendingRisk => write!(f, "PENDING_RISK"),
            OrderStatus::Open => write!(f, "OPEN"),
            OrderStatus::PartiallyFilled => write!(f, "PARTIALLY_FILLED"),
            OrderStatus::Filled => write!(f, "FILLED"),
            OrderStatus::Cancelled => write!(f, "CANCELLED"),
            OrderStatus::Rejected => write!(f, "REJECTED"),
            OrderStatus::Expired => write!(f, "EXPIRED"),
        }
    }
}

// ============================================================================
// ORDER (CORE ENTITY)
// ============================================================================

/// Order represents user's trading intent
///
/// CRITICAL RULES:
/// 1. Order never mutates balances or positions
/// 2. Order must pass risk check before matching
/// 3. Quantity is in contracts (integer), exposure = quantity * contract_size
/// 4. Partial fills update filled_quantity, not quantity
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Order {
    /// Unique order identifier
    pub order_id: Uuid,
    /// User who placed the order
    pub user_id: Uuid,
    /// Instrument being traded (links to OptionInstrument)
    pub instrument_id: String,
    /// Buy or Sell
    pub side: OrderSide,
    /// Limit or Market
    pub order_type: OrderType,
    /// Time-in-force behavior
    pub time_in_force: TimeInForce,
    /// Price per contract (for limit orders). None for market orders (future).
    pub price: Option<f64>,
    /// Total number of contracts requested. This NEVER changes after creation.
    pub quantity: u32,
    /// Number of contracts filled so far. Increments as fills happen.
    pub filled_quantity: u32,
    /// Average fill price (weighted by fills)
    pub avg_fill_price: Option<f64>,
    /// Current order status
    pub status: OrderStatus,
    /// When order was created
    pub created_at: DateTime<Utc>,
    /// Last status update time
    pub updated_at: DateTime<Utc>,
}

impl Order {
    /// Create a new order (starts as PendingRisk)
    pub fn new(
        user_id: Uuid,
        instrument_id: String,
        side: OrderSide,
        order_type: OrderType,
        time_in_force: TimeInForce,
        price: Option<f64>,
        quantity: u32,
    ) -> Self {
        let now = Utc::now();

        Self {
            order_id: Uuid::new_v4(),
            user_id,
            instrument_id,
            side,
            order_type,
            time_in_force,
            price,
            quantity,
            filled_quantity: 0,
            avg_fill_price: None,
            status: OrderStatus::PendingRisk,
            created_at: now,
            updated_at: now,
        }
    }

    /// Get remaining (unfilled) quantity
    pub fn remaining_quantity(&self) -> u32 {
        self.quantity.saturating_sub(self.filled_quantity)
    }

    /// Check if order is fully filled
    pub fn is_filled(&self) -> bool {
        self.filled_quantity >= self.quantity
    }

    /// Check if order has any fills
    pub fn has_fills(&self) -> bool {
        self.filled_quantity > 0
    }

    /// Update order with a fill
    ///
    /// This updates filled_quantity, avg_fill_price, and status.
    /// Does NOT update balances or positions (that's Settlement's job).
    pub fn apply_fill(&mut self, fill_quantity: u32, fill_price: f64) {
        let prev_filled = self.filled_quantity;

        // Update filled quantity
        self.filled_quantity = self.filled_quantity.saturating_add(fill_quantity);

        // Update average fill price
        if let Some(avg) = self.avg_fill_price {
            // Weighted average
            let prev_value = avg * prev_filled as f64;
            let new_value = fill_price * fill_quantity as f64;
            self.avg_fill_price = Some((prev_value + new_value) / self.filled_quantity as f64);
        } else {
            self.avg_fill_price = Some(fill_price);
        }

        // Update status
        if self.is_filled() {
            self.status = OrderStatus::Filled;
        } else if self.has_fills() && self.status == OrderStatus::Open {
            self.status = OrderStatus::PartiallyFilled;
        }

        self.updated_at = Utc::now();
    }

    /// Transition order to new status
    pub fn transition_to(&mut self, new_status: OrderStatus) -> Result<(), String> {
        if !self.status.can_transition_to(new_status) {
            return Err(format!(
                "Invalid transition from {:?} to {:?}",
                self.status, new_status
            ));
        }

        self.status = new_status;
        self.updated_at = Utc::now();
        Ok(())
    }
}

// ============================================================================
// BUILDER PATTERN
// ============================================================================

/// Builder for creating orders with validation
pub struct OrderBuilder {
    user_id: Option<Uuid>,
    instrument_id: Option<String>,
    side: Option<OrderSide>,
    order_type: Option<OrderType>,
    time_in_force: Option<TimeInForce>,
    price: Option<f64>,
    quantity: Option<u32>,
}

impl OrderBuilder {
    pub fn new() -> Self {
        Self {
            user_id: None,
            instrument_id: None,
            side: None,
            order_type: Some(OrderType::Limit),
            time_in_force: Some(TimeInForce::GTC),
            price: None,
            quantity: None,
        }
    }

    pub fn user_id(mut self, user_id: Uuid) -> Self {
        self.user_id = Some(user_id);
        self
    }

    pub fn instrument_id(mut self, instrument_id: impl Into<String>) -> Self {
        self.instrument_id = Some(instrument_id.into());
        self
    }

    pub fn side(mut self, side: OrderSide) -> Self {
        self.side = Some(side);
        self
    }

    pub fn order_type(mut self, order_type: OrderType) -> Self {
        self.order_type = Some(order_type);
        self
    }

    pub fn time_in_force(mut self, tif: TimeInForce) -> Self {
        self.time_in_force = Some(tif);
        self
    }

    pub fn price(mut self, price: f64) -> Self {
        self.price = Some(price);
        self
    }

    pub fn quantity(mut self, quantity: u32) -> Self {
        self.quantity = Some(quantity);
        self
    }

    pub fn build(self) -> Result<Order, String> {
        let user_id = self.user_id.ok_or("user_id is required")?;
        let instrument_id = self.instrument_id.ok_or("instrument_id is required")?;
        let side = self.side.ok_or("side is required")?;
        let order_type = self.order_type.unwrap_or(OrderType::Limit);
        let time_in_force = self.time_in_force.unwrap_or(TimeInForce::GTC);
        let quantity = self.quantity.ok_or("quantity is required")?;

        // Limit orders must have price
        let price = if order_type == OrderType::Limit {
            Some(self.price.ok_or("price is required for limit orders")?)
        } else {
            None
        };

        Ok(Order::new(
            user_id,
            instrument_id,
            side,
            order_type,
            time_in_force,
            price,
            quantity,
        ))
    }
}

impl Default for OrderBuilder {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_order_status_transitions_valid() {
        // PendingRisk transitions
        assert!(OrderStatus::PendingRisk.can_transition_to(OrderStatus::Open));
        assert!(OrderStatus::PendingRisk.can_transition_to(OrderStatus::Rejected));

        // Open transitions
        assert!(OrderStatus::Open.can_transition_to(OrderStatus::PartiallyFilled));
        assert!(OrderStatus::Open.can_transition_to(OrderStatus::Filled));
        assert!(OrderStatus::Open.can_transition_to(OrderStatus::Cancelled));
        assert!(OrderStatus::Open.can_transition_to(OrderStatus::Expired));

        // PartiallyFilled transitions
        assert!(OrderStatus::PartiallyFilled.can_transition_to(OrderStatus::Filled));
        assert!(OrderStatus::PartiallyFilled.can_transition_to(OrderStatus::Cancelled));
        assert!(OrderStatus::PartiallyFilled.can_transition_to(OrderStatus::Expired));
    }

    #[test]
    fn test_order_status_transitions_invalid() {
        // Terminal states can't transition
        assert!(!OrderStatus::Filled.can_transition_to(OrderStatus::Open));
        assert!(!OrderStatus::Cancelled.can_transition_to(OrderStatus::Open));
        assert!(!OrderStatus::Rejected.can_transition_to(OrderStatus::Open));
        assert!(!OrderStatus::Expired.can_transition_to(OrderStatus::Open));

        // PendingRisk can't skip to filled
        assert!(!OrderStatus::PendingRisk.can_transition_to(OrderStatus::Filled));
        assert!(!OrderStatus::PendingRisk.can_transition_to(OrderStatus::PartiallyFilled));

        // Can't go backwards
        assert!(!OrderStatus::Open.can_transition_to(OrderStatus::PendingRisk));
    }

    #[test]
    fn test_terminal_states() {
        assert!(OrderStatus::Filled.is_terminal());
        assert!(OrderStatus::Cancelled.is_terminal());
        assert!(OrderStatus::Rejected.is_terminal());
        assert!(OrderStatus::Expired.is_terminal());

        assert!(!OrderStatus::PendingRisk.is_terminal());
        assert!(!OrderStatus::Open.is_terminal());
        assert!(!OrderStatus::PartiallyFilled.is_terminal());
    }

    #[test]
    fn test_active_states() {
        assert!(OrderStatus::Open.is_active());
        assert!(OrderStatus::PartiallyFilled.is_active());

        assert!(!OrderStatus::PendingRisk.is_active());
        assert!(!OrderStatus::Filled.is_active());
        assert!(!OrderStatus::Cancelled.is_active());
    }

    #[test]
    fn test_order_builder_happy_path() {
        let order = OrderBuilder::new()
            .user_id(Uuid::new_v4())
            .instrument_id("test-instrument")
            .side(OrderSide::Buy)
            .price(100.0)
            .quantity(10)
            .build()
            .unwrap();

        assert_eq!(order.status, OrderStatus::PendingRisk);
        assert_eq!(order.quantity, 10);
        assert_eq!(order.filled_quantity, 0);
        assert_eq!(order.price, Some(100.0));
        assert_eq!(order.side, OrderSide::Buy);
        assert_eq!(order.order_type, OrderType::Limit);
        assert_eq!(order.time_in_force, TimeInForce::GTC);
    }

    #[test]
    fn test_order_builder_missing_fields() {
        // Missing user_id
        assert!(OrderBuilder::new()
            .instrument_id("test")
            .side(OrderSide::Buy)
            .price(100.0)
            .quantity(10)
            .build()
            .is_err());

        // Missing instrument_id
        assert!(OrderBuilder::new()
            .user_id(Uuid::new_v4())
            .side(OrderSide::Buy)
            .price(100.0)
            .quantity(10)
            .build()
            .is_err());

        // Missing price for limit order
        assert!(OrderBuilder::new()
            .user_id(Uuid::new_v4())
            .instrument_id("test")
            .side(OrderSide::Buy)
            .quantity(10)
            .build()
            .is_err());
    }

    #[test]
    fn test_order_apply_fill_partial() {
        let mut order = OrderBuilder::new()
            .user_id(Uuid::new_v4())
            .instrument_id("test")
            .side(OrderSide::Buy)
            .price(100.0)
            .quantity(10)
            .build()
            .unwrap();

        // Transition to Open first
        order.transition_to(OrderStatus::Open).unwrap();

        // Partial fill
        order.apply_fill(4, 100.0);
        assert_eq!(order.filled_quantity, 4);
        assert_eq!(order.remaining_quantity(), 6);
        assert_eq!(order.status, OrderStatus::PartiallyFilled);
        assert_eq!(order.avg_fill_price, Some(100.0));
    }

    #[test]
    fn test_order_apply_fill_complete() {
        let mut order = OrderBuilder::new()
            .user_id(Uuid::new_v4())
            .instrument_id("test")
            .side(OrderSide::Buy)
            .price(100.0)
            .quantity(10)
            .build()
            .unwrap();

        order.transition_to(OrderStatus::Open).unwrap();

        // Full fill
        order.apply_fill(10, 100.0);
        assert_eq!(order.filled_quantity, 10);
        assert_eq!(order.remaining_quantity(), 0);
        assert!(order.is_filled());
        assert_eq!(order.status, OrderStatus::Filled);
    }

    #[test]
    fn test_order_apply_fill_weighted_average() {
        let mut order = OrderBuilder::new()
            .user_id(Uuid::new_v4())
            .instrument_id("test")
            .side(OrderSide::Buy)
            .price(100.0)
            .quantity(10)
            .build()
            .unwrap();

        order.transition_to(OrderStatus::Open).unwrap();

        // First fill: 4 @ 100
        order.apply_fill(4, 100.0);
        assert_eq!(order.avg_fill_price, Some(100.0));

        // Second fill: 6 @ 110
        order.apply_fill(6, 110.0);
        // Weighted avg = (4*100 + 6*110) / 10 = (400 + 660) / 10 = 106
        assert_eq!(order.avg_fill_price, Some(106.0));
        assert_eq!(order.status, OrderStatus::Filled);
    }

    #[test]
    fn test_order_transition_invalid() {
        let mut order = OrderBuilder::new()
            .user_id(Uuid::new_v4())
            .instrument_id("test")
            .side(OrderSide::Buy)
            .price(100.0)
            .quantity(10)
            .build()
            .unwrap();

        // Can't go directly to Filled from PendingRisk
        assert!(order.transition_to(OrderStatus::Filled).is_err());
    }

    #[test]
    fn test_order_side_opposite() {
        assert_eq!(OrderSide::Buy.opposite(), OrderSide::Sell);
        assert_eq!(OrderSide::Sell.opposite(), OrderSide::Buy);
    }

    #[test]
    fn test_order_serialization_roundtrip() {
        let order = OrderBuilder::new()
            .user_id(Uuid::new_v4())
            .instrument_id("test-instrument")
            .side(OrderSide::Buy)
            .price(100.5)
            .quantity(5)
            .build()
            .unwrap();

        let json = serde_json::to_string(&order).unwrap();
        let deserialized: Order = serde_json::from_str(&json).unwrap();

        assert_eq!(order.order_id, deserialized.order_id);
        assert_eq!(order.quantity, deserialized.quantity);
        assert_eq!(order.price, deserialized.price);
        assert_eq!(order.side, deserialized.side);
        assert_eq!(order.status, deserialized.status);
    }

    #[test]
    fn test_order_display_formats() {
        assert_eq!(format!("{}", OrderType::Limit), "LIMIT");
        assert_eq!(format!("{}", TimeInForce::GTC), "GTC");
        assert_eq!(format!("{}", TimeInForce::IOC), "IOC");
        assert_eq!(format!("{}", TimeInForce::FOK), "FOK");
        assert_eq!(format!("{}", OrderSide::Buy), "BUY");
        assert_eq!(format!("{}", OrderSide::Sell), "SELL");
        assert_eq!(format!("{}", OrderStatus::PendingRisk), "PENDING_RISK");
        assert_eq!(format!("{}", OrderStatus::Open), "OPEN");
        assert_eq!(format!("{}", OrderStatus::PartiallyFilled), "PARTIALLY_FILLED");
        assert_eq!(format!("{}", OrderStatus::Filled), "FILLED");
        assert_eq!(format!("{}", OrderStatus::Cancelled), "CANCELLED");
        assert_eq!(format!("{}", OrderStatus::Rejected), "REJECTED");
        assert_eq!(format!("{}", OrderStatus::Expired), "EXPIRED");
    }
}
