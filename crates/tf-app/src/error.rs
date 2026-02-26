//! Error types for the tf-app service layer.

use std::path::PathBuf;

/// Application error type that wraps errors from various backend crates
/// and provides a unified error interface for both CLI and GUI.
#[derive(Debug, thiserror::Error)]
pub enum AppError {
    #[error("Project error: {0}")]
    Project(String),

    #[error("Failed to read project file: {path}")]
    ProjectFileRead {
        path: PathBuf,
        source: std::io::Error,
    },

    #[error("Failed to write project file: {path}")]
    ProjectFileWrite {
        path: PathBuf,
        source: std::io::Error,
    },

    #[error("Project validation failed: {0}")]
    Validation(String),

    #[error("System not found: {0}")]
    SystemNotFound(String),

    #[error("Unsupported system pattern: {message}")]
    Unsupported { message: String },

    #[error("Runtime compilation failed: {0}")]
    Compile(String),

    #[error("Transient compilation failed: {message}")]
    TransientCompile { message: String },

    #[error("Solver error: {0}")]
    Solver(String),

    #[error("Simulation error: {0}")]
    Simulation(String),

    #[error("Results error: {0}")]
    Results(String),

    #[error("Run not found: {0}")]
    RunNotFound(String),

    #[error("Invalid input: {0}")]
    InvalidInput(String),

    #[error("Backend error: {message}")]
    Backend { message: String },

    #[error("I/O error: {0}")]
    Io(#[from] std::io::Error),
}

/// Result type for tf-app operations.
pub type AppResult<T> = Result<T, AppError>;

// Conversions from backend error types
impl From<tf_project::ProjectError> for AppError {
    fn from(err: tf_project::ProjectError) -> Self {
        AppError::Project(err.to_string())
    }
}

impl From<tf_solver::SolverError> for AppError {
    fn from(err: tf_solver::SolverError) -> Self {
        AppError::Solver(err.to_string())
    }
}

impl From<tf_sim::SimError> for AppError {
    fn from(err: tf_sim::SimError) -> Self {
        AppError::Simulation(err.to_string())
    }
}

impl From<tf_results::ResultsError> for AppError {
    fn from(err: tf_results::ResultsError) -> Self {
        AppError::Results(err.to_string())
    }
}
