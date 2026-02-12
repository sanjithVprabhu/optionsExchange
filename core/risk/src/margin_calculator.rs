use std::collections::HashMap;

use exchange_instrument::OptionInstrument;
use exchange_oms::OrderSide;
use exchange_primitives::OptionType;

use crate::domain::*;

/// Margin calculator - implements margin formulas.
///
/// CRITICAL FORMULAS (v0):
///
/// Long Call/Put:
///   Initial = Premium paid (price × quantity)
///   Maintenance = 0
///
/// Short Call:
///   Initial = Q × C × max(α × S, β × (S - K))
///   Maintenance = 0.75 × Initial
///   where α = stress multiplier (15%), β = OTM sensitivity (1.0)
///
/// Short Put:
///   Initial = Q × C × K (worst case: S → 0)
///   Maintenance = 0.75 × Initial
pub struct MarginCalculator {
    config: MarginConfig,
}

impl MarginCalculator {
    pub fn new(config: MarginConfig) -> Self {
        Self { config }
    }

    /// Calculate margin for an existing position.
    pub fn calculate_position_margin(
        &self,
        position: &Position,
        instrument: &OptionInstrument,
        current_price: f64,
    ) -> MarginRequirement {
        match position.side {
            PositionSide::Long => {
                // Long options: risk capped at premium paid.
                // No additional margin required during life of position.
                MarginRequirement::zero()
            }
            PositionSide::Short => {
                self.calculate_short_margin(position.quantity, instrument, current_price)
            }
        }
    }

    /// Calculate margin for short position
    fn calculate_short_margin(
        &self,
        quantity: u32,
        instrument: &OptionInstrument,
        current_price: f64,
    ) -> MarginRequirement {
        let qty = quantity as f64;
        let contract_size = instrument.contract_size;
        let strike = instrument.strike_price;

        let initial = match instrument.option_type {
            OptionType::Call => {
                // Short Call: unbounded risk
                // Formula: Q × C × max(α × S, (S - K) if ITM)
                let alpha = self.config.short_call_stress_multiplier;
                let stress_margin = alpha * current_price;
                let itm_margin = if current_price > strike {
                    current_price - strike
                } else {
                    0.0
                };
                qty * contract_size * stress_margin.max(itm_margin)
            }
            OptionType::Put => {
                // Short Put: worst case S → 0
                // Formula: Q × C × K
                qty * contract_size * strike
            }
        };

        let maintenance = initial * self.config.maintenance_ratio;
        MarginRequirement::new(initial, maintenance)
    }

    /// Calculate margin required for a new order (worst-case full fill).
    ///
    /// This determines how much margin must be reserved BEFORE the order
    /// can be sent to the matching engine.
    pub fn calculate_order_margin(
        &self,
        order_side: OrderSide,
        quantity: u32,
        price: f64,
        instrument: &OptionInstrument,
        current_price: f64,
    ) -> MarginRequirement {
        match order_side {
            OrderSide::Buy => {
                // Buying option = going long = premium is the margin
                let premium = price * quantity as f64;
                MarginRequirement::new(premium, 0.0)
            }
            OrderSide::Sell => {
                // Selling option = going short (writing)
                // Use short margin formulas
                self.calculate_short_margin(quantity, instrument, current_price)
            }
        }
    }

