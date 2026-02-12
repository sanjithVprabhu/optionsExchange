use chrono::{DateTime, Utc};
use exchange_primitives::{
    Asset, Currency, InstrumentStatus, MarketStatus, MarketType, OptionStyle, OptionType,
};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use uuid::Uuid;

/// A Market groups related instruments under a single underlying asset
/// and settlement currency. For example, "BTC Options settled in USDT".
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Market {
    pub market_id: Uuid,
    pub underlying_asset: Asset,
    pub settlement_currency: Currency,
    pub market_type: MarketType,
    pub status: MarketStatus,
    pub created_at: DateTime<Utc>,
}

impl Market {
    pub fn new(underlying_asset: Asset, settlement_currency: Currency) -> Self {
        Self {
            market_id: Uuid::new_v4(),
            underlying_asset,
            settlement_currency,
            market_type: MarketType::Options,
            status: MarketStatus::Active,
            created_at: Utc::now(),
        }
    }
}

/// The core option instrument definition.
///
/// Represents a single tradeable option contract. For example:
/// BTC-28MAR2026-50000-C (BTC Call, strike 50000, expiring 28 Mar 2026).
///
/// Instrument IDs are deterministic SHA-256 hashes of the canonical name,
/// ensuring the same instrument always gets the same ID regardless of
/// which node creates it.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OptionInstrument {
    /// Deterministic hash of canonical_name()
    pub instrument_id: String,
    /// The market this instrument belongs to
    pub market_id: Uuid,

    /// The underlying asset (BTC, ETH, etc.)
    pub underlying_asset: Asset,
    /// Call or Put
    pub option_type: OptionType,
    /// Exercise style (European only in v0)
    pub style: OptionStyle,

    /// Strike price in settlement currency
    pub strike_price: f64,
    /// When the option expires (always UTC)
    pub expiry_timestamp: DateTime<Utc>,

    /// How much underlying per contract (e.g., 0.01 BTC)
    pub contract_size: f64,
    /// Minimum contracts per order
    pub min_order_size: u32,
    /// Settlement currency for this instrument
    pub settlement_currency: Currency,
    /// Minimum price increment
    pub tick_size: f64,

    /// Current lifecycle status
    pub status: InstrumentStatus,
    /// When this instrument was created
    pub created_at: DateTime<Utc>,
}

impl OptionInstrument {
    /// Generates the canonical human-readable name.
    /// Format: {ASSET}-{DDMMMYYYY}-{STRIKE}-{C/P}
    /// Example: BTC-28MAR2026-50000-C
    pub fn canonical_name(&self) -> String {
        format!(
            "{}-{}-{}-{}",
            self.underlying_asset.asset_id,
            self.expiry_timestamp
                .format("%d%b%Y")
                .to_string()
                .to_uppercase(),
            self.strike_price as u64,
            self.option_type
        )
    }

    /// Generates a deterministic instrument ID by hashing the canonical name.
    /// Same instrument parameters always produce the same ID.
    pub fn generate_id(&self) -> String {
        let mut hasher = Sha256::new();
        hasher.update(self.canonical_name().as_bytes());
        format!("{:x}", hasher.finalize())
    }

    /// Returns true if the instrument has expired.
    pub fn is_expired(&self) -> bool {
        self.expiry_timestamp <= Utc::now()
    }

    /// Returns true if the instrument is actively tradeable.
    pub fn is_tradeable(&self) -> bool {
        self.status == InstrumentStatus::Active && !self.is_expired()
    }

    /// Time remaining until expiry in fractional years (for Black-Scholes).
    pub fn time_to_expiry_years(&self) -> f64 {
        let duration = self.expiry_timestamp - Utc::now();
        let seconds = duration.num_seconds().max(0) as f64;
        seconds / (365.25 * 24.0 * 3600.0)
    }
}

/// Builder for constructing OptionInstrument instances.
/// Ensures instrument_id is always set via generate_id().
pub struct OptionInstrumentBuilder {
    market_id: Uuid,
    underlying_asset: Asset,
    option_type: OptionType,
    style: OptionStyle,
    strike_price: f64,
    expiry_timestamp: DateTime<Utc>,
    contract_size: f64,
    min_order_size: u32,
    settlement_currency: Currency,
    tick_size: f64,
    status: InstrumentStatus,
}

