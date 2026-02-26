//! Error types for solver operations.

use tf_components::ComponentError;
use tf_core::error::TfError;
use tf_fluids::FluidError;
use thiserror::Error;

/// Errors that can occur during network solving.
#[derive(Error, Debug)]
pub enum SolverError {
    #[error("Problem setup error: {what}")]
    ProblemSetup { what: String },

    #[error("Convergence failed: {what}")]
    ConvergenceFailed { what: String },

    #[error("Invalid state: {what}")]
    InvalidState { what: String },

    #[error("Component error: {0}")]
    Component(#[from] ComponentError),

    #[error("Fluid error: {0}")]
    Fluid(#[from] FluidError),

    #[error("Graph error: {0}")]
    Graph(#[from] tf_graph::GraphError),

    #[error("Numeric error: {what}")]
    Numeric { what: String },
}

pub type SolverResult<T> = Result<T, SolverError>;

impl From<SolverError> for TfError {
    fn from(e: SolverError) -> Self {
        match e {
            SolverError::ProblemSetup { what: _ } => TfError::InvalidArg {
                what: "problem setup",
            },
            SolverError::ConvergenceFailed { what: _ } => TfError::InvalidArg {
                what: "convergence",
            },
            SolverError::InvalidState { what: _ } => TfError::InvalidArg { what: "state" },
            SolverError::Component(_) => TfError::InvalidArg { what: "component" },
            SolverError::Fluid(_) => TfError::InvalidArg { what: "fluid" },
            SolverError::Graph(_) => TfError::InvalidArg { what: "graph" },
            SolverError::Numeric { what: _ } => TfError::InvalidArg { what: "numeric" },
        }
    }
}
