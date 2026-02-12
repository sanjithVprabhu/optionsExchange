use std::sync::Arc;

use chrono::{Duration, Utc};
use uuid::Uuid;

use exchange_adapters_storage::{
    InMemoryInstrumentStore, InMemoryMarketStore, InMemoryOrderStore, MockRiskClient,
};
use exchange_instrument::{InstrumentRegistry, Market, MarketStore, OptionInstrumentBuilder};
use exchange_matching::{BookOrder, MatchingEngine, OrderBookSnapshot};
use exchange_oms::{
    OrderBuilder, OrderManager, OrderSide, OrderStatus, RiskClient, TimeInForce,
};
use exchange_primitives::{Asset, Currency, InstrumentStatus, OptionType};

// ============================================================================
// TEST HELPERS
// ============================================================================

struct TestEnv {
    manager: OrderManager,
    engine: MatchingEngine,
    instrument_id: String,
    sequence: u64,
}

impl TestEnv {
    /// Create a full test environment with OMS + Matching Engine
    async fn new() -> Self {
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

        // Transition to Active: Draft -> Listed -> Active
        registry
            .transition_status(&instrument_id, InstrumentStatus::Listed)
            .await
            .unwrap();
        registry
            .transition_status(&instrument_id, InstrumentStatus::Active)
            .await
            .unwrap();

        let manager = OrderManager::new(order_store, instrument_store, risk_client);
        let engine = MatchingEngine::new();

        Self {
            manager,
            engine,
            instrument_id,
            sequence: 0,
        }
    }

    /// Submit an order through OMS and convert to BookOrder
    async fn submit_order(
        &mut self,
        user_id: Uuid,
        side: OrderSide,
        price: f64,
        quantity: u32,
        tif: TimeInForce,
    ) -> BookOrder {
        let order = OrderBuilder::new()
            .user_id(user_id)
            .instrument_id(&self.instrument_id)
            .side(side)
            .price(price)
            .quantity(quantity)
            .time_in_force(tif)
            .build()
            .unwrap();

        let order_id = self.manager.submit_order(order).await.unwrap();
        let oms_order = self.manager.get_order(order_id).await.unwrap();

        // Only Open orders go to matching engine
        assert_eq!(oms_order.status, OrderStatus::Open);

        self.sequence += 1;
        BookOrder::from_oms_order(&oms_order, self.sequence)
    }
}

// ============================================================================
// OMS → MATCHING ENGINE PIPELINE
// ============================================================================

#[tokio::test]
async fn test_oms_to_matching_full_lifecycle() {
    let mut env = TestEnv::new().await;
    let maker = Uuid::new_v4();
    let taker = Uuid::new_v4();

    // Maker submits sell through OMS → matching engine
    let sell = env.submit_order(maker, OrderSide::Sell, 1000.0, 10, TimeInForce::GTC).await;
    let result = env.engine.submit_order(sell);
    assert_eq!(result.trades.len(), 0);
    assert!(result.inserted);

    // Taker submits buy through OMS → matching engine → trade!
    let buy = env.submit_order(taker, OrderSide::Buy, 1000.0, 10, TimeInForce::GTC).await;
    let result = env.engine.submit_order(buy);

    assert_eq!(result.trades.len(), 1);
    let trade = &result.trades[0];
    assert_eq!(trade.price, 1000.0);
    assert_eq!(trade.quantity, 10);
    assert_eq!(trade.buyer_id, taker);
    assert_eq!(trade.seller_id, maker);
    assert_eq!(trade.aggressor_side, OrderSide::Buy);

    // Apply fill back to OMS
    let oms_taker_order = env.manager.get_order(trade.taker_order_id).await.unwrap();
    env.manager
        .apply_fill(oms_taker_order.order_id, trade.quantity, trade.price)
        .await
        .unwrap();
    let updated = env.manager.get_order(trade.taker_order_id).await.unwrap();
    assert_eq!(updated.status, OrderStatus::Filled);

    let oms_maker_order = env.manager.get_order(trade.maker_order_id).await.unwrap();
    env.manager
        .apply_fill(oms_maker_order.order_id, trade.quantity, trade.price)
        .await
        .unwrap();
    let updated = env.manager.get_order(trade.maker_order_id).await.unwrap();
    assert_eq!(updated.status, OrderStatus::Filled);
}

