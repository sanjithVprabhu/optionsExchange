/// Integration tests for the Instrument Layer (Module 01).
///
/// These tests exercise the full stack: Registry -> Validation -> InMemoryStore
/// to verify business logic correctness without any infrastructure dependency.
use std::sync::Arc;

use chrono::{Duration, Utc};
use exchange_adapters_storage::{InMemoryInstrumentStore, InMemoryMarketStore};
use exchange_instrument::{
    InstrumentError, InstrumentRegistry, InstrumentStore, Market, MarketStore,
    OptionInstrument, OptionInstrumentBuilder,
};
use exchange_primitives::{Asset, Currency, InstrumentStatus, OptionType};
use uuid::Uuid;

/// Helper: build a registry backed by in-memory stores.
fn build_test_registry() -> InstrumentRegistry {
    InstrumentRegistry::new(
        Arc::new(InMemoryInstrumentStore::new()),
        Arc::new(InMemoryMarketStore::new()),
    )
}

/// Helper: create a BTC/USDT market in the registry and return its ID.
async fn create_btc_market(registry: &InstrumentRegistry) -> Uuid {
    let market = Market::new(Asset::btc(), Currency::usdt());
    let id = market.market_id;
    registry.create_market(market).await.unwrap();
    id
}

/// Helper: build a valid instrument for a given market.
fn make_instrument(
    market_id: Uuid,
    strike: f64,
    option_type: OptionType,
) -> OptionInstrument {
    OptionInstrumentBuilder::new(market_id, Asset::btc(), Currency::usdt())
        .option_type(option_type)
        .strike_price(strike)
        .expiry_timestamp(Utc::now() + Duration::days(30))
        .status(InstrumentStatus::Draft)
        .build()
}

// =========================================================================
// HAPPY PATH TESTS
// =========================================================================

#[tokio::test]
async fn test_create_and_retrieve_instrument() {
    let registry = build_test_registry();
    let market_id = create_btc_market(&registry).await;

    let instrument = make_instrument(market_id, 50000.0, OptionType::Call);
    let id = registry.create_instrument(instrument).await.unwrap();

    let retrieved = registry.get_instrument(&id).await.unwrap();
    assert_eq!(retrieved.strike_price, 50000.0);
    assert_eq!(retrieved.option_type, OptionType::Call);
    assert_eq!(retrieved.status, InstrumentStatus::Draft);
}

#[tokio::test]
async fn test_create_call_and_put_same_strike() {
    let registry = build_test_registry();
    let market_id = create_btc_market(&registry).await;

    let call_id = registry
        .create_instrument(make_instrument(market_id, 50000.0, OptionType::Call))
        .await
        .unwrap();

    let put_id = registry
        .create_instrument(make_instrument(market_id, 50000.0, OptionType::Put))
        .await
        .unwrap();

    // Call and Put at same strike = different instruments
    assert_ne!(call_id, put_id);
}

#[tokio::test]
async fn test_create_multiple_strikes() {
    let registry = build_test_registry();
    let market_id = create_btc_market(&registry).await;

    let strikes = vec![45000.0, 47500.0, 50000.0, 52500.0, 55000.0];
    for strike in &strikes {
        registry
            .create_instrument(make_instrument(market_id, *strike, OptionType::Call))
            .await
            .unwrap();
    }

    let instruments = registry.list_instruments(market_id).await.unwrap();
    assert_eq!(instruments.len(), strikes.len());
}

#[tokio::test]
async fn test_deterministic_id_is_idempotent() {
    let market_id = Uuid::new_v4();
    let expiry = Utc::now() + Duration::days(30);

    let i1 = OptionInstrumentBuilder::new(market_id, Asset::btc(), Currency::usdt())
        .option_type(OptionType::Call)
        .strike_price(50000.0)
        .expiry_timestamp(expiry)
        .build();

    let i2 = OptionInstrumentBuilder::new(market_id, Asset::btc(), Currency::usdt())
        .option_type(OptionType::Call)
        .strike_price(50000.0)
        .expiry_timestamp(expiry)
        .build();

    assert_eq!(i1.instrument_id, i2.instrument_id);
    assert_eq!(i1.canonical_name(), i2.canonical_name());
}

#[tokio::test]
async fn test_canonical_name_format() {
    let instrument = make_instrument(Uuid::new_v4(), 50000.0, OptionType::Call);
    let name = instrument.canonical_name();

    // Should be like: BTC-14MAR2026-50000-C
    assert!(name.starts_with("BTC-"));
    assert!(name.ends_with("-C"));
    assert!(name.contains("50000"));
}

// =========================================================================
// LIFECYCLE / STATUS TRANSITION TESTS
// =========================================================================

#[tokio::test]
async fn test_status_transition_draft_to_listed() {
    let registry = build_test_registry();
    let market_id = create_btc_market(&registry).await;

    let id = registry
        .create_instrument(make_instrument(market_id, 50000.0, OptionType::Call))
        .await
        .unwrap();

    registry
        .transition_status(&id, InstrumentStatus::Listed)
        .await
        .unwrap();

    let instrument = registry.get_instrument(&id).await.unwrap();
    assert_eq!(instrument.status, InstrumentStatus::Listed);
}

