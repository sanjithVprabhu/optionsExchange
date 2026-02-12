use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use uuid::Uuid;

// ============================================================================
// POSITION
// ============================================================================

/// Position represents a user's holding in a specific instrument.
///
/// CRITICAL: This is the ground truth for risk calculations.
/// Long options = capped risk (premium paid)
/// Short options = unbounded/large risk (obligation to pay)
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct Position {
    pub user_id: Uuid,
    pub instrument_id: String,
    pub side: PositionSide,
    /// Number of contracts held
    pub quantity: u32,
    /// Average entry price
    pub avg_price: f64,
    pub opened_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Hash)]
pub enum PositionSide {
    /// Long = bought option (risk capped at premium)
    Long,
    /// Short = sold/wrote option (risk unbounded/large)
    Short,
}

impl std::fmt::Display for PositionSide {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            PositionSide::Long => write!(f, "LONG"),
            PositionSide::Short => write!(f, "SHORT"),
        }
    }
}

impl Position {
    pub fn new(
        user_id: Uuid,
        instrument_id: String,
        side: PositionSide,
        quantity: u32,
        price: f64,
    ) -> Self {
        let now = Utc::now();
        Self {
            user_id,
            instrument_id,
            side,
            quantity,
            avg_price: price,
            opened_at: now,
            updated_at: now,
        }
    }

    /// Update position with new fill (weighted average price)
    pub fn update_fill(&mut self, fill_quantity: u32, fill_price: f64) {
        let total_value =
            self.avg_price * self.quantity as f64 + fill_price * fill_quantity as f64;
        self.quantity += fill_quantity;
        if self.quantity > 0 {
            self.avg_price = total_value / self.quantity as f64;
        }
        self.updated_at = Utc::now();
    }

    /// Reduce position (closing)
    pub fn reduce(&mut self, quantity: u32) {
        self.quantity = self.quantity.saturating_sub(quantity);
        self.updated_at = Utc::now();
    }

    /// Check if position is closed
    pub fn is_closed(&self) -> bool {
        self.quantity == 0
    }
}

// ============================================================================
// MARGIN REQUIREMENTS
// ============================================================================

/// Margin requirement for a position or portfolio
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct MarginRequirement {
    /// Initial margin (required to open/increase position)
    pub initial_margin: f64,
    /// Maintenance margin (required to keep position alive)
    pub maintenance_margin: f64,
}

impl MarginRequirement {
    pub fn new(initial_margin: f64, maintenance_margin: f64) -> Self {
        Self {
            initial_margin,
            maintenance_margin,
        }
    }

    /// Zero margin (for long options after premium paid)
    pub fn zero() -> Self {
        Self {
            initial_margin: 0.0,
            maintenance_margin: 0.0,
        }
    }
}

// ============================================================================
// USER RISK STATE
// ============================================================================

/// Complete risk state for a user
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UserRiskState {
    pub user_id: Uuid,
    /// Wallet balance (in settlement currency, e.g., USDT)
    pub wallet_balance: f64,
    /// All positions keyed by instrument_id
    pub positions: HashMap<String, Position>,
    /// Reserved margin for open orders
    pub reserved_margin: f64,
    /// Total initial margin across all positions
    pub total_initial_margin: f64,
    /// Total maintenance margin across all positions
    pub total_maintenance_margin: f64,
    /// Unrealized PnL across all positions
    pub unrealized_pnl: f64,
    pub updated_at: DateTime<Utc>,
}

impl UserRiskState {
    pub fn new(user_id: Uuid, wallet_balance: f64) -> Self {
        Self {
            user_id,
            wallet_balance,
            positions: HashMap::new(),
            reserved_margin: 0.0,
            total_initial_margin: 0.0,
            total_maintenance_margin: 0.0,
            unrealized_pnl: 0.0,
            updated_at: Utc::now(),
        }
    }

    /// Calculate equity = wallet_balance + unrealized_pnl
    pub fn equity(&self) -> f64 {
        self.wallet_balance + self.unrealized_pnl
    }

    /// Calculate free margin (available for new positions)
    pub fn free_margin(&self) -> f64 {
        self.equity() - self.total_initial_margin - self.reserved_margin
    }

    /// Check if account is liquidatable (equity < maintenance)
    pub fn is_liquidatable(&self) -> bool {
        self.equity() < self.total_maintenance_margin
    }

    /// Get margin usage ratio
    pub fn margin_usage(&self) -> f64 {
        let equity = self.equity();
        if equity == 0.0 {
            0.0
        } else {
            (self.total_initial_margin + self.reserved_margin) / equity
        }
    }
}

// ============================================================================
// MARGIN CALCULATION PARAMETERS
// ============================================================================

/// Configuration for margin calculations
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MarginConfig {
    /// Stress multiplier for short calls (e.g., 0.15 = 15%)
    pub short_call_stress_multiplier: f64,
    /// Maintenance margin ratio (e.g., 0.75 = 75% of initial)
    pub maintenance_ratio: f64,
    /// Max position size per instrument (contracts)
    pub max_position_size: u32,
    /// Max total notional per user (in settlement currency)
    pub max_total_notional: f64,
    /// Max number of open positions per user
    pub max_open_positions: usize,
}

