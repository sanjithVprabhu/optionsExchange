# MASTER RULES - WHITE-LABEL OPTIONS EXCHANGE
# The Constitutional Document
# Version 1.0

---

## PURPOSE OF THIS DOCUMENT

This document defines the **NON-NEGOTIABLE RULES** that ALL modules must follow.

**READ THIS FIRST** before implementing any module.

Every module implementation guide references these rules.
Violations of these rules = system failure.

---

# PART 1: SYSTEM-WIDE INVARIANTS (NEVER BREAK THESE)

## INVARIANT 1: No Negative Balances

```
∀ user, ∀ time: wallet_balance(user, time) ≥ 0
```

**What this means:**
- Users can NEVER have negative wallet balances
- Margin must be locked BEFORE risk is taken
- Liquidations must complete before bankruptcy

**How to enforce:**
- Lock margin synchronously before order enters matching
- Use database transactions with serializable isolation
- Insurance fund absorbs losses beyond user collateral

**Test requirement:**
```rust
#[test]
fn invariant_no_negative_balances() {
    // Run 10,000 random trades
    // Assert: No wallet balance ever goes negative
}
```

---

## INVARIANT 2: No Trade Without Risk Approval

```
∀ order: matched(order) ⇒ risk_approved(order)
```

**What this means:**
- Orders cannot reach the matching engine without passing risk checks
- OMS gates on risk engine approval

**Flow:**
```
User submits order
    ↓
OMS stores with status = PendingRisk
    ↓
Risk Engine checks margin
    ↓
If approved → status = Open → sent to Matcher
If rejected → status = Rejected → user notified
```

**Test requirement:**
```rust
#[test]
fn invariant_risk_gates_matching() {
    // Submit order with insufficient margin
    // Assert: Order rejected, never reaches matcher
}
```

---

## INVARIANT 3: Matching is Deterministic

```
Same inputs → Same outputs (ALWAYS)
```

**What this means:**
- Given the same sequence of orders, matching produces identical trades
- No randomness, no system time, no external calls during matching
- Strict FIFO at each price level

**Why critical:**
- Enables replay for debugging
- Legal/regulatory compliance
- Prevents disputes

**Test requirement:**
```rust
#[test]
fn invariant_matching_determinism() {
    let orders = create_order_sequence();
    
    // Run matching 1000 times
    let mut results = Vec::new();
    for _ in 0..1000 {
        let trades = run_matching(orders.clone());
        results.push(trades);
    }
    
    // All results must be IDENTICAL
    for i in 1..results.len() {
        assert_eq!(results[0], results[i]);
    }
}
```

---

## INVARIANT 4: State is Derivable from Events

```
State = f(Events)
```

**What this means:**
- All state changes emit events
- Events are append-only, immutable
- Current state can be reconstructed by replaying events

**Event types:**
- OrderPlaced
- OrderMatched
- TradeExecuted
- PositionUpdated
- MarginAdjusted
- InstrumentExpired
- BalanceUpdated

**Test requirement:**
```rust
#[test]
fn invariant_event_replay() {
    // Run exchange for 1 hour
    // Snapshot final state
    // Kill everything, replay from event log
    // Assert: Final state identical
}
```

---

## INVARIANT 5: Losses Never Exceed Collateral + Insurance

```
∀ user: realized_loss(user) ≤ collateral(user) + insurance_fund
```

**What this means:**
- Users lose at most their collateral
- System loss absorbed by insurance fund
- Exchange operators never liable for user losses

**How to enforce:**
- Adequate margin requirements
- Timely liquidations
- Properly sized insurance fund

**Test requirement:**
```rust
#[test]
fn invariant_loss_containment() {
    // Simulate black swan (BTC -50% in 1 minute)
    // Assert: No user loss > their collateral
    // Assert: Insurance fund survives or ADL triggers
}
```

---

## INVARIANT 6: Liquidations are Monotonic

```
liquidation(position) → user_equity ≤ initial_equity
```

**What this means:**
- Liquidations never improve user position
- They only reduce risk
- No "accidental profit" from being liquidated

