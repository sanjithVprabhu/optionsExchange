# MASTER IMPLEMENTATION GUIDE
# White-Label Crypto Options Exchange
# Version 1.0 - For Claude Code Execution

---

## CRITICAL READING INSTRUCTIONS FOR CLAUDE CODE

This document is the **SINGLE SOURCE OF TRUTH** for building the entire exchange.

**Before writing ANY code:**
1. Read this document completely
2. Read the referenced architecture documents for the module you're building
3. Understand the trait-based abstraction pattern
4. Follow the exact structure in PROJECT_STRUCTURE.md

**Key Principle:**
Core modules NEVER import infrastructure (postgres, redis, etc.).
They define TRAITS that adapters implement.

---

# PART 1: SYSTEM OVERVIEW

## What You're Building

A **white-label cryptocurrency options exchange** with:
- 7 core modules (Instrument, OMS, Matching, Risk, Settlement, Wallet, Market Data)
- Config-driven infrastructure (Postgres/Supabase/MySQL/Redis/In-memory)
- Multi-chain settlement (Ethereum, Polygon, Arbitrum)
- Multiple market data providers (Binance, Coinbase, custom)
- Production-grade risk management (Greeks, margin, liquidations)
- Black-swan resilience (circuit breakers, insurance fund, deterministic replay)

## Architecture Pattern

```
CONFIG (YAML)
    ↓
BUILDER (constructs services based on config)
    ↓
ADAPTERS (implement traits: PostgresStore, SupabaseStore, etc.)
    ↓
CORE (pure business logic, uses traits)
    ↓
SERVICES (gRPC/HTTP endpoints)
```

## Tech Stack

- **Language**: Rust (async with tokio)
- **Databases**: PostgreSQL (primary), Supabase, MySQL (configurable)
- **Cache**: Redis (orderbook, high-speed data)
- **Blockchain**: Ethereum, Polygon (via ethers-rs)
- **Market Data**: WebSocket (Binance, Coinbase)
- **Communication**: gRPC (primary), HTTP (fallback)
- **Testing**: cargo test, proptest (property-based), testcontainers

---

# PART 2: MODULE-BY-MODULE IMPLEMENTATION

## MODULE 1: MARKET INSTRUMENT LAYER

### Reference Documents
- Read: `/mnt/user-data/uploads/MarketInstrumentLayer.md` (COMPLETELY)
- Read: `PROJECT_STRUCTURE.md` (section: core/instrument)

### Purpose
Defines what can be traded (options on BTC, ETH, etc.).
This is the foundation - everything else depends on it.

### Core Types (domain.rs)

```rust
use chrono::{DateTime, Utc};
use serde::{Serialize, Deserialize};
use uuid::Uuid;

// Primitives
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Hash)]
pub struct Asset {
    pub asset_id: String,        // "BTC", "ETH"
    pub name: String,             // "Bitcoin"
    pub decimals: u8,             // 8 for BTC, 18 for ETH
    pub contract_size: f64,       // 0.01 = fractional contracts
    pub min_order_size: u32,      // minimum contracts per order
    pub tick_size: f64,           // price increment (0.5 USDT)
    pub price_decimals: u8,       // display precision
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Hash)]
pub struct Currency {
    pub currency_id: String,      // "USDT"
    pub name: String,
    pub decimals: u8,
}

// Market grouping
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Market {
    pub market_id: Uuid,
    pub underlying_asset: Asset,
    pub settlement_currency: Currency,
    pub market_type: MarketType,
    pub status: MarketStatus,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub enum MarketType {
    Options,
    // Future: Futures, Perpetuals
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub enum MarketStatus {
    Active,
    Halted,
    Expired,
}

// Option types
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub enum OptionStyle {
    European,  // v0 only
    // Future: American
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub enum OptionType {
    Call,
    Put,
}

// Core instrument
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OptionInstrument {
    pub instrument_id: String,           // deterministic hash
    pub market_id: Uuid,
    
    pub underlying_asset: Asset,
    pub option_type: OptionType,
    pub style: OptionStyle,
    
    pub strike_price: f64,               // in settlement currency
    pub expiry_timestamp: DateTime<Utc>,
    
    pub contract_size: f64,              // how much underlying per contract
    pub min_order_size: u32,
    pub settlement_currency: Currency,
    pub tick_size: f64,
    
    pub status: InstrumentStatus,
    pub created_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub enum InstrumentStatus {
    Draft,
    Listed,
    Active,
    Expired,
    Settled,
    Archived,
}

// Canonical naming
impl OptionInstrument {
    pub fn canonical_name(&self) -> String {
        // Example: BTC-28MAR2026-50000-C
        format!(
            "{}-{}-{}-{}",
            self.underlying_asset.asset_id,
            self.expiry_timestamp.format("%d%b%Y").to_string().to_uppercase(),
            self.strike_price as u64,
            match self.option_type {
                OptionType::Call => "C",
                OptionType::Put => "P",
            }
        )
    }
    
    pub fn generate_id(&self) -> String {
        use sha2::{Sha256, Digest};
        let canonical = self.canonical_name();
        let mut hasher = Sha256::new();
        hasher.update(canonical.as_bytes());
        format!("{:x}", hasher.finalize())
    }
}
```

