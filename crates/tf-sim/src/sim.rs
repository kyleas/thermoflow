//! Simulation runner and result recording.

use crate::error::SimResult;
use crate::integrator::{ForwardEuler, Integrator, RK4};
use crate::model::TransientModel;

/// Integrator selection for simulation.
#[derive(Clone, Copy, Debug, Default)]
pub enum IntegratorType {
    /// 4th-order Runge-Kutta (default, most accurate, 4 rhs calls per step).
    #[default]
    RK4,
    /// Forward Euler (1st-order, faster, 1 rhs call per step).
    ForwardEuler,
}

/// Options for simulation runs.
#[derive(Clone, Debug)]
pub struct SimOptions {
    /// Fixed time step (seconds)
    pub dt: f64,
    /// Final simulation time (seconds)
    pub t_end: f64,
    /// Maximum number of steps (safety limit)
    pub max_steps: usize,
    /// Record every N-th step (decimation)
    pub record_every: usize,
    /// Integrator type (default: RK4)
    pub integrator: IntegratorType,
}

impl Default for SimOptions {
    fn default() -> Self {
        Self {
            dt: 1e-3,
            t_end: 1.0,
            max_steps: 100_000,
            record_every: 10,
            integrator: IntegratorType::default(),
        }
    }
}

/// Record of simulation results.
#[derive(Clone, Debug)]
pub struct SimRecord<S> {
    /// Time points (seconds)
    pub t: Vec<f64>,
    /// State snapshots
    pub x: Vec<S>,
}

/// Run a transient simulation using fixed-step RK4.
pub fn run_sim<M: TransientModel>(
    model: &mut M,
    opts: &SimOptions,
) -> SimResult<SimRecord<M::State>> {
    if opts.dt <= 0.0 {
        return Err(crate::error::SimError::InvalidArg {
            what: "dt must be positive",
        });
    }
    if opts.t_end < 0.0 {
        return Err(crate::error::SimError::InvalidArg {
            what: "t_end must be non-negative",
        });
    }
    if opts.max_steps == 0 {
        return Err(crate::error::SimError::InvalidArg {
            what: "max_steps must be positive",
        });
    }

    let mut t = 0.0;
    let mut x = model.initial_state();

    let mut t_record = vec![t];
    let mut x_record = vec![x.clone()];

    let mut step = 0;
    while t < opts.t_end && step < opts.max_steps {
        // Integrate one step using selected integrator
        x = match opts.integrator {
            IntegratorType::RK4 => {
                let integrator = RK4;
                integrator.step(model, t, &x, opts.dt)?
            }
            IntegratorType::ForwardEuler => {
                let integrator = ForwardEuler;
                integrator.step(model, t, &x, opts.dt)?
            }
        };
        t += opts.dt;
        step += 1;

        // Record if decimation matches
        if step % opts.record_every == 0 {
            t_record.push(t);
            x_record.push(x.clone());
        }
    }

    // Always record final state
    if step % opts.record_every != 0 {
        t_record.push(t);
        x_record.push(x);
    }

    Ok(SimRecord {
        t: t_record,
        x: x_record,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sim_options_defaults() {
        let _opts = SimOptions::default();
        assert_eq!(_opts.dt, 1e-3);
        assert_eq!(_opts.t_end, 1.0);
        assert_eq!(_opts.max_steps, 100_000);
        assert_eq!(_opts.record_every, 10);
    }

    #[test]
    fn sim_options_invalid() {
        // dt <= 0 should fail
        let _opts = SimOptions {
            dt: 0.0,
            t_end: 1.0,
            max_steps: 100,
            record_every: 1,
            integrator: IntegratorType::RK4,
        };
        // This would fail in run_sim; test it in integration test
    }
}
