pub mod domain;
pub mod engine;
pub mod liquidation;
pub mod margin_calculator;

// Re-export key types for convenience
pub use domain::{
    LiquidationState, MarginConfig, MarginRequirement, Position, PositionSide, RiskCheckResult,
    UserRiskState,
};
pub use engine::RiskEngine;
pub use liquidation::{LiquidationDetector, LiquidationEvent};
pub use margin_calculator::MarginCalculator;
