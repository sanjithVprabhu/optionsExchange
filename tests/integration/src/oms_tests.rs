use std::sync::Arc;

use chrono::{Duration, Utc};
use uuid::Uuid;

use exchange_adapters_storage::{
    InMemoryInstrumentStore, InMemoryMarketStore, InMemoryOrderStore, MockRiskClient,
};
use exchange_instrument::{InstrumentRegistry, Market, MarketStore, OptionInstrumentBuilder};
use exchange_oms::{
    Order, OrderBuilder, OrderError, OrderManager, OrderSide, OrderStatus,
    OrderValidationConfig, RiskClient, TimeInForce,
};
use exchange_primitives::{Asset, Currency, InstrumentStatus, OptionType};

// ============================================================================
// TEST HELPERS
// ============================================================================

/// Creates a full test setup: instrument store with a market and active instrument,
/// order store, and order manager with the given risk client.
async fn setup_test_env(
    risk_client: Arc<dyn RiskClient>,
) -> (OrderManager, String, Uuid, Uuid) {
    let instrument_store = Arc::new(InMemoryInstrumentStore::new());
    let market_store = Arc::new(InMemoryMarketStore::new());
    let order_store = Arc::new(InMemoryOrderStore::new());

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

    let user_id = Uuid::new_v4();
    (manager, instrument_id, market_id, user_id)
}

/// Creates a simple test setup with an approving risk client.
async fn setup_approving() -> (OrderManager, String, Uuid, Uuid) {
    setup_test_env(Arc::new(MockRiskClient::approving())).await
}

/// Creates a simple test setup with a rejecting risk client.
async fn setup_rejecting() -> (OrderManager, String, Uuid, Uuid) {
    setup_test_env(Arc::new(MockRiskClient::rejecting("Insufficient margin"))).await
}

/// Build a standard test order.
fn make_order(user_id: Uuid, instrument_id: &str, price: f64, quantity: u32) -> Order {
    OrderBuilder::new()
        .user_id(user_id)
        .instrument_id(instrument_id)
        .side(OrderSide::Buy)
        .price(price)
        .quantity(quantity)
        .build()
        .unwrap()
}

// ============================================================================
// HAPPY PATH TESTS
// ============================================================================

#[tokio::test]
async fn test_submit_order_happy_path() {
    let (manager, instrument_id, _, user_id) = setup_approving().await;

    let order = make_order(user_id, &instrument_id, 100.0, 10);
    let order_id = manager.submit_order(order).await.unwrap();

    let retrieved = manager.get_order(order_id).await.unwrap();
    assert_eq!(retrieved.status, OrderStatus::Open);
    assert_eq!(retrieved.quantity, 10);
    assert_eq!(retrieved.filled_quantity, 0);
    assert_eq!(retrieved.price, Some(100.0));
    assert_eq!(retrieved.side, OrderSide::Buy);
}

#[tokio::test]
async fn test_submit_sell_order() {
    let (manager, instrument_id, _, user_id) = setup_approving().await;

    let order = OrderBuilder::new()
        .user_id(user_id)
        .instrument_id(&instrument_id)
        .side(OrderSide::Sell)
        .price(150.0)
        .quantity(5)
        .build()
        .unwrap();

    let order_id = manager.submit_order(order).await.unwrap();
    let retrieved = manager.get_order(order_id).await.unwrap();
    assert_eq!(retrieved.side, OrderSide::Sell);
    assert_eq!(retrieved.status, OrderStatus::Open);
}

#[tokio::test]
async fn test_submit_order_with_ioc() {
    let (manager, instrument_id, _, user_id) = setup_approving().await;

    let order = OrderBuilder::new()
        .user_id(user_id)
        .instrument_id(&instrument_id)
        .side(OrderSide::Buy)
        .time_in_force(TimeInForce::IOC)
        .price(100.0)
        .quantity(10)
        .build()
        .unwrap();

    let order_id = manager.submit_order(order).await.unwrap();
    let retrieved = manager.get_order(order_id).await.unwrap();
    assert_eq!(retrieved.time_in_force, TimeInForce::IOC);
}

#[tokio::test]
async fn test_submit_order_with_fok() {
    let (manager, instrument_id, _, user_id) = setup_approving().await;

    let order = OrderBuilder::new()
        .user_id(user_id)
        .instrument_id(&instrument_id)
        .side(OrderSide::Buy)
        .time_in_force(TimeInForce::FOK)
        .price(100.0)
        .quantity(10)
        .build()
        .unwrap();

    let order_id = manager.submit_order(order).await.unwrap();
    let retrieved = manager.get_order(order_id).await.unwrap();
    assert_eq!(retrieved.time_in_force, TimeInForce::FOK);
}