**Test requirement:**
```rust
#[test]
fn invariant_liquidation_monotonic() {
    // Liquidate user
    // Assert: equity_after ≤ equity_before
}
```

---

# PART 2: ARCHITECTURAL PATTERNS (FOLLOW EXACTLY)

## PATTERN 1: Trait-Based Abstraction

### Rule
**Core modules NEVER import infrastructure.**

❌ **WRONG:**
```rust
// core/instrument/registry.rs
use sqlx::PgPool;  // ❌ NEVER DO THIS

pub struct InstrumentRegistry {
    db: PgPool,  // ❌ Direct dependency on Postgres
}
```

✅ **CORRECT:**
```rust
// core/instrument/traits.rs
#[async_trait]
pub trait InstrumentStore: Send + Sync {
    async fn create(&self, instrument: Instrument) -> Result<String>;
    async fn get(&self, id: &str) -> Result<Option<Instrument>>;
}

// core/instrument/registry.rs
pub struct InstrumentRegistry {
    store: Arc<dyn InstrumentStore>,  // ✅ Trait, not concrete type
}

// adapters/storage/postgres/instruments.rs
pub struct PostgresInstrumentStore {
    pool: PgPool,  // ✅ Infrastructure isolated here
}

impl InstrumentStore for PostgresInstrumentStore {
    // Implementation
}
```

### Benefits
- Customers swap Postgres → Supabase → MySQL by changing config
- Core logic testable with in-memory stores
- No compile-time coupling to infrastructure

---

## PATTERN 2: Config-Driven Wiring

### Rule
**All infrastructure choices come from config, not code.**

```rust
// config_loader/builder.rs
pub fn build_instrument_layer(config: InstrumentConfig) 
    -> Result<InstrumentRegistry> 
{
    // Read config to determine which adapter
    let store: Arc<dyn InstrumentStore> = match config.storage_type.as_str() {
        "postgres" => {
            let pool = create_postgres_pool(&config.postgres).await?;
            Arc::new(PostgresInstrumentStore::new(pool))
        }
        "supabase" => {
            let client = create_supabase_client(&config.supabase)?;
            Arc::new(SupabaseInstrumentStore::new(client))
        }
        "inmemory" => {
            Arc::new(InMemoryInstrumentStore::new())
        }
        _ => return Err(BuildError::UnsupportedStorage),
    };
    
    Ok(InstrumentRegistry::new(store))
}
```

### Config Structure
```yaml
instrument_layer:
  storage:
    type: "postgres"  # or "supabase", "inmemory"
    postgres:
      host: "${DB_HOST}"
      database: "instruments"
```

---

## PATTERN 3: Event-Driven State Updates

### Rule
**State changes emit events. Events are source of truth.**

```rust
// Event definition
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum ExchangeEvent {
    OrderPlaced(OrderPlacedEvent),
    TradeExecuted(TradeExecutedEvent),
    PositionUpdated(PositionUpdatedEvent),
    // ... etc
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TradeExecutedEvent {
    pub event_id: Uuid,
    pub trade_id: Uuid,
    pub instrument_id: String,
    pub buyer_id: Uuid,
    pub seller_id: Uuid,
    pub price: f64,
    pub quantity: u32,
    pub timestamp: DateTime<Utc>,
    pub sequence: u64,
}

// Every state-changing operation emits event
impl ClearingEngine {
    pub async fn clear_trade(&self, trade: Trade) -> Result<()> {
        // 1. Update state
        self.update_positions(&trade).await?;
        
        // 2. Emit event
        let event = ExchangeEvent::TradeExecuted(TradeExecutedEvent {
            event_id: Uuid::new_v4(),
            trade_id: trade.id,
            // ... fill in fields
        });
        
        self.event_log.append(event).await?;
        
        Ok(())
    }
}
```

### Event Log Properties
- Append-only (never delete/edit)
- Sequenced (monotonic sequence numbers)
- Partitioned by time (for efficient replay)
- Retained indefinitely (compliance requirement)

---

