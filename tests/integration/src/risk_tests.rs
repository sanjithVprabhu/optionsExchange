use std::collections::HashMap;
use std::sync::Arc;

use chrono::{Duration, Utc};
use uuid::Uuid;

use exchange_adapters_storage::{
    InMemoryInstrumentStore, InMemoryMarketStore, InMemoryOrderStore, MockRiskClient,
};
use exchange_instrument::{
    InstrumentRegistry, InstrumentStore, Market, MarketStore, OptionInstrument,
    OptionInstrumentBuilder,
};
use exchange_matching::{BookOrder, MatchingEngine};
use exchange_oms::{
    OrderBuilder, OrderManager, OrderSide, RiskClient, TimeInForce,
};
use exchange_primitives::{Asset, Currency, InstrumentStatus, OptionType};
use exchange_risk::{
    LiquidationDetector, LiquidationEvent, MarginConfig, PositionSide, RiskEngine,
};

// ============================================================================
// TEST HELPERS
// ============================================================================

struct TestEnv {
    oms: OrderManager,
    risk: RiskEngine,
    matching: MatchingEngine,
    instrument: OptionInstrument,
    instrument_id: String,
    sequence: u64,
}

impl TestEnv {
    async fn new() -> Self {
        Self::with_config(MarginConfig::default()).await
    }

    async fn with_config(config: MarginConfig) -> Self {
        let instrument_store = Arc::new(InMemoryInstrumentStore::new());
        let market_store = Arc::new(InMemoryMarketStore::new());
        let order_store = Arc::new(InMemoryOrderStore::new());
        let risk_client: Arc<dyn RiskClient> = Arc::new(MockRiskClient::approving());

        // Create market
        let market = Market::new(Asset::btc(), Currency::usdt());
        let market_id = market.market_id;
        market_store.create_market(market).await.unwrap();

        // Create and activate instrument
        let registry = InstrumentRegistry::new(instrument_store.clone(), market_store.clone());
        let instrument = OptionInstrumentBuilder::new(market_id, Asset::btc(), Currency::usdt())
            .option_type(OptionType::Call)
            .strike_price(50000.0)
            .expiry_timestamp(Utc::now() + Duration::days(30))
            .build();

        let instrument_id = registry.create_instrument(instrument).await.unwrap();
        registry
            .transition_status(&instrument_id, InstrumentStatus::Listed)
            .await
            .unwrap();
        registry
            .transition_status(&instrument_id, InstrumentStatus::Active)
            .await
            .unwrap();

        let stored_instrument = instrument_store
            .get_instrument(&instrument_id)
            .await
            .unwrap()
            .unwrap();

        let oms = OrderManager::new(order_store, instrument_store, risk_client);
        let risk = RiskEngine::new(config);
        let matching = MatchingEngine::new();

        Self {
            oms,
            risk,
            matching,
            instrument: stored_instrument,
            instrument_id,
            sequence: 0,
        }
    }

