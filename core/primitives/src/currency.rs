use serde::{Deserialize, Serialize};

/// Represents a settlement currency (e.g., USDT, USDC).
///
/// All option payoffs, margin requirements, and fees are denominated
/// in a settlement currency.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Hash)]
pub struct Currency {
    /// Unique identifier (e.g., "USDT", "USDC")
    pub currency_id: String,
    /// Human-readable name (e.g., "Tether USD")
    pub name: String,
    /// Number of decimal places (6 for USDT/USDC)
    pub decimals: u8,
}

impl Currency {
    pub fn new(
        currency_id: impl Into<String>,
        name: impl Into<String>,
        decimals: u8,
    ) -> Self {
        Self {
            currency_id: currency_id.into(),
            name: name.into(),
            decimals,
        }
    }

    /// Creates a standard USDT currency.
    pub fn usdt() -> Self {
        Self::new("USDT", "Tether USD", 6)
    }

    /// Creates a standard USDC currency.
    pub fn usdc() -> Self {
        Self::new("USDC", "USD Coin", 6)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_usdt_defaults() {
        let usdt = Currency::usdt();
        assert_eq!(usdt.currency_id, "USDT");
        assert_eq!(usdt.decimals, 6);
    }

    #[test]
    fn test_currency_serialization() {
        let usdt = Currency::usdt();
        let json = serde_json::to_string(&usdt).unwrap();
        let deserialized: Currency = serde_json::from_str(&json).unwrap();
        assert_eq!(usdt, deserialized);
    }
}
