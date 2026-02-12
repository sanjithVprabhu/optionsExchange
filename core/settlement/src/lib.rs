//! Settlement and Clearing Module
//! 
//! This module handles:
//! 1. Continuous clearing (after every trade) - position updates, margin transitions, PnL tracking
//! 2. Terminal settlement (at expiry) - payoff calculations, wallet updates, position closing

pub mod domain;
pub mod clearing_engine;
pub mod settlement_engine;
pub mod coordinator;
pub mod traits;
pub mod expiry_monitor;

pub use domain::*;
pub use clearing_engine::ClearingEngine;
pub use settlement_engine::SettlementEngine;
pub use coordinator::SettlementCoordinator;
pub use traits::*;
pub use expiry_monitor::ExpiryMonitor;
