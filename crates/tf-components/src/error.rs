//! Error types for component operations.

use tf_core::error::TfError;
use tf_fluids::FluidError;
use thiserror::Error;

/// Errors that can occur during component calculations.
#[derive(Error, Debug, Clone)]
pub enum ComponentError {
    #[error("Non-physical value: {what}")]
    NonPhysical { what: &'static str },

    #[error("Not supported: {what}")]
    NotSupported { what: &'static str },

    #[error("Backend error: {message}")]
    Backend { message: String },

    #[error("Convergence failed: {what}")]
    ConvergenceFailed { what: &'static str },

    #[error("Invalid argument: {what}")]
    InvalidArg { what: &'static str },
}

pub type ComponentResult<T> = Result<T, ComponentError>;

impl From<FluidError> for ComponentError {
    fn from(e: FluidError) -> Self {
        ComponentError::Backend {
            message: format!("Fluid model error: {}", e),
        }
    }
}

impl From<ComponentError> for TfError {
    fn from(e: ComponentError) -> Self {
        match e {
            ComponentError::NonPhysical { what } => TfError::InvalidArg { what },
            ComponentError::NotSupported { what } => TfError::InvalidArg { what },
            ComponentError::Backend { message: _ } => TfError::InvalidArg {
                what: "backend error",
            },
            ComponentError::ConvergenceFailed { what } => TfError::InvalidArg { what },
            ComponentError::InvalidArg { what } => TfError::InvalidArg { what },
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn error_display() {
        let err = ComponentError::NonPhysical { what: "density" };
        assert!(err.to_string().contains("density"));
    }

    #[test]
    fn error_conversion() {
        let comp_err = ComponentError::InvalidArg { what: "test" };
        let tf_err: TfError = comp_err.into();
        assert!(matches!(tf_err, TfError::InvalidArg { .. }));
    }
}
