pub mod domain;
pub mod error;
pub mod order_manager;
pub mod traits;
pub mod validation;

// Re-export key types for convenience
pub use domain::{Order, OrderBuilder, OrderSide, OrderStatus, OrderType, TimeInForce};
pub use error::OrderError;
pub use order_manager::OrderManager;
pub use traits::{OrderStore, RiskApproval, RiskClient};
pub use validation::{validate_order, validate_price_deviation, OrderValidationConfig, OrderValidationError};
