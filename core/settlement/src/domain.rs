use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

// ============================================================================
// CLEARING EVENT (PER TRADE)
// ============================================================================

/// Clearing event - generated after each trade
/// 
/// This represents the obligation created by a trade.
/// Positions and margins are updated based on these events.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ClearingEvent {
    /// Unique clearing event ID
    pub event_id: Uuid,
    
    /// Trade that triggered this clearing
    pub trade_id: Uuid,
    
    /// Instrument traded
    pub instrument_id: String,
    
    /// Buyer updates
    pub buyer_update: PositionUpdate,
    
    /// Seller updates
    pub seller_update: PositionUpdate,
    
    /// Sequence number (for ordering)
    pub sequence: u64,
    
    /// When clearing occurred
    pub timestamp: DateTime<Utc>,
}

/// Position update from a trade
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PositionUpdate {
    pub user_id: Uuid,
    pub side: exchange_risk::domain::PositionSide,
    pub quantity_change: i32,  // Signed: positive = increase, negative = decrease
    pub price: f64,
    pub margin_change: f64,
}

// ============================================================================
// SETTLEMENT EVENT (AT EXPIRY)
// ============================================================================

/// Settlement event - generated once at expiry
/// 
/// This represents the final obligation resolution.
/// Wallet balances are updated based on this event.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SettlementEvent {
    /// Unique settlement event ID
    pub event_id: Uuid,
    
    /// Instrument being settled
    pub instrument_id: String,
    
    /// Settlement price (from index)
    pub settlement_price: f64,
    
    /// Per-user settlements
    pub user_settlements: Vec<UserSettlement>,
    
    /// Sequence number (for ordering)
    pub sequence: u64,
    
    /// When settlement occurred
    pub timestamp: DateTime<Utc>,
}

/// Settlement for a single user
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UserSettlement {
    pub user_id: Uuid,
    
    /// Position being settled
    pub position_quantity: i32,
    pub position_side: exchange_risk::domain::PositionSide,
    pub avg_entry_price: f64,
    
    /// Payoff calculation
    pub payoff: f64,  // Can be positive (profit) or negative (loss)
    
    /// Margin released
    pub margin_released: f64,
    
    /// Net wallet change (payoff - premium paid for longs, etc.)
    pub wallet_change: f64,
}

// ============================================================================
// UNREALIZED PNL
// ============================================================================

/// Unrealized PnL for a position
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UnrealizedPnL {
    pub user_id: Uuid,
    pub instrument_id: String,
    
    /// Position details
    pub quantity: u32,
    pub side: exchange_risk::domain::PositionSide,
    pub avg_price: f64,
    
    /// Current mark price
    pub mark_price: f64,
    
    /// Calculated PnL
    pub pnl: f64,
    
    /// Contract size multiplier
    pub contract_size: f64,
}

impl UnrealizedPnL {
    pub fn calculate(
        position: &exchange_risk::domain::Position,
        mark_price: f64,
        contract_size: f64,
    ) -> f64 {
        use exchange_risk::domain::PositionSide;
        
        let qty = position.quantity as f64;
        let price_diff = match position.side {
            PositionSide::Long => mark_price - position.avg_price,
            PositionSide::Short => position.avg_price - mark_price,
        };
        
        price_diff * qty * contract_size
    }
}

// ============================================================================
// SETTLEMENT PRICE SOURCE
// ============================================================================

/// How settlement price is determined
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum SettlementPriceSource {
    /// Index price at expiry timestamp
    IndexPrice { provider: String, timestamp: DateTime<Utc> },
    
    /// Last trade price
    LastTrade { price: f64 },
    
    /// Manual override (admin only, rare)
    Manual { price: f64, reason: String },
}

// ============================================================================
// PAYOFF CALCULATOR
// ============================================================================

/// Option payoff at expiry
pub struct PayoffCalculator;

impl PayoffCalculator {
    /// Calculate call option payoff
    /// 
    /// Payoff = max(S - K, 0) × Q × C
    /// 
    /// where:
    /// - S = settlement price
    /// - K = strike price
    /// - Q = quantity
    /// - C = contract size
    pub fn call_payoff(
        settlement_price: f64,
        strike_price: f64,
        quantity: u32,
        contract_size: f64,
    ) -> f64 {
        let intrinsic_value = (settlement_price - strike_price).max(0.0);
        intrinsic_value * quantity as f64 * contract_size
    }
    
    /// Calculate put option payoff
    /// 
    /// Payoff = max(K - S, 0) × Q × C
    pub fn put_payoff(
        settlement_price: f64,
        strike_price: f64,
        quantity: u32,
        contract_size: f64,
    ) -> f64 {
        let intrinsic_value = (strike_price - settlement_price).max(0.0);
        intrinsic_value * quantity as f64 * contract_size
    }
    
    /// Calculate payoff for a position
    pub fn position_payoff(
        settlement_price: f64,
        strike_price: f64,
        option_type: exchange_instrument::domain::OptionType,
        position_side: exchange_risk::domain::PositionSide,
        quantity: u32,
        contract_size: f64,
    ) -> f64 {
        use exchange_instrument::domain::OptionType;
        use exchange_risk::domain::PositionSide;
        
        let payoff = match option_type {
            OptionType::Call => Self::call_payoff(
                settlement_price,
                strike_price,
                quantity,
                contract_size,
            ),
            OptionType::Put => Self::put_payoff(
                settlement_price,
                strike_price,
                quantity,
                contract_size,
            ),
        };
        
        // Long = receives payoff
        // Short = pays payoff
        match position_side {
            PositionSide::Long => payoff,
            PositionSide::Short => -payoff,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    
    #[test]
    fn test_call_payoff_itm() {
        // Settlement price above strike = ITM
        let payoff = PayoffCalculator::call_payoff(
            60000.0,  // settlement
            50000.0,  // strike
            10,       // quantity
            0.01,     // contract size
        );
        
        // (60000 - 50000) × 10 × 0.01 = 10000 × 0.1 = 100
        assert_eq!(payoff, 100.0);
    }
    
    #[test]
    fn test_call_payoff_otm() {
        // Settlement price below strike = OTM
        let payoff = PayoffCalculator::call_payoff(
            40000.0,  // settlement
            50000.0,  // strike
            10,
            0.01,
        );
        
        // max(40000 - 50000, 0) = 0
        assert_eq!(payoff, 0.0);
    }
    
    #[test]
    fn test_put_payoff_itm() {
        // Settlement price below strike = ITM
        let payoff = PayoffCalculator::put_payoff(
            40000.0,  // settlement
            50000.0,  // strike
            10,
            0.01,
        );
        
        // (50000 - 40000) × 10 × 0.01 = 10000 × 0.1 = 100
        assert_eq!(payoff, 100.0);
    }
    
    #[test]
    fn test_position_payoff_long_call() {
        use exchange_instrument::domain::OptionType;
        use exchange_risk::domain::PositionSide;
        
        let payoff = PayoffCalculator::position_payoff(
            60000.0,
            50000.0,
            OptionType::Call,
            PositionSide::Long,
            10,
            0.01,
        );
        
        // Long call: receives payoff
        assert_eq!(payoff, 100.0);
    }
    
    #[test]
    fn test_position_payoff_short_call() {
        use exchange_instrument::domain::OptionType;
        use exchange_risk::domain::PositionSide;
        
        let payoff = PayoffCalculator::position_payoff(
            60000.0,
            50000.0,
            OptionType::Call,
            PositionSide::Short,
            10,
            0.01,
        );
        
        // Short call: pays payoff (negative)
        assert_eq!(payoff, -100.0);
    }
}
