use std::collections::HashSet;
use uuid::Uuid;

use crate::domain::*;

/// Liquidation detector - monitors for liquidatable accounts.
///
/// CRITICAL: Liquidation is STATE, not action.
/// This detector declares "LIQUIDATABLE" - a separate liquidation engine
/// would execute the actual forced orders.
pub struct LiquidationDetector {
    /// Users currently in liquidatable state
    liquidatable_users: HashSet<Uuid>,
}

/// Events emitted by the liquidation detector
#[derive(Debug, Clone)]
pub enum LiquidationEvent {
    /// Account crossed below maintenance margin
    BecameLiquidatable {
        user_id: Uuid,
        equity: f64,
        maintenance_margin: f64,
    },
    /// Account recovered above maintenance margin
    Recovered { user_id: Uuid },
}

impl LiquidationDetector {
    pub fn new() -> Self {
        Self {
            liquidatable_users: HashSet::new(),
        }
    }

    /// Check and update liquidation status for a user.
    ///
    /// Returns an event if the state changed (became liquidatable or recovered).
    pub fn check_user(&mut self, state: &UserRiskState) -> Option<LiquidationEvent> {
        let is_liquidatable = state.is_liquidatable();
        let was_liquidatable = self.liquidatable_users.contains(&state.user_id);

        match (was_liquidatable, is_liquidatable) {
            (false, true) => {
                // Became liquidatable
                self.liquidatable_users.insert(state.user_id);
                Some(LiquidationEvent::BecameLiquidatable {
                    user_id: state.user_id,
                    equity: state.equity(),
                    maintenance_margin: state.total_maintenance_margin,
                })
            }
            (true, false) => {
                // Recovered from liquidation
                self.liquidatable_users.remove(&state.user_id);
                Some(LiquidationEvent::Recovered {
                    user_id: state.user_id,
                })
            }
            _ => None, // No state change
        }
    }

    /// Get all currently liquidatable users
    pub fn get_liquidatable_users(&self) -> Vec<Uuid> {
        self.liquidatable_users.iter().copied().collect()
    }

    /// Check if a specific user is liquidatable
    pub fn is_liquidatable(&self, user_id: Uuid) -> bool {
        self.liquidatable_users.contains(&user_id)
    }
}

impl Default for LiquidationDetector {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_healthy_state(user_id: Uuid) -> UserRiskState {
        let mut state = UserRiskState::new(user_id, 10000.0);
        state.total_maintenance_margin = 2000.0;
        state
    }

    fn make_liquidatable_state(user_id: Uuid) -> UserRiskState {
        let mut state = UserRiskState::new(user_id, 1000.0);
        state.total_maintenance_margin = 5000.0;
        state
    }

    #[test]
    fn test_healthy_user_no_event() {
        let mut detector = LiquidationDetector::new();
        let user = Uuid::new_v4();
        let state = make_healthy_state(user);

        let event = detector.check_user(&state);
        assert!(event.is_none());
        assert!(detector.get_liquidatable_users().is_empty());
    }

    #[test]
    fn test_becomes_liquidatable() {
        let mut detector = LiquidationDetector::new();
        let user = Uuid::new_v4();
        let state = make_liquidatable_state(user);

        let event = detector.check_user(&state);
        assert!(event.is_some());

        if let Some(LiquidationEvent::BecameLiquidatable {
            user_id,
            equity,
            maintenance_margin,
        }) = event
        {
            assert_eq!(user_id, user);
            assert_eq!(equity, 1000.0);
            assert_eq!(maintenance_margin, 5000.0);
        } else {
            panic!("Expected BecameLiquidatable event");
        }

        assert!(detector.is_liquidatable(user));
        assert_eq!(detector.get_liquidatable_users().len(), 1);
    }

    #[test]
    fn test_stays_liquidatable_no_event() {
        let mut detector = LiquidationDetector::new();
        let user = Uuid::new_v4();
        let state = make_liquidatable_state(user);

        // First check → becomes liquidatable
        detector.check_user(&state);
        // Second check → still liquidatable, no new event
        let event = detector.check_user(&state);
        assert!(event.is_none());
    }

    #[test]
    fn test_recovery() {
        let mut detector = LiquidationDetector::new();
        let user = Uuid::new_v4();

        // Become liquidatable
        let bad_state = make_liquidatable_state(user);
        detector.check_user(&bad_state);
        assert!(detector.is_liquidatable(user));

        // Recover
        let good_state = make_healthy_state(user);
        let event = detector.check_user(&good_state);

        assert!(event.is_some());
        if let Some(LiquidationEvent::Recovered { user_id }) = event {
            assert_eq!(user_id, user);
        } else {
            panic!("Expected Recovered event");
        }

        assert!(!detector.is_liquidatable(user));
        assert!(detector.get_liquidatable_users().is_empty());
    }

    #[test]
    fn test_multiple_users() {
        let mut detector = LiquidationDetector::new();
        let user_a = Uuid::new_v4();
        let user_b = Uuid::new_v4();

        detector.check_user(&make_liquidatable_state(user_a));
        detector.check_user(&make_healthy_state(user_b));

        assert!(detector.is_liquidatable(user_a));
        assert!(!detector.is_liquidatable(user_b));
        assert_eq!(detector.get_liquidatable_users().len(), 1);
    }
}