#[tokio::test]
async fn test_full_lifecycle_transition() {
    let registry = build_test_registry();
    let market_id = create_btc_market(&registry).await;

    let id = registry
        .create_instrument(make_instrument(market_id, 50000.0, OptionType::Call))
        .await
        .unwrap();

    // Draft -> Listed -> Active -> Expired -> Settled -> Archived
    let transitions = vec![
        InstrumentStatus::Listed,
        InstrumentStatus::Active,
        InstrumentStatus::Expired,
        InstrumentStatus::Settled,
        InstrumentStatus::Archived,
    ];

    for status in transitions {
        registry.transition_status(&id, status).await.unwrap();
        let instrument = registry.get_instrument(&id).await.unwrap();
        assert_eq!(instrument.status, status);
    }
}

#[tokio::test]
async fn test_invalid_status_transition_rejected() {
    let registry = build_test_registry();
    let market_id = create_btc_market(&registry).await;

    let id = registry
        .create_instrument(make_instrument(market_id, 50000.0, OptionType::Call))
        .await
        .unwrap();

    // Draft -> Active is invalid (must go through Listed)
    let result = registry
        .transition_status(&id, InstrumentStatus::Active)
        .await;

    assert!(matches!(
        result,
        Err(InstrumentError::InvalidStatusTransition { .. })
    ));
}

#[tokio::test]
async fn test_halt_and_resume() {
    let registry = build_test_registry();
    let market_id = create_btc_market(&registry).await;

    let id = registry
        .create_instrument(make_instrument(market_id, 50000.0, OptionType::Call))
        .await
        .unwrap();

    // Draft -> Listed -> Active
    registry
        .transition_status(&id, InstrumentStatus::Listed)
        .await
        .unwrap();
    registry
        .transition_status(&id, InstrumentStatus::Active)
        .await
        .unwrap();

    // Active -> Halted
    registry
        .transition_status(&id, InstrumentStatus::Halted)
        .await
        .unwrap();
    let halted = registry.get_instrument(&id).await.unwrap();
    assert_eq!(halted.status, InstrumentStatus::Halted);

    // Halted -> Active (resume)
    registry
        .transition_status(&id, InstrumentStatus::Active)
        .await
        .unwrap();
    let resumed = registry.get_instrument(&id).await.unwrap();
    assert_eq!(resumed.status, InstrumentStatus::Active);
}

// =========================================================================
// VALIDATION TESTS
// =========================================================================

#[tokio::test]
async fn test_reject_expired_instrument() {
    let registry = build_test_registry();
    let market_id = create_btc_market(&registry).await;

    let mut instrument = make_instrument(market_id, 50000.0, OptionType::Call);
    instrument.expiry_timestamp = Utc::now() - Duration::hours(1);

    let result = registry.create_instrument(instrument).await;
    assert!(matches!(
        result,
        Err(InstrumentError::InvalidInstrument(_))
    ));
}

#[tokio::test]
async fn test_reject_zero_strike() {
    let registry = build_test_registry();
    let market_id = create_btc_market(&registry).await;

    let mut instrument = make_instrument(market_id, 50000.0, OptionType::Call);
    instrument.strike_price = 0.0;

    let result = registry.create_instrument(instrument).await;
    assert!(matches!(
        result,
        Err(InstrumentError::InvalidInstrument(_))
    ));
}

#[tokio::test]
async fn test_reject_negative_strike() {
    let registry = build_test_registry();
    let market_id = create_btc_market(&registry).await;

    let mut instrument = make_instrument(market_id, 50000.0, OptionType::Call);
    instrument.strike_price = -100.0;

    let result = registry.create_instrument(instrument).await;
    assert!(matches!(
        result,
        Err(InstrumentError::InvalidInstrument(_))
    ));
}

#[tokio::test]
async fn test_reject_misaligned_strike() {
    let registry = build_test_registry();
    let market_id = create_btc_market(&registry).await;

    let mut instrument = make_instrument(market_id, 50000.0, OptionType::Call);
    instrument.strike_price = 50000.3; // tick_size is 0.5

    let result = registry.create_instrument(instrument).await;
    assert!(matches!(
        result,
        Err(InstrumentError::InvalidInstrument(_))
    ));
}

#[tokio::test]
async fn test_reject_duplicate_instrument() {
    let registry = build_test_registry();
    let market_id = create_btc_market(&registry).await;

    let instrument = make_instrument(market_id, 50000.0, OptionType::Call);

    registry
        .create_instrument(instrument.clone())
        .await
        .unwrap();

    let result = registry.create_instrument(instrument).await;
    assert!(matches!(result, Err(InstrumentError::AlreadyExists(_))));
}

#[tokio::test]
async fn test_reject_instrument_for_nonexistent_market() {
    let registry = build_test_registry();
    let fake_market_id = Uuid::new_v4();

    let instrument = make_instrument(fake_market_id, 50000.0, OptionType::Call);
    let result = registry.create_instrument(instrument).await;
    assert!(matches!(result, Err(InstrumentError::MarketNotFound(_))));
}

