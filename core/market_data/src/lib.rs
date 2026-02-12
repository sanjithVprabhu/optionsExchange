//! # Market Data & Pricing Engine
//!
//! This module provides deterministic projection of market data from events.
//!
//! ## Critical Principles:
//!
//! 1. **Pure Projection**: Market Data NEVER writes state, only reads and projects
//! 2. **Mark Price ≠ Last Trade**: Mark price uses Black-Scholes (manipulation-resistant)
//! 3. **Volatility Surface Required**: Every option needs IV from the surface
//! 4. **Greeks Drive Margin**: Delta, Gamma, Vega used by Risk Engine
//! 5. **Index Price is External**: Multiple sources, median aggregation, outlier rejection
//!
//! ## Modules:
//!
//! - `black_scholes`: Black-Scholes pricing, Greeks, implied volatility
//! - `vol_surface`: Volatility surface construction and arbitrage detection
//! - `mark_price`: Mark price engine with EMA smoothing
//! - `order_book`: Order book projections (read-only)
//! - `coordinator`: Orchestrates all market data components

pub mod black_scholes;
pub mod vol_surface;
pub mod mark_price;
pub mod order_book;
pub mod coordinator;

// Re-export commonly used types
pub use black_scholes::{BSInputs, Greeks, OptionType, black_scholes_greeks, black_scholes_price, implied_volatility, intrinsic_value};
pub use coordinator::MarketDataCoordinator;
pub use mark_price::MarkPriceEngine;
pub use order_book::{OrderBookBuilder, OrderBookSnapshot, PriceLevel};
pub use vol_surface::VolSurface;
