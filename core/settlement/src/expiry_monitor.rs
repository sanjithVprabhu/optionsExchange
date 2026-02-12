use exchange_instrument::{domain::InstrumentStatus, traits::InstrumentStore};
use std::sync::Arc;
use tokio::time::{interval, Duration};
use tracing::{error, info};

/// Expiry monitor - detects expired instruments and triggers settlement
pub struct ExpiryMonitor {
    instrument_store: Arc<dyn InstrumentStore>,
    check_interval: Duration,
}

impl ExpiryMonitor {
    pub fn new(instrument_store: Arc<dyn InstrumentStore>) -> Self {
        Self {
            instrument_store,
            check_interval: Duration::from_secs(60), // Check every minute
        }
    }
    
    /// Create with custom check interval
    pub fn with_interval(instrument_store: Arc<dyn InstrumentStore>, check_interval: Duration) -> Self {
        Self {
            instrument_store,
            check_interval,
        }
    }
    
    /// Start monitoring for expiries
    pub async fn start(&self) {
        let mut ticker = interval(self.check_interval);
        
        loop {
            ticker.tick().await;
            
            if let Err(e) = self.check_expiries().await {
                error!("Error checking expiries: {}", e);
            }
        }
    }
    
    /// Check for expired instruments
    async fn check_expiries(&self) -> Result<(), String> {
        // Get all active instruments
        let instruments = self
            .instrument_store
            .list_instruments_by_status(InstrumentStatus::Active)
            .await
            .map_err(|e| format!("Failed to list instruments: {}", e))?;
        
        let now = chrono::Utc::now();
        
        for instrument in instruments {
            if instrument.expiry_timestamp <= now {
                info!(
                    instrument = %instrument.instrument_id,
                    expiry = %instrument.expiry_timestamp,
                    "Instrument expired"
                );
                
                // Transition to EXPIRED status
                self.instrument_store
                    .update_status(&instrument.instrument_id, InstrumentStatus::Expired)
                    .await
                    .map_err(|e| format!("Failed to update status: {}", e))?;
                
                // Emit expiry event
                // Settlement will be triggered by separate process
            }
        }
        
        Ok(())
    }
}