// ============================================================================
// RISK ENGINE TESTS
// ============================================================================

#[tokio::test]
async fn test_risk_rejection() {
    let (manager, instrument_id, _, user_id) = setup_rejecting().await;

    let order = make_order(user_id, &instrument_id, 100.0, 10);
    let order_id = manager.submit_order(order).await.unwrap();

    let retrieved = manager.get_order(order_id).await.unwrap();
    assert_eq!(retrieved.status, OrderStatus::Rejected);
}

#[tokio::test]
async fn test_rejected_order_is_terminal() {
    let (manager, instrument_id, _, user_id) = setup_rejecting().await;

    let order = make_order(user_id, &instrument_id, 100.0, 10);
    let order_id = manager.submit_order(order).await.unwrap();

    let retrieved = manager.get_order(order_id).await.unwrap();
    assert!(retrieved.status.is_terminal());
    assert!(!retrieved.status.is_active());
}

// ============================================================================
// ORDER LIFECYCLE TESTS (FILLS)
// ============================================================================

#[tokio::test]
async fn test_full_lifecycle_partial_then_complete() {
    let (manager, instrument_id, _, user_id) = setup_approving().await;

    let order = make_order(user_id, &instrument_id, 100.0, 10);
    let order_id = manager.submit_order(order).await.unwrap();

    // Verify Open
    let retrieved = manager.get_order(order_id).await.unwrap();
    assert_eq!(retrieved.status, OrderStatus::Open);

    // Partial fill: 4 out of 10
    manager.apply_fill(order_id, 4, 100.0).await.unwrap();
    let retrieved = manager.get_order(order_id).await.unwrap();
    assert_eq!(retrieved.status, OrderStatus::PartiallyFilled);
    assert_eq!(retrieved.filled_quantity, 4);
    assert_eq!(retrieved.remaining_quantity(), 6);
    assert_eq!(retrieved.avg_fill_price, Some(100.0));

    // Complete fill: remaining 6
    manager.apply_fill(order_id, 6, 100.0).await.unwrap();
    let retrieved = manager.get_order(order_id).await.unwrap();
    assert_eq!(retrieved.status, OrderStatus::Filled);
    assert_eq!(retrieved.filled_quantity, 10);
    assert_eq!(retrieved.remaining_quantity(), 0);
    assert!(retrieved.is_filled());
}

#[tokio::test]
async fn test_immediate_full_fill() {
    let (manager, instrument_id, _, user_id) = setup_approving().await;

    let order = make_order(user_id, &instrument_id, 100.0, 10);
    let order_id = manager.submit_order(order).await.unwrap();

    // Full fill in one go
    manager.apply_fill(order_id, 10, 100.0).await.unwrap();
    let retrieved = manager.get_order(order_id).await.unwrap();
    assert_eq!(retrieved.status, OrderStatus::Filled);
    assert!(retrieved.status.is_terminal());
}

#[tokio::test]
async fn test_multiple_partial_fills() {
    let (manager, instrument_id, _, user_id) = setup_approving().await;

    let order = make_order(user_id, &instrument_id, 100.0, 100);
    let order_id = manager.submit_order(order).await.unwrap();

    // Fill in several chunks
    manager.apply_fill(order_id, 20, 100.0).await.unwrap();
    let r = manager.get_order(order_id).await.unwrap();
    assert_eq!(r.status, OrderStatus::PartiallyFilled);
    assert_eq!(r.filled_quantity, 20);

    manager.apply_fill(order_id, 30, 100.0).await.unwrap();
    let r = manager.get_order(order_id).await.unwrap();
    assert_eq!(r.status, OrderStatus::PartiallyFilled);
    assert_eq!(r.filled_quantity, 50);

    manager.apply_fill(order_id, 50, 100.0).await.unwrap();
    let r = manager.get_order(order_id).await.unwrap();
    assert_eq!(r.status, OrderStatus::Filled);
    assert_eq!(r.filled_quantity, 100);
}

#[tokio::test]
async fn test_weighted_average_fill_price() {
    let (manager, instrument_id, _, user_id) = setup_approving().await;

    let order = make_order(user_id, &instrument_id, 120.0, 10);
    let order_id = manager.submit_order(order).await.unwrap();

    // Fill 4 @ 100.0
    manager.apply_fill(order_id, 4, 100.0).await.unwrap();
    let r = manager.get_order(order_id).await.unwrap();
    assert_eq!(r.avg_fill_price, Some(100.0));

    // Fill 6 @ 110.0
    // Weighted avg = (4*100 + 6*110) / 10 = (400 + 660) / 10 = 106.0
    manager.apply_fill(order_id, 6, 110.0).await.unwrap();
    let r = manager.get_order(order_id).await.unwrap();
    assert_eq!(r.avg_fill_price, Some(106.0));
}

