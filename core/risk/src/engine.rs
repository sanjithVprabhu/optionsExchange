use std::collections::HashMap;
use tracing::{info, warn};
use uuid::Uuid;

use exchange_instrument::OptionInstrument;
use exchange_oms::Order;

use crate::domain::*;
use crate::margin_calculator::MarginCalculator;

/// Risk Engine - The gatekeeper of the exchange.
///
/// CRITICAL RESPONSIBILITIES:
/// 1. Maintain canonical positions
/// 2. Calculate margin requirements
/// 3. Approve/reject orders BEFORE sequencing
/// 4. Detect liquidation eligibility
/// 5. Enforce exposure limits
pub struct RiskEngine {
    /// User risk states
    user_states: HashMap<Uuid, UserRiskState>,
    /// Margin calculator
    margin_calc: MarginCalculator,
    /// Configuration
    config: MarginConfig,
    /// Current prices (from market data feed)
    current_prices: HashMap<String, f64>,
}

impl RiskEngine {
    pub fn new(config: MarginConfig) -> Self {
        let margin_calc = MarginCalculator::new(config.clone());
        Self {
            user_states: HashMap::new(),
            margin_calc,
            config,
            current_prices: HashMap::new(),
        }
    }

    /// Update current price for an instrument
    pub fn update_price(&mut self, instrument_id: String, price: f64) {
        self.current_prices.insert(instrument_id, price);
    }

    /// Check if order is acceptable (CORE FUNCTION).
    ///
    /// Called by OMS BEFORE order goes to sequencer.
    /// CRITICAL: Assumes FULL FILL for worst-case analysis.
    pub fn check_order(
        &self,
        order: &Order,
        instrument: &OptionInstrument,
    ) -> RiskCheckResult {
        info!(
            order_id = %order.order_id,
            user_id = %order.user_id,
            instrument = %order.instrument_id,
            "Checking order risk"
        );

        // Get user state
        let user_state = match self.user_states.get(&order.user_id) {
            Some(state) => state,
            None => {
                return RiskCheckResult::rejected(
                    "User not registered with risk engine".to_string(),
                    0.0,
                    0.0,
                );
            }
        };

        // Get current price (fallback to strike)
        let current_price = self
            .current_prices
            .get(&order.instrument_id)
            .copied()
            .unwrap_or(instrument.strike_price);

        // Calculate required margin for FULL fill (worst case)
        let required_margin = self.margin_calc.calculate_order_margin(
            order.side,
            order.quantity,
            order.price.unwrap_or(current_price),
            instrument,
            current_price,
        );

        let free_margin = user_state.free_margin();

        // Check 1: Sufficient margin
        if required_margin.initial_margin > free_margin {
            warn!(
                order_id = %order.order_id,
                required = required_margin.initial_margin,
                free = free_margin,
                "Insufficient margin"
            );
            return RiskCheckResult::rejected(
                format!(
                    "Insufficient margin: required {:.2}, available {:.2}",
                    required_margin.initial_margin, free_margin
                ),
                required_margin.initial_margin,
                free_margin,
            );
        }

        // Check 2: Position size limits
        let current_position_qty = user_state
            .positions
            .get(&order.instrument_id)
            .map(|p| p.quantity)
            .unwrap_or(0);

        let new_position_size = current_position_qty + order.quantity;
        if new_position_size > self.config.max_position_size {
            return RiskCheckResult::rejected(
                format!(
                    "Position size limit exceeded: {} > {}",
                    new_position_size, self.config.max_position_size
                ),
                required_margin.initial_margin,
                free_margin,
            );
        }

        // Check 3: Max open positions (only for new instruments)
        if !user_state.positions.contains_key(&order.instrument_id)
            && user_state.positions.len() >= self.config.max_open_positions
        {
            return RiskCheckResult::rejected(
                format!(
                    "Max open positions exceeded: {}",
                    self.config.max_open_positions
                ),
                required_margin.initial_margin,
                free_margin,
            );
        }

        // Approved
        let projected_free = free_margin - required_margin.initial_margin;

        info!(
            order_id = %order.order_id,
            required = required_margin.initial_margin,
            free = free_margin,
            projected_free = projected_free,
            "Order approved"
        );

        RiskCheckResult::approved(required_margin.initial_margin, free_margin, projected_free)
    }

