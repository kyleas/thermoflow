//! Finite difference Jacobian computation.

use crate::error::SolverResult;
use nalgebra::{DMatrix, DVector};

/// Compute Jacobian using forward finite differences.
///
/// For each column j, perturbs x[j] by epsilon and computes (f(x+e) - f(x))/epsilon.
pub fn finite_difference_jacobian<F>(
    x: &DVector<f64>,
    f: F,
    epsilon: f64,
) -> SolverResult<DMatrix<f64>>
where
    F: Fn(&DVector<f64>) -> SolverResult<DVector<f64>>,
{
    let n = x.len();
    let f_x = f(x)?;
    let m = f_x.len();

    let mut jac = DMatrix::zeros(m, n);

    for j in 0..n {
        let mut x_perturbed = x.clone();
        let dx = epsilon * x[j].abs().max(1.0);
        x_perturbed[j] += dx;

        let f_perturbed = f(&x_perturbed)?;
        let df = (f_perturbed - &f_x) / dx;

        for i in 0..m {
            jac[(i, j)] = df[i];
        }
    }

    Ok(jac)
}

/// Compute Jacobian using central finite differences (more accurate but 2x cost).
pub fn central_difference_jacobian<F>(
    x: &DVector<f64>,
    f: F,
    epsilon: f64,
) -> SolverResult<DMatrix<f64>>
where
    F: Fn(&DVector<f64>) -> SolverResult<DVector<f64>>,
{
    let n = x.len();
    let f_x = f(x)?;
    let m = f_x.len();

    let mut jac = DMatrix::zeros(m, n);

    for j in 0..n {
        let dx = epsilon * x[j].abs().max(1.0);

        let mut x_plus = x.clone();
        x_plus[j] += dx;
        let f_plus = f(&x_plus)?;

        let mut x_minus = x.clone();
        x_minus[j] -= dx;
        let f_minus = f(&x_minus)?;

        let df = (f_plus - f_minus) / (2.0 * dx);

        for i in 0..m {
            jac[(i, j)] = df[i];
        }
    }

    Ok(jac)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn jacobian_linear() {
        // f(x) = 2*x, J = 2
        let f = |x: &DVector<f64>| -> SolverResult<DVector<f64>> {
            Ok(DVector::from_element(1, 2.0 * x[0]))
        };

        let x = DVector::from_element(1, 3.0);
        let jac = finite_difference_jacobian(&x, f, 1e-7).unwrap();

        assert!((jac[(0, 0)] - 2.0).abs() < 1e-5);
    }

    #[test]
    fn jacobian_quadratic() {
        // f(x) = x^2, J = 2*x
        let f = |x: &DVector<f64>| -> SolverResult<DVector<f64>> {
            Ok(DVector::from_element(1, x[0] * x[0]))
        };

        let x = DVector::from_element(1, 3.0);
        let jac = finite_difference_jacobian(&x, f, 1e-7).unwrap();

        assert!((jac[(0, 0)] - 6.0).abs() < 1e-5);
    }
}
