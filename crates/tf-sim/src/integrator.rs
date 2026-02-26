//! Fixed-step time integrators.

use crate::error::SimResult;
use crate::model::TransientModel;

/// Trait for time integrators.
pub trait Integrator {
    /// Advance state by one time step using the transient model.
    fn step<M: TransientModel>(
        &self,
        model: &mut M,
        t: f64,
        x: &M::State,
        dt: f64,
    ) -> SimResult<M::State>;
}

/// Classical RK4 (Runge-Kutta 4th order) integrator.
#[derive(Clone, Debug)]
pub struct RK4;

impl Integrator for RK4 {
    fn step<M: TransientModel>(
        &self,
        model: &mut M,
        t: f64,
        x: &M::State,
        dt: f64,
    ) -> SimResult<M::State> {
        let k1 = model.rhs(t, x)?;

        let x2 = model.add(x, &model.scale(&k1, 0.5 * dt));
        let k2 = model.rhs(t + 0.5 * dt, &x2)?;

        let x3 = model.add(x, &model.scale(&k2, 0.5 * dt));
        let k3 = model.rhs(t + 0.5 * dt, &x3)?;

        let x4 = model.add(x, &model.scale(&k3, dt));
        let k4 = model.rhs(t + dt, &x4)?;

        // Combine: x_new = x + (dt/6) * (k1 + 2*k2 + 2*k3 + k4)
        let k_sum = model.add(
            &model.add(&k1, &model.scale(&k2, 2.0)),
            &model.add(&model.scale(&k3, 2.0), &k4),
        );

        Ok(model.add(x, &model.scale(&k_sum, dt / 6.0)))
    }
}

/// Semi-implicit Euler (for reference, not used yet).
#[allow(dead_code)]
#[derive(Clone, Debug)]
pub struct SemiImplicitEuler;

#[allow(dead_code)]
impl Integrator for SemiImplicitEuler {
    fn step<M: TransientModel>(
        &self,
        model: &mut M,
        t: f64,
        x: &M::State,
        dt: f64,
    ) -> SimResult<M::State> {
        // Simple forward Euler: x_new = x + dt * rhs(t, x)
        let xdot = model.rhs(t, x)?;
        Ok(model.add(x, &model.scale(&xdot, dt)))
    }
}

/// Forward Euler (explicit, 1st order, fast for testing).
/// Calls rhs() once per step instead of 4 times (RK4).
#[derive(Clone, Debug)]
pub struct ForwardEuler;

impl Integrator for ForwardEuler {
    fn step<M: TransientModel>(
        &self,
        model: &mut M,
        t: f64,
        x: &M::State,
        dt: f64,
    ) -> SimResult<M::State> {
        let xdot = model.rhs(t, x)?;
        Ok(model.add(x, &model.scale(&xdot, dt)))
    }
}

#[cfg(test)]
mod tests {}
