//! Fluid property errors.

use tf_core::TfError;
use thiserror::Error;

/// Result type for fluid operations.
pub type FluidResult<T> = Result<T, FluidError>;

/// Errors that can occur during fluid property calculations.
#[derive(Error, Debug, Clone, PartialEq)]
pub enum FluidError {
    /// Non-physical values (negative density, pressure, etc.).
    #[error("Non-physical value for {what}")]
    NonPhysical { what: &'static str },

    /// Value out of valid range.
    #[error("Value out of range for {what}")]
    OutOfRange { what: &'static str },

    /// Invalid argument.
    #[error("Invalid argument: {what}")]
    InvalidArg { what: &'static str },

    /// Operation not supported (e.g., mixtures, unsupported species).
    #[error("Not supported: {what}")]
    NotSupported { what: &'static str },

    /// Backend (CoolProp) error.
    #[error("Backend error: {message}")]
    Backend { message: String },

    /// Convergence failure (e.g., solving for T given P,h).
    #[error("Convergence failed for {what}")]
    ConvergenceFailed { what: &'static str },
}

impl From<FluidError> for TfError {
    fn from(err: FluidError) -> Self {
        // Convert to TfError while preserving context
        match err {
            FluidError::NonPhysical { what } => TfError::Invariant {
                what: Box::leak(format!("Non-physical fluid value: {}", what).into_boxed_str()),
            },
            FluidError::OutOfRange { what } => TfError::InvalidArg {
                what: Box::leak(format!("Fluid value out of range: {}", what).into_boxed_str()),
            },
            FluidError::InvalidArg { what } => TfError::InvalidArg {
                what: Box::leak(format!("Invalid fluid argument: {}", what).into_boxed_str()),
            },
            FluidError::NotSupported { what } => TfError::Invariant {
                what: Box::leak(
                    format!("Fluid operation not supported: {}", what).into_boxed_str(),
                ),
            },
            FluidError::Backend { message } => TfError::Invariant {
                what: Box::leak(format!("Fluid backend error: {}", message).into_boxed_str()),
            },
            FluidError::ConvergenceFailed { what } => TfError::Invariant {
                what: Box::leak(format!("Fluid convergence failed: {}", what).into_boxed_str()),
            },
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn error_display() {
        let err = FluidError::NonPhysical { what: "pressure" };
        assert!(err.to_string().contains("pressure"));

        let err = FluidError::Backend {
            message: "CoolProp failed".into(),
        };
        assert!(err.to_string().contains("CoolProp"));
    }

    #[test]
    fn error_to_tf_error() {
        let fluid_err = FluidError::NotSupported { what: "mixtures" };
        let tf_err: TfError = fluid_err.into();
        assert!(matches!(tf_err, TfError::Invariant { .. }));
    }
}
