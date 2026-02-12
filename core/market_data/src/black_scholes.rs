use std::f64::consts::PI;

// ============================================================================
// OPTION TYPE
// ============================================================================

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OptionType {
    Call,
    Put,
}

// ============================================================================
// BLACK-SCHOLES INPUTS
// ============================================================================

/// Inputs for Black-Scholes pricing
/// 
/// CRITICAL: All inputs must be validated before use
#[derive(Debug, Clone, Copy)]
pub struct BSInputs {
    /// Spot price (index price of underlying)
    pub spot: f64,
    
    /// Strike price
    pub strike: f64,
    
    /// Time to expiry (in years)
    pub time: f64,
    
    /// Implied volatility (as decimal, e.g., 0.5 = 50%)
    pub vol: f64,
    
    /// Risk-free rate (typically ~0 for crypto)
    pub rate: f64,
    
    /// Option type
    pub option_type: OptionType,
}

impl BSInputs {
    /// Validate and clamp inputs to safe ranges
    pub fn validate(&mut self) {
        // Time must be positive (at least 1 second)
        self.time = self.time.max(1.0 / (365.25 * 24.0 * 3600.0));
        
        // Volatility must be in reasonable range (1% to 500%)
        self.vol = self.vol.clamp(0.01, 5.0);
        
        // Spot and strike must be positive
        self.spot = self.spot.max(1e-6);
        self.strike = self.strike.max(1e-6);
    }
}

// ============================================================================
// GREEKS
// ============================================================================

/// Option Greeks
#[derive(Debug, Clone, Copy)]
pub struct Greeks {
    /// Delta: ∂V/∂S (rate of change with spot)
    pub delta: f64,
    
    /// Gamma: ∂²V/∂S² (curvature of delta)
    pub gamma: f64,
    
    /// Vega: ∂V/∂σ (sensitivity to volatility)
    pub vega: f64,
    
    /// Theta: ∂V/∂t (time decay)
    pub theta: f64,
    
    /// Rho: ∂V/∂r (sensitivity to interest rate)
    pub rho: f64,
}

// ============================================================================
// NORMAL DISTRIBUTION HELPERS
// ============================================================================

/// Normal probability density function
fn norm_pdf(x: f64) -> f64 {
    (1.0 / (2.0 * PI).sqrt()) * (-0.5 * x * x).exp()
}

/// Normal cumulative distribution function
/// 
/// Uses Abramowitz-Stegun approximation (accurate to 7 decimal places)
fn norm_cdf(x: f64) -> f64 {
    let k = 1.0 / (1.0 + 0.2316419 * x.abs());
    let poly = k * (0.319381530
        + k * (-0.356563782
        + k * (1.781477937
        + k * (-1.821255978
        + k * 1.330274429))));
    
    let approx = 1.0 - norm_pdf(x) * poly;
    
    if x >= 0.0 {
        approx
    } else {
        1.0 - approx
    }
}

// ============================================================================
// D1, D2 CALCULATION
// ============================================================================

/// Calculate d1 and d2 for Black-Scholes
/// 
/// d1 = [ln(S/K) + (r + σ²/2)T] / (σ√T)
/// d2 = d1 - σ√T
fn d1_d2(input: &BSInputs) -> (f64, f64) {
    let s = input.spot;
    let k = input.strike;
    let t = input.time.max(1e-6);
    let v = input.vol.max(1e-6);
    let r = input.rate;
    
    let d1 = ((s / k).ln() + (r + 0.5 * v * v) * t) / (v * t.sqrt());
    let d2 = d1 - v * t.sqrt();
    
    (d1, d2)
}

// ============================================================================
// BLACK-SCHOLES PRICING
// ============================================================================

/// Calculate option price using Black-Scholes
/// 
/// Call: C = S·N(d1) - K·e^(-rT)·N(d2)
/// Put:  P = K·e^(-rT)·N(-d2) - S·N(-d1)
pub fn black_scholes_price(mut input: BSInputs) -> f64 {
    input.validate();
    
    let (d1, d2) = d1_d2(&input);
    let s = input.spot;
    let k = input.strike;
    let t = input.time;
    let r = input.rate;
    
    let price = match input.option_type {
        OptionType::Call => {
            s * norm_cdf(d1) - k * (-r * t).exp() * norm_cdf(d2)
        }
        OptionType::Put => {
            k * (-r * t).exp() * norm_cdf(-d2) - s * norm_cdf(-d1)
        }
    };
    
    // Price can never be negative
    price.max(0.0)
}

/// Calculate intrinsic value (for near-expiry)
pub fn intrinsic_value(spot: f64, strike: f64, option_type: OptionType) -> f64 {
    match option_type {
        OptionType::Call => (spot - strike).max(0.0),
        OptionType::Put => (strike - spot).max(0.0),
    }
}

// ============================================================================
// GREEKS CALCULATION
// ============================================================================

