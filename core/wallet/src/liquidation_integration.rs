use super::{domain::*, engine::WalletEngine};
use std::sync::Arc;
use tokio::sync::RwLock;
use tracing::{info, warn};
use uuid::Uuid;

/// Liquidation wallet handler
pub struct WalletLiquidationHandler {
    wallet_engine: Arc<RwLock<WalletEngine>>,
    liquidation_fee_rate: f64,
}

impl WalletLiquidationHandler {
    pub fn new(wallet_engine: Arc<RwLock<WalletEngine>>, liquidation_fee_rate: f64) -> Self {
        Self {
            wallet_engine,
            liquidation_fee_rate,
        }
    }
    
    /// Process liquidation
    /// 
    /// Called when a user position is liquidated.
    pub async fn process_liquidation(
        &self,
        user_id: Uuid,
        collateral_asset: Asset,
        realized_loss: f64,
        margin_released: f64,
    ) -> Result<Vec<WalletEvent>, String> {
        let mut engine = self.wallet_engine.write().await;
        let mut events = Vec::new();
        
        info!(
            user_id = %user_id,
            realized_loss = realized_loss,
            margin_released = margin_released,
            "Processing liquidation"
        );
        
        // 1. Debit realized loss
        if realized_loss > 0.0 {
            let event = engine.debit(user_id, collateral_asset.clone(), realized_loss, None)?;
            events.push(event);
        }
        
        // 2. Liquidation fee to insurance fund
        let fee = realized_loss * self.liquidation_fee_rate;
        if fee > 0.0 {
            let (user_event, insurance_event) =
                engine.insurance_transfer(user_id, collateral_asset.clone(), fee, true, None)?;
            events.push(user_event);
            events.push(insurance_event);
        }
        
        // 3. Release remaining margin
        if margin_released > 0.0 {
            let event = engine.unlock_balance(user_id, collateral_asset, margin_released, None)?;
            events.push(event);
        }
        
        Ok(events)
    }
    
    /// Handle insurance fund bailout
    /// 
    /// Called when user goes negative and insurance must cover.
    pub async fn insurance_bailout(
        &self,
        user_id: Uuid,
        collateral_asset: Asset,
        deficit: f64,
    ) -> Result<(WalletEvent, WalletEvent), String> {
        let mut engine = self.wallet_engine.write().await;
        
        warn!(
            user_id = %user_id,
            deficit = deficit,
            "Insurance fund bailout required"
        );
        
        // Check insurance fund balance
        let insurance_balance = engine.get_insurance_balance(&collateral_asset);
        
        if insurance_balance < deficit {
            return Err(format!(
                "Insurance fund insufficient: {} < {}",
                insurance_balance, deficit
            ));
        }
        
        // Transfer from insurance to user
        engine.insurance_transfer(user_id, collateral_asset, deficit, false, None)
    }
}
