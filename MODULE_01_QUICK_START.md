# MODULE 01: INSTRUMENT LAYER - QUICK START GUIDE
# For Claude Code - Build This First

## WHAT TO BUILD

The Market Instrument Layer - defines what can be traded (option instruments).

## FILES TO CREATE (In Order)

```
core/instrument/
├── domain.rs           ← Domain types (Asset, Currency, Market, OptionInstrument)
├── validation.rs       ← Validation logic
├── traits.rs          ← InstrumentStore trait
├── registry.rs        ← Business logic
└── lib.rs             ← Module exports

adapters/storage/postgres/
└── instruments.rs     ← Postgres implementation

tests/
└── instrument_tests.rs ← Integration tests
```

## STEP 1: Domain Types (domain.rs)

Copy this EXACTLY:

```rust
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use uuid::Uuid;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Hash)]
pub struct Asset {
    pub asset_id: String,
    pub name: String,
    pub chain: String,
    pub decimals: u8,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Hash)]
pub struct Currency {
    pub currency_id: String,
    pub name: String,
    pub decimals: u8,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OptionInstrument {
    pub instrument_id: String,
    pub market_id: Uuid,
    pub underlying_asset: Asset,
    pub option_type: OptionType,
    pub style: OptionStyle,
    pub strike_price: f64,
    pub expiry_timestamp: DateTime<Utc>,
    pub contract_size: f64,
    pub min_order_size: u32,
    pub settlement_currency: Currency,
    pub tick_size: f64,
    pub status: InstrumentStatus,
    pub created_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
pub enum OptionType { Call, Put }

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
pub enum OptionStyle { European }

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
pub enum InstrumentStatus {
    Draft, Listed, Active, Expired, Settled, Archived
}

impl OptionInstrument {
    pub fn canonical_name(&self) -> String {
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
        let mut hasher = Sha256::new();
        hasher.update(self.canonical_name().as_bytes());
        format!("{:x}", hasher.finalize())
    }
}
```

## STEP 2: Validation (validation.rs)

```rust
use super::domain::*;
use chrono::Utc;
use thiserror::Error;

#[derive(Debug, Error)]
pub enum ValidationError {
    #[error("Expiry must be in future")]
    ExpiryInPast,
    #[error("Strike must be positive")]
    InvalidStrike,
    #[error("Strike not aligned to tick")]
    StrikeMisaligned,
}

pub fn validate_instrument(instrument: &OptionInstrument) -> Result<(), ValidationError> {
    if instrument.expiry_timestamp <= Utc::now() {
        return Err(ValidationError::ExpiryInPast);
    }
    if instrument.strike_price <= 0.0 {
        return Err(ValidationError::InvalidStrike);
    }
    if (instrument.strike_price % instrument.tick_size).abs() > 1e-6 {
        return Err(ValidationError::StrikeMisaligned);
    }
    Ok(())
}
```

## STEP 3: Storage Trait (traits.rs)

```rust
use async_trait::async_trait;
use super::domain::*;

#[async_trait]
pub trait InstrumentStore: Send + Sync {
    async fn create_instrument(&self, instrument: OptionInstrument) -> Result<String, InstrumentError>;
    async fn get_instrument(&self, id: &str) -> Result<Option<OptionInstrument>, InstrumentError>;
    async fn list_instruments(&self, market_id: Uuid) -> Result<Vec<OptionInstrument>, InstrumentError>;
}
```

## STEP 4: Business Logic (registry.rs)

```rust
use std::sync::Arc;

pub struct InstrumentRegistry {
    store: Arc<dyn InstrumentStore>,
}

impl InstrumentRegistry {
    pub fn new(store: Arc<dyn InstrumentStore>) -> Self {
        Self { store }
    }
    
    pub async fn create_instrument(&self, mut instrument: OptionInstrument) -> Result<String, InstrumentError> {
        validate_instrument(&instrument)?;
        instrument.instrument_id = instrument.generate_id();
        self.store.create_instrument(instrument).await
    }
}
```

## STEP 5: Tests

```rust
#[cfg(test)]
mod tests {
    #[tokio::test]
    async fn test_create_instrument() {
        let store = Arc::new(InMemoryInstrumentStore::new());
        let registry = InstrumentRegistry::new(store);
        
        let instrument = create_test_instrument();
        let id = registry.create_instrument(instrument).await.unwrap();
        
        let retrieved = registry.get_instrument(&id).await.unwrap();
        assert_eq!(retrieved.strike_price, 50000.0);
    }
}
```

## BUILD COMMAND

```bash
cd core/instrument
cargo test
```

## SUCCESS CRITERIA

- [ ] All types compile
- [ ] Validation tests pass
- [ ] Can create and retrieve instruments
- [ ] Deterministic IDs work

## NEXT: Build OMS Module

After this works, request MODULE_02_OMS_IMPLEMENTATION.md
