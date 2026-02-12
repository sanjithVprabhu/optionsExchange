//! Wallet System Module
//! 
//! This module is the **authoritative collateral ledger** - the ONLY place where money exists.
//! 
//! Key Principles:
//! 1. Wallet is the single source of truth for all funds
//! 2. Only mutates via WalletEvents (deterministic, replayable)
//! 3. Maintains free vs locked balance correctly
//! 4. Integrates with Settlement (payoffs) and Liquidation (margin release)

pub mod domain;
pub mod engine;
pub mod settlement_integration;
pub mod liquidation_integration;
pub mod traits;
pub mod virtual_wallet;

pub use domain::*;
pub use engine::WalletEngine;
pub use settlement_integration::WalletSettlementHandler;
pub use liquidation_integration::WalletLiquidationHandler;
pub use traits::*;
pub use virtual_wallet::VirtualWalletProvider;
