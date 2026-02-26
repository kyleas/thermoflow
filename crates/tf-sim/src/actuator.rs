//! Valve actuator with first-order dynamics and rate limiting.

use crate::error::SimResult;

/// State of a first-order actuator (e.g., valve position).
#[derive(Clone, Debug)]
pub struct ActuatorState {
    /// Current position [0, 1]
    pub position: f64,
}

/// First-order actuator with rate limiting.
///
/// Dynamics: dpos/dt = (1/tau) * (cmd - pos), clamped to [-rate_limit, rate_limit].
#[derive(Clone, Debug)]
pub struct FirstOrderActuator {
    /// Time constant (seconds)
    pub tau: f64,
    /// Rate limit (1/second), must be positive
    pub rate_limit: f64,
}

impl FirstOrderActuator {
    /// Create a new first-order actuator.
    pub fn new(tau: f64, rate_limit: f64) -> SimResult<Self> {
        if tau <= 0.0 {
            return Err(crate::error::SimError::InvalidArg {
                what: "tau must be positive",
            });
        }
        if rate_limit <= 0.0 {
            return Err(crate::error::SimError::InvalidArg {
                what: "rate_limit must be positive",
            });
        }
        Ok(Self { tau, rate_limit })
    }

    /// Compute position derivative given current position and command.
    pub fn dpdt(&self, position: f64, command: f64) -> f64 {
        let raw = (command - position) / self.tau;
        raw.clamp(-self.rate_limit, self.rate_limit)
    }

    /// Advance state by dt given command.
    pub fn step(&self, state: &ActuatorState, dt: f64, command: f64) -> ActuatorState {
        let dpdt = self.dpdt(state.position, command);
        let new_pos = state.position + dpdt * dt;
        ActuatorState {
            position: new_pos.clamp(0.0, 1.0),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn first_order_step_response() {
        let act = FirstOrderActuator::new(0.1, 10.0).unwrap();
        let mut state = ActuatorState { position: 0.0 };

        // Step to 1.0 with tau=0.1, dt=0.01
        // Expected: large dpdt, clamped to rate_limit
        let cmd = 1.0;
        let dt = 0.01;
        state = act.step(&state, dt, cmd);

        // After one step with unlimited rate, should move toward 1.0
        assert!(state.position > 0.0);
        assert!(state.position <= 1.0);
    }

    #[test]
    fn rate_limiting() {
        let act = FirstOrderActuator::new(1.0, 0.5).unwrap(); // 1s tau, 0.5/s limit
        let _state = ActuatorState { position: 0.0 };

        let dpdt = act.dpdt(0.0, 1.0);
        // raw would be 1.0, clamped to 0.5
        assert!((dpdt - 0.5).abs() < 1e-10);
    }

    #[test]
    fn position_clamped() {
        let act = FirstOrderActuator::new(0.01, 100.0).unwrap(); // aggressive
        let mut state = ActuatorState { position: 0.5 };

        // Try to go to 2.0 (should clamp to [0,1])
        state = act.step(&state, 0.1, 2.0);
        assert!(state.position <= 1.0);

        // Try to go to -1.0
        state = ActuatorState { position: 0.5 };
        state = act.step(&state, 0.1, -1.0);
        assert!(state.position >= 0.0);
    }
}
