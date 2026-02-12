use crate::domain::*;
use exchange_instrument::domain::OptionInstrument;
use exchange_matching::domain::Trade;
use exchange_risk::domain::{Position, PositionSide};
use std::collections::HashMap;
use tracing::{debug, info};
use uuid::Uuid;

/// Clearing Engine - handles continuous clearing after each trade
/// 
/// RESPONSIBILITIES:
/// 1. Generate clearing events from trades
/// 2. Update positions (buyer + seller)
/// 3. Transition margin (reserved → actual)
/// 4. Calculate unrealized PnL
pub struct ClearingEngine {
    /// Current positions (derived from clearing events)
    positions: HashMap<(Uuid, String), Position>,
    
    /// Sequence counter
    sequence: u64,
}

impl ClearingEngine {
    pub fn new() -> Self {
        Self {
            positions: HashMap::new(),
            sequence: 0,
        }
    }
    
    /// Process a trade and generate clearing event
    /// 
    /// This is called immediately after matching engine produces a trade.
    pub fn process_trade(
        &mut self,
        trade: &Trade,
        instrument: &OptionInstrument,
    ) -> ClearingEvent {
        self.sequence += 1;
        
        info!(
            trade_id = %trade.trade_id,
            instrument = %trade.instrument_id,
            buyer = %trade.buyer_id,
            seller = %trade.seller_id,
            quantity = trade.quantity,
            price = trade.price,
            "Processing clearing for trade"
        );
        
        // Buyer updates (going long)
        let buyer_update = PositionUpdate {
            user_id: trade.buyer_id,
            side: PositionSide::Long,
            quantity_change: trade.quantity as i32,
            price: trade.price,
            margin_change: self.calculate_margin_change_long(
                trade.quantity,
                trade.price,
            ),
        };
        
        // Seller updates (going short)
        let seller_update = PositionUpdate {
            user_id: trade.seller_id,
            side: PositionSide::Short,
            quantity_change: trade.quantity as i32,
            price: trade.price,
            margin_change: self.calculate_margin_change_short(
                trade.quantity,
                trade.price,
                instrument,
            ),
        };
        
        // Update internal position tracking
        self.apply_position_update(&trade.instrument_id, &buyer_update);
        self.apply_position_update(&trade.instrument_id, &seller_update);
        
        ClearingEvent {
            event_id: Uuid::new_v4(),
            trade_id: trade.trade_id,
            instrument_id: trade.instrument_id.clone(),
            buyer_update,
            seller_update,
            sequence: self.sequence,
            timestamp: chrono::Utc::now(),
        }
    }
    
    /// Calculate margin change for long position (buyer)
    /// 
    /// Long options: margin = premium paid
    fn calculate_margin_change_long(&self, quantity: u32, price: f64) -> f64 {
        // Premium paid upfront
        price * quantity as f64
    }
    
    /// Calculate margin change for short position (seller)
    /// 
    /// Short options: margin calculated by risk engine
    /// This is a placeholder - actual margin comes from risk engine
    fn calculate_margin_change_short(
        &self,
        quantity: u32,
        price: f64,
        _instrument: &OptionInstrument,
    ) -> f64 {
        // Simplified: will be computed by risk engine
        // This is just the premium received
        price * quantity as f64
    }
    
    /// Apply position update to internal state
    fn apply_position_update(
        &mut self,
        instrument_id: &str,
        update: &PositionUpdate,
    ) {
        let key = (update.user_id, instrument_id.to_string());
        
        match self.positions.get_mut(&key) {
            Some(position) => {
                // Update existing position using weighted average
                position.update_fill(update.quantity_change.unsigned_abs(), update.price);
                
                debug!(
                    user_id = %update.user_id,
                    instrument = %instrument_id,
                    new_qty = position.quantity,
                    new_avg = position.avg_price,
                    "Position updated"
                );
            }
            None => {
                // Create new position
                let position = Position::new(
                    update.user_id,
                    instrument_id.to_string(),
                    update.side,
                    update.quantity_change.unsigned_abs(),
                    update.price,
                );
                
                self.positions.insert(key, position);
                
                debug!(
                    user_id = %update.user_id,
                    instrument = %instrument_id,
                    side = ?update.side,
                    qty = update.quantity_change.abs(),
                    "Position created"
                );
            }
        }
    }
    
    /// Calculate unrealized PnL for a position
    pub fn calculate_unrealized_pnl(
        &self,
        user_id: Uuid,
        instrument_id: &str,
        mark_price: f64,
        contract_size: f64,
    ) -> Option<f64> {
        let key = (user_id, instrument_id.to_string());
        
        self.positions.get(&key).map(|position| {
            UnrealizedPnL::calculate(position, mark_price, contract_size)
        })
    }
    