#[tokio::test]
async fn test_partial_fill_lifecycle() {
    let mut env = TestEnv::new().await;
    let maker = Uuid::new_v4();
    let taker = Uuid::new_v4();

    // Maker sells 5
    let sell = env.submit_order(maker, OrderSide::Sell, 1000.0, 5, TimeInForce::GTC).await;
    env.engine.submit_order(sell);

    // Taker buys 10 → partial fill (5/10)
    let buy = env.submit_order(taker, OrderSide::Buy, 1000.0, 10, TimeInForce::GTC).await;
    let result = env.engine.submit_order(buy);

    assert_eq!(result.trades.len(), 1);
    assert_eq!(result.trades[0].quantity, 5);
    assert!(result.inserted); // GTC remainder inserted

    // Apply fill to taker → PartiallyFilled
    env.manager
        .apply_fill(result.trades[0].taker_order_id, 5, 1000.0)
        .await
        .unwrap();
    let taker_order = env.manager.get_order(result.trades[0].taker_order_id).await.unwrap();
    assert_eq!(taker_order.status, OrderStatus::PartiallyFilled);
    assert_eq!(taker_order.remaining_quantity(), 5);

    // Apply fill to maker → Filled
    env.manager
        .apply_fill(result.trades[0].maker_order_id, 5, 1000.0)
        .await
        .unwrap();
    let maker_order = env.manager.get_order(result.trades[0].maker_order_id).await.unwrap();
    assert_eq!(maker_order.status, OrderStatus::Filled);
}

#[tokio::test]
async fn test_multiple_makers_fifo() {
    let mut env = TestEnv::new().await;
    let maker_a = Uuid::new_v4();
    let maker_b = Uuid::new_v4();
    let maker_c = Uuid::new_v4();
    let taker = Uuid::new_v4();

    // Three makers sell at same price, different "times" (sequence)
    let sell_a = env.submit_order(maker_a, OrderSide::Sell, 1000.0, 10, TimeInForce::GTC).await;
    let sell_a_id = sell_a.order_id;
    env.engine.submit_order(sell_a);

    let sell_b = env.submit_order(maker_b, OrderSide::Sell, 1000.0, 10, TimeInForce::GTC).await;
    let sell_b_id = sell_b.order_id;
    env.engine.submit_order(sell_b);

    let sell_c = env.submit_order(maker_c, OrderSide::Sell, 1000.0, 10, TimeInForce::GTC).await;
    env.engine.submit_order(sell_c);

    // Taker buys 15 → should fill sell_a fully (10), sell_b partially (5)
    let buy = env.submit_order(taker, OrderSide::Buy, 1000.0, 15, TimeInForce::GTC).await;
    let result = env.engine.submit_order(buy);

    assert_eq!(result.trades.len(), 2);
    assert_eq!(result.trades[0].maker_order_id, sell_a_id); // FIFO: A first
    assert_eq!(result.trades[0].quantity, 10);
    assert_eq!(result.trades[1].maker_order_id, sell_b_id); // FIFO: B second
    assert_eq!(result.trades[1].quantity, 5);

    // Book should have 5 of B remaining + 10 of C
    let book = env.engine.get_book(&env.instrument_id).unwrap();
    assert_eq!(book.ask_order_count(), 2);
    assert_eq!(book.ask_quantity_at(1000.0), 15); // 5 + 10
}

#[tokio::test]
async fn test_price_improvement() {
    let mut env = TestEnv::new().await;
    let maker = Uuid::new_v4();
    let taker = Uuid::new_v4();

    // Maker sells at 900 (low ask)
    let sell = env.submit_order(maker, OrderSide::Sell, 900.0, 10, TimeInForce::GTC).await;
    env.engine.submit_order(sell);

    // Taker buys at 1000 (high bid) → gets price improvement at maker's 900
    let buy = env.submit_order(taker, OrderSide::Buy, 1000.0, 10, TimeInForce::GTC).await;
    let result = env.engine.submit_order(buy);

    assert_eq!(result.trades.len(), 1);
    assert_eq!(result.trades[0].price, 900.0); // Trade at MAKER's price
}