// ============================================================================
// CANCELLATION TESTS
// ============================================================================

#[tokio::test]
async fn test_cancel_open_order() {
    let (manager, instrument_id, _, user_id) = setup_approving().await;

    let order = make_order(user_id, &instrument_id, 100.0, 10);
    let order_id = manager.submit_order(order).await.unwrap();

    // Cancel
    manager.cancel_order(order_id, user_id).await.unwrap();

    let retrieved = manager.get_order(order_id).await.unwrap();
    assert_eq!(retrieved.status, OrderStatus::Cancelled);
    assert!(retrieved.status.is_terminal());
}

#[tokio::test]
async fn test_cancel_partially_filled_order() {
    let (manager, instrument_id, _, user_id) = setup_approving().await;

    let order = make_order(user_id, &instrument_id, 100.0, 10);
    let order_id = manager.submit_order(order).await.unwrap();

    // Partial fill
    manager.apply_fill(order_id, 4, 100.0).await.unwrap();

    // Cancel the remaining 6
    manager.cancel_order(order_id, user_id).await.unwrap();

    let retrieved = manager.get_order(order_id).await.unwrap();
    assert_eq!(retrieved.status, OrderStatus::Cancelled);
    assert_eq!(retrieved.filled_quantity, 4); // fills preserved
}

#[tokio::test]
async fn test_cancel_wrong_user() {
    let (manager, instrument_id, _, user_id) = setup_approving().await;

    let order = make_order(user_id, &instrument_id, 100.0, 10);
    let order_id = manager.submit_order(order).await.unwrap();

    // Another user tries to cancel
    let other_user = Uuid::new_v4();
    let result = manager.cancel_order(order_id, other_user).await;
    assert!(matches!(result, Err(OrderError::NotAuthorized(_))));
}

#[tokio::test]
async fn test_cancel_filled_order_fails() {
    let (manager, instrument_id, _, user_id) = setup_approving().await;

    let order = make_order(user_id, &instrument_id, 100.0, 10);
    let order_id = manager.submit_order(order).await.unwrap();

    // Fill completely
    manager.apply_fill(order_id, 10, 100.0).await.unwrap();

    // Can't cancel a filled order
    let result = manager.cancel_order(order_id, user_id).await;
    assert!(matches!(result, Err(OrderError::InvalidTransition { .. })));
}

#[tokio::test]
async fn test_cancel_rejected_order_fails() {
    let (manager, instrument_id, _, user_id) = setup_rejecting().await;

    let order = make_order(user_id, &instrument_id, 100.0, 10);
    let order_id = manager.submit_order(order).await.unwrap();

    // Order is already rejected
    let result = manager.cancel_order(order_id, user_id).await;
    assert!(matches!(result, Err(OrderError::InvalidTransition { .. })));
}

#[tokio::test]
async fn test_cancel_nonexistent_order() {
    let (manager, _, _, user_id) = setup_approving().await;
    let result = manager.cancel_order(Uuid::new_v4(), user_id).await;
    assert!(matches!(result, Err(OrderError::NotFound(_))));
}

// ============================================================================
// VALIDATION TESTS
// ============================================================================

#[tokio::test]
async fn test_order_instrument_not_found() {
    let (manager, _, _, user_id) = setup_approving().await;

    let order = make_order(user_id, "nonexistent-instrument", 100.0, 10);
    let result = manager.submit_order(order).await;
    assert!(matches!(result, Err(OrderError::Storage(_))));
}

#[tokio::test]
async fn test_order_price_not_aligned() {
    let (manager, instrument_id, _, user_id) = setup_approving().await;

    // BTC tick_size = 0.5, so price 100.3 is not aligned
    let order = make_order(user_id, &instrument_id, 100.3, 10);
    let result = manager.submit_order(order).await;
    assert!(matches!(result, Err(OrderError::Validation(_))));
}

#[tokio::test]
async fn test_order_negative_price() {
    let (manager, instrument_id, _, user_id) = setup_approving().await;

    let order = make_order(user_id, &instrument_id, -50.0, 10);
    let result = manager.submit_order(order).await;
    assert!(matches!(result, Err(OrderError::Validation(_))));
}

