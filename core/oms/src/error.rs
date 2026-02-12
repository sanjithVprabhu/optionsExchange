use thiserror::Error;
use uuid::Uuid;

use crate::domain::OrderStatus;
use crate::validation::OrderValidationError;

/// Errors that can occur in the Order Management System
#[derive(Debug, Error)]
pub enum OrderError {
    #[error("Order not found: {0}")]
    NotFound(Uuid),

    #[error("Storage error: {0}")]
    Storage(String),

    #[error("Validation error: {0}")]
    Validation(#[from] OrderValidationError),

    #[error("Risk engine error: {0}")]
    RiskEngine(String),

    #[error("Invalid state transition: {from} -> {to}")]
    InvalidTransition { from: OrderStatus, to: OrderStatus },

    #[error("Not authorized: {0}")]
    NotAuthorized(String),
}

pub type Result<T> = std::result::Result<T, OrderError>;