#[tokio::test]
async fn test_multi_level_matching_via_oms() {
    let mut env = TestEnv::new().await;
    let maker_a = Uuid::new_v4();
    let maker_b = Uuid::new_v4();
    let taker = Uuid::new_v4();

    // Sell 4 @ 900, Sell 7 @ 1000
    let sell_a = env.submit_order(maker_a, OrderSide::Sell, 900.0, 4, TimeInForce::GTC).await;
    env.engine.submit_order(sell_a);

    let sell_b = env.submit_order(maker_b, OrderSide::Sell, 1000.0, 7, TimeInForce::GTC).await;
    env.engine.submit_order(sell_b);

    // Buy 10 @ 1000 → matches both levels: 4 @ 900, 6 @ 1000
    let buy = env.submit_order(taker, OrderSide::Buy, 1000.0, 10, TimeInForce::GTC).await;
    let result = env.engine.submit_order(buy);

    assert_eq!(result.trades.len(), 2);
    assert_eq!(result.trades[0].price, 900.0); // Best price first
    assert_eq!(result.trades[0].quantity, 4);
    assert_eq!(result.trades[1].price, 1000.0);
    assert_eq!(result.trades[1].quantity, 6);

    assert!(result.remaining_order.is_none()); // fully filled

    // 1 ask remaining (1 @ 1000)
    let book = env.engine.get_book(&env.instrument_id).unwrap();
    assert_eq!(book.ask_quantity_at(1000.0), 1);
}

// ============================================================================
// TIME-IN-FORCE INTEGRATION
// ============================================================================

#[tokio::test]
async fn test_ioc_partial_fill_via_oms() {
    let mut env = TestEnv::new().await;
    let maker = Uuid::new_v4();
    let taker = Uuid::new_v4();

    // Maker sells 5
    let sell = env.submit_order(maker, OrderSide::Sell, 1000.0, 5, TimeInForce::GTC).await;
    env.engine.submit_order(sell);

    // IOC buy 10 → fills 5, cancels remainder (doesn't insert)
    let buy = env.submit_order(taker, OrderSide::Buy, 1000.0, 10, TimeInForce::IOC).await;
    let result = env.engine.submit_order(buy);

    assert_eq!(result.trades.len(), 1);
    assert_eq!(result.trades[0].quantity, 5);
    assert!(!result.inserted);

    // Nothing in bid book (IOC doesn't rest)
    let book = env.engine.get_book(&env.instrument_id).unwrap();
    assert_eq!(book.bid_order_count(), 0);
}

#[tokio::test]
async fn test_ioc_no_liquidity_via_oms() {
    let mut env = TestEnv::new().await;
    let taker = Uuid::new_v4();

    // IOC buy with no sellers → cancelled immediately
    let buy = env.submit_order(taker, OrderSide::Buy, 1000.0, 10, TimeInForce::IOC).await;
    let result = env.engine.submit_order(buy);

    assert_eq!(result.trades.len(), 0);
    assert!(!result.inserted);
}

#[tokio::test]
async fn test_fok_success_via_oms() {
    let mut env = TestEnv::new().await;
    let maker = Uuid::new_v4();
    let taker = Uuid::new_v4();

    // Maker sells 10
    let sell = env.submit_order(maker, OrderSide::Sell, 1000.0, 10, TimeInForce::GTC).await;
    env.engine.submit_order(sell);

    // FOK buy 10 → fills completely
    let buy = env.submit_order(taker, OrderSide::Buy, 1000.0, 10, TimeInForce::FOK).await;
    let result = env.engine.submit_order(buy);

    assert_eq!(result.trades.len(), 1);
    assert_eq!(result.trades[0].quantity, 10);
    assert!(result.remaining_order.is_none());
}

