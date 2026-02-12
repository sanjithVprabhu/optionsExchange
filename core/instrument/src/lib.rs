pub mod domain;
pub mod error;
pub mod registry;
pub mod traits;
pub mod validation;

// Re-export key types for convenience
pub use domain::{Market, OptionInstrument, OptionInstrumentBuilder};
pub use error::InstrumentError;
pub use registry::InstrumentRegistry;
pub use traits::{InstrumentStore, MarketStore};
pub use validation::validate_instrument;
