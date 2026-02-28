//! Error types for control system operations.

use thiserror::Error;

/// Result type for control system operations.
pub type ControlResult<T> = Result<T, ControlError>;

/// Errors that can occur in control system operations.
#[derive(Debug, Error, Clone, PartialEq)]
pub enum ControlError {
    /// Invalid argument provided to a control function.
    #[error("Invalid argument: {what}")]
    InvalidArg { what: &'static str },

    /// Invalid signal graph connection.
    #[error("Invalid signal connection: {what}")]
    InvalidConnection { what: String },

    /// Signal reference not found or invalid.
    #[error("Invalid signal reference: {what}")]
    InvalidReference { what: String },

    /// Controller state error.
    #[error("Controller state error: {what}")]
    StateError { what: String },

    /// Signal graph topology error.
    #[error("Graph topology error: {what}")]
    TopologyError { what: String },
}
