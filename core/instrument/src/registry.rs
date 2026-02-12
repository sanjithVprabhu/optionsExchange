use std::sync::Arc;

use exchange_primitives::InstrumentStatus;
use tracing::{error, info, instrument};
use uuid::Uuid;

use crate::domain::{Market, OptionInstrument};
use crate::error::InstrumentError;
use crate::traits::{InstrumentStore, MarketStore};
use crate::validation::validate_instrument;

/// The central business logic hub for instrument management.
///
/// InstrumentRegistry holds a trait-based store reference, NEVER a concrete
/// database type. Infrastructure is chosen at startup via config.
pub struct InstrumentRegistry {
    instrument_store: Arc<dyn InstrumentStore>,
    market_store: Arc<dyn MarketStore>,
}

impl InstrumentRegistry {
    pub fn new(
        instrument_store: Arc<dyn InstrumentStore>,
        market_store: Arc<dyn MarketStore>,
    ) -> Self {
        Self {
            instrument_store,
            market_store,
        }
    }

    /// Creates a new market for an underlying asset.
    #[instrument(skip(self, market), fields(market_id = %market.market_id))]
    pub async fn create_market(&self, market: Market) -> Result<Uuid, InstrumentError> {
        info!(
            asset = %market.underlying_asset.asset_id,
            currency = %market.settlement_currency.currency_id,
            "Creating market"
        );
        self.market_store.create_market(market).await
    }

    /// Retrieves a market by ID.
    pub async fn get_market(&self, market_id: Uuid) -> Result<Market, InstrumentError> {
        self.market_store
            .get_market(market_id)
            .await?
            .ok_or_else(|| InstrumentError::MarketNotFound(market_id.to_string()))
    }

    /// Lists all markets.
    pub async fn list_markets(&self) -> Result<Vec<Market>, InstrumentError> {
        self.market_store.list_markets().await
    }

    /// Creates a new option instrument after validation.
    ///
    /// Steps:
    /// 1. Validate all instrument fields
    /// 2. Generate deterministic ID from canonical name
    /// 3. Check instrument doesn't already exist (idempotency)
    /// 4. Verify the parent market exists
    /// 5. Persist the instrument
    #[instrument(skip(self, instrument), fields(canonical_name))]
    pub async fn create_instrument(
        &self,
        mut instrument: OptionInstrument,
    ) -> Result<String, InstrumentError> {
        // Step 1: Validate
        validate_instrument(&instrument)?;

        // Step 2: Generate deterministic ID
        instrument.instrument_id = instrument.generate_id();

        let canonical = instrument.canonical_name();
        tracing::Span::current().record("canonical_name", &canonical.as_str());

        info!(
            instrument_id = %instrument.instrument_id,
            strike = instrument.strike_price,
            option_type = %instrument.option_type,
            "Creating instrument"
        );

        // Step 3: Check for duplicates
        if self
            .instrument_store
            .instrument_exists(&instrument.instrument_id)
            .await?
        {
            return Err(InstrumentError::AlreadyExists(
                instrument.instrument_id.clone(),
            ));
        }

        // Step 4: Verify market exists
        if self
            .market_store
            .get_market(instrument.market_id)
            .await?
            .is_none()
        {
            return Err(InstrumentError::MarketNotFound(
                instrument.market_id.to_string(),
            ));
        }

        // Step 5: Persist
        let id = self
            .instrument_store
            .create_instrument(instrument)
            .await?;

        info!(instrument_id = %id, "Instrument created successfully");
        Ok(id)
    }

    /// Retrieves an instrument by its deterministic ID.
    pub async fn get_instrument(
        &self,
        instrument_id: &str,
    ) -> Result<OptionInstrument, InstrumentError> {
        self.instrument_store
            .get_instrument(instrument_id)
            .await?
            .ok_or_else(|| InstrumentError::NotFound(instrument_id.to_string()))
    }

    /// Lists all instruments in a given market.
    pub async fn list_instruments(
        &self,
        market_id: Uuid,
    ) -> Result<Vec<OptionInstrument>, InstrumentError> {
        self.instrument_store.list_instruments(market_id).await
    }

    /// Lists all active instruments for a given underlying asset.
    pub async fn list_active_instruments(
        &self,
        asset_id: &str,
    ) -> Result<Vec<OptionInstrument>, InstrumentError> {
        self.instrument_store.list_active_by_asset(asset_id).await
    }

    /// Transitions an instrument to a new lifecycle status.
    ///
    /// Validates the transition according to the state machine:
    /// Draft -> Listed -> Active -> Expired -> Settled -> Archived
    /// Active <-> Halted
    #[instrument(skip(self))]
    pub async fn transition_status(
        &self,
        instrument_id: &str,
        new_status: InstrumentStatus,
    ) -> Result<(), InstrumentError> {
        let instrument = self.get_instrument(instrument_id).await?;

        if !instrument.status.can_transition_to(&new_status) {
            error!(
                from = %instrument.status,
                to = %new_status,
                "Invalid status transition"
            );
            return Err(InstrumentError::InvalidStatusTransition {
                from: instrument.status,
                to: new_status,
            });
        }

        info!(
            instrument_id = %instrument_id,
            from = %instrument.status,
            to = %new_status,
            "Transitioning instrument status"
        );

        self.instrument_store
            .update_status(instrument_id, new_status)
            .await
    }

    /// Expires all instruments that are past their expiry timestamp
    /// and currently in Active status.
    pub async fn expire_instruments(&self) -> Result<Vec<String>, InstrumentError> {
        let active_instruments = self
            .instrument_store
            .list_by_status(InstrumentStatus::Active)
            .await?;

        let mut expired_ids = Vec::new();

        for instrument in active_instruments {
            if instrument.is_expired() {
                info!(
                    instrument_id = %instrument.instrument_id,
                    canonical = %instrument.canonical_name(),
                    "Expiring instrument"
                );

                self.instrument_store
                    .update_status(&instrument.instrument_id, InstrumentStatus::Expired)
                    .await?;

                expired_ids.push(instrument.instrument_id);
            }
        }

        if !expired_ids.is_empty() {
            info!(count = expired_ids.len(), "Expired instruments batch");
        }

        Ok(expired_ids)
    }
}