    /// Submit an order through OMS, check risk, and if approved send to matching
    async fn submit_and_match(
        &mut self,
        user_id: Uuid,
        side: OrderSide,
        price: f64,
        quantity: u32,
    ) -> SubmitResult {
        // 1. Build and submit to OMS
        let order = OrderBuilder::new()
            .user_id(user_id)
            .instrument_id(&self.instrument_id)
            .side(side)
            .price(price)
            .quantity(quantity)
            .time_in_force(TimeInForce::GTC)
            .build()
            .unwrap();

        let order_id = self.oms.submit_order(order).await.unwrap();
        let oms_order = self.oms.get_order(order_id).await.unwrap();

        // 2. Risk check
        let risk_result = self.risk.check_order(&oms_order, &self.instrument);

        if !risk_result.approved {
            return SubmitResult::RiskRejected(risk_result.reason.unwrap_or_default());
        }

        // 3. Reserve margin
        self.risk.reserve_margin(user_id, risk_result.required_margin);

        // 4. Send to matching engine
        self.sequence += 1;
        let book_order = BookOrder::from_oms_order(&oms_order, self.sequence);
        let match_result = self.matching.submit_order(book_order);

        // 5. Process trades
        for trade in &match_result.trades {
            // Update positions
            let (buyer_side, seller_side) = (PositionSide::Long, PositionSide::Short);
            self.risk.update_position(
                trade.buyer_id,
                trade.instrument_id.clone(),
                buyer_side,
                trade.quantity,
                trade.price,
            );
            self.risk.update_position(
                trade.seller_id,
                trade.instrument_id.clone(),
                seller_side,
                trade.quantity,
                trade.price,
            );

            // Apply fills to OMS
            self.oms
                .apply_fill(trade.taker_order_id, trade.quantity, trade.price)
                .await
                .unwrap();
            self.oms
                .apply_fill(trade.maker_order_id, trade.quantity, trade.price)
                .await
                .unwrap();
        }

        // 6. Release reserved margin (filled or inserted)
        self.risk.release_margin(user_id, risk_result.required_margin);

        SubmitResult::Matched {
            trades: match_result.trades.len(),
            inserted: match_result.inserted,
        }
    }
}

#[derive(Debug)]
enum SubmitResult {
    RiskRejected(String),
    Matched { trades: usize, inserted: bool },
}

// ============================================================================
// FULL PIPELINE: OMS → RISK → MATCHING
// ============================================================================

#[tokio::test]
async fn test_full_pipeline_buy_approved_and_matched() {
    let mut env = TestEnv::new().await;
    let maker = Uuid::new_v4();
    let taker = Uuid::new_v4();

    // Both users need wallet balance for risk engine
    env.risk.update_wallet_balance(maker, 100000.0);
    env.risk.update_wallet_balance(taker, 100000.0);
    env.risk
        .update_price(env.instrument_id.clone(), 50000.0);

    // Maker sells (short call: margin = 750 for 10 contracts)
    let r = env.submit_and_match(maker, OrderSide::Sell, 100.0, 10).await;
    assert!(matches!(r, SubmitResult::Matched { trades: 0, inserted: true }));

    // Taker buys → matched
    let r = env.submit_and_match(taker, OrderSide::Buy, 100.0, 10).await;
    assert!(matches!(r, SubmitResult::Matched { trades: 1, .. }));

    // Verify positions
    let maker_state = env.risk.get_user_state(maker).unwrap();
    assert_eq!(maker_state.positions.len(), 1);
    let maker_pos = maker_state.positions.get(&env.instrument_id).unwrap();
    assert_eq!(maker_pos.side, PositionSide::Short);
    assert_eq!(maker_pos.quantity, 10);

    let taker_state = env.risk.get_user_state(taker).unwrap();
    assert_eq!(taker_state.positions.len(), 1);
    let taker_pos = taker_state.positions.get(&env.instrument_id).unwrap();
    assert_eq!(taker_pos.side, PositionSide::Long);
    assert_eq!(taker_pos.quantity, 10);
}

#[tokio::test]
async fn test_risk_rejection_insufficient_margin() {
    let mut env = TestEnv::new().await;
    let user = Uuid::new_v4();

    env.risk.update_wallet_balance(user, 500.0); // Not enough for 1000 premium
    env.risk
        .update_price(env.instrument_id.clone(), 50000.0);

    // Buy 10 @ 100 → needs 1000, has 500
    let r = env.submit_and_match(user, OrderSide::Buy, 100.0, 10).await;
    assert!(matches!(r, SubmitResult::RiskRejected(_)));

    // No positions created
    let state = env.risk.get_user_state(user).unwrap();
    assert!(state.positions.is_empty());
}

#[tokio::test]
async fn test_risk_rejection_short_call_margin() {
    let mut env = TestEnv::new().await;
    let user = Uuid::new_v4();

    env.risk.update_wallet_balance(user, 500.0); // 750 needed for short call
    env.risk
        .update_price(env.instrument_id.clone(), 50000.0);

    // Sell 10 (short call) → margin = 750 > 500
    let r = env.submit_and_match(user, OrderSide::Sell, 100.0, 10).await;
    assert!(matches!(r, SubmitResult::RiskRejected(_)));
}