#[tokio::test]
async fn test_fok_insufficient_liquidity_via_oms() {
    let mut env = TestEnv::new().await;
    let maker = Uuid::new_v4();
    let taker = Uuid::new_v4();

    // Maker sells 5 (not enough for FOK 10)
    let sell = env.submit_order(maker, OrderSide::Sell, 1000.0, 5, TimeInForce::GTC).await;
    env.engine.submit_order(sell);

    // FOK buy 10 → rejected, book untouched
    let buy = env.submit_order(taker, OrderSide::Buy, 1000.0, 10, TimeInForce::FOK).await;
    let result = env.engine.submit_order(buy);

    assert_eq!(result.trades.len(), 0);
    assert!(!result.inserted);

    // Maker's order still in book (FOK pre-check didn't touch it)
    let book = env.engine.get_book(&env.instrument_id).unwrap();
    assert_eq!(book.ask_quantity_at(1000.0), 5);
}

// ============================================================================
// CANCEL FLOW
// ============================================================================

#[tokio::test]
async fn test_cancel_resting_order() {
    let mut env = TestEnv::new().await;
    let user = Uuid::new_v4();

    // Submit a GTC sell → rests in book
    let sell = env.submit_order(user, OrderSide::Sell, 1000.0, 10, TimeInForce::GTC).await;
    let order_id = sell.order_id;
    env.engine.submit_order(sell);

    assert_eq!(env.engine.get_book(&env.instrument_id).unwrap().ask_order_count(), 1);

    // Cancel from matching engine
    let cancelled = env.engine.cancel_order(&env.instrument_id, order_id);
    assert!(cancelled.is_some());
    assert_eq!(cancelled.unwrap().order_id, order_id);

    // Book is empty
    assert_eq!(env.engine.get_book(&env.instrument_id).unwrap().ask_order_count(), 0);

    // Cancel in OMS too
    env.manager.cancel_order(order_id, user).await.unwrap();
    let oms_order = env.manager.get_order(order_id).await.unwrap();
    assert_eq!(oms_order.status, OrderStatus::Cancelled);
}

// ============================================================================
// BOOK SNAPSHOTS
// ============================================================================

#[tokio::test]
async fn test_book_snapshot_via_oms_pipeline() {
    let mut env = TestEnv::new().await;
    let user_a = Uuid::new_v4();
    let user_b = Uuid::new_v4();

    // Build a multi-level book
    let buy1 = env.submit_order(user_a, OrderSide::Buy, 900.0, 5, TimeInForce::GTC).await;
    env.engine.submit_order(buy1);

    let buy2 = env.submit_order(user_a, OrderSide::Buy, 1000.0, 10, TimeInForce::GTC).await;
    env.engine.submit_order(buy2);

    let sell1 = env.submit_order(user_b, OrderSide::Sell, 1100.0, 8, TimeInForce::GTC).await;
    env.engine.submit_order(sell1);

    let sell2 = env.submit_order(user_b, OrderSide::Sell, 1200.0, 3, TimeInForce::GTC).await;
    env.engine.submit_order(sell2);

    // Take snapshot
    let book = env.engine.get_book(&env.instrument_id).unwrap();
    let snapshot = OrderBookSnapshot::from_book(book);

    assert_eq!(snapshot.instrument_id, env.instrument_id);

    // Bids: descending price
    assert_eq!(snapshot.bids.len(), 2);
    assert_eq!(snapshot.bids[0].price, 1000.0);
    assert_eq!(snapshot.bids[0].quantity, 10);
    assert_eq!(snapshot.bids[1].price, 900.0);
    assert_eq!(snapshot.bids[1].quantity, 5);

    // Asks: ascending price
    assert_eq!(snapshot.asks.len(), 2);
    assert_eq!(snapshot.asks[0].price, 1100.0);
    assert_eq!(snapshot.asks[0].quantity, 8);
    assert_eq!(snapshot.asks[1].price, 1200.0);
    assert_eq!(snapshot.asks[1].quantity, 3);

    // Spread
    assert_eq!(book.best_bid(), Some(1000.0));
    assert_eq!(book.best_ask(), Some(1100.0));
    assert_eq!(book.spread(), Some(100.0));
}