### Storage Trait (traits.rs)

```rust
use async_trait::async_trait;
use uuid::Uuid;

#[derive(Debug, thiserror::Error)]
pub enum InstrumentError {
    #[error("Instrument not found: {0}")]
    NotFound(String),
    
    #[error("Instrument already exists: {0}")]
    AlreadyExists(String),
    
    #[error("Invalid instrument: {0}")]
    InvalidInstrument(String),
    
    #[error("Database error: {0}")]
    DatabaseError(String),
}

/// Storage interface for instruments
/// Implementations: PostgresInstrumentStore, SupabaseInstrumentStore, etc.
#[async_trait]
pub trait InstrumentStore: Send + Sync {
    async fn create_instrument(
        &self,
        instrument: OptionInstrument
    ) -> Result<String, InstrumentError>;
    
    async fn get_instrument(
        &self,
        instrument_id: &str
    ) -> Result<Option<OptionInstrument>, InstrumentError>;
    
    async fn list_instruments(
        &self,
        market_id: Uuid
    ) -> Result<Vec<OptionInstrument>, InstrumentError>;
    
    async fn update_status(
        &self,
        instrument_id: &str,
        status: InstrumentStatus
    ) -> Result<(), InstrumentError>;
    
    async fn instrument_exists(
        &self,
        instrument_id: &str
    ) -> Result<bool, InstrumentError>;
}
```

### Business Logic (registry.rs)

```rust
use std::sync::Arc;

pub struct InstrumentRegistry {
    store: Arc<dyn InstrumentStore>,
}

impl InstrumentRegistry {
    pub fn new(store: Arc<dyn InstrumentStore>) -> Self {
        Self { store }
    }
    
    pub async fn create_instrument(
        &self,
        instrument: OptionInstrument
    ) -> Result<String, InstrumentError> {
        // Validate
        validate_instrument(&instrument)?;
        
        // Check doesn't exist
        let id = instrument.generate_id();
        if self.store.instrument_exists(&id).await? {
            return Err(InstrumentError::AlreadyExists(id));
        }
        
        // Create
        self.store.create_instrument(instrument).await
    }
    
    pub async fn get_instrument(
        &self,
        instrument_id: &str
    ) -> Result<OptionInstrument, InstrumentError> {
        self.store
            .get_instrument(instrument_id)
            .await?
            .ok_or_else(|| InstrumentError::NotFound(instrument_id.to_string()))
    }
    
    // ... other methods
}
```

### Validation (validation.rs)

```rust
pub fn validate_instrument(instrument: &OptionInstrument) -> Result<(), InstrumentError> {
    // Expiry must be in future
    if instrument.expiry_timestamp <= Utc::now() {
        return Err(InstrumentError::InvalidInstrument(
            "Expiry must be in the future".to_string()
        ));
    }
    
    // Strike must be positive
    if instrument.strike_price <= 0.0 {
        return Err(InstrumentError::InvalidInstrument(
            "Strike price must be positive".to_string()
        ));
    }
    
    // Tick size must divide strike
    let remainder = instrument.strike_price % instrument.tick_size;
    if remainder.abs() > 1e-6 {
        return Err(InstrumentError::InvalidInstrument(
            format!("Strike {} not aligned to tick size {}", 
                instrument.strike_price, instrument.tick_size)
        ));
    }
    
    // Contract size must be positive
    if instrument.contract_size <= 0.0 {
        return Err(InstrumentError::InvalidInstrument(
            "Contract size must be positive".to_string()
        ));
    }
    
    Ok(())
}
```

