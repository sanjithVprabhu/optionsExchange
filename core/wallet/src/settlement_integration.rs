use super::{domain::*, engine::WalletEngine};
use exchange_settlement::domain::SettlementEvent;
use std::sync::Arc;
use tokio::sync::RwLock;
use tracing::info;

/// Wallet Settlement Handler - processes settlement events
pub struct WalletSettlementHandler {
    wallet_engine: Arc<RwLock<WalletEngine>>,
}

impl WalletSettlementHandler {
    pub fn new(wallet_engine: Arc<RwLock<WalletEngine>>) -> Self {
        Self { wallet_engine }
    }
    
    /// Process settlement event
    /// 
    /// This is called when an instrument settles at expiry.
    /// Updates wallet balances based on payoffs.
    pub async fn process_settlement(
        &self,
        settlement: &SettlementEvent,
        collateral_asset: Asset,
    ) -> Result<Vec<WalletEvent>, String> {
        let mut engine = self.wallet_engine.write().await;
        let mut events = Vec::new();
        
        info!(
            instrument = %settlement.instrument_id,
            num_users = settlement.user_settlements.len(),
            "Processing settlement wallet updates"
        );
        
        for user_settlement in &settlement.user_settlements {
            // 1. Unlock margin
            if user_settlement.margin_released > 0.0 {
                let event = engine.unlock_balance(
                    user_settlement.user_id,
                    collateral_asset.clone(),
                    user_settlement.margin_released,
                    Some(settlement.event_id),
                )?;
                events.push(event);
            }
            
            // 2. Apply wallet change (payoff - premium paid)
            if user_settlement.wallet_change > 0.0 {
                // Profit: credit free balance
                let event = engine.credit(
                    user_settlement.user_id,
                    collateral_asset.clone(),
                    user_settlement.wallet_change,
                    Some(settlement.event_id),
                )?;
                events.push(event);
            } else if user_settlement.wallet_change < 0.0 {
                // Loss: debit locked balance
                let event = engine.debit(
                    user_settlement.user_id,
                    collateral_asset.clone(),
                    user_settlement.wallet_change.abs(),
                    Some(settlement.event_id),
                )?;
                events.push(event);
            }
        }
        
        Ok(events)
    }
}
