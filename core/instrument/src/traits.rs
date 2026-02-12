use async_trait::async_trait;
use exchange_primitives::InstrumentStatus;
use uuid::Uuid;

use crate::domain::{Market, OptionInstrument};
use crate::error::InstrumentError;

/// Storage interface for option instruments.
///
/// Implementations: InMemoryInstrumentStore, PostgresInstrumentStore, etc.
/// Core business logic uses this trait, NEVER a concrete database type.
#[async_trait]
pub trait InstrumentStore: Send + Sync {
    /// Persist a new instrument. Returns the instrument_id.
    async fn create_instrument(
        &self,
        instrument: OptionInstrument,
    ) -> Result<String, InstrumentError>;

    /// Retrieve an instrument by its deterministic ID.
    async fn get_instrument(
        &self,
        instrument_id: &str,
    ) -> Result<Option<OptionInstrument>, InstrumentError>;

    /// List all instruments belonging to a market.
    async fn list_instruments(
        &self,
        market_id: Uuid,
    ) -> Result<Vec<OptionInstrument>, InstrumentError>;

    /// List all instruments with a given status.
    async fn list_by_status(
        &self,
        status: InstrumentStatus,
    ) -> Result<Vec<OptionInstrument>, InstrumentError>;

    /// Update an instrument's lifecycle status.
    async fn update_status(
        &self,
        instrument_id: &str,
        status: InstrumentStatus,
    ) -> Result<(), InstrumentError>;

    /// Check if an instrument exists by ID.
    async fn instrument_exists(
        &self,
        instrument_id: &str,
    ) -> Result<bool, InstrumentError>;

    /// List all active instruments for a given underlying asset.
    async fn list_active_by_asset(
        &self,
        asset_id: &str,
    ) -> Result<Vec<OptionInstrument>, InstrumentError>;
}

/// Storage interface for markets.
#[async_trait]
pub trait MarketStore: Send + Sync {
    /// Create a new market.
    async fn create_market(&self, market: Market) -> Result<Uuid, InstrumentError>;

    /// Retrieve a market by ID.
    async fn get_market(&self, market_id: Uuid) -> Result<Option<Market>, InstrumentError>;

    /// List all markets.
    async fn list_markets(&self) -> Result<Vec<Market>, InstrumentError>;
}