## PATTERN 4: Module Communication

### Rule
**Modules communicate via defined interfaces, never direct coupling.**

### Communication Methods

#### Method 1: Trait-based (preferred for sync logic)
```rust
// OMS needs Risk Engine
pub struct OrderManagementSystem {
    risk_client: Arc<dyn RiskClient>,
}

#[async_trait]
pub trait RiskClient: Send + Sync {
    async fn check_order(&self, order: &Order) -> Result<RiskApproval>;
}
```

#### Method 2: gRPC (preferred for distributed services)
```rust
// Service hosts gRPC endpoint
// Other services call via gRPC client
let risk_client = RiskServiceClient::connect(config.risk_service_url).await?;
let approval = risk_client.check_order(order).await?;
```

#### Method 3: Event bus (for async notifications)
```rust
// Matching engine emits trade events
// Settlement service subscribes to trades
self.event_bus.publish(ExchangeEvent::TradeExecuted(trade)).await?;
```

### Module Dependency Rules

```
Allowed dependencies:
Instrument → (none)
OMS → Instrument, Risk
Matching → Instrument
Risk → Instrument, MarketData
Settlement → Instrument, Wallet
Wallet → (none)
MarketData → Instrument, Matching

Forbidden dependencies:
- No circular dependencies
- Matching NEVER depends on Settlement
- OMS NEVER depends on Matching
```

---

## PATTERN 5: Error Handling

### Rule
**Use Result types. Panic only on programmer errors.**

```rust
// Define module-specific error types
#[derive(Debug, thiserror::Error)]
pub enum InstrumentError {
    #[error("Instrument not found: {0}")]
    NotFound(String),
    
    #[error("Invalid instrument: {0}")]
    Invalid(String),
    
    #[error("Database error: {0}")]
    Database(String),
}

// All fallible operations return Result
pub async fn create_instrument(
    &self,
    instrument: Instrument
) -> Result<String, InstrumentError> {
    // Validate
    validate_instrument(&instrument)
        .map_err(|e| InstrumentError::Invalid(e.to_string()))?;
    
    // Store
    self.store.create(instrument).await
        .map_err(|e| InstrumentError::Database(e.to_string()))
}

// Panic only for logic errors (programmer mistakes)
fn calculate_delta(spot: f64, strike: f64) -> f64 {
    assert!(spot > 0.0, "Spot price must be positive");  // OK to panic
    assert!(strike > 0.0, "Strike must be positive");    // OK to panic
    // ... calculation
}
```

### Error Propagation
- Use `?` operator for propagation
- Convert errors at module boundaries
- Log errors at service layer
- Never swallow errors silently

---

# PART 3: DATA MODELING RULES

## RULE 1: Money is Never f64 in Storage

**Problem:** Floating point has rounding errors.

**Solution:** Store as integers (smallest unit).

```rust
// ❌ WRONG
pub struct Balance {
    pub amount: f64,  // ❌ Rounding errors
}

// ✅ CORRECT
pub struct Balance {
    pub amount_cents: i64,  // ✅ Store as cents/satoshis
    pub decimals: u8,       // ✅ Know the precision
}

impl Balance {
    pub fn to_float(&self) -> f64 {
        self.amount_cents as f64 / 10_f64.powi(self.decimals as i32)
    }
    
    pub fn from_float(amount: f64, decimals: u8) -> Self {
        let multiplier = 10_f64.powi(decimals as i32);
        Self {
            amount_cents: (amount * multiplier).round() as i64,
            decimals,
        }
    }
}
```

**Exception:** Calculations (Greeks, pricing) use f64, but ONLY for storage we use integers.

---

## RULE 2: Timestamps are Always UTC

```rust
use chrono::{DateTime, Utc};

// ✅ ALWAYS use Utc
pub struct Trade {
    pub timestamp: DateTime<Utc>,
}

// ❌ NEVER use Local or naive timestamps
```

---

## RULE 3: UUIDs for IDs, Deterministic Hashes for Instruments