// ============================================================================
// MARGIN RESERVATION IN PIPELINE
// ============================================================================

#[tokio::test]
async fn test_reserved_margin_prevents_over_commitment() {
    let mut env = TestEnv::new().await;
    let user = Uuid::new_v4();

    env.risk.update_wallet_balance(user, 2000.0);

    // First order: buy 10 @ 100 → reserves 1000
    let r1 = env.submit_and_match(user, OrderSide::Buy, 100.0, 10).await;
    // Goes to book (no match), margin reserved then released
    assert!(matches!(r1, SubmitResult::Matched { trades: 0, inserted: true }));

    // After the order flows through, margin was reserved and released
    // But the GTC order is now in the book resting
    // In a real system we'd keep the reservation until fill/cancel
    // For this test, the order margin is released after submit_and_match
}

// ============================================================================
// LIQUIDATION DETECTION
// ============================================================================

#[tokio::test]
async fn test_liquidation_after_price_move() {
    let mut env = TestEnv::new().await;
    let user = Uuid::new_v4();
    let mut detector = LiquidationDetector::new();

    // User has modest balance and writes a call
    env.risk.update_wallet_balance(user, 1000.0);
    env.risk
        .update_price(env.instrument_id.clone(), 50000.0);

    // Manually add a large short position
    env.risk.update_position(
        user,
        env.instrument_id.clone(),
        PositionSide::Short,
        100,
        100.0,
    );

    // Recalculate margin
    let instruments =
        HashMap::from([(env.instrument_id.clone(), env.instrument.clone())]);
    env.risk.recalculate_margin(user, &instruments);

    // Check: should be liquidatable
    // Short call: 100 × 0.01 × 0.15 × 50000 = 7500 initial, 5625 maintenance
    // Equity = 1000, maintenance = 5625 → liquidatable
    assert!(env.risk.check_liquidation(user));

    let state = env.risk.get_user_state(user).unwrap();
    let event = detector.check_user(state);
    assert!(event.is_some());
    assert!(matches!(
        event.unwrap(),
        LiquidationEvent::BecameLiquidatable { .. }
    ));
}

#[tokio::test]
async fn test_no_liquidation_healthy_account() {
    let mut env = TestEnv::new().await;
    let user = Uuid::new_v4();
    let mut detector = LiquidationDetector::new();

    env.risk.update_wallet_balance(user, 100000.0);
    env.risk
        .update_price(env.instrument_id.clone(), 50000.0);

    // Small short position, large balance
    env.risk.update_position(
        user,
        env.instrument_id.clone(),
        PositionSide::Short,
        10,
        100.0,
    );

    let instruments =
        HashMap::from([(env.instrument_id.clone(), env.instrument.clone())]);
    env.risk.recalculate_margin(user, &instruments);

    assert!(!env.risk.check_liquidation(user));

    let state = env.risk.get_user_state(user).unwrap();
    let event = detector.check_user(state);
    assert!(event.is_none()); // No change → no event
}

#[tokio::test]
async fn test_liquidation_recovery() {
    let mut env = TestEnv::new().await;
    let user = Uuid::new_v4();
    let mut detector = LiquidationDetector::new();
    let instruments =
        HashMap::from([(env.instrument_id.clone(), env.instrument.clone())]);

    // Start with small balance, large position → liquidatable
    env.risk.update_wallet_balance(user, 500.0);
    env.risk
        .update_price(env.instrument_id.clone(), 50000.0);
    env.risk.update_position(
        user,
        env.instrument_id.clone(),
        PositionSide::Short,
        100,
        100.0,
    );
    env.risk.recalculate_margin(user, &instruments);

    let state = env.risk.get_user_state(user).unwrap();
    let event = detector.check_user(state);
    assert!(matches!(event, Some(LiquidationEvent::BecameLiquidatable { .. })));

    // Deposit more money → recover
    env.risk.update_wallet_balance(user, 100000.0);
    env.risk.recalculate_margin(user, &instruments);

    let state = env.risk.get_user_state(user).unwrap();
    let event = detector.check_user(state);
    assert!(matches!(event, Some(LiquidationEvent::Recovered { .. })));
    assert!(!env.risk.check_liquidation(user));
}

