//! tf-core: stable foundation for thermoflow.
//!
//! Contains:
//! - units (uom SI types + constructors)
//! - numeric (Real + tolerances + float helpers)
//! - ids (stable compact IDs for graph/model objects)
//! - error (shared error types)

pub mod error;
pub mod ids;
pub mod numeric;
pub mod units;

// Re-exports: nice ergonomics for downstream crates
pub use error::{TfError, TfResult};
pub use ids::*;
pub use numeric::*;
pub use units::*;
