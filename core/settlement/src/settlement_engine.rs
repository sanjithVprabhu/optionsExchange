use crate::domain::*;
use exchange_instrument::domain::{InstrumentStatus, OptionInstrument};
use exchange_risk::domain::Position;
use std::collections::HashMap;
use tracing::{info, warn};
use uuid::Uuid;

/// Settlement Engine - handles terminal settlement at expiry
/// 
/// RESPONSIBILITIES:
/// 1. Detect expired instruments
/// 2. Get settlement price from index
/// 3. Calculate payoffs for all positions
/// 4. Generate settlement events
/// 5. Update wallet balances
/// 6. Close positions and release margin
pub struct SettlementEngine {
    /// Sequence counter
    sequence: u64,
}

impl SettlementEngine {
    pub fn new() -> Self {
        Self {
            sequence: 0,
        }
    }
    
    /// Settle an expired instrument
    /// 
    /// This is called once when an instrument expires.
    /// It computes payoffs and generates wallet updates.
    pub fn settle_instrument(
        &mut self,
        instrument: &OptionInstrument,
        settlement_price: f64,
        positions: &HashMap<Uuid, Position>,
    ) -> SettlementEvent {
        self.sequence += 1;
        
        info!(
            instrument = %instrument.instrument_id,
            settlement_price = settlement_price,
            num_positions = positions.len(),
            "Settling instrument"
        );
        
        // Verify instrument is expired
        if instrument.status != InstrumentStatus::Expired {
            warn!(
                instrument = %instrument.instrument_id,
                status = ?instrument.status,
                "Attempting to settle non-expired instrument"
            );
        }
        
        // Calculate settlements for each position
        let user_settlements: Vec<UserSettlement> = positions
            .iter()
            .map(|(user_id, position)| {
                self.calculate_user_settlement(
                    *user_id,
                    position,
                    instrument,
                    settlement_price,
                )
            })
            .collect();
        
        // Verify zero-sum property
        let total_payoff: f64 = user_settlements.iter().map(|s| s.payoff).sum();
        
        if total_payoff.abs() > 1e-6 {
            warn!(
                instrument = %instrument.instrument_id,
                total_payoff = total_payoff,
                "Settlement is not zero-sum! This is a bug."
            );
        }
        
        info!(
            instrument = %instrument.instrument_id,
            settlements = user_settlements.len(),
            total_payoff = total_payoff,
            "Settlement completed"
        );
        
        SettlementEvent {
            event_id: Uuid::new_v4(),
            instrument_id: instrument.instrument_id.clone(),
            settlement_price,
            user_settlements,
            sequence: self.sequence,
            timestamp: chrono::Utc::now(),
        }
    }
    
    /// Calculate settlement for a single user position
    fn calculate_user_settlement(
        &self,
        user_id: Uuid,
        position: &Position,
        instrument: &OptionInstrument,
        settlement_price: f64,
    ) -> UserSettlement {
        // Calculate payoff using option formulas
        let payoff = PayoffCalculator::position_payoff(
            settlement_price,
            instrument.strike_price,
            instrument.option_type,
            position.side,
            position.quantity,
            instrument.contract_size,
        );
        
        // For long positions: paid premium upfront, receive payoff
        // For short positions: received premium upfront, pay payoff
        let premium_paid = match position.side {
            exchange_risk::domain::PositionSide::Long => {
                // Paid premium when bought
                position.avg_price * position.quantity as f64 * instrument.contract_size
            }
            exchange_risk::domain::PositionSide::Short => {
                // Received premium when sold (negative because it's income)
                -(position.avg_price * position.quantity as f64 * instrument.contract_size)
            }
        };
        
        // Net wallet change = payoff - premium_paid
        // For long: if ITM, payoff > premium, profit
        // For short: received premium, pay payoff if ITM
        let wallet_change = payoff - premium_paid;
        
        // Margin to be released (calculated by risk engine)
        // This is a placeholder - actual margin comes from risk engine
        let margin_released = 0.0;  // TODO: Get from risk engine
        
        UserSettlement {
            user_id,
            position_quantity: position.quantity as i32,
            position_side: position.side,
            avg_entry_price: position.avg_price,
            payoff,
            margin_released,
            wallet_change,
        }
    }
}

impl Default for SettlementEngine {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use exchange_instrument::domain::*;
    use exchange_risk::domain::PositionSide;
    
    fn create_test_instrument(strike: f64, option_type: OptionType) -> OptionInstrument {
        OptionInstrument {
            instrument_id: format!("test-btc-{}-{}", strike, match option_type {
                OptionType::Call => "call",
                OptionType::Put => "put",
            }),
            market_id: Uuid::new_v4(),
            underlying_asset: Asset {
                asset_id: "BTC".to_string(),
                name: "Bitcoin".to_string(),
                symbol: "BTC".to_string(),
                decimals: 8,
            },
            option_type,
            style: OptionStyle::European,
            strike_price: strike,
            expiry_timestamp: chrono::Utc::now() - chrono::Duration::hours(1), // Expired
            contract_size: 0.01,
            min_order_size: 1,
            settlement_currency: Currency {
                currency_id: "USDT".to_string(),
                name: "Tether".to_string(),
                symbol: "USDT".to_string(),
                decimals: 6,
            },
            tick_size: 0.5,
            status: InstrumentStatus::Expired,
            created_at: chrono::Utc::now(),
        }
    }
    
