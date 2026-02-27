//! Newton solver with positivity constraints.

use crate::error::{SolverError, SolverResult};
use nalgebra::DVector;

/// Newton solver configuration.
#[derive(Clone, Copy, Debug)]
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
    /// Maximum absolute enthalpy change per Newton step (J/kg)
    pub enthalpy_delta_abs: f64,
    /// Maximum relative enthalpy change per Newton step (fraction of |h|)
    pub enthalpy_delta_rel: f64,
    /// Maximum absolute enthalpy deviation from prior state (J/kg)
    pub enthalpy_total_abs: f64,
    /// Maximum relative enthalpy deviation from prior state (fraction of |h_prior|)
    pub enthalpy_total_rel: f64,
    /// Reference enthalpy scale for relative limits (J/kg)
    pub enthalpy_ref: f64,
    /// Weak-flow threshold for enthalpy trust-region scaling (kg/s)
    pub weak_flow_mdot: f64,
    /// Scaling applied to enthalpy step limits under weak-flow conditions
    pub weak_flow_enthalpy_scale: f64,
}

impl Default for NewtonConfig {
    fn default() -> Self {
        Self {
            max_iterations: 200, // Increased for robust startup with closed/nearly-closed valves
            abs_tol: 1e-6,
            rel_tol: 1e-6,
            min_pressure: 1.0,
            line_search_beta: 0.5,
            max_line_search_iters: 25, // Slightly increased line search iterations
            enthalpy_delta_abs: f64::INFINITY,
            enthalpy_delta_rel: f64::INFINITY,
            enthalpy_total_abs: f64::INFINITY,
            enthalpy_total_rel: f64::INFINITY,
            enthalpy_ref: 3.0e5,
            weak_flow_mdot: 1.0e-3,
            weak_flow_enthalpy_scale: 0.25,
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
    // Use a no-op validator that always returns true
    let always_valid = |_: &DVector<f64>| true;
    newton_solve_with_validator(
        x0,
        residual_fn,
        jacobian_fn,
        config,
        Some(always_valid),
        None::<fn(&DVector<f64>, &DVector<f64>) -> bool>,
        None::<fn(&DVector<f64>, &DVector<f64>) -> DVector<f64>>,
        None,
    )
}

/// Newton solver with line search, positivity constraints, and optional state validator.
///
/// The state_validator callback can reject trial states that are physically invalid
/// (e.g., P,h combinations outside valid fluid region). If provided and returns false,
/// the line search will backtrack without computing residuals.
#[allow(clippy::too_many_arguments)]
pub fn newton_solve_with_validator<F, J, V, S, L>(
    x0: DVector<f64>,
    residual_fn: F,
    jacobian_fn: J,
    config: &NewtonConfig,
    state_validator: Option<V>,
    step_validator: Option<S>,
    step_limiter: Option<L>,
    mut iteration_observer: Option<&mut dyn FnMut(usize, f64)>,
) -> SolverResult<NewtonResult>
where
    F: Fn(&DVector<f64>) -> SolverResult<DVector<f64>>,
    J: Fn(&DVector<f64>) -> SolverResult<nalgebra::DMatrix<f64>>,
    V: Fn(&DVector<f64>) -> bool,
    S: Fn(&DVector<f64>, &DVector<f64>) -> bool,
    L: Fn(&DVector<f64>, &DVector<f64>) -> DVector<f64>,
{
    let mut x = x0.clone();
    let mut r = residual_fn(&x)?;
    let mut r_norm = r.norm();
    let r0_norm = r_norm;

    for iter in 0..config.max_iterations {
        if let Some(observer) = iteration_observer.as_mut() {
            observer(iter, r_norm);
        }

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

        // Solve J * dx = -r with robust fallback for singular/ill-conditioned matrices
        let dx = {
            // Try standard LU decomposition first (fastest for well-conditioned systems)
            match jac.clone().lu().solve(&(-r.clone())) {
                Some(solution) => solution,
                None => {
                    // LU failed, Jacobian is singular or near-singular
                    // Use SVD-based pseudo-inverse with regularization
                    // This handles underdetermined and rank-deficient systems
                    let svd = jac.svd(true, true);
                    let threshold = 1e-10 * svd.singular_values.max(); // Relative threshold for singular values

                    // Compute regularized pseudo-inverse
                    svd.solve(&(-r.clone()), threshold)
                        .map_err(|_| SolverError::Numeric {
                            what: "Jacobian is severely ill-conditioned; SVD pseudo-inverse failed"
                                .to_string(),
                        })?
                }
            }
        };

        // Line search with positivity constraints and feasibility validation
        let mut alpha = 1.0;
        let mut x_new = &x + alpha * &dx;
        let mut r_new: Option<DVector<f64>> = None;
        let mut r_new_norm = f64::INFINITY;

        for _ls_iter in 0..config.max_line_search_iters {
            if let Some(ref limiter) = step_limiter {
                x_new = limiter(&x, &x_new);
            }

            // Check positivity (every other element is pressure)
            let mut valid = true;
            for i in (0..x_new.len()).step_by(2) {
                if x_new[i] < config.min_pressure {
                    valid = false;
                    break;
                }
            }

            // Check state feasibility if validator provided
            if valid
                && !state_validator
                    .as_ref()
                    .is_none_or(|validator| validator(&x_new))
            {
                valid = false;
            }

            // Check step feasibility (trust-region or other step constraints)
            if valid
                && !step_validator
                    .as_ref()
                    .is_none_or(|validator| validator(&x, &x_new))
            {
                valid = false;
            }

            // Only compute residuals if state passed all validity checks
            if valid {
                match residual_fn(&x_new) {
                    Ok(r) => {
                        r_new_norm = r.norm();
                        // Check residual reduction
                        if r_new_norm < r_norm {
                            r_new = Some(r);
                            break;
                        }
                    }
                    Err(_) => {
                        // Residual computation failed, treat as invalid state
                    }
                }
            }

            // Backtrack
            alpha *= config.line_search_beta;
            x_new = &x + alpha * &dx;
        }

        // Ensure we have a valid step
        let r_new = r_new.ok_or_else(|| SolverError::ConvergenceFailed {
            what: format!(
                "Line search failed to find valid step at iteration {}",
                iter
            ),
        })?;

        // Update solution
        x = x_new;
        r = r_new;
        r_norm = r_new_norm;

        // Check for stagnation
        if alpha < 1e-12 {
            return Err(SolverError::ConvergenceFailed {
                what: format!(
                    "Line search stagnated (alpha < 1e-12) at iteration {}",
                    iter
                ),
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
