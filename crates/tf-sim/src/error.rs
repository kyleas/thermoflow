//! Error types for simulation operations.

use thiserror::Error;

/// Errors encountered during transient simulation.
#[derive(Error, Debug)]
pub enum SimError {
    #[error("Invalid argument: {what}")]
    InvalidArg { what: &'static str },

    #[error("Non-physical condition: {what}")]
    NonPhysical { what: &'static str },

    #[error("Convergence failed: {what}")]
    ConvergenceFailed { what: &'static str },

    #[error("Backend error: {message}")]
    Backend { message: String },
}

pub type SimResult<T> = Result<T, SimError>;

impl From<tf_solver::SolverError> for SimError {
    fn from(e: tf_solver::SolverError) -> Self {
        SimError::Backend {
            message: e.to_string(),
        }
    }
}

impl From<tf_fluids::FluidError> for SimError {
    fn from(e: tf_fluids::FluidError) -> Self {
        SimError::Backend {
            message: e.to_string(),
        }
    }
}

impl From<tf_components::ComponentError> for SimError {
    fn from(e: tf_components::ComponentError) -> Self {
        SimError::Backend {
            message: e.to_string(),
        }
    }
}

impl From<tf_graph::GraphError> for SimError {
    fn from(e: tf_graph::GraphError) -> Self {
        SimError::Backend {
            message: e.to_string(),
        }
    }
}

impl From<tf_core::error::TfError> for SimError {
    fn from(e: tf_core::error::TfError) -> Self {
        SimError::Backend {
            message: e.to_string(),
        }
    }
}
