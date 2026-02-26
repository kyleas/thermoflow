//! TransientModel trait for pluggable dynamic systems.

use crate::error::SimResult;

/// Trait for transient (dynamic) system models.
///
/// A TransientModel must implement:
/// - State type (Clone, for snapshots)
/// - Initial state
/// - RHS (right-hand side) computation: x_dot = f(t, x)
/// - Scalar field arithmetic for integration: add states, scale by scalar
pub trait TransientModel {
    /// State type (must be Clone).
    type State: Clone;

    /// Return the initial state at t=0.
    fn initial_state(&self) -> Self::State;

    /// Compute state derivative dxdt = f(t, x).
    ///
    /// This function should:
    /// 1) Extract dynamic states from x
    /// 2) Compute boundary conditions and solve algebraic equations (e.g., network solver)
    /// 3) Apply conservation laws (mass, energy, actuator dynamics)
    /// 4) Return time derivatives
    ///
    /// Note: Takes &mut self to allow models to cache previous solutions for performance.
    fn rhs(&mut self, t: f64, x: &Self::State) -> SimResult<Self::State>;

    /// Add two states element-wise: result = a + b.
    fn add(&self, a: &Self::State, b: &Self::State) -> Self::State;

    /// Scale a state by a scalar: result = scale * a.
    fn scale(&self, a: &Self::State, scale: f64) -> Self::State;
}