### Postgres Adapter (adapters/storage/postgres/instruments.rs)

```rust
use sqlx::PgPool;

pub struct PostgresInstrumentStore {
    pool: PgPool,
}

impl PostgresInstrumentStore {
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }
}

#[async_trait]
impl InstrumentStore for PostgresInstrumentStore {
    async fn create_instrument(
        &self,
        instrument: OptionInstrument
    ) -> Result<String, InstrumentError> {
        let id = instrument.generate_id();
        
        sqlx::query!(
            r#"
            INSERT INTO instruments (
                instrument_id, market_id, underlying_asset_id, option_type,
                strike_price, expiry_timestamp, contract_size, min_order_size,
                settlement_currency_id, tick_size, status, created_at
            )
            VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12)
            "#,
            id,
            instrument.market_id,
            instrument.underlying_asset.asset_id,
            instrument.option_type.to_string(),
            instrument.strike_price,
            instrument.expiry_timestamp,
            instrument.contract_size,
            instrument.min_order_size as i32,
            instrument.settlement_currency.currency_id,
            instrument.tick_size,
            instrument.status.to_string(),
            instrument.created_at
        )
        .execute(&self.pool)
        .await
        .map_err(|e| InstrumentError::DatabaseError(e.to_string()))?;
        
        Ok(id)
    }
    
    async fn get_instrument(
        &self,
        instrument_id: &str
    ) -> Result<Option<OptionInstrument>, InstrumentError> {
        let row = sqlx::query!(
            r#"
            SELECT * FROM instruments WHERE instrument_id = $1
            "#,
            instrument_id
        )
        .fetch_optional(&self.pool)
        .await
        .map_err(|e| InstrumentError::DatabaseError(e.to_string()))?;
        
        Ok(row.map(|r| {
            // Map database row to OptionInstrument
            // TODO: implement row mapping
            unimplemented!("Row mapping")
        }))
    }
    
    // ... implement other trait methods
}
```

### Testing (tests/integration/instrument_tests.rs)

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use testcontainers::*;
    
    #[tokio::test]
    async fn test_create_and_retrieve_instrument() {
        // Setup test database with testcontainers
        let docker = clients::Cli::default();
        let postgres = docker.run(images::postgres::Postgres::default());
        
        let connection_string = format!(
            "postgres://postgres:postgres@localhost:{}/postgres",
            postgres.get_host_port_ipv4(5432)
        );
        
        let pool = PgPool::connect(&connection_string).await.unwrap();
        
        // Run migrations
        sqlx::migrate!("./migrations")
            .run(&pool)
            .await
            .unwrap();
        
        // Create store and registry
        let store = Arc::new(PostgresInstrumentStore::new(pool));
        let registry = InstrumentRegistry::new(store);
        
        // Create test instrument
        let instrument = OptionInstrument {
            instrument_id: String::new(), // will be generated
            market_id: Uuid::new_v4(),
            underlying_asset: Asset {
                asset_id: "BTC".to_string(),
                name: "Bitcoin".to_string(),
                decimals: 8,
                contract_size: 0.01,
                min_order_size: 1,
                tick_size: 0.5,
                price_decimals: 2,
            },
            option_type: OptionType::Call,
            style: OptionStyle::European,
            strike_price: 50000.0,
            expiry_timestamp: Utc::now() + Duration::days(30),
            contract_size: 0.01,
            min_order_size: 1,
            settlement_currency: Currency {
                currency_id: "USDT".to_string(),
                name: "Tether".to_string(),
                decimals: 6,
            },
            tick_size: 0.5,
            status: InstrumentStatus::Listed,
            created_at: Utc::now(),
        };
        
        // Create instrument
        let id = registry.create_instrument(instrument.clone()).await.unwrap();
        
        // Retrieve and verify
        let retrieved = registry.get_instrument(&id).await.unwrap();
        assert_eq!(retrieved.strike_price, 50000.0);
        assert_eq!(retrieved.option_type, OptionType::Call);
    }
    
    #[test]
    fn test_canonical_naming() {
        let instrument = create_test_instrument();
        let name = instrument.canonical_name();
        
        // Should be: BTC-DDMMMYYYY-STRIKE-C/P
        assert!(name.starts_with("BTC-"));
        assert!(name.ends_with("-C") || name.ends_with("-P"));
    }
    
    #[test]
    fn test_validation_rejects_expired() {
        let mut instrument = create_test_instrument();
        instrument.expiry_timestamp = Utc::now() - Duration::hours(1);
        
        let result = validate_instrument(&instrument);
        assert!(result.is_err());
    }
}
```

### Config Integration (config_loader/instrument_builder.rs)

```rust
use serde::Deserialize;

