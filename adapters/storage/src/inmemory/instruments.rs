use async_trait::async_trait;
use exchange_instrument::{
    InstrumentError, InstrumentStore, Market, MarketStore, OptionInstrument,
};
use exchange_primitives::InstrumentStatus;
use std::collections::HashMap;
use tokio::sync::RwLock;
use uuid::Uuid;

/// In-memory implementation of InstrumentStore.
///
/// Used for:
/// - Unit and integration testing (no database required)
/// - Development mode
/// - Config-driven selection: `storage.type: "inmemory"`
///
/// Thread-safe via tokio::sync::RwLock.
pub struct InMemoryInstrumentStore {
    instruments: RwLock<HashMap<String, OptionInstrument>>,
}

impl InMemoryInstrumentStore {
    pub fn new() -> Self {
        Self {
            instruments: RwLock::new(HashMap::new()),
        }
    }
}

impl Default for InMemoryInstrumentStore {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl InstrumentStore for InMemoryInstrumentStore {
    async fn create_instrument(
        &self,
        instrument: OptionInstrument,
    ) -> Result<String, InstrumentError> {
        let mut store = self.instruments.write().await;
        let id = instrument.instrument_id.clone();

        if store.contains_key(&id) {
            return Err(InstrumentError::AlreadyExists(id));
        }

        store.insert(id.clone(), instrument);
        Ok(id)
    }

    async fn get_instrument(
        &self,
        instrument_id: &str,
    ) -> Result<Option<OptionInstrument>, InstrumentError> {
        let store = self.instruments.read().await;
        Ok(store.get(instrument_id).cloned())
    }

    async fn list_instruments(
        &self,
        market_id: Uuid,
    ) -> Result<Vec<OptionInstrument>, InstrumentError> {
        let store = self.instruments.read().await;
        Ok(store
            .values()
            .filter(|i| i.market_id == market_id)
            .cloned()
            .collect())
    }

    async fn list_by_status(
        &self,
        status: InstrumentStatus,
    ) -> Result<Vec<OptionInstrument>, InstrumentError> {
        let store = self.instruments.read().await;
        Ok(store
            .values()
            .filter(|i| i.status == status)
            .cloned()
            .collect())
    }

    async fn update_status(
        &self,
        instrument_id: &str,
        status: InstrumentStatus,
    ) -> Result<(), InstrumentError> {
        let mut store = self.instruments.write().await;
        let instrument = store
            .get_mut(instrument_id)
            .ok_or_else(|| InstrumentError::NotFound(instrument_id.to_string()))?;
        instrument.status = status;
        Ok(())
    }

    async fn instrument_exists(
        &self,
        instrument_id: &str,
    ) -> Result<bool, InstrumentError> {
        let store = self.instruments.read().await;
        Ok(store.contains_key(instrument_id))
    }

    async fn list_active_by_asset(
        &self,
        asset_id: &str,
    ) -> Result<Vec<OptionInstrument>, InstrumentError> {
        let store = self.instruments.read().await;
        Ok(store
            .values()
            .filter(|i| {
                i.underlying_asset.asset_id == asset_id
                    && i.status == InstrumentStatus::Active
            })
            .cloned()
            .collect())
    }
}

/// In-memory implementation of MarketStore.
pub struct InMemoryMarketStore {
    markets: RwLock<HashMap<Uuid, Market>>,
}

impl InMemoryMarketStore {
    pub fn new() -> Self {
        Self {
            markets: RwLock::new(HashMap::new()),
        }
    }
}

impl Default for InMemoryMarketStore {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl MarketStore for InMemoryMarketStore {
    async fn create_market(&self, market: Market) -> Result<Uuid, InstrumentError> {
        let mut store = self.markets.write().await;
        let id = market.market_id;
        store.insert(id, market);
        Ok(id)
    }

    async fn get_market(&self, market_id: Uuid) -> Result<Option<Market>, InstrumentError> {
        let store = self.markets.read().await;
        Ok(store.get(&market_id).cloned())
    }

