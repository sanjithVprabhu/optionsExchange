use exchange_instrument::OptionInstrument;
use exchange_primitives::InstrumentStatus;
use thiserror::Error;

use crate::domain::{Order, OrderStatus, OrderType};

/// Validation errors for order submission
#[derive(Debug, Error)]
pub enum OrderValidationError {
    #[error("Quantity must be at least {min} (got {actual})")]
    QuantityTooSmall { min: u32, actual: u32 },

    #[error("Quantity exceeds maximum {max} (got {actual})")]
    QuantityTooLarge { max: u32, actual: u32 },

    #[error("Price must be positive (got {0})")]
    InvalidPrice(f64),

    #[error("Price {price} not aligned to tick size {tick_size}")]
    PriceNotAligned { price: f64, tick_size: f64 },

    #[error("Price deviation too large: {price} vs mark {mark} (max {max}%)")]
    PriceDeviationTooLarge { price: f64, mark: f64, max: f64 },

    #[error("Limit orders require a price")]
    MissingPrice,

    #[error("Instrument not tradeable (status: {0})")]
    InstrumentNotTradeable(InstrumentStatus),

    #[error("Invalid status transition: {from:?} -> {to:?}")]
    InvalidTransition { from: OrderStatus, to: OrderStatus },
}

/// Validation configuration for orders
pub struct OrderValidationConfig {
    /// Minimum contracts per order (system-level)
    pub min_order_size: u32,
    /// Maximum contracts per order (system-level)
    pub max_order_size: u32,
    /// Maximum allowed price deviation from mark as decimal (0.20 = 20%)
    pub max_price_deviation: f64,
}

impl Default for OrderValidationConfig {
    fn default() -> Self {
        Self {
            min_order_size: 1,
            max_order_size: 10000,
            max_price_deviation: 0.20,
        }
    }
}

/// Validate order against instrument rules and system config.
///
/// Checks:
/// 1. Instrument is tradeable (Active + not expired)
/// 2. Quantity within system bounds
/// 3. Quantity >= instrument's min_order_size
/// 4. Price is positive (limit orders)
/// 5. Price aligns to tick size (round-trip check to avoid float issues)
/// 6. Limit orders have a price
pub fn validate_order(
    order: &Order,
    instrument: &OptionInstrument,
    config: &OrderValidationConfig,
) -> Result<(), OrderValidationError> {
    // 1. Instrument must be tradeable
    if !instrument.is_tradeable() {
        return Err(OrderValidationError::InstrumentNotTradeable(
            instrument.status,
        ));
    }

    // 2. System-level quantity bounds
    if order.quantity < config.min_order_size {
        return Err(OrderValidationError::QuantityTooSmall {
            min: config.min_order_size,
            actual: order.quantity,
        });
    }

    if order.quantity > config.max_order_size {
        return Err(OrderValidationError::QuantityTooLarge {
            max: config.max_order_size,
            actual: order.quantity,
        });
    }

    // 3. Instrument-specific quantity bounds
    if order.quantity < instrument.min_order_size {
        return Err(OrderValidationError::QuantityTooSmall {
            min: instrument.min_order_size,
            actual: order.quantity,
        });
    }

    // 4. Price validation (limit orders only)
    if let Some(price) = order.price {
        if price <= 0.0 {
            return Err(OrderValidationError::InvalidPrice(price));
        }

        // Price must align to tick size (using round-trip check, same as Module 01)
        let ticks = (price / instrument.tick_size).round();
        let reconstructed = ticks * instrument.tick_size;
        let deviation = (price - reconstructed).abs();
        if deviation > instrument.tick_size * 1e-9 {
            return Err(OrderValidationError::PriceNotAligned {
                price,
                tick_size: instrument.tick_size,
            });
        }
    } else if order.order_type == OrderType::Limit {
        return Err(OrderValidationError::MissingPrice);
    }

    Ok(())
}

