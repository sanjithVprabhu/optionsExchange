use chrono::Utc;

use crate::domain::OptionInstrument;
use crate::error::InstrumentError;

/// Validates an instrument before creation.
///
/// Rules:
/// - Expiry must be in the future
/// - Strike price must be positive
/// - Strike must be aligned to tick size
/// - Contract size must be positive
/// - Min order size must be >= 1
/// - Tick size must be positive
pub fn validate_instrument(instrument: &OptionInstrument) -> Result<(), InstrumentError> {
    // Expiry must be in the future
    if instrument.expiry_timestamp <= Utc::now() {
        return Err(InstrumentError::InvalidInstrument(
            "Expiry must be in the future".to_string(),
        ));
    }

    // Strike must be positive
    if instrument.strike_price <= 0.0 {
        return Err(InstrumentError::InvalidInstrument(
            "Strike price must be positive".to_string(),
        ));
    }

    // Tick size must be positive
    if instrument.tick_size <= 0.0 {
        return Err(InstrumentError::InvalidInstrument(
            "Tick size must be positive".to_string(),
        ));
    }

    // Strike must be aligned to tick size
    // Use round-trip check to handle floating point precision
    let ticks = (instrument.strike_price / instrument.tick_size).round();
    let reconstructed = ticks * instrument.tick_size;
    let deviation = (instrument.strike_price - reconstructed).abs();
    if deviation > instrument.tick_size * 1e-9 {
        return Err(InstrumentError::InvalidInstrument(format!(
            "Strike {} not aligned to tick size {}",
            instrument.strike_price, instrument.tick_size
        )));
    }

    // Contract size must be positive
    if instrument.contract_size <= 0.0 {
        return Err(InstrumentError::InvalidInstrument(
            "Contract size must be positive".to_string(),
        ));
    }

    // Min order size must be at least 1
    if instrument.min_order_size < 1 {
        return Err(InstrumentError::InvalidInstrument(
            "Minimum order size must be at least 1".to_string(),
        ));
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::OptionInstrumentBuilder;
    use chrono::Duration;
    use exchange_primitives::{Asset, Currency, InstrumentStatus, OptionType};
    use uuid::Uuid;

    fn valid_instrument() -> OptionInstrument {
        OptionInstrumentBuilder::new(Uuid::new_v4(), Asset::btc(), Currency::usdt())
            .option_type(OptionType::Call)
            .strike_price(50000.0)
            .expiry_timestamp(Utc::now() + Duration::days(30))
            .status(InstrumentStatus::Draft)
            .build()
    }

    #[test]
    fn test_valid_instrument_passes() {
        let instrument = valid_instrument();
        assert!(validate_instrument(&instrument).is_ok());
    }

    #[test]
    fn test_rejects_expired_instrument() {
        let mut instrument = valid_instrument();
        instrument.expiry_timestamp = Utc::now() - Duration::hours(1);
        let result = validate_instrument(&instrument);
        assert!(result.is_err());
        assert!(matches!(result, Err(InstrumentError::InvalidInstrument(_))));
    }

    #[test]
    fn test_rejects_zero_strike() {
        let mut instrument = valid_instrument();
        instrument.strike_price = 0.0;
        assert!(validate_instrument(&instrument).is_err());
    }

    #[test]
    fn test_rejects_negative_strike() {
        let mut instrument = valid_instrument();
        instrument.strike_price = -100.0;
        assert!(validate_instrument(&instrument).is_err());
    }

    #[test]
    fn test_rejects_misaligned_strike() {
        let mut instrument = valid_instrument();
        // tick_size is 0.5, so 50000.3 is misaligned
        instrument.strike_price = 50000.3;
        assert!(validate_instrument(&instrument).is_err());
    }

    #[test]
    fn test_accepts_aligned_strike() {
        let mut instrument = valid_instrument();
        instrument.strike_price = 50000.5; // aligned to 0.5 tick
        assert!(validate_instrument(&instrument).is_ok());
    }

    #[test]
    fn test_rejects_zero_contract_size() {
        let mut instrument = valid_instrument();
        instrument.contract_size = 0.0;
        assert!(validate_instrument(&instrument).is_err());
    }

    #[test]
    fn test_rejects_negative_contract_size() {
        let mut instrument = valid_instrument();
        instrument.contract_size = -0.01;
        assert!(validate_instrument(&instrument).is_err());
    }

    #[test]
    fn test_rejects_zero_tick_size() {
        let mut instrument = valid_instrument();
        instrument.tick_size = 0.0;
        assert!(validate_instrument(&instrument).is_err());
    }

    #[test]
    fn test_rejects_zero_min_order_size() {
        let mut instrument = valid_instrument();
        instrument.min_order_size = 0;
        assert!(validate_instrument(&instrument).is_err());
    }
}