#[derive(Debug, Deserialize)]
pub struct InstrumentLayerConfig {
    pub supported_assets: Vec<AssetConfig>,
    pub settlement_currencies: Vec<CurrencyConfig>,
    pub storage: StorageConfig,
    pub expiry_schedule: ExpiryScheduleConfig,
}

pub struct InstrumentLayerBuilder;

impl InstrumentLayerBuilder {
    pub async fn build(
        config: InstrumentLayerConfig
    ) -> Result<InstrumentRegistry, BuildError> {
        // Choose storage based on config
        let store: Arc<dyn InstrumentStore> = match config.storage.r#type.as_str() {
            "postgres" => {
                let pool = create_postgres_pool(&config.storage.postgres).await?;
                Arc::new(PostgresInstrumentStore::new(pool))
            }
            "supabase" => {
                let client = create_supabase_client(&config.storage.supabase)?;
                Arc::new(SupabaseInstrumentStore::new(client))
            }
            "inmemory" => {
                Arc::new(InMemoryInstrumentStore::new())
            }
            _ => return Err(BuildError::UnsupportedStorage(config.storage.r#type)),
        };
        
        Ok(InstrumentRegistry::new(store))
    }
}
```

---

## MODULE 2: ORDER MANAGEMENT SYSTEM (OMS)

### Reference Documents
- Read: `/mnt/user-data/uploads/MatchingEngine.md` (sections on OMS)
- Read: `PROJECT_STRUCTURE.md` (section: core/oms)

### Purpose
Manages user orders, validates them, maintains order lifecycle.
OMS does NOT execute trades - it only manages intent.

### Core Types (order.rs)

```rust
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Order {
    pub order_id: Uuid,
    pub user_id: Uuid,
    pub instrument_id: String,
    pub side: OrderSide,
    pub order_type: OrderType,
    pub price: Option<f64>,          // None for market orders
    pub quantity: u32,                // number of contracts
    pub time_in_force: TimeInForce,
    pub status: OrderStatus,
    pub filled_quantity: u32,
    pub avg_fill_price: Option<f64>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub enum OrderSide {
    Buy,
    Sell,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub enum OrderType {
    Limit,
    Market,
    // Future: StopLimit, StopMarket
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub enum TimeInForce {
    GTC,  // Good Till Cancel
    IOC,  // Immediate or Cancel
    FOK,  // Fill or Kill
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub enum OrderStatus {
    PendingRisk,      // Waiting for risk approval
    Open,             // Active in order book
    PartiallyFilled,
    Filled,
    Cancelled,
    Rejected,
    Expired,
}
```

### OMS Invariants (CRITICAL)

```rust
// INVARIANT 1: OMS never decides risk - only intent
// Risk engine must approve before order enters matching

// INVARIANT 2: Order state transitions are strict
impl Order {
    pub fn can_transition(&self, new_status: OrderStatus) -> bool {
        match (&self.status, &new_status) {
            (OrderStatus::PendingRisk, OrderStatus::Open) => true,
            (OrderStatus::PendingRisk, OrderStatus::Rejected) => true,
            (OrderStatus::Open, OrderStatus::PartiallyFilled) => true,
            (OrderStatus::Open, OrderStatus::Filled) => true,
            (OrderStatus::Open, OrderStatus::Cancelled) => true,
            (OrderStatus::PartiallyFilled, OrderStatus::Filled) => true,
            (OrderStatus::PartiallyFilled, OrderStatus::Cancelled) => true,
            _ => false,
        }
    }
}
```

### Storage Trait (traits.rs)

```rust
#[async_trait]
pub trait OrderStore: Send + Sync {
    async fn create_order(&self, order: Order) -> Result<Uuid, OrderError>;
    async fn get_order(&self, order_id: Uuid) -> Result<Option<Order>, OrderError>;
    async fn update_order_status(
        &self,
        order_id: Uuid,
        status: OrderStatus
    ) -> Result<(), OrderError>;
    async fn list_user_orders(
        &self,
        user_id: Uuid,
        instrument_id: Option<String>
    ) -> Result<Vec<Order>, OrderError>;
}
```

### Integration with Risk Engine

```rust
pub struct OrderManagementSystem {
    order_store: Arc<dyn OrderStore>,
    risk_client: Arc<dyn RiskClient>,  // Calls risk service
}

impl OrderManagementSystem {
    pub async fn submit_order(&self, order: Order) -> Result<Uuid, OrderError> {
        // 1. Validate order
        validate_order(&order)?;
        
        // 2. Store with PendingRisk status
        let order_id = self.order_store.create_order(order.clone()).await?;
        
        // 3. Send to risk engine for approval
        let risk_check = self.risk_client.check_order_risk(&order).await?;
        
        if risk_check.approved {
            // 4. Update to Open status
            self.order_store
                .update_order_status(order_id, OrderStatus::Open)
                .await?;
            
            // 5. Send to matching engine
            self.send_to_matcher(order).await?;
        } else {
            // Reject
            self.order_store
                .update_order_status(order_id, OrderStatus::Rejected)
                .await?;
        }
        
        Ok(order_id)
    }
}
```

---

## MODULE 3: MATCHING ENGINE

### Reference Documents
- Read: `/mnt/user-data/uploads/MatchingEngine.md` (COMPLETELY)
- Read: `PROJECT_STRUCTURE.md` (section: core/matching)

### Purpose
Price-time priority matching. Deterministic, atomic trade execution.

### Core Algorithm (engine.rs)

```rust
pub struct MatchingEngine {
    orderbooks: HashMap<String, OrderBook>,  // instrument_id -> OrderBook
}

pub struct OrderBook {
    instrument_id: String,
    bids: BTreeMap<OrderedFloat<f64>, VecDeque<Order>>,  // price -> orders
    asks: BTreeMap<OrderedFloat<f64>, VecDeque<Order>>,
    sequence: u64,
}

impl MatchingEngine {
    pub fn match_order(&mut self, order: Order) -> Vec<Trade> {
        let book = self.orderbooks
            .entry(order.instrument_id.clone())
            .or_insert_with(|| OrderBook::new(order.instrument_id.clone()));
        
        match order.side {
            OrderSide::Buy => self.match_buy(book, order),
            OrderSide::Sell => self.match_sell(book, order),
        }
    }
    
    fn match_buy(&mut self, book: &mut OrderBook, mut order: Order) -> Vec<Trade> {
        let mut trades = Vec::new();
        let order_price = order.price.expect("Limit order must have price");
        
        // Match against asks (sellers)
        while order.quantity > order.filled_quantity {
            // Get best ask (lowest price)
            let best_ask_entry = book.asks.first_entry();
            if best_ask_entry.is_none() {
                break; // No sellers
            }
            
            let ask_price = *best_ask_entry.as_ref().unwrap().key();
            if ask_price.0 > order_price {
                break; // Price not good enough
            }
            
            // Match with first order at this price level (FIFO)
            let mut ask_orders = best_ask_entry.unwrap();
            if let Some(mut ask_order) = ask_orders.get_mut().pop_front() {
                let trade_qty = (order.quantity - order.filled_quantity)
                    .min(ask_order.quantity - ask_order.filled_quantity);
                
                // Create trade
                let trade = Trade {
                    trade_id: Uuid::new_v4(),
                    instrument_id: order.instrument_id.clone(),
                    buyer_order_id: order.order_id,
                    seller_order_id: ask_order.order_id,
                    price: ask_price.0,
                    quantity: trade_qty,
                    timestamp: Utc::now(),
                    sequence: book.next_sequence(),
                };
                
                // Update fill quantities
                order.filled_quantity += trade_qty;
                ask_order.filled_quantity += trade_qty;
                
                trades.push(trade);
                
                // If ask order not fully filled, put back
                if ask_order.filled_quantity < ask_order.quantity {
                    ask_orders.get_mut().push_front(ask_order);
                }
            }
            
            // Remove price level if empty
            if ask_orders.get().is_empty() {
                ask_orders.remove();
            }
        }
        
        // If order not fully filled, add to book
        if order.filled_quantity < order.quantity {
            book.bids
                .entry(OrderedFloat(order_price))
                .or_insert_with(VecDeque::new)
                .push_back(order);
        }
        
        trades
    }
}
```

### Determinism CRITICAL

```rust
// INVARIANT: Same inputs ALWAYS produce same outputs
// This means:
// 1. No system time in matching logic
// 2. No randomness
// 3. No external calls during matching
// 4. Strict FIFO at each price level

#[cfg(test)]
mod determinism_tests {
    #[test]
    fn test_matching_determinism() {
        let orders = create_test_order_sequence();
        
        // Run matching 1000 times
        let mut results = Vec::new();
        for _ in 0..1000 {
            let mut engine = MatchingEngine::new();
            let trades = engine.match_orders(orders.clone());
            results.push(trades);
        }
        
        // All results must be identical
        for i in 1..results.len() {
            assert_eq!(results[0], results[i], "Matching not deterministic!");
        }
    }
}
```

---

## MODULE 4: RISK ENGINE

### Reference Documents
- Read: `/mnt/user-data/uploads/Risk_Engine.md` (COMPLETELY)
- Read: `/mnt/user-data/uploads/marketdatapricing.md` (Black-Scholes section)

### Purpose
Calculate margin, Greeks, manage liquidations. GATES the OMS.

### Black-Scholes Implementation (greeks.rs)

```rust
use statrs::distribution::{Normal, Continuous, ContinuousCDF};

pub struct BSInputs {
    pub spot: f64,      // Index price
    pub strike: f64,    // Strike price
    pub time: f64,      // Time to expiry (years)
    pub vol: f64,       // Implied volatility
    pub rate: f64,      // Risk-free rate (≈0 for crypto)
    pub option_type: OptionType,
}

pub struct Greeks {
    pub delta: f64,
    pub gamma: f64,
    pub vega: f64,
    pub theta: f64,
    pub rho: f64,
}

pub fn calculate_greeks(input: &BSInputs) -> (f64, Greeks) {
    let s = input.spot;
    let k = input.strike;
    let t = input.time.max(1e-6);  // Prevent division by zero
    let v = input.vol.clamp(0.01, 5.0);  // Clamp volatility
    let r = input.rate;
    
    let sqrt_t = t.sqrt();
    let d1 = ((s / k).ln() + (r + 0.5 * v * v) * t) / (v * sqrt_t);
    let d2 = d1 - v * sqrt_t;
    
    let norm = Normal::new(0.0, 1.0).unwrap();
    let n_d1 = norm.cdf(d1);
    let n_d2 = norm.cdf(d2);
    let n_prime_d1 = norm.pdf(d1);
    
    let (price, delta) = match input.option_type {
        OptionType::Call => {
            let price = s * n_d1 - k * (-r * t).exp() * n_d2;
            let delta = n_d1;
            (price, delta)
        }
        OptionType::Put => {
            let price = k * (-r * t).exp() * norm.cdf(-d2) - s * norm.cdf(-d1);
            let delta = n_d1 - 1.0;
            (price, delta)
        }
    };
    
    let gamma = n_prime_d1 / (s * v * sqrt_t);
    let vega = s * n_prime_d1 * sqrt_t;
    let theta = -(s * n_prime_d1 * v) / (2.0 * sqrt_t)
        - r * k * (-r * t).exp() * n_d2;
    let rho = k * t * (-r * t).exp() * n_d2;
    
    (price, Greeks { delta, gamma, vega, theta, rho })
}
```

### Margin Calculation (margin.rs)

```rust
pub fn calculate_initial_margin(
    position: &Position,
    mark_price: f64,
    greeks: &Greeks,
    config: &MarginConfig,
) -> f64 {
    // Notional value
    let notional = position.quantity as f64
        * position.contract_size
        * mark_price;
    
    // Base margin (percentage of notional)
    let base_margin = notional * config.initial_margin_rate;
    
    // Add gamma risk
    let gamma_risk = greeks.gamma.abs() * notional * 0.01;
    
    // Add vega risk
    let vega_risk = greeks.vega.abs() * 0.1;
    
    base_margin + gamma_risk + vega_risk
}
```

### Liquidation Logic (liquidation.rs)

```rust
pub struct LiquidationEngine {
    risk_store: Arc<dyn RiskStore>,
    matching_client: Arc<dyn MatchingClient>,
}

impl LiquidationEngine {
    pub async fn check_liquidations(&self) -> Result<Vec<Uuid>, LiquidationError> {
        // Get all positions with margin < maintenance
        let at_risk = self.risk_store.get_undercollateralized().await?;
        
        let mut liquidated = Vec::new();
        
        for user_id in at_risk {
            if self.should_liquidate(user_id).await? {
                self.liquidate_user(user_id).await?;
                liquidated.push(user_id);
            }
        }
        
        Ok(liquidated)
    }
    
    async fn liquidate_user(&self, user_id: Uuid) -> Result<(), LiquidationError> {
        // 1. Freeze user's OMS access
        self.freeze_user(user_id).await?;
        
        // 2. Get all positions
        let positions = self.risk_store.get_user_positions(user_id).await?;
        
        // 3. Liquidate in order of risk contribution
        let sorted_positions = self.sort_by_risk(positions);
        
        for position in sorted_positions {
            // Try limit order first
            let success = self.liquidate_via_limit(position).await?;
            
            if !success {
                // Fallback to market order
                self.liquidate_via_market(position).await?;
            }
        }
        
        Ok(())
    }
}
```

---

## MODULE 5: CLEARING & SETTLEMENT

### Reference Documents
- Read: `/mnt/user-data/uploads/Clearing_and_setlement.md` (COMPLETELY)

### Two Modes

```rust
// Mode 1: Continuous Clearing (after every trade)
pub async fn clear_trade(&self, trade: Trade) -> Result<(), ClearingError> {
    // Update positions
    self.update_positions(&trade).await?;
    
    // Adjust margin
    self.adjust_margin(&trade).await?;
    
    // Calculate unrealized PnL
    self.update_pnl(&trade).await?;
    
    // Emit event
    self.emit_clearing_event(trade).await?;
    
    Ok(())
}

// Mode 2: Terminal Settlement (at expiry)
pub async fn settle_expiry(&self, instrument_id: &str) -> Result<(), SettlementError> {
    // 1. Get settlement price
    let settlement_price = self.get_settlement_price(instrument_id).await?;
    
    // 2. Get all positions for this instrument
    let positions = self.get_positions_for_instrument(instrument_id).await?;
    
    // 3. Calculate payoffs
    for position in positions {
        let payoff = calculate_payoff(
            position.option_type,
            position.strike_price,
            settlement_price,
            position.quantity,
            position.contract_size,
        );
        
        // 4. Update wallet balances
        self.settle_wallet(position.user_id, payoff).await?;
        
        // 5. Release margin
        self.release_margin(position.user_id, position.margin_locked).await?;
        
        // 6. Close position
        self.close_position(position.position_id).await?;
    }
    
    // 7. Mark instrument as settled
    self.update_instrument_status(instrument_id, InstrumentStatus::Settled).await?;
    
    Ok(())
}

fn calculate_payoff(
    option_type: OptionType,
    strike: f64,
    settlement: f64,
    quantity: i32,
    contract_size: f64,
) -> f64 {
    let intrinsic = match option_type {
        OptionType::Call => (settlement - strike).max(0.0),
        OptionType::Put => (strike - settlement).max(0.0),
    };
    
    intrinsic * quantity as f64 * contract_size
}
```

---

## MODULE 6: WALLET & COLLATERAL

### Reference Documents
- Read: `/mnt/user-data/uploads/Wallet_System.md`

### Key Invariant

```rust
// INVARIANT: Wallet balance ONLY changes on:
// 1. Deposit
// 2. Withdrawal
// 3. Settlement

// NOT on:
// - Trades (only margin changes)
// - Position updates
// - PnL changes (unrealized)
```

### Balance Management (balance.rs)

```rust
pub struct WalletBalance {
    pub user_id: Uuid,
    pub currency: String,
    pub total_balance: f64,
    pub available_balance: f64,   // total - locked
    pub locked_balance: f64,       // margin + pending withdrawals
}

impl WalletBalance {
    pub fn lock_margin(&mut self, amount: f64) -> Result<(), WalletError> {
        if self.available_balance < amount {
            return Err(WalletError::InsufficientBalance);
        }
        
        self.available_balance -= amount;
        self.locked_balance += amount;
        
        Ok(())
    }
    
    pub fn release_margin(&mut self, amount: f64) {
        self.locked_balance -= amount;
        self.available_balance += amount;
    }
}
```

---

## MODULE 7: MARKET DATA & PRICING

### Reference Documents
- Read: `/mnt/user-data/uploads/marketdatapricing.md` (COMPLETELY)

### Mark Price Calculation (mark_price.rs)

```rust
pub fn calculate_mark_price(
    index_price: f64,
    last_trade_price: Option<f64>,
    greeks_price: f64,
    bid: Option<f64>,
    ask: Option<f64>,
) -> f64 {
    // Mark price is model-based, not last trade
    let model_price = greeks_price;
    
    // Bound by bid/ask if they exist
    if let (Some(bid), Some(ask)) = (bid, ask) {
        let mid = (bid + ask) / 2.0;
        
        // Weight: 70% model, 30% market
        let mark = 0.7 * model_price + 0.3 * mid;
        
        // Clamp to bid/ask range
        mark.clamp(bid, ask)
    } else {
        model_price
    }
}
```

---

# PART 3: BLACK-SWAN TESTING

### Reference Documents
- Read: `/mnt/user-data/uploads/BlackSwan.md` (COMPLETELY)

### Required Test Scenarios

```rust
#[cfg(test)]
mod blackswan_tests {
    // Test 1: Replay Determinism
    #[tokio::test]
    async fn test_replay_consistency() {
        // Kill system mid-liquidation
        // Replay from event log
        // Assert: Same positions, balances, liquidations
    }
    
    // Test 2: Price Feed Attack
    #[tokio::test]
    async fn test_price_spike_rejection() {
        // One source spikes +500%
        // Others normal
        // Assert: Median rejects outlier
    }
    
    // Test 3: Liquidation Cascade
    #[tokio::test]
    async fn test_liquidation_cascade() {
        // BTC drops 20% instantly
        // Multiple users liquidated
        // Assert: Insurance fund survives, no negative balances
    }
    
    // Test 4: Circuit Breaker
    #[tokio::test]
    async fn test_circuit_breaker() {
        // Volatility > threshold
        // Assert: Matching paused, cancels allowed
    }
}
```

---

# PART 4: IMPLEMENTATION SEQUENCE

## Week 1-2: Foundation
1. Project structure setup
2. Core primitives
3. Instrument Layer (complete with tests)
4. Config loader for instruments

## Week 3-4: Order Flow
5. OMS module
6. Matching Engine
7. Integration tests (OMS → Matching)

## Week 5-6: Risk & Safety
8. Risk Engine (margin, Greeks)
9. Liquidation engine
10. Black-swan tests

## Week 7-8: Settlement & Completion
11. Clearing & Settlement
12. Wallet system
13. Market Data & Pricing
14. End-to-end integration
15. Production hardening

---

# PART 5: TESTING REQUIREMENTS

## Unit Tests
- Every public function
- Edge cases (zero, negative, overflow)
- Error paths

## Integration Tests
- Use testcontainers for real databases
- Test module interactions
- Test config-driven wiring

## Property-Based Tests (proptest)
- Invariants hold under random inputs
- No negative balances
- Margin always sufficient
- Deterministic matching

## Black-Swan Tests
- Liquidation cascades
- Price shocks
- Replay determinism
- Circuit breakers

---

# PART 6: CLAUDE CODE EXECUTION INSTRUCTIONS

When you (Claude Code) start building:

1. **Read this guide completely**
2. **Read the specific module's architecture document**
3. **Create the file structure from PROJECT_STRUCTURE.md**
4. **Build one module at a time in the order listed**
5. **Write tests BEFORE implementing (TDD)**
6. **Validate against config after each module**
7. **Run black-swan tests before calling module complete**

For each module:
```bash
# Step 1: Create files
touch core/instrument/domain.rs
touch core/instrument/traits.rs
touch core/instrument/registry.rs

# Step 2: Implement domain types

# Step 3: Implement traits

# Step 4: Implement business logic

# Step 5: Create adapter (e.g., Postgres)

# Step 6: Write tests

# Step 7: Run tests
cargo test --package instrument

# Step 8: Integration test
cargo test --test instrument_integration
```

---

# END OF MASTER IMPLEMENTATION GUIDE

This document is complete. Follow it exactly.
The architecture is battle-tested.
Build it module by module.
Test relentlessly.

Good luck.