/// Calculate Greeks for an option
pub fn black_scholes_greeks(mut input: BSInputs) -> Greeks {
    input.validate();
    
    let (d1, d2) = d1_d2(&input);
    let s = input.spot;
    let k = input.strike;
    let t = input.time;
    let v = input.vol;
    let r = input.rate;
    
    let pdf = norm_pdf(d1);
    let sqrt_t = t.sqrt();
    
    // Delta
    let delta = match input.option_type {
        OptionType::Call => norm_cdf(d1),
        OptionType::Put => norm_cdf(d1) - 1.0,
    };
    
    // Gamma (same for call and put)
    let gamma = pdf / (s * v * sqrt_t);
    
    // Vega (same for call and put)
    let vega = s * pdf * sqrt_t;
    
    // Theta
    let theta = match input.option_type {
        OptionType::Call => {
            -(s * pdf * v) / (2.0 * sqrt_t) - r * k * (-r * t).exp() * norm_cdf(d2)
        }
        OptionType::Put => {
            -(s * pdf * v) / (2.0 * sqrt_t) + r * k * (-r * t).exp() * norm_cdf(-d2)
        }
    };
    
    // Rho
    let rho = match input.option_type {
        OptionType::Call => k * t * (-r * t).exp() * norm_cdf(d2),
        OptionType::Put => -k * t * (-r * t).exp() * norm_cdf(-d2),
    };
    
    Greeks {
        delta,
        gamma,
        vega,
        theta,
        rho,
    }
}

// ============================================================================
// IMPLIED VOLATILITY SOLVER
// ============================================================================

/// Solve for implied volatility using Newton-Raphson
/// 
/// Given market price, find σ such that BS(σ) = market_price
pub fn implied_volatility(
    market_price: f64,
    mut input: BSInputs,
) -> Option<f64> {
    // Initial guess
    let mut vol = 0.3;
    
    // Newton-Raphson iteration
    for _ in 0..100 {
        input.vol = vol;
        
        let price = black_scholes_price(input);
        let vega = black_scholes_greeks(input).vega;
        
        // Check for convergence
        if (price - market_price).abs() < 1e-6 {
            return Some(vol);
        }
        
        // Vega too small = no convergence possible
        if vega.abs() < 1e-8 {
            break;
        }
        
        // Newton step: vol_new = vol_old - f(vol)/f'(vol)
        vol -= (price - market_price) / vega;
        
        // Clamp to reasonable range
        vol = vol.clamp(0.01, 5.0);
    }
    
    None
}

#[cfg(test)]
mod tests {
    use super::*;
    
    #[test]
    fn test_call_price_itm() {
        let input = BSInputs {
            spot: 60000.0,
            strike: 50000.0,
            time: 30.0 / 365.25,  // 30 days
            vol: 0.5,             // 50% vol
            rate: 0.0,
            option_type: OptionType::Call,
        };
        
        let price = black_scholes_price(input);
        
        // ITM call should be at least intrinsic value
        assert!(price >= 10000.0);
    }
    
    #[test]
    fn test_put_price_otm() {
        let input = BSInputs {
            spot: 60000.0,
            strike: 50000.0,
            time: 30.0 / 365.25,
            vol: 0.5,
            rate: 0.0,
            option_type: OptionType::Put,
        };
        
        let price = black_scholes_price(input);
        
        // OTM put should be small but positive
        assert!(price > 0.0 && price < 1000.0);
    }
    
    #[test]
    fn test_call_delta_positive() {
        let input = BSInputs {
            spot: 50000.0,
            strike: 50000.0,
            time: 30.0 / 365.25,
            vol: 0.5,
            rate: 0.0,
            option_type: OptionType::Call,
        };
        
        let greeks = black_scholes_greeks(input);
        
        // ATM call delta should be around 0.5
        assert!(greeks.delta > 0.4 && greeks.delta < 0.6);
    }
    
    #[test]
    fn test_put_call_parity() {
        let spot = 50000.0;
        let strike = 50000.0;
        let time = 30.0 / 365.25;
        let vol = 0.5;
        let rate = 0.0;
        
        let call = black_scholes_price(BSInputs {
            spot, strike, time, vol, rate,
            option_type: OptionType::Call,
        });
        
        let put = black_scholes_price(BSInputs {
            spot, strike, time, vol, rate,
            option_type: OptionType::Put,
        });
        
        // Put-call parity: C - P = S - K·e^(-rT)
        let parity_lhs = call - put;
        let parity_rhs = spot - strike * (-rate * time).exp();
        
        assert!((parity_lhs - parity_rhs).abs() < 1.0);
    }
    
    #[test]
    fn test_implied_vol_roundtrip() {
        let input = BSInputs {
            spot: 50000.0,
            strike: 50000.0,
            time: 30.0 / 365.25,
            vol: 0.5,
            rate: 0.0,
            option_type: OptionType::Call,
        };
        
        let price = black_scholes_price(input);
        let recovered_vol = implied_volatility(price, input).unwrap();
        
        // Should recover original volatility
        assert!((recovered_vol - 0.5).abs() < 0.01);
    }
}