    /// Get position
    pub fn get_position(
        &self,
        user_id: Uuid,
        instrument_id: &str,
    ) -> Option<&Position> {
        self.positions.get(&(user_id, instrument_id.to_string()))
    }
    
    /// Get all positions for a user
    pub fn get_user_positions(&self, user_id: Uuid) -> Vec<&Position> {
        self.positions
            .iter()
            .filter(|((uid, _), _)| *uid == user_id)
            .map(|(_, pos)| pos)
            .collect()
    }
    
    /// Get all positions for an instrument
    pub fn get_instrument_positions(&self, instrument_id: &str) -> Vec<&Position> {
        self.positions
            .iter()
            .filter(|((_, iid), _)| iid == instrument_id)
            .map(|(_, pos)| pos)
            .collect()
    }
}

impl Default for ClearingEngine {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use exchange_instrument::domain::*;
    use exchange_matching::domain::*;
    
    fn create_test_instrument() -> OptionInstrument {
        OptionInstrument {
            instrument_id: "test-btc-call".to_string(),
            market_id: Uuid::new_v4(),
            underlying_asset: Asset {
                asset_id: "BTC".to_string(),
                name: "Bitcoin".to_string(),
                symbol: "BTC".to_string(),
                decimals: 8,
            },
            option_type: OptionType::Call,
            style: OptionStyle::European,
            strike_price: 50000.0,
            expiry_timestamp: chrono::Utc::now() + chrono::Duration::days(30),
            contract_size: 0.01,
            min_order_size: 1,
            settlement_currency: Currency {
                currency_id: "USDT".to_string(),
                name: "Tether".to_string(),
                symbol: "USDT".to_string(),
                decimals: 6,
            },
            tick_size: 0.5,
            status: InstrumentStatus::Active,
            created_at: chrono::Utc::now(),
        }
    }
    
    fn create_test_trade(buyer_id: Uuid, seller_id: Uuid, price: f64, quantity: u32) -> Trade {
        Trade {
            trade_id: Uuid::new_v4(),
            instrument_id: "test-btc-call".to_string(),
            taker_order_id: Uuid::new_v4(),
            maker_order_id: Uuid::new_v4(),
            buyer_id,
            seller_id,
            price,
            quantity,
            aggressor_side: OrderSide::Buy,
            sequence: 1,
            timestamp: chrono::Utc::now(),
        }
    }
    
    #[test]
    fn test_process_trade_creates_positions() {
        let mut engine = ClearingEngine::new();
        let instrument = create_test_instrument();
        let buyer_id = Uuid::new_v4();
        let seller_id = Uuid::new_v4();
        
        let trade = create_test_trade(buyer_id, seller_id, 100.0, 10);
        
        let clearing_event = engine.process_trade(&trade, &instrument);
        
        // Verify clearing event
        assert_eq!(clearing_event.trade_id, trade.trade_id);
        assert_eq!(clearing_event.buyer_update.user_id, buyer_id);
        assert_eq!(clearing_event.seller_update.user_id, seller_id);
        
        // Verify positions created
        let buyer_pos = engine.get_position(buyer_id, &trade.instrument_id);
        assert!(buyer_pos.is_some());
        assert_eq!(buyer_pos.unwrap().quantity, 10);
        assert_eq!(buyer_pos.unwrap().side, PositionSide::Long);
        
        let seller_pos = engine.get_position(seller_id, &trade.instrument_id);
        assert!(seller_pos.is_some());
        assert_eq!(seller_pos.unwrap().quantity, 10);
        assert_eq!(seller_pos.unwrap().side, PositionSide::Short);
    }
    
    #[test]
    fn test_multiple_trades_update_avg_price() {
        let mut engine = ClearingEngine::new();
        let instrument = create_test_instrument();
        let buyer_id = Uuid::new_v4();
        let seller_id = Uuid::new_v4();
        
        // First trade: buy 10 @ 100
        let trade1 = create_test_trade(buyer_id, seller_id, 100.0, 10);
        engine.process_trade(&trade1, &instrument);
        
        // Second trade: buy 10 @ 120
        let trade2 = create_test_trade(buyer_id, Uuid::new_v4(), 120.0, 10);
        engine.process_trade(&trade2, &instrument);
        
        // Check average price: (10*100 + 10*120) / 20 = 110
        let buyer_pos = engine.get_position(buyer_id, &trade1.instrument_id).unwrap();
        assert_eq!(buyer_pos.quantity, 20);
        assert_eq!(buyer_pos.avg_price, 110.0);
    }
}