    #[test]
    fn test_settle_itm_call() {
        let mut engine = SettlementEngine::new();
        let instrument = create_test_instrument(50000.0, OptionType::Call);
        
        // Long position in ITM call
        let buyer_id = Uuid::new_v4();
        let position = Position::new(
            buyer_id,
            instrument.instrument_id.clone(),
            PositionSide::Long,
            10,
            100.0,  // Paid 100 USDT per contract
        );
        
        let mut positions = HashMap::new();
        positions.insert(buyer_id, position);
        
        // Settlement price above strike = ITM
        // Settlement: 60000, Strike: 50000
        // Payoff = (60000 - 50000) * 10 * 0.01 = 100 USDT
        let settlement = engine.settle_instrument(
            &instrument,
            60000.0,
            &positions,
        );
        
        assert_eq!(settlement.user_settlements.len(), 1);
        
        let user_settlement = &settlement.user_settlements[0];
        
        // Payoff = (60000 - 50000) × 10 × 0.01 = 100 USDT
        assert_eq!(user_settlement.payoff, 100.0);
        
        // Premium paid = 100 × 10 × 0.01 = 10 USDT
        // Net = 100 - 10 = 90 USDT profit
        assert_eq!(user_settlement.wallet_change, 90.0);
    }
    
    #[test]
    fn test_settle_otm_call() {
        let mut engine = SettlementEngine::new();
        let instrument = create_test_instrument(50000.0, OptionType::Call);
        
        // Long position in OTM call
        let buyer_id = Uuid::new_v4();
        let position = Position::new(
            buyer_id,
            instrument.instrument_id.clone(),
            PositionSide::Long,
            10,
            100.0,
        );
        
        let mut positions = HashMap::new();
        positions.insert(buyer_id, position);
        
        // Settlement price below strike = OTM
        let settlement = engine.settle_instrument(
            &instrument,
            40000.0,
            &positions,
        );
        
        let user_settlement = &settlement.user_settlements[0];
        
        // Payoff = 0 (OTM)
        assert_eq!(user_settlement.payoff, 0.0);
        
        // Lost premium (10 USDT)
        assert_eq!(user_settlement.wallet_change, -10.0);
    }
    
    #[test]
    fn test_settlement_zero_sum() {
        let mut engine = SettlementEngine::new();
        let instrument = create_test_instrument(50000.0, OptionType::Call);
        
        let buyer_id = Uuid::new_v4();
        let seller_id = Uuid::new_v4();
        
        // Long position (buyer)
        let long_position = Position::new(
            buyer_id,
            instrument.instrument_id.clone(),
            PositionSide::Long,
            10,
            100.0,
        );
        
        // Short position (seller)
        let short_position = Position::new(
            seller_id,
            instrument.instrument_id.clone(),
            PositionSide::Short,
            10,
            100.0,
        );
        
        let mut positions = HashMap::new();
        positions.insert(buyer_id, long_position);
        positions.insert(seller_id, short_position);
        
        // Settle at 60000 (ITM)
        let settlement = engine.settle_instrument(
            &instrument,
            60000.0,
            &positions,
        );
        
        // Sum of all payoffs should be zero (zero-sum)
        let total_payoff: f64 = settlement
            .user_settlements
            .iter()
            .map(|s| s.payoff)
            .sum();
        
        assert!(total_payoff.abs() < 1e-6);
    }
    
    #[test]
    fn test_settle_itm_put() {
        let mut engine = SettlementEngine::new();
        let instrument = create_test_instrument(50000.0, OptionType::Put);
        
        // Long position in ITM put
        let buyer_id = Uuid::new_v4();
        let position = Position::new(
            buyer_id,
            instrument.instrument_id.clone(),
            PositionSide::Long,
            10,
            100.0,
        );
        
        let mut positions = HashMap::new();
        positions.insert(buyer_id, position);
        
        // Settlement price below strike = ITM for put
        // Settlement: 40000, Strike: 50000
        // Payoff = (50000 - 40000) * 10 * 0.01 = 100 USDT
        let settlement = engine.settle_instrument(
            &instrument,
            40000.0,
            &positions,
        );
        
        let user_settlement = &settlement.user_settlements[0];
        
        // Payoff = (50000 - 40000) × 10 × 0.01 = 100 USDT
        assert_eq!(user_settlement.payoff, 100.0);
        
        // Premium paid = 100 × 10 × 0.01 = 10 USDT
        // Net = 100 - 10 = 90 USDT profit
        assert_eq!(user_settlement.wallet_change, 90.0);
    }
}