impl OptionInstrumentBuilder {
    pub fn new(
        market_id: Uuid,
        underlying_asset: Asset,
        settlement_currency: Currency,
    ) -> Self {
        let contract_size = underlying_asset.contract_size;
        let min_order_size = underlying_asset.min_order_size;
        let tick_size = underlying_asset.tick_size;

        Self {
            market_id,
            underlying_asset,
            option_type: OptionType::Call,
            style: OptionStyle::European,
            strike_price: 0.0,
            expiry_timestamp: Utc::now(),
            contract_size,
            min_order_size,
            settlement_currency,
            tick_size,
            status: InstrumentStatus::Draft,
        }
    }

    pub fn option_type(mut self, option_type: OptionType) -> Self {
        self.option_type = option_type;
        self
    }

    pub fn strike_price(mut self, strike_price: f64) -> Self {
        self.strike_price = strike_price;
        self
    }

    pub fn expiry_timestamp(mut self, expiry: DateTime<Utc>) -> Self {
        self.expiry_timestamp = expiry;
        self
    }

    pub fn status(mut self, status: InstrumentStatus) -> Self {
        self.status = status;
        self
    }

    pub fn build(self) -> OptionInstrument {
        let mut instrument = OptionInstrument {
            instrument_id: String::new(),
            market_id: self.market_id,
            underlying_asset: self.underlying_asset,
            option_type: self.option_type,
            style: self.style,
            strike_price: self.strike_price,
            expiry_timestamp: self.expiry_timestamp,
            contract_size: self.contract_size,
            min_order_size: self.min_order_size,
            settlement_currency: self.settlement_currency,
            tick_size: self.tick_size,
            status: self.status,
            created_at: Utc::now(),
        };
        instrument.instrument_id = instrument.generate_id();
        instrument
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::Duration;

    fn test_btc_call() -> OptionInstrument {
        OptionInstrumentBuilder::new(
            Uuid::new_v4(),
            Asset::btc(),
            Currency::usdt(),
        )
        .option_type(OptionType::Call)
        .strike_price(50000.0)
        .expiry_timestamp(Utc::now() + Duration::days(30))
        .status(InstrumentStatus::Active)
        .build()
    }

    #[test]
    fn test_canonical_name_format() {
        let instrument = test_btc_call();
        let name = instrument.canonical_name();

        assert!(name.starts_with("BTC-"));
        assert!(name.ends_with("-C"));
        assert!(name.contains("50000"));
    }

    #[test]
    fn test_deterministic_id_generation() {
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

        // Same params = same ID
        assert_eq!(i1.instrument_id, i2.instrument_id);
        assert_eq!(i1.canonical_name(), i2.canonical_name());
    }

    #[test]
    fn test_different_params_different_id() {
        let market_id = Uuid::new_v4();
        let expiry = Utc::now() + Duration::days(30);

        let call = OptionInstrumentBuilder::new(market_id, Asset::btc(), Currency::usdt())
            .option_type(OptionType::Call)
            .strike_price(50000.0)
            .expiry_timestamp(expiry)
            .build();

        let put = OptionInstrumentBuilder::new(market_id, Asset::btc(), Currency::usdt())
            .option_type(OptionType::Put)
            .strike_price(50000.0)
            .expiry_timestamp(expiry)
            .build();

        assert_ne!(call.instrument_id, put.instrument_id);
    }

    #[test]
    fn test_is_tradeable() {
        let active = test_btc_call();
        assert!(active.is_tradeable());

        let mut draft = test_btc_call();
        draft.status = InstrumentStatus::Draft;
        assert!(!draft.is_tradeable());
    }

    #[test]
    fn test_time_to_expiry_positive() {
        let instrument = test_btc_call();
        let tte = instrument.time_to_expiry_years();
        assert!(tte > 0.0);
        assert!(tte < 1.0); // 30 days < 1 year
    }

    #[test]
    fn test_serialization_roundtrip() {
        let instrument = test_btc_call();
        let json = serde_json::to_string(&instrument).unwrap();
        let deserialized: OptionInstrument = serde_json::from_str(&json).unwrap();
        assert_eq!(instrument.instrument_id, deserialized.instrument_id);
        assert_eq!(instrument.strike_price, deserialized.strike_price);
        assert_eq!(instrument.option_type, deserialized.option_type);
    }
}