```rust
// User-generated entities: UUID
pub struct Order {
    pub order_id: Uuid,  // ✅ Random UUID
}

// Deterministic entities: Hash
pub struct Instrument {
    pub instrument_id: String,  // ✅ Hash of canonical name
}

impl Instrument {
    pub fn generate_id(&self) -> String {
        use sha2::{Sha256, Digest};
        let canonical = self.canonical_name();
        let mut hasher = Sha256::new();
        hasher.update(canonical.as_bytes());
        format!("{:x}", hasher.finalize())
    }
}
```

---

## RULE 4: Enums for State Machines

```rust
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub enum OrderStatus {
    PendingRisk,
    Open,
    PartiallyFilled,
    Filled,
    Cancelled,
    Rejected,
}

impl OrderStatus {
    pub fn can_transition_to(&self, new: &OrderStatus) -> bool {
        match (self, new) {
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

---

# PART 4: TESTING REQUIREMENTS

## Test Level 1: Unit Tests

**What:** Test individual functions.

**Coverage:** Every public function.

**Example:**
```rust
#[cfg(test)]
mod tests {
    use super::*;
    
    #[test]
    fn test_validate_instrument_rejects_expired() {
        let instrument = Instrument {
            expiry: Utc::now() - Duration::hours(1),
            // ... other fields
        };
        
        let result = validate_instrument(&instrument);
        assert!(result.is_err());
    }
    
    #[test]
    fn test_validate_instrument_accepts_valid() {
        let instrument = create_valid_instrument();
        let result = validate_instrument(&instrument);
        assert!(result.is_ok());
    }
}
```

---

## Test Level 2: Integration Tests

**What:** Test module interactions.

**Use:** Real databases (testcontainers).

**Example:**
```rust
#[cfg(test)]
mod integration {
    use testcontainers::*;
    
    #[tokio::test]
    async fn test_order_flow_end_to_end() {
        // Setup real Postgres
        let docker = clients::Cli::default();
        let postgres = docker.run(images::postgres::Postgres::default());
        let pool = create_pool(&postgres).await;
        
        // Build services
        let instrument_svc = build_instrument_service(pool.clone()).await;
        let oms = build_oms(pool.clone()).await;
        let matcher = build_matcher().await;
        
        // Test flow
        let instrument_id = instrument_svc.create_instrument(...).await?;
        let order = oms.submit_order(...).await?;
        let trades = matcher.match_order(order).await?;
        
        assert!(!trades.is_empty());
    }
}
```

---

## Test Level 3: Property-Based Tests

**What:** Test invariants hold under random inputs.

**Use:** `proptest` crate.

**Example:**
```rust
use proptest::prelude::*;

proptest! {
    #[test]
    fn no_negative_balances_under_random_trades(
        trades in prop::collection::vec(arbitrary_trade(), 100..1000)
    ) {
        let mut exchange = create_test_exchange();
        
        for trade in trades {
            exchange.execute_trade(trade);
        }
        
        // INVARIANT: No negative balances
        for user in exchange.all_users() {
            let balance = exchange.get_balance(user);
            prop_assert!(balance >= 0.0);
        }
    }
}
```

---

## Test Level 4: Black-Swan Tests

**What:** Test system survives extreme scenarios.

**Required scenarios:**
1. Replay determinism (kill mid-process, replay events)
2. Price feed attack (one source spikes 500%)
3. Liquidation cascade (BTC -20% instant)
4. Circuit breaker (volatility threshold)
5. Negative balance attempt (race OMS vs wallet)

**Example:**
```rust
#[tokio::test]
async fn blackswan_liquidation_cascade() {
    let mut exchange = create_test_exchange();
    
    // Setup: 100 users with leveraged positions
    for i in 0..100 {
        let user = create_user();
        exchange.open_position(user, leveraged_position()).await;
    }
    
    // Black swan: BTC drops 20% instantly
    exchange.update_index_price("BTC", -0.20).await;
    
    // Run liquidation engine
    let liquidated = exchange.run_liquidations().await?;
    
    // INVARIANTS
    assert!(liquidated.len() > 0, "Some users should be liquidated");
    
    for user in exchange.all_users() {
        let balance = exchange.get_balance(user);
        assert!(balance >= 0.0, "No negative balances");
    }
    
    let insurance_fund = exchange.get_insurance_fund();
    assert!(insurance_fund >= 0.0, "Insurance fund must survive");
}
```

---

# PART 5: CONFIGURATION INTEGRATION

## Config Structure for Each Module

Every module must:
1. Define its config schema
2. Implement a Builder that reads config
3. Support multiple storage backends
4. Validate config before starting

**Example:**
```rust
// Module config schema
#[derive(Debug, Deserialize)]
pub struct InstrumentLayerConfig {
    pub supported_assets: Vec<AssetConfig>,
    pub storage: StorageConfig,
}

