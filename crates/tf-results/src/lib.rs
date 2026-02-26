//! tf-results: run cache and timeseries storage.

pub mod hash;
pub mod store;
pub mod types;

pub use hash::compute_run_id;
pub use store::RunStore;
pub use types::*;

pub type ResultsResult<T> = Result<T, ResultsError>;

#[derive(thiserror::Error, Debug)]
pub enum ResultsError {
    #[error("I/O error: {0}")]
    Io(#[from] std::io::Error),

    #[error("JSON error: {0}")]
    Json(#[from] serde_json::Error),

    #[error("Run not found: {run_id}")]
    RunNotFound { run_id: String },

    #[error("Invalid hash: {0}")]
    InvalidHash(String),
}