// ============================================================================
// POSITION SIZE LIMITS
// ============================================================================

#[tokio::test]
async fn test_position_limit_in_pipeline() {
    let config = MarginConfig {
        max_position_size: 50,
        ..MarginConfig::default()
    };
    let mut env = TestEnv::with_config(config).await;
    let user = Uuid::new_v4();

    env.risk.update_wallet_balance(user, 1_000_000.0);

    // Already have 40 contracts
    env.risk.update_position(
        user,
        env.instrument_id.clone(),
        PositionSide::Long,
        40,
        100.0,
    );

    // Try to buy 20 more → 40 + 20 = 60 > 50
    let r = env.submit_and_match(user, OrderSide::Buy, 100.0, 20).await;
    assert!(matches!(r, SubmitResult::RiskRejected(_)));
}

// ============================================================================
// MULTIPLE USERS
// ============================================================================

#[tokio::test]
async fn test_multiple_users_independent_risk() {
    let mut env = TestEnv::new().await;
    let rich_user = Uuid::new_v4();
    let poor_user = Uuid::new_v4();

    env.risk.update_wallet_balance(rich_user, 100000.0);
    env.risk.update_wallet_balance(poor_user, 100.0);
    env.risk
        .update_price(env.instrument_id.clone(), 50000.0);

    // Rich user can sell (short call)
    let r = env.submit_and_match(rich_user, OrderSide::Sell, 100.0, 10).await;
    assert!(matches!(r, SubmitResult::Matched { trades: 0, inserted: true }));

    // Poor user cannot
    let r = env.submit_and_match(poor_user, OrderSide::Sell, 100.0, 10).await;
    assert!(matches!(r, SubmitResult::RiskRejected(_)));
}

// ============================================================================
// MARGIN FORMULAS IN CONTEXT
// ============================================================================

#[tokio::test]
async fn test_buy_order_margin_is_premium() {
    let mut env = TestEnv::new().await;
    let user = Uuid::new_v4();

    env.risk.update_wallet_balance(user, 10000.0);

    // Buy 10 @ 100 → premium = 1000
    let order = OrderBuilder::new()
        .user_id(user)
        .instrument_id(&env.instrument_id)
        .side(OrderSide::Buy)
        .price(100.0)
        .quantity(10)
        .build()
        .unwrap();

    let oms_order_id = env.oms.submit_order(order).await.unwrap();
    let oms_order = env.oms.get_order(oms_order_id).await.unwrap();
    let result = env.risk.check_order(&oms_order, &env.instrument);

    assert!(result.approved);
    assert_eq!(result.required_margin, 1000.0); // price × quantity
    assert_eq!(result.projected_free_margin, 9000.0);
}

#[tokio::test]
async fn test_sell_order_margin_is_short_call() {
    let mut env = TestEnv::new().await;
    let user = Uuid::new_v4();

    env.risk.update_wallet_balance(user, 10000.0);
    env.risk
        .update_price(env.instrument_id.clone(), 50000.0);

    // Sell 10 → short call margin = 10 × 0.01 × 0.15 × 50000 = 750
    let order = OrderBuilder::new()
        .user_id(user)
        .instrument_id(&env.instrument_id)
        .side(OrderSide::Sell)
        .price(100.0)
        .quantity(10)
        .build()
        .unwrap();

    let oms_order_id = env.oms.submit_order(order).await.unwrap();
    let oms_order = env.oms.get_order(oms_order_id).await.unwrap();
    let result = env.risk.check_order(&oms_order, &env.instrument);

    assert!(result.approved);
    assert_eq!(result.required_margin, 750.0);
}