// =========================================================================
// QUERY TESTS
// =========================================================================

#[tokio::test]
async fn test_list_instruments_by_market() {
    let registry = build_test_registry();
    let btc_market = create_btc_market(&registry).await;

    let eth_market_obj = Market::new(Asset::eth(), Currency::usdt());
    let eth_market = eth_market_obj.market_id;
    registry.create_market(eth_market_obj).await.unwrap();

    // BTC instruments
    for strike in [45000.0, 50000.0, 55000.0] {
        registry
            .create_instrument(make_instrument(btc_market, strike, OptionType::Call))
            .await
            .unwrap();
    }

    // ETH instrument
    let eth_instrument =
        OptionInstrumentBuilder::new(eth_market, Asset::eth(), Currency::usdt())
            .option_type(OptionType::Call)
            .strike_price(3000.0)
            .expiry_timestamp(Utc::now() + Duration::days(30))
            .status(InstrumentStatus::Draft)
            .build();
    registry.create_instrument(eth_instrument).await.unwrap();

    let btc_list = registry.list_instruments(btc_market).await.unwrap();
    assert_eq!(btc_list.len(), 3);

    let eth_list = registry.list_instruments(eth_market).await.unwrap();
    assert_eq!(eth_list.len(), 1);
}

#[tokio::test]
async fn test_list_active_by_asset() {
    let registry = build_test_registry();
    let market_id = create_btc_market(&registry).await;

    // Create instrument and activate it
    let id = registry
        .create_instrument(make_instrument(market_id, 50000.0, OptionType::Call))
        .await
        .unwrap();

    registry
        .transition_status(&id, InstrumentStatus::Listed)
        .await
        .unwrap();
    registry
        .transition_status(&id, InstrumentStatus::Active)
        .await
        .unwrap();

    // Create another instrument but leave as Draft
    registry
        .create_instrument(make_instrument(market_id, 55000.0, OptionType::Call))
        .await
        .unwrap();

    let active = registry.list_active_instruments("BTC").await.unwrap();
    assert_eq!(active.len(), 1);
    assert_eq!(active[0].strike_price, 50000.0);
}

#[tokio::test]
async fn test_get_nonexistent_instrument() {
    let registry = build_test_registry();
    let result = registry.get_instrument("nonexistent_id").await;
    assert!(matches!(result, Err(InstrumentError::NotFound(_))));
}

// =========================================================================
// EXPIRY TESTS
// =========================================================================

#[tokio::test]
async fn test_expire_instruments() {
    let registry = build_test_registry();
    let market_id = create_btc_market(&registry).await;

    // Create an instrument that is already past expiry but marked Active
    let mut instrument = make_instrument(market_id, 50000.0, OptionType::Call);
    instrument.expiry_timestamp = Utc::now() - Duration::hours(1);
    instrument.status = InstrumentStatus::Active;

    // Bypass validation by inserting directly via store
    // (In production, this would come from the lifecycle manager)
    let id = instrument.instrument_id.clone();

    // We need to use the store directly since registry validates expiry
    let instrument_store = Arc::new(InMemoryInstrumentStore::new());
    let market_store = Arc::new(InMemoryMarketStore::new());
    market_store
        .create_market(Market::new(Asset::btc(), Currency::usdt()))
        .await
        .unwrap();

    instrument_store
        .create_instrument(instrument)
        .await
        .unwrap();

    let registry = InstrumentRegistry::new(instrument_store.clone(), market_store);

    let expired = registry.expire_instruments().await.unwrap();
    assert_eq!(expired.len(), 1);
    assert_eq!(expired[0], id);

    // Verify status changed
    let instrument = instrument_store.get_instrument(&id).await.unwrap().unwrap();
    assert_eq!(instrument.status, InstrumentStatus::Expired);
}

// =========================================================================
// MARKET TESTS
// =========================================================================

#[tokio::test]
async fn test_create_and_list_markets() {
    let registry = build_test_registry();

    let btc_market = Market::new(Asset::btc(), Currency::usdt());
    let btc_id = btc_market.market_id;
    registry.create_market(btc_market).await.unwrap();

    let eth_market = Market::new(Asset::eth(), Currency::usdt());
    let eth_id = eth_market.market_id;
    registry.create_market(eth_market).await.unwrap();

    let markets = registry.list_markets().await.unwrap();
    assert_eq!(markets.len(), 2);

    let btc = registry.get_market(btc_id).await.unwrap();
    assert_eq!(btc.underlying_asset.asset_id, "BTC");

    let eth = registry.get_market(eth_id).await.unwrap();
    assert_eq!(eth.underlying_asset.asset_id, "ETH");
}

#[tokio::test]
async fn test_get_nonexistent_market() {
    let registry = build_test_registry();
    let result = registry.get_market(Uuid::new_v4()).await;
    assert!(matches!(result, Err(InstrumentError::MarketNotFound(_))));
}