#[derive(Debug, Deserialize)]
pub struct StorageConfig {
    pub r#type: String,  // "postgres", "supabase", "inmemory"
    pub postgres: Option<PostgresConfig>,
    pub supabase: Option<SupabaseConfig>,
}

// Builder
pub struct InstrumentLayerBuilder;

impl InstrumentLayerBuilder {
    pub async fn build(config: InstrumentLayerConfig) 
        -> Result<InstrumentRegistry> 
    {
        // Validate config
        validate_config(&config)?;
        
        // Choose storage based on config
        let store = match config.storage.r#type.as_str() {
            "postgres" => {
                let cfg = config.storage.postgres
                    .ok_or(BuildError::MissingPostgresConfig)?;
                Arc::new(PostgresInstrumentStore::new(cfg).await?)
            }
            "supabase" => {
                let cfg = config.storage.supabase
                    .ok_or(BuildError::MissingSupabaseConfig)?;
                Arc::new(SupabaseInstrumentStore::new(cfg)?)
            }
            "inmemory" => Arc::new(InMemoryInstrumentStore::new()),
            _ => return Err(BuildError::UnsupportedStorage(config.storage.r#type)),
        };
        
        Ok(InstrumentRegistry::new(store))
    }
}
```

---

# PART 6: DATABASE SCHEMA RULES

## Schema Design Principles

1. **Event log is append-only**
   ```sql
   CREATE TABLE events (
       event_id UUID PRIMARY KEY,
       sequence BIGSERIAL UNIQUE NOT NULL,
       event_type VARCHAR(50) NOT NULL,
       payload JSONB NOT NULL,
       created_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
   );
   
   -- Index for sequential reading
   CREATE INDEX idx_events_sequence ON events(sequence);
   ```

2. **Materialized views for current state**
   ```sql
   -- Event log is source of truth
   -- This table is derived
   CREATE TABLE positions (
       position_id UUID PRIMARY KEY,
       user_id UUID NOT NULL,
       instrument_id VARCHAR(64) NOT NULL,
       quantity INTEGER NOT NULL,
       avg_price DECIMAL(18, 6) NOT NULL,
       updated_at TIMESTAMPTZ NOT NULL
   );
   ```

3. **Use serializable isolation for critical operations**
   ```sql
   BEGIN TRANSACTION ISOLATION LEVEL SERIALIZABLE;
   
   -- Lock margin
   UPDATE wallets
   SET locked_balance = locked_balance + $1
   WHERE user_id = $2
   AND available_balance >= $1;
   
   -- If no rows updated, insufficient balance
   
   COMMIT;
   ```

4. **Partition large tables**
   ```sql
   CREATE TABLE trades (
       trade_id UUID NOT NULL,
       instrument_id VARCHAR(64) NOT NULL,
       created_at TIMESTAMPTZ NOT NULL,
       -- ... other fields
       PRIMARY KEY (trade_id, created_at)
   ) PARTITION BY RANGE (created_at);
   