// ============================================================================
// DETERMINISM
// ============================================================================

#[tokio::test]
async fn test_determinism_across_oms_pipeline() {
    // Run the same order sequence twice and verify identical results
    async fn run_sequence() -> Vec<Vec<(f64, u32)>> {
        let mut env = TestEnv::new().await;
        let user_a = Uuid::from_u128(1);
        let user_b = Uuid::from_u128(2);
        let user_c = Uuid::from_u128(3);
        let mut all_results = Vec::new();

        // Order 1: sell 10 @ 1000
        let sell = env.submit_order(user_a, OrderSide::Sell, 1000.0, 10, TimeInForce::GTC).await;
        let r = env.engine.submit_order(sell);
        all_results.push(r.trades.iter().map(|t| (t.price, t.quantity)).collect());

        // Order 2: sell 5 @ 900
        let sell = env.submit_order(user_b, OrderSide::Sell, 900.0, 5, TimeInForce::GTC).await;
        let r = env.engine.submit_order(sell);
        all_results.push(r.trades.iter().map(|t| (t.price, t.quantity)).collect());

        // Order 3: buy 12 @ 1000 → matches sell @ 900 (5) and sell @ 1000 (7)
        let buy = env.submit_order(user_c, OrderSide::Buy, 1000.0, 12, TimeInForce::GTC).await;
        let r = env.engine.submit_order(buy);
        all_results.push(r.trades.iter().map(|t| (t.price, t.quantity)).collect());

        all_results
    }

    let results1 = run_sequence().await;
    let results2 = run_sequence().await;

    assert_eq!(results1, results2);
}

// ============================================================================
// SEQUENCE NUMBERS
// ============================================================================

#[tokio::test]
async fn test_trade_sequence_numbers_monotonic() {
    let mut env = TestEnv::new().await;
    let maker = Uuid::new_v4();
    let taker = Uuid::new_v4();

    // Two sells at different prices
    let sell1 = env.submit_order(maker, OrderSide::Sell, 900.0, 5, TimeInForce::GTC).await;
    env.engine.submit_order(sell1);

    let sell2 = env.submit_order(maker, OrderSide::Sell, 1000.0, 5, TimeInForce::GTC).await;
    env.engine.submit_order(sell2);

    // Buy that crosses both levels → 2 trades
    let buy = env.submit_order(taker, OrderSide::Buy, 1000.0, 10, TimeInForce::GTC).await;
    let result = env.engine.submit_order(buy);

    assert_eq!(result.trades.len(), 2);
    assert!(result.trades[0].sequence < result.trades[1].sequence);
}

// ============================================================================
// BOOK ORDER CONVERSION
// ============================================================================

#[tokio::test]
async fn test_book_order_from_oms_order() {
    let env = TestEnv::new().await;
    let user = Uuid::new_v4();

    let order = OrderBuilder::new()
        .user_id(user)
        .instrument_id(&env.instrument_id)
        .side(OrderSide::Buy)
        .price(1000.0)
        .quantity(10)
        .time_in_force(TimeInForce::GTC)
        .build()
        .unwrap();

    let order_id = env.manager.submit_order(order).await.unwrap();
    let oms_order = env.manager.get_order(order_id).await.unwrap();

    let book_order = BookOrder::from_oms_order(&oms_order, 42);

    assert_eq!(book_order.order_id, oms_order.order_id);
    assert_eq!(book_order.user_id, user);
    assert_eq!(book_order.instrument_id, env.instrument_id);
    assert_eq!(book_order.side, OrderSide::Buy);
    assert_eq!(book_order.price, 1000.0);
    assert_eq!(book_order.quantity, 10);
    assert_eq!(book_order.sequence, 42);
    assert_eq!(book_order.time_in_force, TimeInForce::GTC);
}

// ============================================================================
// SELL AGGRESSOR SCENARIOS
// ============================================================================