    /// Calculate total portfolio margin across all positions.
    pub fn calculate_portfolio_margin(
        &self,
        positions: &HashMap<String, Position>,
        instruments: &HashMap<String, OptionInstrument>,
        current_prices: &HashMap<String, f64>,
    ) -> MarginRequirement {
        let mut total_initial = 0.0;
        let mut total_maintenance = 0.0;

        for (instrument_id, position) in positions {
            if let Some(instrument) = instruments.get(instrument_id) {
                let current_price = current_prices
                    .get(instrument_id)
                    .copied()
                    .unwrap_or(instrument.strike_price);

                let margin =
                    self.calculate_position_margin(position, instrument, current_price);

                total_initial += margin.initial_margin;
                total_maintenance += margin.maintenance_margin;
            }
        }

        MarginRequirement::new(total_initial, total_maintenance)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::{Duration, Utc};
    use exchange_instrument::OptionInstrumentBuilder;
    use exchange_primitives::{Asset, Currency};
    use uuid::Uuid;

    fn make_instrument(option_type: OptionType, strike: f64) -> OptionInstrument {
        OptionInstrumentBuilder::new(Uuid::new_v4(), Asset::btc(), Currency::usdt())
            .option_type(option_type)
            .strike_price(strike)
            .expiry_timestamp(Utc::now() + Duration::days(30))
            .build()
    }

    #[test]
    fn test_long_call_margin_zero() {
        let calc = MarginCalculator::new(MarginConfig::default());
        let instrument = make_instrument(OptionType::Call, 50000.0);

        let position = Position::new(
            Uuid::new_v4(),
            instrument.instrument_id.clone(),
            PositionSide::Long,
            10,
            100.0,
        );

        let margin = calc.calculate_position_margin(&position, &instrument, 50000.0);
        assert_eq!(margin.initial_margin, 0.0);
        assert_eq!(margin.maintenance_margin, 0.0);
    }

    #[test]
    fn test_long_put_margin_zero() {
        let calc = MarginCalculator::new(MarginConfig::default());
        let instrument = make_instrument(OptionType::Put, 40000.0);

        let position = Position::new(
            Uuid::new_v4(),
            instrument.instrument_id.clone(),
            PositionSide::Long,
            10,
            50.0,
        );

        let margin = calc.calculate_position_margin(&position, &instrument, 50000.0);
        assert_eq!(margin.initial_margin, 0.0);
        assert_eq!(margin.maintenance_margin, 0.0);
    }

    #[test]
    fn test_short_call_margin() {
        let calc = MarginCalculator::new(MarginConfig::default());
        let instrument = make_instrument(OptionType::Call, 50000.0);

        let position = Position::new(
            Uuid::new_v4(),
            instrument.instrument_id.clone(),
            PositionSide::Short,
            10,
            100.0,
        );

        // BTC contract_size = 0.01
        // Short call at-the-money: Q × C × (α × S)
        // 10 × 0.01 × (0.15 × 50000) = 0.1 × 7500 = 750
        let margin = calc.calculate_position_margin(&position, &instrument, 50000.0);
        assert_eq!(margin.initial_margin, 750.0);
        assert_eq!(margin.maintenance_margin, 750.0 * 0.75);
    }

    #[test]
    fn test_short_call_itm_margin() {
        let calc = MarginCalculator::new(MarginConfig::default());
        // Strike 40000, current price 50000 → deep ITM
        let instrument = make_instrument(OptionType::Call, 40000.0);

        let position = Position::new(
            Uuid::new_v4(),
            instrument.instrument_id.clone(),
            PositionSide::Short,
            10,
            100.0,
        );

        // max(0.15 × 50000, 50000 - 40000) = max(7500, 10000) = 10000
        // 10 × 0.01 × 10000 = 1000
        let margin = calc.calculate_position_margin(&position, &instrument, 50000.0);
        assert_eq!(margin.initial_margin, 1000.0);
        assert_eq!(margin.maintenance_margin, 750.0);
    }

    #[test]
    fn test_short_put_margin() {
        let calc = MarginCalculator::new(MarginConfig::default());
        let instrument = make_instrument(OptionType::Put, 40000.0);

        let position = Position::new(
            Uuid::new_v4(),
            instrument.instrument_id.clone(),
            PositionSide::Short,
            10,
            100.0,
        );

        // Short put: Q × C × K = 10 × 0.01 × 40000 = 4000
        let margin = calc.calculate_position_margin(&position, &instrument, 50000.0);
        assert_eq!(margin.initial_margin, 4000.0);
        assert_eq!(margin.maintenance_margin, 3000.0);
    }

    #[test]
    fn test_order_margin_buy() {
        let calc = MarginCalculator::new(MarginConfig::default());
        let instrument = make_instrument(OptionType::Call, 50000.0);

        // Buy 10 @ 100 → premium = 1000
        let margin = calc.calculate_order_margin(
            OrderSide::Buy,
            10,
            100.0,
            &instrument,
            50000.0,
        );
        assert_eq!(margin.initial_margin, 1000.0);
        assert_eq!(margin.maintenance_margin, 0.0);
    }

    #[test]
    fn test_order_margin_sell() {
        let calc = MarginCalculator::new(MarginConfig::default());
        let instrument = make_instrument(OptionType::Call, 50000.0);

        // Sell 10 → short call margin
        let margin = calc.calculate_order_margin(
            OrderSide::Sell,
            10,
            100.0,
            &instrument,
            50000.0,
        );
        // Same as short call: 750
        assert_eq!(margin.initial_margin, 750.0);
    }

    #[test]
    fn test_portfolio_margin() {
        let calc = MarginCalculator::new(MarginConfig::default());
        let call = make_instrument(OptionType::Call, 50000.0);
        let put = make_instrument(OptionType::Put, 40000.0);

        let user = Uuid::new_v4();
        let mut positions = HashMap::new();

        // Long call → zero margin
        positions.insert(
            call.instrument_id.clone(),
            Position::new(user, call.instrument_id.clone(), PositionSide::Long, 10, 100.0),
        );
        // Short put → Q × C × K = 10 × 0.01 × 40000 = 4000
        positions.insert(
            put.instrument_id.clone(),
            Position::new(user, put.instrument_id.clone(), PositionSide::Short, 10, 50.0),
        );

        let instruments: HashMap<String, OptionInstrument> = HashMap::from([
            (call.instrument_id.clone(), call),
            (put.instrument_id.clone(), put),
        ]);
        let prices = HashMap::new(); // No prices → falls back to strike

        let margin = calc.calculate_portfolio_margin(&positions, &instruments, &prices);
        assert_eq!(margin.initial_margin, 4000.0); // Only short put contributes
        assert_eq!(margin.maintenance_margin, 3000.0);
    }
}