#[tokio::test]
async fn test_order_zero_price() {
    let (manager, instrument_id, _, user_id) = setup_approving().await;

    let order = make_order(user_id, &instrument_id, 0.0, 10);
    let result = manager.submit_order(order).await;
    assert!(matches!(result, Err(OrderError::Validation(_))));
}

#[tokio::test]
async fn test_order_quantity_too_large() {
    let instrument_store = Arc::new(InMemoryInstrumentStore::new());
    let market_store = Arc::new(InMemoryMarketStore::new());
    let order_store = Arc::new(InMemoryOrderStore::new());
    let risk_client: Arc<dyn RiskClient> = Arc::new(MockRiskClient::approving());

    // Create and activate instrument
    let market = Market::new(Asset::btc(), Currency::usdt());
    let market_id = market.market_id;
    market_store.create_market(market).await.unwrap();

    let registry = InstrumentRegistry::new(instrument_store.clone(), market_store.clone());
    let instrument = OptionInstrumentBuilder::new(market_id, Asset::btc(), Currency::usdt())
        .option_type(OptionType::Call)
        .strike_price(50000.0)
        .expiry_timestamp(Utc::now() + Duration::days(30))
        .build();
    let instrument_id = registry.create_instrument(instrument).await.unwrap();
    registry.transition_status(&instrument_id, InstrumentStatus::Listed).await.unwrap();
    registry.transition_status(&instrument_id, InstrumentStatus::Active).await.unwrap();

    let config = OrderValidationConfig {
        max_order_size: 50,
        ..Default::default()
    };
    let manager = OrderManager::with_validation_config(
        order_store,
        instrument_store,
        risk_client,
        config,
    );

    let order = make_order(Uuid::new_v4(), &instrument_id, 100.0, 100);
    let result = manager.submit_order(order).await;
    assert!(matches!(result, Err(OrderError::Validation(_))));
}

#[tokio::test]
async fn test_order_on_non_active_instrument() {
    let instrument_store = Arc::new(InMemoryInstrumentStore::new());
    let market_store = Arc::new(InMemoryMarketStore::new());
    let order_store = Arc::new(InMemoryOrderStore::new());
    let risk_client: Arc<dyn RiskClient> = Arc::new(MockRiskClient::approving());

    let market = Market::new(Asset::btc(), Currency::usdt());
    let market_id = market.market_id;
    market_store.create_market(market).await.unwrap();

    let registry = InstrumentRegistry::new(instrument_store.clone(), market_store.clone());
    let instrument = OptionInstrumentBuilder::new(market_id, Asset::btc(), Currency::usdt())
        .option_type(OptionType::Call)
        .strike_price(50000.0)
        .expiry_timestamp(Utc::now() + Duration::days(30))
        .build();

    // Leave instrument in Draft status (not tradeable)
    let instrument_id = registry.create_instrument(instrument).await.unwrap();

    let manager = OrderManager::new(order_store, instrument_store, risk_client);

    let order = make_order(Uuid::new_v4(), &instrument_id, 100.0, 10);
    let result = manager.submit_order(order).await;
    assert!(matches!(result, Err(OrderError::Validation(_))));
}

// ============================================================================
// QUERY TESTS
// ============================================================================

#[tokio::test]
async fn test_list_user_orders() {
    let (manager, instrument_id, _, user_id) = setup_approving().await;

    // Submit 3 orders
    for price in [100.0, 100.5, 101.0] {
        let order = make_order(user_id, &instrument_id, price, 5);
        manager.submit_order(order).await.unwrap();
    }

    let orders = manager.list_user_orders(user_id).await.unwrap();
    assert_eq!(orders.len(), 3);
}

#[tokio::test]
async fn test_list_active_orders() {
    let (manager, instrument_id, _, user_id) = setup_approving().await;

    // Submit 3 orders
    let id1 = manager
        .submit_order(make_order(user_id, &instrument_id, 100.0, 5))
        .await
        .unwrap();
    let _id2 = manager
        .submit_order(make_order(user_id, &instrument_id, 100.5, 5))
        .await
        .unwrap();
    let _id3 = manager
        .submit_order(make_order(user_id, &instrument_id, 101.0, 5))
        .await
        .unwrap();

    // Fill one completely
    manager.apply_fill(id1, 5, 100.0).await.unwrap();

    // Only 2 should be active
    let active = manager.list_active_orders(user_id).await.unwrap();
    assert_eq!(active.len(), 2);
    assert!(active.iter().all(|o| o.status.is_active()));
}

