use serde::{Deserialize, Serialize};
use std::fmt;

/// Option type: Call or Put.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Hash)]
pub enum OptionType {
    Call,
    Put,
}

impl fmt::Display for OptionType {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            OptionType::Call => write!(f, "C"),
            OptionType::Put => write!(f, "P"),
        }
    }
}

/// Option exercise style.
/// V0 supports European only (exercise at expiry).
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Hash)]
pub enum OptionStyle {
    European,
}

impl fmt::Display for OptionStyle {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            OptionStyle::European => write!(f, "European"),
        }
    }
}

/// Instrument lifecycle status.
/// Transitions: Draft -> Listed -> Active -> Expired -> Settled -> Archived
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Hash)]
pub enum InstrumentStatus {
    /// Newly created, not yet visible to traders
    Draft,
    /// Visible but not yet tradeable
    Listed,
    /// Open for trading
    Active,
    /// Trading temporarily halted (circuit breaker, admin action)
    Halted,
    /// Past expiry timestamp, no new trades
    Expired,
    /// Final settlement completed
    Settled,
    /// Permanently archived
    Archived,
}

impl InstrumentStatus {
    /// Returns whether a transition from the current status to `new_status` is valid.
    pub fn can_transition_to(&self, new_status: &InstrumentStatus) -> bool {
        matches!(
            (self, new_status),
            (InstrumentStatus::Draft, InstrumentStatus::Listed)
                | (InstrumentStatus::Listed, InstrumentStatus::Active)
                | (InstrumentStatus::Active, InstrumentStatus::Expired)
                | (InstrumentStatus::Active, InstrumentStatus::Halted)
                | (InstrumentStatus::Halted, InstrumentStatus::Active)
                | (InstrumentStatus::Expired, InstrumentStatus::Settled)
                | (InstrumentStatus::Settled, InstrumentStatus::Archived)
        )
    }
}

impl fmt::Display for InstrumentStatus {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            InstrumentStatus::Draft => write!(f, "Draft"),
            InstrumentStatus::Listed => write!(f, "Listed"),
            InstrumentStatus::Active => write!(f, "Active"),
            InstrumentStatus::Expired => write!(f, "Expired"),
            InstrumentStatus::Settled => write!(f, "Settled"),
            InstrumentStatus::Archived => write!(f, "Archived"),
            InstrumentStatus::Halted => write!(f, "Halted"),
        }
    }
}

/// Market type classification.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Hash)]
pub enum MarketType {
    Options,
}

impl fmt::Display for MarketType {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            MarketType::Options => write!(f, "Options"),
        }
    }
}

/// Market-level status.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Hash)]
pub enum MarketStatus {
    Active,
    Halted,
    Expired,
}

impl fmt::Display for MarketStatus {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            MarketStatus::Active => write!(f, "Active"),
            MarketStatus::Halted => write!(f, "Halted"),
            MarketStatus::Expired => write!(f, "Expired"),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_instrument_status_transitions() {
        assert!(InstrumentStatus::Draft.can_transition_to(&InstrumentStatus::Listed));
        assert!(InstrumentStatus::Listed.can_transition_to(&InstrumentStatus::Active));
        assert!(InstrumentStatus::Active.can_transition_to(&InstrumentStatus::Expired));
        assert!(InstrumentStatus::Expired.can_transition_to(&InstrumentStatus::Settled));
        assert!(InstrumentStatus::Settled.can_transition_to(&InstrumentStatus::Archived));

        // Invalid transitions
        assert!(!InstrumentStatus::Draft.can_transition_to(&InstrumentStatus::Active));
        assert!(!InstrumentStatus::Active.can_transition_to(&InstrumentStatus::Draft));
        assert!(!InstrumentStatus::Archived.can_transition_to(&InstrumentStatus::Draft));
        assert!(!InstrumentStatus::Expired.can_transition_to(&InstrumentStatus::Active));
    }

    #[test]
    fn test_halted_transitions() {
        assert!(InstrumentStatus::Active.can_transition_to(&InstrumentStatus::Halted));
        assert!(InstrumentStatus::Halted.can_transition_to(&InstrumentStatus::Active));
        assert!(!InstrumentStatus::Halted.can_transition_to(&InstrumentStatus::Listed));
    }

    #[test]
    fn test_option_type_display() {
        assert_eq!(format!("{}", OptionType::Call), "C");
        assert_eq!(format!("{}", OptionType::Put), "P");
    }

    #[test]
    fn test_option_type_serialization() {
        let call = OptionType::Call;
        let json = serde_json::to_string(&call).unwrap();
        let deserialized: OptionType = serde_json::from_str(&json).unwrap();
        assert_eq!(call, deserialized);
    }
}