    /// Reserve margin for an accepted order (before matching).
    pub fn reserve_margin(&mut self, user_id: Uuid, amount: f64) {
        let state = self
            .user_states
            .entry(user_id)
            .or_insert_with(|| UserRiskState::new(user_id, 0.0));
        state.reserved_margin += amount;
        state.updated_at = chrono::Utc::now();
    }

    /// Release reserved margin (order cancelled or filled).
    pub fn release_margin(&mut self, user_id: Uuid, amount: f64) {
        if let Some(state) = self.user_states.get_mut(&user_id) {
            state.reserved_margin = (state.reserved_margin - amount).max(0.0);
            state.updated_at = chrono::Utc::now();
        }
    }

    /// Update position after a trade fill.
    pub fn update_position(
        &mut self,
        user_id: Uuid,
        instrument_id: String,
        side: PositionSide,
        quantity: u32,
        price: f64,
    ) {
        let state = self
            .user_states
            .entry(user_id)
            .or_insert_with(|| UserRiskState::new(user_id, 0.0));

        match state.positions.get_mut(&instrument_id) {
            Some(position) if position.side == side => {
                // Same side: increase position
                position.update_fill(quantity, price);
            }
            Some(position) => {
                // Opposite side: reduce position
                position.reduce(quantity);
                if position.is_closed() {
                    state.positions.remove(&instrument_id);
                }
            }
            None => {
                // New position
                let position =
                    Position::new(user_id, instrument_id.clone(), side, quantity, price);
                state.positions.insert(instrument_id, position);
            }
        }

        state.updated_at = chrono::Utc::now();
    }

    /// Recalculate portfolio margin for a user.
    ///
    /// Should be called periodically or after significant price moves.
    pub fn recalculate_margin(
        &mut self,
        user_id: Uuid,
        instruments: &HashMap<String, OptionInstrument>,
    ) {
        // Use split borrows: access user_states and margin_calc/current_prices separately
        let state = self
            .user_states
            .entry(user_id)
            .or_insert_with(|| UserRiskState::new(user_id, 0.0));

        let portfolio_margin = self.margin_calc.calculate_portfolio_margin(
            &state.positions,
            instruments,
            &self.current_prices,
        );

        state.total_initial_margin = portfolio_margin.initial_margin;
        state.total_maintenance_margin = portfolio_margin.maintenance_margin;
        state.updated_at = chrono::Utc::now();
    }

    /// Check if user is liquidatable.
    pub fn check_liquidation(&self, user_id: Uuid) -> bool {
        self.user_states
            .get(&user_id)
            .map(|state| state.is_liquidatable())
            .unwrap_or(false)
    }

    /// Get user risk state.
    pub fn get_user_state(&self, user_id: Uuid) -> Option<&UserRiskState> {
        self.user_states.get(&user_id)
    }

    /// Update user wallet balance.
    pub fn update_wallet_balance(&mut self, user_id: Uuid, balance: f64) {
        let state = self
            .user_states
            .entry(user_id)
            .or_insert_with(|| UserRiskState::new(user_id, 0.0));
        state.wallet_balance = balance;
        state.updated_at = chrono::Utc::now();
    }

    /// Get the margin config.
    pub fn config(&self) -> &MarginConfig {
        &self.config
    }
}