#[tokio::test]
async fn test_sell_aggressor_full_pipeline() {
    let mut env = TestEnv::new().await;
    let buyer = Uuid::new_v4();
    let seller = Uuid::new_v4();

    // Buyer rests a bid
    let buy = env.submit_order(buyer, OrderSide::Buy, 1000.0, 10, TimeInForce::GTC).await;
    let buy_id = buy.order_id;
    env.engine.submit_order(buy);

    // Seller aggresses
    let sell = env.submit_order(seller, OrderSide::Sell, 1000.0, 10, TimeInForce::GTC).await;
    let result = env.engine.submit_order(sell);

    assert_eq!(result.trades.len(), 1);
    let trade = &result.trades[0];
    assert_eq!(trade.maker_order_id, buy_id);
    assert_eq!(trade.buyer_id, buyer);
    assert_eq!(trade.seller_id, seller);
    assert_eq!(trade.price, 1000.0);
    assert_eq!(trade.aggressor_side, OrderSide::Sell);
}

// ============================================================================
// EDGE CASES
// ============================================================================

#[tokio::test]
async fn test_no_self_trade_different_users() {
    let mut env = TestEnv::new().await;
    let user_a = Uuid::new_v4();
    let user_b = Uuid::new_v4();

    // A sells, B buys → trade between different users
    let sell = env.submit_order(user_a, OrderSide::Sell, 1000.0, 10, TimeInForce::GTC).await;
    env.engine.submit_order(sell);

    let buy = env.submit_order(user_b, OrderSide::Buy, 1000.0, 10, TimeInForce::GTC).await;
    let result = env.engine.submit_order(buy);

    assert_eq!(result.trades.len(), 1);
    assert_ne!(result.trades[0].buyer_id, result.trades[0].seller_id);
}

#[tokio::test]
async fn test_no_crossing_different_prices() {
    let mut env = TestEnv::new().await;
    let user_a = Uuid::new_v4();
    let user_b = Uuid::new_v4();

    // Sell @ 1100
    let sell = env.submit_order(user_a, OrderSide::Sell, 1100.0, 10, TimeInForce::GTC).await;
    env.engine.submit_order(sell);

    // Buy @ 1000 → no match (buy < ask)
    let buy = env.submit_order(user_b, OrderSide::Buy, 1000.0, 10, TimeInForce::GTC).await;
    let result = env.engine.submit_order(buy);

    assert_eq!(result.trades.len(), 0);
    assert!(result.inserted); // GTC rests in book

    let book = env.engine.get_book(&env.instrument_id).unwrap();
    assert_eq!(book.bid_order_count(), 1);
    assert_eq!(book.ask_order_count(), 1);
    assert_eq!(book.spread(), Some(100.0));
}

#[tokio::test]
async fn test_multiple_fills_then_complete() {
    let mut env = TestEnv::new().await;
    let maker = Uuid::new_v4();
    let taker = Uuid::new_v4();

    // Maker sells 10
    let sell = env.submit_order(maker, OrderSide::Sell, 1000.0, 10, TimeInForce::GTC).await;
    env.engine.submit_order(sell);

    // Taker buys 3 → partial
    let buy1 = env.submit_order(taker, OrderSide::Buy, 1000.0, 3, TimeInForce::GTC).await;
    let r1 = env.engine.submit_order(buy1);
    assert_eq!(r1.trades.len(), 1);
    assert_eq!(r1.trades[0].quantity, 3);

    // Taker buys 4 → partial
    let buy2 = env.submit_order(taker, OrderSide::Buy, 1000.0, 4, TimeInForce::GTC).await;
    let r2 = env.engine.submit_order(buy2);
    assert_eq!(r2.trades.len(), 1);
    assert_eq!(r2.trades[0].quantity, 4);

    // Taker buys 3 → completes the maker
    let buy3 = env.submit_order(taker, OrderSide::Buy, 1000.0, 3, TimeInForce::GTC).await;
    let r3 = env.engine.submit_order(buy3);
    assert_eq!(r3.trades.len(), 1);
    assert_eq!(r3.trades[0].quantity, 3);

    // Book should be empty
    let book = env.engine.get_book(&env.instrument_id).unwrap();
    assert_eq!(book.ask_order_count(), 0);
    assert_eq!(book.bid_order_count(), 0);
}
