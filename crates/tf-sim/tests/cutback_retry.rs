//! Cutback retry test for transient simulator.

use tf_sim::{IntegratorType, SimError, SimOptions, TransientModel, run_sim};

struct FailOnceModel {
    failures_left: usize,
}

impl TransientModel for FailOnceModel {
    type State = f64;

    fn initial_state(&self) -> Self::State {
        0.0
    }

    fn rhs(&mut self, _t: f64, _x: &Self::State) -> tf_sim::SimResult<Self::State> {
        if self.failures_left > 0 {
            self.failures_left -= 1;
            return Err(SimError::Retryable {
                message: "intentional retryable failure".to_string(),
            });
        }
        Ok(0.0)
    }

    fn add(&self, a: &Self::State, b: &Self::State) -> Self::State {
        a + b
    }

    fn scale(&self, a: &Self::State, scale: f64) -> Self::State {
        a * scale
    }
}

#[test]
fn transient_cutback_retries_step() {
    let mut model = FailOnceModel { failures_left: 1 };

    let opts = SimOptions {
        dt: 0.1,
        t_end: 0.2,
        max_steps: 10,
        record_every: 1,
        integrator: IntegratorType::RK4,
        min_dt: 0.01,
        max_retries: 4,
        cutback_factor: 0.5,
        grow_factor: 2.0,
    };

    let record = run_sim(&mut model, &opts).expect("cutback retry should succeed");

    assert!(record.t.len() >= 2, "Expected at least one step recorded");
    assert!(record.t[1] < opts.dt, "First step should be cut back");
    assert_eq!(
        model.failures_left, 0,
        "Failure should have been consumed by retry"
    );
}
