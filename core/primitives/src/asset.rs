use serde::{Deserialize, Serialize};

/// Represents a tradeable underlying asset (e.g., BTC, ETH).
///
/// Assets define the properties of the underlying cryptocurrency
/// that option contracts are written against.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct Asset {
    /// Unique identifier / ticker symbol (e.g., "BTC", "ETH")
    pub asset_id: String,
    /// Human-readable name (e.g., "Bitcoin")
    pub name: String,
    /// Number of decimal places for the asset (8 for BTC, 18 for ETH)
    pub decimals: u8,
    /// Size of one contract in units of the underlying (e.g., 0.01 BTC)
    pub contract_size: f64,
    /// Minimum number of contracts per order
    pub min_order_size: u32,
    /// Minimum price increment in settlement currency (e.g., 0.5 USDT)
    pub tick_size: f64,
    /// Number of decimal places for price display
    pub price_decimals: u8,
}

impl Asset {
    pub fn new(
        asset_id: impl Into<String>,
        name: impl Into<String>,
        decimals: u8,
        contract_size: f64,
        min_order_size: u32,
        tick_size: f64,
        price_decimals: u8,
    ) -> Self {
        Self {
            asset_id: asset_id.into(),
            name: name.into(),
            decimals,
            contract_size,
            min_order_size,
            tick_size,
            price_decimals,
        }
    }

    /// Creates a standard BTC asset configuration.
    pub fn btc() -> Self {
        Self::new("BTC", "Bitcoin", 8, 0.01, 1, 0.5, 2)
    }

    /// Creates a standard ETH asset configuration.
    pub fn eth() -> Self {
        Self::new("ETH", "Ethereum", 18, 0.1, 1, 0.1, 2)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_btc_defaults() {
        let btc = Asset::btc();
        assert_eq!(btc.asset_id, "BTC");
        assert_eq!(btc.decimals, 8);
        assert_eq!(btc.contract_size, 0.01);
        assert_eq!(btc.tick_size, 0.5);
    }

    #[test]
    fn test_asset_equality() {
        let a1 = Asset::btc();
        let a2 = Asset::btc();
        assert_eq!(a1, a2);
    }

    #[test]
    fn test_asset_serialization() {
        let btc = Asset::btc();
        let json = serde_json::to_string(&btc).unwrap();
        let deserialized: Asset = serde_json::from_str(&json).unwrap();
        assert_eq!(btc, deserialized);
    }
}