impl Default for RiskEngine {
    fn default() -> Self {
        Self::new(MarginConfig::default())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::{Duration, Utc};
    use exchange_instrument::OptionInstrumentBuilder;
    use exchange_oms::{OrderBuilder, OrderSide};
    use exchange_primitives::{Asset, Currency, OptionType};

    fn make_engine() -> RiskEngine {
        RiskEngine::new(MarginConfig::default())
    }

    fn make_call(strike: f64) -> OptionInstrument {
        OptionInstrumentBuilder::new(Uuid::new_v4(), Asset::btc(), Currency::usdt())
            .option_type(OptionType::Call)
            .strike_price(strike)
            .expiry_timestamp(Utc::now() + Duration::days(30))
            .build()
    }

    fn make_put(strike: f64) -> OptionInstrument {
        OptionInstrumentBuilder::new(Uuid::new_v4(), Asset::btc(), Currency::usdt())
            .option_type(OptionType::Put)
            .strike_price(strike)
            .expiry_timestamp(Utc::now() + Duration::days(30))
            .build()
    }

    // ========================================================================
    // ORDER APPROVAL
    // ========================================================================

    #[test]
    fn test_buy_order_approved_sufficient_margin() {
        let mut engine = make_engine();
        let user = Uuid::new_v4();
        let instrument = make_call(50000.0);

        engine.update_wallet_balance(user, 10000.0);

        // Buy 10 @ 100 → premium = 1000 (well within 10000 balance)
        let order = OrderBuilder::new()
            .user_id(user)
            .instrument_id(&instrument.instrument_id)
            .side(OrderSide::Buy)
            .price(100.0)
            .quantity(10)
            .build()
            .unwrap();

        let result = engine.check_order(&order, &instrument);
        assert!(result.approved);
        assert_eq!(result.required_margin, 1000.0);
        assert_eq!(result.free_margin, 10000.0);
        assert_eq!(result.projected_free_margin, 9000.0);
    }

    #[test]
    fn test_buy_order_rejected_insufficient_margin() {
        let mut engine = make_engine();
        let user = Uuid::new_v4();
        let instrument = make_call(50000.0);

        engine.update_wallet_balance(user, 500.0); // Not enough for 1000 premium

        let order = OrderBuilder::new()
            .user_id(user)
            .instrument_id(&instrument.instrument_id)
            .side(OrderSide::Buy)
            .price(100.0)
            .quantity(10)
            .build()
            .unwrap();

        let result = engine.check_order(&order, &instrument);
        assert!(!result.approved);
        assert!(result.reason.unwrap().contains("Insufficient margin"));
    }

    #[test]
    fn test_sell_order_approved() {
        let mut engine = make_engine();
        let user = Uuid::new_v4();
        let instrument = make_call(50000.0);

        engine.update_wallet_balance(user, 10000.0);
        engine.update_price(instrument.instrument_id.clone(), 50000.0);

        // Sell 10 (short call) → margin = 750
        let order = OrderBuilder::new()
            .user_id(user)
            .instrument_id(&instrument.instrument_id)
            .side(OrderSide::Sell)
            .price(100.0)
            .quantity(10)
            .build()
            .unwrap();

        let result = engine.check_order(&order, &instrument);
        assert!(result.approved);
        assert_eq!(result.required_margin, 750.0);
    }

    #[test]
    fn test_sell_order_rejected_insufficient() {
        let mut engine = make_engine();
        let user = Uuid::new_v4();
        let instrument = make_call(50000.0);

        engine.update_wallet_balance(user, 100.0); // Way too little
        engine.update_price(instrument.instrument_id.clone(), 50000.0);

        let order = OrderBuilder::new()
            .user_id(user)
            .instrument_id(&instrument.instrument_id)
            .side(OrderSide::Sell)
            .price(100.0)
            .quantity(10)
            .build()
            .unwrap();

        let result = engine.check_order(&order, &instrument);
        assert!(!result.approved);
    }

    #[test]
    fn test_unknown_user_rejected() {
        let engine = make_engine();
        let user = Uuid::new_v4();
        let instrument = make_call(50000.0);

        let order = OrderBuilder::new()
            .user_id(user)
            .instrument_id(&instrument.instrument_id)
            .side(OrderSide::Buy)
            .price(100.0)
            .quantity(10)
            .build()
            .unwrap();

        let result = engine.check_order(&order, &instrument);
        assert!(!result.approved);
        assert!(result.reason.unwrap().contains("not registered"));
    }

    // ========================================================================
    // MARGIN RESERVATION
    // ========================================================================

    #[test]
    fn test_reserve_and_release_margin() {
        let mut engine = make_engine();
        let user = Uuid::new_v4();
        engine.update_wallet_balance(user, 10000.0);

        engine.reserve_margin(user, 1000.0);
        let state = engine.get_user_state(user).unwrap();
        assert_eq!(state.reserved_margin, 1000.0);
        assert_eq!(state.free_margin(), 9000.0);

        engine.reserve_margin(user, 500.0);
        let state = engine.get_user_state(user).unwrap();
        assert_eq!(state.reserved_margin, 1500.0);

        engine.release_margin(user, 1500.0);
        let state = engine.get_user_state(user).unwrap();
        assert_eq!(state.reserved_margin, 0.0);
        assert_eq!(state.free_margin(), 10000.0);
    }

    #[test]
    fn test_release_margin_saturates_at_zero() {
        let mut engine = make_engine();
        let user = Uuid::new_v4();
        engine.update_wallet_balance(user, 1000.0);
        engine.reserve_margin(user, 500.0);

        // Release more than reserved
        engine.release_margin(user, 1000.0);
        let state = engine.get_user_state(user).unwrap();
        assert_eq!(state.reserved_margin, 0.0);
    }

    // ========================================================================
    // POSITION UPDATES
    // ========================================================================

    #[test]
    fn test_update_position_new() {
        let mut engine = make_engine();
        let user = Uuid::new_v4();
        engine.update_wallet_balance(user, 10000.0);

        engine.update_position(user, "BTC-CALL".to_string(), PositionSide::Long, 10, 100.0);

        let state = engine.get_user_state(user).unwrap();
        assert_eq!(state.positions.len(), 1);
        let pos = state.positions.get("BTC-CALL").unwrap();
        assert_eq!(pos.quantity, 10);
        assert_eq!(pos.avg_price, 100.0);
        assert_eq!(pos.side, PositionSide::Long);
    }

    #[test]
    fn test_update_position_increase() {
        let mut engine = make_engine();
        let user = Uuid::new_v4();
        engine.update_wallet_balance(user, 10000.0);

        engine.update_position(user, "BTC-CALL".to_string(), PositionSide::Long, 10, 100.0);
        engine.update_position(user, "BTC-CALL".to_string(), PositionSide::Long, 10, 200.0);

        let state = engine.get_user_state(user).unwrap();
        let pos = state.positions.get("BTC-CALL").unwrap();
        assert_eq!(pos.quantity, 20);
        assert_eq!(pos.avg_price, 150.0);
    }

    #[test]
    fn test_update_position_close() {
        let mut engine = make_engine();
        let user = Uuid::new_v4();
        engine.update_wallet_balance(user, 10000.0);

        engine.update_position(user, "BTC-CALL".to_string(), PositionSide::Long, 10, 100.0);
        // Opposite side closes
        engine.update_position(user, "BTC-CALL".to_string(), PositionSide::Short, 10, 150.0);

        let state = engine.get_user_state(user).unwrap();
        assert!(state.positions.is_empty()); // Fully closed, removed
    }

    // ========================================================================
    // PORTFOLIO MARGIN RECALCULATION
    // ========================================================================

    #[test]
    fn test_recalculate_margin() {
        let mut engine = make_engine();
        let user = Uuid::new_v4();
        let instrument = make_put(40000.0);

        engine.update_wallet_balance(user, 100000.0);
        engine.update_position(
            user,
            instrument.instrument_id.clone(),
            PositionSide::Short,
            10,
            50.0,
        );

        let instruments =
            HashMap::from([(instrument.instrument_id.clone(), instrument)]);
        engine.recalculate_margin(user, &instruments);

        let state = engine.get_user_state(user).unwrap();
        // Short put: 10 × 0.01 × 40000 = 4000
        assert_eq!(state.total_initial_margin, 4000.0);
        assert_eq!(state.total_maintenance_margin, 3000.0);
    }

    // ========================================================================
    // LIQUIDATION DETECTION
    // ========================================================================

    #[test]
    fn test_not_liquidatable_healthy() {
        let mut engine = make_engine();
        let user = Uuid::new_v4();
        engine.update_wallet_balance(user, 100000.0);
        assert!(!engine.check_liquidation(user));
    }

    #[test]
    fn test_liquidatable_after_margin_recalc() {
        let mut engine = make_engine();
        let user = Uuid::new_v4();
        let instrument = make_call(50000.0);

        // Small balance, large short position
        engine.update_wallet_balance(user, 500.0);
        engine.update_position(
            user,
            instrument.instrument_id.clone(),
            PositionSide::Short,
            100,
            100.0,
        );
        engine.update_price(instrument.instrument_id.clone(), 50000.0);

        let instruments =
            HashMap::from([(instrument.instrument_id.clone(), instrument)]);
        engine.recalculate_margin(user, &instruments);

        // Short call: 100 × 0.01 × 0.15 × 50000 = 7500 initial, 5625 maintenance
        // Equity = 500, maintenance = 5625 → liquidatable
        assert!(engine.check_liquidation(user));
    }

    // ========================================================================
    // POSITION SIZE LIMITS
    // ========================================================================

    #[test]
    fn test_position_size_limit() {
        let config = MarginConfig {
            max_position_size: 100,
            ..MarginConfig::default()
        };
        let mut engine = RiskEngine::new(config);
        let user = Uuid::new_v4();
        let instrument = make_call(50000.0);

        engine.update_wallet_balance(user, 1_000_000.0);

        // Already have 90 contracts
        engine.update_position(
            user,
            instrument.instrument_id.clone(),
            PositionSide::Long,
            90,
            100.0,
        );

        // Try to add 20 more (90 + 20 = 110 > 100)
        let order = OrderBuilder::new()
            .user_id(user)
            .instrument_id(&instrument.instrument_id)
            .side(OrderSide::Buy)
            .price(100.0)
            .quantity(20)
            .build()
            .unwrap();

        let result = engine.check_order(&order, &instrument);
        assert!(!result.approved);
        assert!(result.reason.unwrap().contains("Position size limit"));
    }

    // ========================================================================
    // MAX OPEN POSITIONS LIMIT
    // ========================================================================

    #[test]
    fn test_max_open_positions_limit() {
        let config = MarginConfig {
            max_open_positions: 2,
            ..MarginConfig::default()
        };
        let mut engine = RiskEngine::new(config);
        let user = Uuid::new_v4();

        engine.update_wallet_balance(user, 1_000_000.0);

        // Open 2 positions
        engine.update_position(user, "INST-A".to_string(), PositionSide::Long, 10, 100.0);
        engine.update_position(user, "INST-B".to_string(), PositionSide::Long, 10, 100.0);

        // Try to open a 3rd on new instrument
        let instrument_c = make_call(60000.0);
        let order = OrderBuilder::new()
            .user_id(user)
            .instrument_id(&instrument_c.instrument_id)
            .side(OrderSide::Buy)
            .price(100.0)
            .quantity(10)
            .build()
            .unwrap();

        let result = engine.check_order(&order, &instrument_c);
        assert!(!result.approved);
        assert!(result.reason.unwrap().contains("Max open positions"));
    }

    // ========================================================================
    // FULL ORDER LIFECYCLE
    // ========================================================================

    #[test]
    fn test_full_order_lifecycle() {
        let mut engine = make_engine();
        let user = Uuid::new_v4();
        let instrument = make_call(50000.0);

        // 1. Setup user with balance
        engine.update_wallet_balance(user, 10000.0);

        // 2. Check buy order (long call, premium = 100 × 10 = 1000)
        let order = OrderBuilder::new()
            .user_id(user)
            .instrument_id(&instrument.instrument_id)
            .side(OrderSide::Buy)
            .price(100.0)
            .quantity(10)
            .build()
            .unwrap();

        let result = engine.check_order(&order, &instrument);
        assert!(result.approved);
        assert_eq!(result.required_margin, 1000.0);

        // 3. Reserve margin
        engine.reserve_margin(user, result.required_margin);
        assert_eq!(engine.get_user_state(user).unwrap().free_margin(), 9000.0);

        // 4. Simulate fill → update position
        engine.update_position(
            user,
            instrument.instrument_id.clone(),
            PositionSide::Long,
            10,
            100.0,
        );

        // 5. Release reserved margin (filled)
        engine.release_margin(user, result.required_margin);

        // 6. Verify state
        let state = engine.get_user_state(user).unwrap();
        assert_eq!(state.positions.len(), 1);
        assert_eq!(state.reserved_margin, 0.0);
        assert_eq!(state.free_margin(), 10000.0); // Long has zero ongoing margin
    }

    #[test]
    fn test_reserved_margin_reduces_free() {
        let mut engine = make_engine();
        let user = Uuid::new_v4();
        let instrument = make_call(50000.0);

        engine.update_wallet_balance(user, 2000.0);

        // First order: buy 10 @ 100 → reserve 1000
        let order1 = OrderBuilder::new()
            .user_id(user)
            .instrument_id(&instrument.instrument_id)
            .side(OrderSide::Buy)
            .price(100.0)
            .quantity(10)
            .build()
            .unwrap();

        let r1 = engine.check_order(&order1, &instrument);
        assert!(r1.approved);
        engine.reserve_margin(user, r1.required_margin);

        // Second order: buy 15 @ 100 → needs 1500, but only 1000 free
        let order2 = OrderBuilder::new()
            .user_id(user)
            .instrument_id(&instrument.instrument_id)
            .side(OrderSide::Buy)
            .price(100.0)
            .quantity(15)
            .build()
            .unwrap();

        let r2 = engine.check_order(&order2, &instrument);
        assert!(!r2.approved);
    }
}