impl Default for MarginConfig {
    fn default() -> Self {
        Self {
            short_call_stress_multiplier: 0.15, // 15%
            maintenance_ratio: 0.75,             // 75% of initial
            max_position_size: 10000,
            max_total_notional: 1_000_000.0, // 1M USDT
            max_open_positions: 100,
        }
    }
}

// ============================================================================
// RISK CHECK RESULT
// ============================================================================

/// Result of risk check for an order
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RiskCheckResult {
    /// Whether order is approved
    pub approved: bool,
    /// Rejection reason (if rejected)
    pub reason: Option<String>,
    /// Required initial margin for this order
    pub required_margin: f64,
    /// User's free margin before order
    pub free_margin: f64,
    /// Projected free margin after order
    pub projected_free_margin: f64,
}

impl RiskCheckResult {
    pub fn approved(required_margin: f64, free_margin: f64, projected_free: f64) -> Self {
        Self {
            approved: true,
            reason: None,
            required_margin,
            free_margin,
            projected_free_margin: projected_free,
        }
    }

    pub fn rejected(reason: String, required: f64, free: f64) -> Self {
        Self {
            approved: false,
            reason: Some(reason),
            required_margin: required,
            free_margin: free,
            projected_free_margin: free,
        }
    }
}

// ============================================================================
// LIQUIDATION STATE
// ============================================================================

/// Liquidation eligibility state machine
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
pub enum LiquidationState {
    /// Account is healthy
    Healthy,
    /// Account is eligible for liquidation
    Liquidatable,
    /// Liquidation is in progress
    Liquidating,
    /// Liquidation completed
    Resolved,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_position_new_and_fill() {
        let user = Uuid::new_v4();
        let mut pos = Position::new(user, "BTC-CALL".to_string(), PositionSide::Long, 10, 100.0);

        assert_eq!(pos.quantity, 10);
        assert_eq!(pos.avg_price, 100.0);
        assert!(!pos.is_closed());

        // Add more at different price -> weighted average
        pos.update_fill(10, 200.0);
        assert_eq!(pos.quantity, 20);
        assert_eq!(pos.avg_price, 150.0); // (100*10 + 200*10) / 20

        pos.reduce(15);
        assert_eq!(pos.quantity, 5);
        assert!(!pos.is_closed());

        pos.reduce(5);
        assert!(pos.is_closed());
    }

    #[test]
    fn test_position_reduce_saturates() {
        let mut pos = Position::new(
            Uuid::new_v4(),
            "BTC-CALL".to_string(),
            PositionSide::Long,
            5,
            100.0,
        );
        pos.reduce(10); // More than available
        assert_eq!(pos.quantity, 0);
        assert!(pos.is_closed());
    }

    #[test]
    fn test_user_risk_state_equity_and_margin() {
        let user = Uuid::new_v4();
        let mut state = UserRiskState::new(user, 10000.0);

        assert_eq!(state.equity(), 10000.0);
        assert_eq!(state.free_margin(), 10000.0);
        assert!(!state.is_liquidatable());

        // Reserve some margin
        state.reserved_margin = 2000.0;
        assert_eq!(state.free_margin(), 8000.0);

        // Set position margins
        state.total_initial_margin = 3000.0;
        state.total_maintenance_margin = 2250.0;
        assert_eq!(state.free_margin(), 5000.0); // 10000 - 3000 - 2000

        // Add negative unrealized PnL
        state.unrealized_pnl = -8000.0;
        assert_eq!(state.equity(), 2000.0);
        assert!(state.is_liquidatable()); // 2000 < 2250
    }

    #[test]
    fn test_user_risk_state_margin_usage() {
        let user = Uuid::new_v4();
        let mut state = UserRiskState::new(user, 10000.0);
        assert_eq!(state.margin_usage(), 0.0);

        state.total_initial_margin = 5000.0;
        state.reserved_margin = 2000.0;
        // (5000 + 2000) / 10000 = 0.7
        assert!((state.margin_usage() - 0.7).abs() < 1e-10);
    }

    #[test]
    fn test_risk_check_result() {
        let approved = RiskCheckResult::approved(1000.0, 5000.0, 4000.0);
        assert!(approved.approved);
        assert!(approved.reason.is_none());

        let rejected = RiskCheckResult::rejected("No funds".to_string(), 5000.0, 100.0);
        assert!(!rejected.approved);
        assert_eq!(rejected.reason.as_deref(), Some("No funds"));
    }

    #[test]
    fn test_margin_requirement() {
        let margin = MarginRequirement::new(1000.0, 750.0);
        assert_eq!(margin.initial_margin, 1000.0);
        assert_eq!(margin.maintenance_margin, 750.0);

        let zero = MarginRequirement::zero();
        assert_eq!(zero.initial_margin, 0.0);
        assert_eq!(zero.maintenance_margin, 0.0);
    }
}
