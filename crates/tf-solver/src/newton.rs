//! Newton solver with positivity constraints.

use crate::error::{SolverError, SolverResult};
use nalgebra::DVector;

/// Newton solver configuration.
pub struct NewtonConfig {
    /// Maximum iterations
    pub max_iterations: usize,
    /// Absolute tolerance for residual norm
    pub abs_tol: f64,
    /// Relative tolerance for residual norm
    pub rel_tol: f64,
    /// Minimum allowed pressure (Pa)
    pub min_pressure: f64,
    /// Line search backtracking factor
    pub line_search_beta: f64,
    /// Maximum line search iterations
    pub max_line_search_iters: usize,
}

impl Default for NewtonConfig {
    fn default() -> Self {
        Self {
            max_iterations: 50,
            abs_tol: 1e-6,
            rel_tol: 1e-6,
            min_pressure: 1.0,
            line_search_beta: 0.5,
            max_line_search_iters: 20,
        }
    }
}

/// Newton iteration result.
pub struct NewtonResult {
    /// Solution vector
    pub x: DVector<f64>,
    /// Final residual norm
    pub residual_norm: f64,
    /// Number of iterations
    pub iterations: usize,
    /// Converged flag
    pub converged: bool,
}

/// Newton solver with line search and positivity constraints.
pub fn newton_solve<F, J>(
    x0: DVector<f64>,
    residual_fn: F,
    jacobian_fn: J,
    config: &NewtonConfig,
) -> SolverResult<NewtonResult>
where
    F: Fn(&DVector<f64>) -> SolverResult<DVector<f64>>,
    J: Fn(&DVector<f64>) -> SolverResult<nalgebra::DMatrix<f64>>,
{
    let mut x = x0.clone();
    let mut r = residual_fn(&x)?;
    let mut r_norm = r.norm();
    let r0_norm = r_norm;

    for iter in 0..config.max_iterations {
        // Check convergence
        if r_norm < config.abs_tol || r_norm < config.rel_tol * r0_norm {
            return Ok(NewtonResult {
                x,
                residual_norm: r_norm,
                iterations: iter,
                converged: true,
            });
        }

        // Compute Jacobian
        let jac = jacobian_fn(&x)?;

        // Solve J * dx = -r
        let dx = jac
            .lu()
            .solve(&(-r.clone()))
            .ok_or_else(|| SolverError::Numeric {
                what: "Jacobian solve failed".to_string(),
            })?;

        // Line search with positivity constraints
        let mut alpha = 1.0;
        let mut x_new = &x + alpha * &dx;
        let mut r_new = residual_fn(&x_new)?;
        let mut r_new_norm = r_new.norm();

        for _ in 0..config.max_line_search_iters {
            // Check positivity (every other element is pressure)
            let mut valid = true;
            for i in (0..x_new.len()).step_by(2) {
                if x_new[i] < config.min_pressure {
                    valid = false;
                    break;
                }
            }

            // Check residual reduction
            if valid && r_new_norm < r_norm {
                break;
            }

            // Backtrack
            alpha *= config.line_search_beta;
            x_new = &x + alpha * &dx;
            r_new = residual_fn(&x_new)?;
            r_new_norm = r_new.norm();
        }

        // Update solution
        x = x_new;
        r = r_new;
        r_norm = r_new_norm;

        // Check for stagnation
        if alpha < 1e-10 {
            return Err(SolverError::ConvergenceFailed {
                what: format!("Line search stagnated at iteration {}", iter),
            });
        }
    }

    Err(SolverError::ConvergenceFailed {
        what: format!(
            "Maximum iterations {} reached, residual = {}",
            config.max_iterations, r_norm
        ),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn simple_quadratic() {
        // Solve x^2 - 4 = 0, x > 0
        let residual = |x: &DVector<f64>| -> SolverResult<DVector<f64>> {
            Ok(DVector::from_element(1, x[0] * x[0] - 4.0))
        };
        let jacobian = |x: &DVector<f64>| -> SolverResult<nalgebra::DMatrix<f64>> {
            Ok(nalgebra::DMatrix::from_element(1, 1, 2.0 * x[0]))
        };

        let x0 = DVector::from_element(1, 3.0);
        let config = NewtonConfig::default();
        let result = newton_solve(x0, residual, jacobian, &config).unwrap();

        assert!(result.converged);
        assert!((result.x[0] - 2.0).abs() < 1e-6);
    }
}
