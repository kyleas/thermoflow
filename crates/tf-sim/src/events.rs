//! Minimal event handling for transient simulation.

use crate::error::SimResult;

/// Clamp value to [0, 1].
#[allow(dead_code)]
pub(crate) fn clamp_position(pos: f64) -> f64 {
    pos.clamp(0.0, 1.0)
}

/// Clamp positive quantity, check if finite.
#[allow(dead_code)]
pub(crate) fn validate_positive(val: f64, name: &'static str) -> SimResult<f64> {
    if !val.is_finite() {
        return Err(crate::error::SimError::NonPhysical { what: name });
    }
    if val < 0.0 {
        return Err(crate::error::SimError::NonPhysical { what: name });
    }
    Ok(val)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn clamp_position_range() {
        assert_eq!(clamp_position(-0.5), 0.0);
        assert_eq!(clamp_position(0.5), 0.5);
        assert_eq!(clamp_position(1.5), 1.0);
    }

    #[test]
    fn validate_positive_ok() {
        assert!(validate_positive(1.0, "test").is_ok());
        assert!(validate_positive(0.0, "test").is_ok()); // zero is allowed
    }

    #[test]
    fn validate_positive_fails_on_negative() {
        assert!(validate_positive(-0.1, "test").is_err());
    }

    #[test]
    fn validate_positive_fails_on_nan() {
        assert!(validate_positive(f64::NAN, "test").is_err());
    }
}
