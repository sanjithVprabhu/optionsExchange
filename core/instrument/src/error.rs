use exchange_primitives::InstrumentStatus;
use thiserror::Error;

/// Errors that can occur in the instrument layer.
#[derive(Debug, Error)]
pub enum InstrumentError {
    #[error("Instrument not found: {0}")]
    NotFound(String),

    #[error("Instrument already exists: {0}")]
    AlreadyExists(String),

    #[error("Invalid instrument: {0}")]
    InvalidInstrument(String),

    #[error("Invalid status transition from {from} to {to}")]
    InvalidStatusTransition {
        from: InstrumentStatus,
        to: InstrumentStatus,
    },

    #[error("Market not found: {0}")]
    MarketNotFound(String),

    #[error("Storage error: {0}")]
    StorageError(String),
}