    async fn list_markets(&self) -> Result<Vec<Market>, InstrumentError> {
        let store = self.markets.read().await;
        Ok(store.values().cloned().collect())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::{Duration, Utc};
    use exchange_instrument::OptionInstrumentBuilder;
    use exchange_primitives::{Asset, Currency, OptionType};

    fn make_instrument(market_id: Uuid, strike: f64, opt_type: OptionType) -> OptionInstrument {
        OptionInstrumentBuilder::new(market_id, Asset::btc(), Currency::usdt())
            .option_type(opt_type)
            .strike_price(strike)
            .expiry_timestamp(Utc::now() + Duration::days(30))
            .status(InstrumentStatus::Active)
            .build()
    }

    #[tokio::test]
    async fn test_create_and_get() {
        let store = InMemoryInstrumentStore::new();
        let market_id = Uuid::new_v4();
        let instrument = make_instrument(market_id, 50000.0, OptionType::Call);
        let id = instrument.instrument_id.clone();

        let result = store.create_instrument(instrument).await.unwrap();
        assert_eq!(result, id);

        let retrieved = store.get_instrument(&id).await.unwrap().unwrap();
        assert_eq!(retrieved.strike_price, 50000.0);
    }

    #[tokio::test]
    async fn test_duplicate_rejected() {
        let store = InMemoryInstrumentStore::new();
        let market_id = Uuid::new_v4();
        let instrument = make_instrument(market_id, 50000.0, OptionType::Call);

        store
            .create_instrument(instrument.clone())
            .await
            .unwrap();
        let result = store.create_instrument(instrument).await;
        assert!(matches!(result, Err(InstrumentError::AlreadyExists(_))));
    }

    #[tokio::test]
    async fn test_list_by_market() {
        let store = InMemoryInstrumentStore::new();
        let market_a = Uuid::new_v4();
        let market_b = Uuid::new_v4();

        store
            .create_instrument(make_instrument(market_a, 50000.0, OptionType::Call))
            .await
            .unwrap();
        store
            .create_instrument(make_instrument(market_a, 60000.0, OptionType::Put))
            .await
            .unwrap();
        store
            .create_instrument(make_instrument(market_b, 70000.0, OptionType::Call))
            .await
            .unwrap();

        let list_a = store.list_instruments(market_a).await.unwrap();
        assert_eq!(list_a.len(), 2);

        let list_b = store.list_instruments(market_b).await.unwrap();
        assert_eq!(list_b.len(), 1);
    }

    #[tokio::test]
    async fn test_update_status() {
        let store = InMemoryInstrumentStore::new();
        let market_id = Uuid::new_v4();
        let instrument = make_instrument(market_id, 50000.0, OptionType::Call);
        let id = instrument.instrument_id.clone();

        store.create_instrument(instrument).await.unwrap();

        store
            .update_status(&id, InstrumentStatus::Expired)
            .await
            .unwrap();

        let updated = store.get_instrument(&id).await.unwrap().unwrap();
        assert_eq!(updated.status, InstrumentStatus::Expired);
    }

    #[tokio::test]
    async fn test_update_status_not_found() {
        let store = InMemoryInstrumentStore::new();
        let result = store
            .update_status("nonexistent", InstrumentStatus::Expired)
            .await;
        assert!(matches!(result, Err(InstrumentError::NotFound(_))));
    }

    #[tokio::test]
    async fn test_list_active_by_asset() {
        let store = InMemoryInstrumentStore::new();
        let market_id = Uuid::new_v4();

        let mut active = make_instrument(market_id, 50000.0, OptionType::Call);
        active.status = InstrumentStatus::Active;

        let mut draft = make_instrument(market_id, 60000.0, OptionType::Call);
        draft.status = InstrumentStatus::Draft;

        store.create_instrument(active).await.unwrap();
        store.create_instrument(draft).await.unwrap();

        let active_list = store.list_active_by_asset("BTC").await.unwrap();
        assert_eq!(active_list.len(), 1);
        assert_eq!(active_list[0].strike_price, 50000.0);
    }

    #[tokio::test]
    async fn test_market_store() {
        let store = InMemoryMarketStore::new();
        let market = Market::new(Asset::btc(), Currency::usdt());
        let id = market.market_id;

        store.create_market(market).await.unwrap();

        let retrieved = store.get_market(id).await.unwrap().unwrap();
        assert_eq!(retrieved.underlying_asset.asset_id, "BTC");

        let all = store.list_markets().await.unwrap();
        assert_eq!(all.len(), 1);
    }
}
