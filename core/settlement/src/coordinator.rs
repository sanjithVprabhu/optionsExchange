use crate::{
    clearing_engine::ClearingEngine,
    domain::*,
    settlement_engine::SettlementEngine,
};
use exchange_instrument::traits::InstrumentStore;
use exchange_matching::domain::Trade;
use exchange_risk::domain::Position;
use std::sync::Arc;
use tokio::sync::RwLock;
use tracing::{error, info};
use uuid::Uuid;

/// Settlement Coordinator - orchestrates clearing and settlement
/// 
/// This coordinates between:
/// - Clearing Engine (continuous)
/// - Settlement Engine (terminal)
/// - Risk Engine (margin updates)
/// - Wallet System (balance updates)
pub struct SettlementCoordinator {
    clearing_engine: Arc<RwLock<ClearingEngine>>,
    settlement_engine: Arc<RwLock<SettlementEngine>>,
    instrument_store: Arc<dyn InstrumentStore>,
}

impl SettlementCoordinator {
    pub fn new(instrument_store: Arc<dyn InstrumentStore>) -> Self {
        Self {
            clearing_engine: Arc::new(RwLock::new(ClearingEngine::new())),
            settlement_engine: Arc::new(RwLock::new(SettlementEngine::new())),
            instrument_store,
        }
    }
    
    /// Process a trade (continuous clearing)
    /// 
    /// Called immediately after matching engine produces a trade.
    pub async fn process_trade(&self, trade: &Trade) -> Result<ClearingEvent, String> {
        // Get instrument
        let instrument = self
            .instrument_store
            .get_instrument(&trade.instrument_id)
            .await
            .map_err(|e| format!("Failed to get instrument: {}", e))?
            .ok_or_else(|| format!("Instrument not found: {}", trade.instrument_id))?;
        
        // Process clearing
        let mut engine = self.clearing_engine.write().await;
        let clearing_event = engine.process_trade(trade, &instrument);
        
        info!(
            trade_id = %trade.trade_id,
            clearing_event_id = %clearing_event.event_id,
            "Trade cleared"
        );
        
        // TODO: Notify risk engine to update positions/margin
        // TODO: Emit event to event log
        
        Ok(clearing_event)
    }
    
    /// Settle an expired instrument (terminal settlement)
    /// 
    /// Called once when instrument expires.
    pub async fn settle_instrument(
        &self,
        instrument_id: &str,
        settlement_price: f64,
        positions: std::collections::HashMap<Uuid, Position>,
    ) -> Result<SettlementEvent, String> {
        // Get instrument
        let instrument = self
            .instrument_store
            .get_instrument(instrument_id)
            .await
            .map_err(|e| format!("Failed to get instrument: {}", e))?
            .ok_or_else(|| format!("Instrument not found: {}", instrument_id))?;
        
        // Settle
        let mut engine = self.settlement_engine.write().await;
        let settlement_event = engine.settle_instrument(&instrument, settlement_price, &positions);
        
        info!(
            instrument = %instrument_id,
            settlement_event_id = %settlement_event.event_id,
            num_users = settlement_event.user_settlements.len(),
            "Instrument settled"
        );
        
        // TODO: Update wallet balances
        // TODO: Release margins
        // TODO: Close positions
        // TODO: Emit event to event log
        
        Ok(settlement_event)
    }
    
    /// Calculate unrealized PnL for a user
    pub async fn calculate_unrealized_pnl(
        &self,
        user_id: Uuid,
        instrument_id: &str,
        mark_price: f64,
    ) -> Result<f64, String> {
        // Get instrument
        let instrument = self
            .instrument_store
            .get_instrument(instrument_id)
            .await
            .map_err(|e| format!("Failed to get instrument: {}", e))?
            .ok_or_else(|| format!("Instrument not found: {}", instrument_id))?;
        
        // Calculate PnL
        let engine = self.clearing_engine.read().await;
        let pnl = engine.calculate_unrealized_pnl(
            user_id,
            instrument_id,
            mark_price,
            instrument.contract_size,
        );
        
        Ok(pnl.unwrap_or(0.0))
    }
    
    /// Get position for a user
    pub async fn get_position(
        &self,
        user_id: Uuid,
        instrument_id: &str,
    ) -> Option<Position> {
        let engine = self.clearing_engine.read().await;
        engine.get_position(user_id, instrument_id).cloned()
    }
    
    /// Get all positions for a user
    pub async fn get_user_positions(&self, user_id: Uuid) -> Vec<Position> {
        let engine = self.clearing_engine.read().await;
        engine
            .get_user_positions(user_id)
            .into_iter()
            .cloned()
            .collect()
    }
    
    /// Get all positions for an instrument
    pub async fn get_instrument_positions(&self, instrument_id: &str) -> Vec<Position> {
        let engine = self.clearing_engine.read().await;
        engine
            .get_instrument_positions(instrument_id)
            .into_iter()
            .cloned()
            .collect()
    }
}
