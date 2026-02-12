use crate::domain::*;
use async_trait::async_trait;

/// Storage interface for settlement events
#[async_trait]
pub trait SettlementStore: Send + Sync {
    /// Store clearing event
    async fn store_clearing_event(&self, event: ClearingEvent) -> Result<(), String>;
    
    /// Store settlement event
    async fn store_settlement_event(&self, event: SettlementEvent) -> Result<(), String>;
    
    /// Get clearing events for an instrument
    async fn get_clearing_events(&self, instrument_id: &str) -> Result<Vec<ClearingEvent>, String>;
    
    /// Get settlement event for an instrument
    async fn get_settlement_event(&self, instrument_id: &str) -> Result<Option<SettlementEvent>, String>;
    
    /// Check if instrument is settled
    async fn is_settled(&self, instrument_id: &str) -> Result<bool, String>;
}
