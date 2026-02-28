//! Signal value types and identifiers.

use serde::{Deserialize, Serialize};

/// Unique identifier for a signal in the control graph.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
pub struct SignalId(pub u64);

impl SignalId {
    /// Create a new signal ID.
    pub fn new(id: u64) -> Self {
        Self(id)
    }

    /// Get the raw value.
    pub fn value(&self) -> u64 {
        self.0
    }
}

impl From<u64> for SignalId {
    fn from(id: u64) -> Self {
        Self(id)
    }
}

impl From<SignalId> for u64 {
    fn from(id: SignalId) -> Self {
        id.0
    }
}

/// Signal value type.
///
/// Currently supports only scalar `f64` values. Future extensions may include
/// vector signals, boolean signals, or structured data.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub enum SignalValue {
    /// Scalar floating-point signal.
    Scalar(f64),
}

impl SignalValue {
    /// Create a scalar signal.
    pub fn scalar(value: f64) -> Self {
        Self::Scalar(value)
    }

    /// Get the scalar value, panicking if not scalar.
    pub fn as_scalar(&self) -> f64 {
        match self {
            Self::Scalar(v) => *v,
        }
    }

    /// Get the scalar value as an option.
    pub fn as_scalar_opt(&self) -> Option<f64> {
        match self {
            Self::Scalar(v) => Some(*v),
        }
    }
}

impl From<f64> for SignalValue {
    fn from(value: f64) -> Self {
        Self::Scalar(value)
    }
}

impl Default for SignalValue {
    fn default() -> Self {
        Self::Scalar(0.0)
    }
}

/// Signal represents a value in the control graph at a point in time.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct Signal {
    /// Signal identifier.
    pub id: SignalId,
    /// Signal value.
    pub value: SignalValue,
}

impl Signal {
    /// Create a new signal.
    pub fn new(id: SignalId, value: SignalValue) -> Self {
        Self { id, value }
    }

    /// Create a scalar signal.
    pub fn scalar(id: SignalId, value: f64) -> Self {
        Self {
            id,
            value: SignalValue::Scalar(value),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn signal_id_creation() {
        let id = SignalId::new(42);
        assert_eq!(id.value(), 42);
    }

    #[test]
    fn signal_value_scalar() {
        let val = SignalValue::scalar(2.5);
        assert_eq!(val.as_scalar(), 2.5);
    }

    #[test]
    fn signal_creation() {
        let sig = Signal::scalar(SignalId::new(1), 2.5);
        assert_eq!(sig.id.value(), 1);
        assert_eq!(sig.value.as_scalar(), 2.5);
    }
}
