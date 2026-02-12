pub mod domain;
pub mod engine;

// Re-export key types for convenience
pub use domain::{BookOrder, MatchResult, OrderBook, OrderBookSnapshot, PriceLevel, Trade};
pub use engine::MatchingEngine;
