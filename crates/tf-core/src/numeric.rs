use crate::TfError;

/// Floating point type used throughout system
pub type Real = f64;

/// One tolerance for everything
#[derive(Clone, Copy, Debug)]
pub struct Tolerances {
    pub abs: Real,
    pub rel: Real,
}

impl Default for Tolerances {
    fn default() -> Self {
        Self {
            abs: 1e-12,
            rel: 1e-9,
        }
    }
}

pub fn nearly_equal(a: Real, b: Real, tol: Tolerances) -> bool {
    let diff = (a - b).abs();
    if diff <= tol.abs {
        return true;
    }
    diff <= tol.rel * a.abs().max(b.abs())
}

pub fn ensure_finite(v: Real, what: &'static str) -> Result<Real, TfError> {
    if v.is_finite() {
        Ok(v)
    } else {
        Err(TfError::NonFinite { what, value: v })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn nearly_equal_basic() {
        let tol = Tolerances {
            abs: 1e-12,
            rel: 1e-9,
        };
        assert!(nearly_equal(1.0, 1.0 + 1e-12, tol));
        assert!(nearly_equal(0.0, 1e-13, tol));
        assert!(!nearly_equal(1.0, 1.0 + 1e-6, tol));
    }

    #[test]
    fn ensure_finite_detects_nan() {
        let err = ensure_finite(Real::NAN, "test").unwrap_err();
        let msg = format!("{err}");
        assert!(msg.contains("Non-finite"));
    }
}