   -- Create monthly partitions
   CREATE TABLE trades_2026_01 PARTITION OF trades
       FOR VALUES FROM ('2026-01-01') TO ('2026-02-01');
   ```

---

# PART 7: SECURITY RULES

## Rule 1: Never Trust User Input

```rust
pub async fn submit_order(&self, order: Order) -> Result<Uuid> {
    // Validate EVERYTHING
    if order.quantity == 0 {
        return Err(OrderError::InvalidQuantity);
    }
    
    if order.quantity > MAX_ORDER_SIZE {
        return Err(OrderError::QuantityTooLarge);
    }
    
    if let Some(price) = order.price {
        if price <= 0.0 {
            return Err(OrderError::InvalidPrice);
        }
        
        // Check price deviation from mark
        let mark_price = self.get_mark_price(&order.instrument_id).await?;
        let deviation = (price - mark_price).abs() / mark_price;
        if deviation > MAX_PRICE_DEVIATION {
            return Err(OrderError::PriceDeviationTooLarge);
        }
    }
    
    // ... more validation
}
```

## Rule 2: Authentication & Authorization

```rust
#[async_trait]
pub trait AuthService: Send + Sync {
    async fn verify_token(&self, token: &str) -> Result<UserId>;
    async fn check_permission(&self, user: UserId, action: Action) -> Result<bool>;
}

// In API handlers
pub async fn handle_submit_order(
    auth: &dyn AuthService,
    token: String,
    order: Order,
) -> Result<Response> {
    // Verify user
    let user_id = auth.verify_token(&token).await?;
    
    // Check permission
    if !auth.check_permission(user_id, Action::SubmitOrder).await? {
        return Err(ApiError::Unauthorized);
    }
    
    // Verify user owns this order
    if order.user_id != user_id {
        return Err(ApiError::Forbidden);
    }
    
    // Process order
    // ...
}
```

## Rule 3: Rate Limiting

```rust
pub struct RateLimiter {
    limits: HashMap<UserId, TokenBucket>,
}

impl RateLimiter {
    pub async fn check_limit(&mut self, user: UserId) -> Result<()> {
        let bucket = self.limits.entry(user).or_insert_with(|| {
            TokenBucket::new(100, Duration::from_secs(1))  // 100 req/sec
        });
        
        if bucket.try_consume(1) {
            Ok(())
        } else {
            Err(RateLimitError::TooManyRequests)
        }
    }
}
```

---

# PART 8: PERFORMANCE RULES

## Rule 1: Use Connection Pools

```rust
// ✅ CORRECT: Pool connections
let pool = PgPoolOptions::new()
    .max_connections(20)
    .connect(&database_url)
    .await?;

// ❌ WRONG: Connect per request
async fn get_order(order_id: Uuid) -> Result<Order> {
    let conn = PgConnection::connect(&database_url).await?;  // ❌ Slow!
    // ...
}
```

## Rule 2: Batch Operations

```rust
// ✅ CORRECT: Batch inserts
let mut tx = pool.begin().await?;
for trade in trades {
    sqlx::query!("INSERT INTO trades (...) VALUES (...)")
        .execute(&mut tx)
        .await?;
}
tx.commit().await?;

// Even better: Use COPY for bulk inserts
```

## Rule 3: Index Properly

```sql
-- ✅ CORRECT: Index frequently queried columns
CREATE INDEX idx_orders_user_instrument 
    ON orders(user_id, instrument_id);

CREATE INDEX idx_trades_instrument_timestamp
    ON trades(instrument_id, created_at DESC);
```

## Rule 4: Cache Aggressively (Where Safe)

```rust
// Instrument data rarely changes - cache it
pub struct InstrumentCache {
    cache: Arc<RwLock<HashMap<String, Instrument>>>,
    store: Arc<dyn InstrumentStore>,
    ttl: Duration,
}

impl InstrumentCache {
    pub async fn get(&self, id: &str) -> Result<Instrument> {
        // Check cache
        {
            let cache = self.cache.read().await;
            if let Some(instrument) = cache.get(id) {
                return Ok(instrument.clone());
            }
        }
        
        // Cache miss - fetch from store
        let instrument = self.store.get(id).await?
            .ok_or(InstrumentError::NotFound(id.to_string()))?;
        
        // Update cache
        {
            let mut cache = self.cache.write().await;
            cache.insert(id.to_string(), instrument.clone());
        }
        
        Ok(instrument)
    }
}
```

---

# PART 9: MONITORING & OBSERVABILITY

## Required Metrics

Every module must expose:

```rust
use prometheus::{IntCounter, Histogram, register_int_counter, register_histogram};

pub struct ModuleMetrics {
    pub operations_total: IntCounter,
    pub operation_duration: Histogram,
    pub errors_total: IntCounter,
}

impl ModuleMetrics {
    pub fn new(module_name: &str) -> Self {
        Self {
            operations_total: register_int_counter!(
                format!("{}_operations_total", module_name),
                "Total operations"
            ).unwrap(),
            
            operation_duration: register_histogram!(
                format!("{}_operation_duration_seconds", module_name),
                "Operation duration"
            ).unwrap(),
            
            errors_total: register_int_counter!(
                format!("{}_errors_total", module_name),
                "Total errors"
            ).unwrap(),
        }
    }
}

// Usage
pub async fn submit_order(&self, order: Order) -> Result<Uuid> {
    let timer = self.metrics.operation_duration.start_timer();
    self.metrics.operations_total.inc();
    
    let result = self.submit_order_impl(order).await;
    
    if result.is_err() {
        self.metrics.errors_total.inc();
    }
    
    timer.observe_duration();
    result
}
```

## Structured Logging

```rust
use tracing::{info, warn, error, instrument};

#[instrument(skip(self))]
pub async fn submit_order(&self, order: Order) -> Result<Uuid> {
    info!(
        order_id = %order.order_id,
        user_id = %order.user_id,
        instrument_id = %order.instrument_id,
        "Submitting order"
    );
    
    match self.submit_order_impl(order).await {
        Ok(id) => {
            info!(order_id = %id, "Order submitted successfully");
            Ok(id)
        }
        Err(e) => {
            error!(error = %e, "Failed to submit order");
            Err(e)
        }
    }
}
```

---

# PART 10: DEPLOYMENT CHECKLIST

Before deploying ANY module to production:

- [ ] All unit tests pass
- [ ] All integration tests pass
- [ ] All property-based tests pass
- [ ] All black-swan tests pass
- [ ] Config validation implemented
- [ ] Error handling complete (no unwrap() in production paths)
- [ ] Metrics exposed
- [ ] Logging implemented
- [ ] Database migrations tested
- [ ] Rollback plan exists
- [ ] Circuit breakers configured
- [ ] Rate limiting enabled
- [ ] Authentication/authorization working
- [ ] TLS enabled
- [ ] Secrets in environment variables (not hardcoded)
- [ ] Connection pools configured
- [ ] Monitoring dashboards created
- [ ] Alerting rules defined
- [ ] Documentation updated

---

# PART 11: REFERENCE TO MODULE GUIDES

Each module implementation guide will follow this structure:

```
MODULE_XX_<NAME>_IMPLEMENTATION.md
├── 1. Purpose & Responsibility
├── 2. Domain Types (complete Rust code)
├── 3. Trait Definitions (storage, clients, etc.)
├── 4. Business Logic Implementation
├── 5. Adapter Implementations
│   ├── Postgres
│   ├── Supabase
│   └── In-memory
├── 6. Integration Points (how it talks to other modules)
├── 7. Config Schema
├── 8. Complete Test Suite
├── 9. Edge Cases & Validation
└── 10. Deployment Considerations
```

When you request a module implementation guide, I will:
1. Reference these MASTER_RULES
2. Use your architecture documents for that module
3. Provide complete, copy-paste-ready Rust code
4. Include all tests
5. Show exact config integration

---

# END OF MASTER_RULES.md

**These rules are non-negotiable.**

Every module must follow these patterns.
Violations = system failure.

When Claude Code implements a module:
1. Read MASTER_RULES.md (this document)
2. Read PROJECT_STRUCTURE.md (for file layout)
3. Read MODULE_XX_IMPLEMENTATION.md (for specific module)
4. Follow ALL rules exactly

Good luck building a bulletproof options exchange.