/// Validate price deviation from mark price.
///
/// Ensures the order price is within allowed deviation from the current
/// mark price. This prevents obviously mispriced orders.
pub fn validate_price_deviation(
    order_price: f64,
    mark_price: f64,
    config: &OrderValidationConfig,
) -> Result<(), OrderValidationError> {
    if mark_price <= 0.0 {
        return Ok(());
    }

    let deviation = (order_price - mark_price).abs() / mark_price;

    if deviation > config.max_price_deviation {
        return Err(OrderValidationError::PriceDeviationTooLarge {
            price: order_price,
            mark: mark_price,
            max: config.max_price_deviation * 100.0,
        });
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::{OrderBuilder, OrderSide};
    use chrono::{Duration, Utc};
    use exchange_instrument::OptionInstrumentBuilder;
    use exchange_primitives::{Asset, Currency};
    use uuid::Uuid;

    fn make_active_instrument() -> OptionInstrument {
        OptionInstrumentBuilder::new(Uuid::new_v4(), Asset::btc(), Currency::usdt())
            .option_type(exchange_primitives::OptionType::Call)
            .strike_price(50000.0)
            .expiry_timestamp(Utc::now() + Duration::days(30))
            .status(InstrumentStatus::Active)
            .build()
    }

    #[test]
    fn test_valid_order() {
        let instrument = make_active_instrument();
        let config = OrderValidationConfig::default();

        let order = OrderBuilder::new()
            .user_id(Uuid::new_v4())
            .instrument_id(&instrument.instrument_id)
            .side(OrderSide::Buy)
            .price(100.0)
            .quantity(10)
            .build()
            .unwrap();

        assert!(validate_order(&order, &instrument, &config).is_ok());
    }

    #[test]
    fn test_instrument_not_tradeable() {
        let mut instrument = make_active_instrument();
        instrument.status = InstrumentStatus::Draft;
        let config = OrderValidationConfig::default();

        let order = OrderBuilder::new()
            .user_id(Uuid::new_v4())
            .instrument_id(&instrument.instrument_id)
            .side(OrderSide::Buy)
            .price(100.0)
            .quantity(10)
            .build()
            .unwrap();

        let result = validate_order(&order, &instrument, &config);
        assert!(matches!(
            result,
            Err(OrderValidationError::InstrumentNotTradeable(_))
        ));
    }

    #[test]
    fn test_quantity_too_small() {
        let instrument = make_active_instrument();
        let config = OrderValidationConfig {
            min_order_size: 5,
            ..Default::default()
        };

        let order = OrderBuilder::new()
            .user_id(Uuid::new_v4())
            .instrument_id(&instrument.instrument_id)
            .side(OrderSide::Buy)
            .price(100.0)
            .quantity(2)
            .build()
            .unwrap();

        let result = validate_order(&order, &instrument, &config);
        assert!(matches!(
            result,
            Err(OrderValidationError::QuantityTooSmall { .. })
        ));
    }

    #[test]
    fn test_quantity_too_large() {
        let instrument = make_active_instrument();
        let config = OrderValidationConfig {
            max_order_size: 100,
            ..Default::default()
        };

        let order = OrderBuilder::new()
            .user_id(Uuid::new_v4())
            .instrument_id(&instrument.instrument_id)
            .side(OrderSide::Buy)
            .price(100.0)
            .quantity(200)
            .build()
            .unwrap();

        let result = validate_order(&order, &instrument, &config);
        assert!(matches!(
            result,
            Err(OrderValidationError::QuantityTooLarge { .. })
        ));
    }

    #[test]
    fn test_invalid_price() {
        let instrument = make_active_instrument();
        let config = OrderValidationConfig::default();

        let order = OrderBuilder::new()
            .user_id(Uuid::new_v4())
            .instrument_id(&instrument.instrument_id)
            .side(OrderSide::Buy)
            .price(-10.0)
            .quantity(10)
            .build()
            .unwrap();

        let result = validate_order(&order, &instrument, &config);
        assert!(matches!(
            result,
            Err(OrderValidationError::InvalidPrice(_))
        ));
    }

    #[test]
    fn test_price_not_aligned() {
        let instrument = make_active_instrument(); // tick_size = 0.5
        let config = OrderValidationConfig::default();

        let order = OrderBuilder::new()
            .user_id(Uuid::new_v4())
            .instrument_id(&instrument.instrument_id)
            .side(OrderSide::Buy)
            .price(100.3) // Not aligned to 0.5
            .quantity(10)
            .build()
            .unwrap();

        let result = validate_order(&order, &instrument, &config);
        assert!(matches!(
            result,
            Err(OrderValidationError::PriceNotAligned { .. })
        ));
    }

    #[test]
    fn test_price_aligned() {
        let instrument = make_active_instrument(); // tick_size = 0.5
        let config = OrderValidationConfig::default();

        // 100.5 is aligned to 0.5
        let order = OrderBuilder::new()
            .user_id(Uuid::new_v4())
            .instrument_id(&instrument.instrument_id)
            .side(OrderSide::Buy)
            .price(100.5)
            .quantity(10)
            .build()
            .unwrap();

        assert!(validate_order(&order, &instrument, &config).is_ok());
    }

    #[test]
    fn test_price_deviation_within_bounds() {
        let config = OrderValidationConfig::default(); // 20% max deviation
        assert!(validate_price_deviation(110.0, 100.0, &config).is_ok());
        assert!(validate_price_deviation(90.0, 100.0, &config).is_ok());
    }

    #[test]
    fn test_price_deviation_exceeded() {
        let config = OrderValidationConfig::default(); // 20% max deviation

        let result = validate_price_deviation(150.0, 100.0, &config);
        assert!(matches!(
            result,
            Err(OrderValidationError::PriceDeviationTooLarge { .. })
        ));
    }
}
