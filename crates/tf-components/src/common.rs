//! Common utilities for component calculations.

use crate::error::{ComponentError, ComponentResult};
use tf_core::numeric::ensure_finite;

/// Small epsilon for pressure differences (Pa)
pub const EPSILON_PRESSURE: f64 = 1e-3;

/// Small epsilon for mass flow rate (kg/s)
pub const EPSILON_MDOT: f64 = 1e-9;

/// Ensure a value is finite, returning ComponentError if not.
pub fn check_finite(value: f64, what: &'static str) -> ComponentResult<()> {
    ensure_finite(value, what).map_err(|_| ComponentError::NonPhysical { what })?;
    Ok(())
}

/// Determine flow direction: 1.0 for forward (inlet > outlet), -1.0 for reverse.
///
/// Returns 0.0 if pressure difference is negligible.
pub fn flow_direction(p_inlet: f64, p_outlet: f64) -> f64 {
    let dp = p_inlet - p_outlet;
    if dp.abs() < EPSILON_PRESSURE {
        0.0
    } else if dp > 0.0 {
        1.0
    } else {
        -1.0
    }
}

/// Clamp a value between min and max.
pub fn clamp(value: f64, min: f64, max: f64) -> f64 {
    if value < min {
        min
    } else if value > max {
        max
    } else {
        value
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_flow_direction() {
        assert_eq!(flow_direction(100.0, 50.0), 1.0);
        assert_eq!(flow_direction(50.0, 100.0), -1.0);
        assert_eq!(flow_direction(100.0, 100.0), 0.0);
    }

    #[test]
    fn test_clamp() {
        assert_eq!(clamp(5.0, 0.0, 10.0), 5.0);
        assert_eq!(clamp(-1.0, 0.0, 10.0), 0.0);
        assert_eq!(clamp(11.0, 0.0, 10.0), 10.0);
    }

    #[test]
    fn test_check_finite() {
        assert!(check_finite(1.0, "test").is_ok());
        assert!(check_finite(f64::INFINITY, "test").is_err());
        assert!(check_finite(f64::NAN, "test").is_err());
    }
}