#[tokio::test]
async fn test_list_instrument_orders() {
    let (manager, instrument_id, _, user_id) = setup_approving().await;

    // Submit 2 orders on same instrument
    manager
        .submit_order(make_order(user_id, &instrument_id, 100.0, 5))
        .await
        .unwrap();
    manager
        .submit_order(make_order(user_id, &instrument_id, 100.5, 5))
        .await
        .unwrap();

    let orders = manager.list_instrument_orders(&instrument_id).await.unwrap();
    assert_eq!(orders.len(), 2);
}

#[tokio::test]
async fn test_get_nonexistent_order() {
    let (manager, _, _, _) = setup_approving().await;
    let result = manager.get_order(Uuid::new_v4()).await;
    assert!(matches!(result, Err(OrderError::NotFound(_))));
}

// ============================================================================
// MULTIPLE USERS TESTS
// ============================================================================

#[tokio::test]
async fn test_multiple_users_independent_orders() {
    let (manager, instrument_id, _, _) = setup_approving().await;

    let user_a = Uuid::new_v4();
    let user_b = Uuid::new_v4();

    // User A submits 2 orders
    manager
        .submit_order(make_order(user_a, &instrument_id, 100.0, 5))
        .await
        .unwrap();
    manager
        .submit_order(make_order(user_a, &instrument_id, 100.5, 5))
        .await
        .unwrap();

    // User B submits 1 order
    manager
        .submit_order(make_order(user_b, &instrument_id, 101.0, 10))
        .await
        .unwrap();

    let orders_a = manager.list_user_orders(user_a).await.unwrap();
    assert_eq!(orders_a.len(), 2);

    let orders_b = manager.list_user_orders(user_b).await.unwrap();
    assert_eq!(orders_b.len(), 1);
}

// ============================================================================
// PRICE ALIGNMENT TESTS (FLOATING POINT)
// ============================================================================

#[tokio::test]
async fn test_price_aligned_to_tick() {
    let (manager, instrument_id, _, user_id) = setup_approving().await;

    // BTC tick_size = 0.5; these should all pass
    for price in [100.0, 100.5, 101.0, 50.5, 1.0, 0.5] {
        let order = make_order(user_id, &instrument_id, price, 1);
        let result = manager.submit_order(order).await;
        assert!(result.is_ok(), "Price {} should be aligned to tick 0.5", price);
    }
}

#[tokio::test]
async fn test_price_not_aligned_to_tick() {
    let (manager, instrument_id, _, user_id) = setup_approving().await;

    // BTC tick_size = 0.5; these should all fail
    for price in [100.1, 100.2, 100.3, 100.4, 100.7] {
        let order = make_order(user_id, &instrument_id, price, 1);
        let result = manager.submit_order(order).await;
        assert!(
            matches!(result, Err(OrderError::Validation(_))),
            "Price {} should NOT be aligned to tick 0.5",
            price
        );
    }
}

// ============================================================================
// ETH INSTRUMENT TESTS
// ============================================================================

#[tokio::test]
async fn test_eth_instrument_order() {
    let instrument_store = Arc::new(InMemoryInstrumentStore::new());
    let market_store = Arc::new(InMemoryMarketStore::new());
    let order_store = Arc::new(InMemoryOrderStore::new());
    let risk_client: Arc<dyn RiskClient> = Arc::new(MockRiskClient::approving());

    let market = Market::new(Asset::eth(), Currency::usdt());
    let market_id = market.market_id;
    market_store.create_market(market).await.unwrap();

    let registry = InstrumentRegistry::new(instrument_store.clone(), market_store.clone());
    let instrument = OptionInstrumentBuilder::new(market_id, Asset::eth(), Currency::usdt())
        .option_type(OptionType::Put)
        .strike_price(3000.0)
        .expiry_timestamp(Utc::now() + Duration::days(30))
        .build();

    let instrument_id = registry.create_instrument(instrument).await.unwrap();
    registry.transition_status(&instrument_id, InstrumentStatus::Listed).await.unwrap();
    registry.transition_status(&instrument_id, InstrumentStatus::Active).await.unwrap();

    let manager = OrderManager::new(order_store, instrument_store, risk_client);

    // ETH tick_size = 0.1; price 50.1 should be valid
    let user_id = Uuid::new_v4();
    let order = OrderBuilder::new()
        .user_id(user_id)
        .instrument_id(&instrument_id)
        .side(OrderSide::Sell)
        .price(50.1)
        .quantity(5)
        .build()
        .unwrap();

    let order_id = manager.submit_order(order).await.unwrap();
    let retrieved = manager.get_order(order_id).await.unwrap();
    assert_eq!(retrieved.status, OrderStatus::Open);
    assert_eq!(retrieved.price, Some(50.1));
}
