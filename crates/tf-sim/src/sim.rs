//! Simulation runner and result recording.

use crate::error::SimResult;
use crate::integrator::{ForwardEuler, Integrator, RK4};
use crate::model::TransientModel;
use std::time::Instant;

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
    /// Minimum time step allowed for cutback (seconds)
    pub min_dt: f64,
    /// Maximum retry attempts per step when cutback is enabled
    pub max_retries: usize,
    /// Cutback factor applied to dt on retry (0 < factor < 1)
    pub cutback_factor: f64,
    /// Growth factor applied after a successful cutback step (>= 1)
    pub grow_factor: f64,
}

impl Default for SimOptions {
    fn default() -> Self {
        Self {
            dt: 1e-3,
            t_end: 1.0,
            max_steps: 100_000,
            record_every: 10,
            integrator: IntegratorType::default(),
            min_dt: 1e-6,
            max_retries: 6,
            cutback_factor: 0.5,
            grow_factor: 2.0,
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
    /// Number of successful integration steps
    pub steps: usize,
    /// Number of retry/cutback attempts across all steps
    pub cutback_retries: usize,
    /// Total wall-clock solve time for the simulation loop
    pub wall_time_s: f64,
}

#[derive(Clone, Debug)]
pub struct SimProgress {
    pub step: usize,
    pub sim_time: f64,
    pub t_end: f64,
    pub fraction_complete: f64,
    pub cutback_retries: usize,
    pub dt_last: f64,
    pub elapsed_wall_s: f64,
}

/// Run a transient simulation using fixed-step RK4.
pub fn run_sim<M: TransientModel>(
    model: &mut M,
    opts: &SimOptions,
) -> SimResult<SimRecord<M::State>> {
    run_sim_with_progress(model, opts, None)
}

/// Run a transient simulation and emit progress snapshots.
pub fn run_sim_with_progress<M: TransientModel>(
    model: &mut M,
    opts: &SimOptions,
    mut progress_cb: Option<&mut dyn FnMut(SimProgress)>,
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
    if opts.min_dt <= 0.0 {
        return Err(crate::error::SimError::InvalidArg {
            what: "min_dt must be positive",
        });
    }
    if opts.min_dt > opts.dt {
        return Err(crate::error::SimError::InvalidArg {
            what: "min_dt must be <= dt",
        });
    }
    if !(0.0 < opts.cutback_factor && opts.cutback_factor < 1.0) {
        return Err(crate::error::SimError::InvalidArg {
            what: "cutback_factor must be between 0 and 1",
        });
    }
    if opts.grow_factor < 1.0 {
        return Err(crate::error::SimError::InvalidArg {
            what: "grow_factor must be >= 1",
        });
    }

    let mut t = 0.0;
    let mut x = model.initial_state();
    let mut dt_current = opts.dt;
    let mut cutback_retries = 0usize;
    let started = Instant::now();

    let mut t_record = vec![t];
    let mut x_record = vec![x.clone()];

    let mut step = 0;
    while t < opts.t_end && step < opts.max_steps {
        let mut attempt = 0usize;
        let mut dt_step = dt_current.min(opts.t_end - t);

        loop {
            attempt += 1;

            let step_result = match opts.integrator {
                IntegratorType::RK4 => {
                    let integrator = RK4;
                    integrator.step(model, t, &x, dt_step)
                }
                IntegratorType::ForwardEuler => {
                    let integrator = ForwardEuler;
                    integrator.step(model, t, &x, dt_step)
                }
            };

            match step_result {
                Ok(x_next) => {
                    x = x_next;
                    break;
                }
                Err(err) => {
                    if is_retryable(&err) && attempt <= opts.max_retries && dt_step > opts.min_dt {
                        let new_dt = (dt_step * opts.cutback_factor).max(opts.min_dt);
                        cutback_retries += 1;
                        eprintln!(
                            "[CUTBACK] t={:.6} failed: {}. retry {}/{} with dt {:.6} -> {:.6}",
                            t, err, attempt, opts.max_retries, dt_step, new_dt
                        );
                        dt_step = new_dt;
                        continue;
                    }
                    return Err(err);
                }
            }
        }

        t += dt_step;
        step += 1;

        if dt_step < dt_current {
            dt_current = (dt_step * opts.grow_factor).min(opts.dt);
        } else {
            dt_current = opts.dt;
        }

        // Record if decimation matches
        if step % opts.record_every == 0 {
            t_record.push(t);
            x_record.push(x.clone());
        }

        if let Some(cb) = progress_cb.as_mut() {
            let fraction_complete = if opts.t_end > 0.0 {
                (t / opts.t_end).clamp(0.0, 1.0)
            } else {
                1.0
            };
            cb(SimProgress {
                step,
                sim_time: t,
                t_end: opts.t_end,
                fraction_complete,
                cutback_retries,
                dt_last: dt_step,
                elapsed_wall_s: started.elapsed().as_secs_f64(),
            });
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
        steps: step,
        cutback_retries,
        wall_time_s: started.elapsed().as_secs_f64(),
    })
}

fn is_retryable(err: &crate::error::SimError) -> bool {
    matches!(err, crate::error::SimError::Retryable { .. })
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
        assert_eq!(_opts.min_dt, 1e-6);
        assert_eq!(_opts.max_retries, 6);
        assert_eq!(_opts.cutback_factor, 0.5);
        assert_eq!(_opts.grow_factor, 2.0);
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
            min_dt: 1e-6,
            max_retries: 2,
            cutback_factor: 0.5,
            grow_factor: 2.0,
        };
        // This would fail in run_sim; test it in integration test
    }
}
