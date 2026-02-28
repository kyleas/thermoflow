//! Actuator dynamics for control systems.
//!
//! Actuators introduce physical dynamics between controller output and system response.
//! The primary actuator model is first-order lag with rate limiting, which models:
//! - Mechanical time constants (e.g., valve motor speed)
//! - Rate limits (maximum actuation speed)
//! - Position limits (e.g., [0, 1] for valve position)

use crate::error::{ControlError, ControlResult};
use serde::{Deserialize, Serialize};

/// State of a first-order actuator (e.g., valve position).
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ActuatorState {
    /// Current position [0, 1]
    pub position: f64,
}

impl Default for ActuatorState {
    fn default() -> Self {
        Self { position: 0.0 }
    }
}

/// First-order actuator with rate limiting.
///
/// Dynamics: `dpos/dt = (1/tau) * (cmd - pos)`, clamped to `[-rate_limit, rate_limit]`.
///
/// This model captures:
/// - **First-order lag**: Time constant `tau` models mechanical/electrical response time
/// - **Rate limiting**: Maximum velocity `rate_limit` models physical speed constraints
/// - **Position clamping**: Output is clamped to [0, 1] for valve position semantics
///
/// # Example
///
/// ```
/// use tf_controls::{FirstOrderActuator, ActuatorState};
///
/// let actuator = FirstOrderActuator::new(0.2, 5.0).unwrap();
/// let mut state = ActuatorState { position: 0.0 };
///
/// // Step to 1.0 over time
/// for _ in 0..100 {
///     state = actuator.step(&state, 0.01, 1.0);
/// }
///
/// // Position should approach 1.0
/// assert!(state.position > 0.9);
/// ```
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct FirstOrderActuator {
    /// Time constant (seconds), must be positive
    pub tau: f64,
    /// Rate limit (1/second), must be positive
    pub rate_limit: f64,
}

impl FirstOrderActuator {
    /// Create a new first-order actuator.
    ///
    /// # Arguments
    ///
    /// * `tau` - Time constant in seconds (must be positive)
    /// * `rate_limit` - Maximum rate of change in 1/s (must be positive)
    ///
    /// # Errors
    ///
    /// Returns error if `tau` or `rate_limit` are not positive.
    pub fn new(tau: f64, rate_limit: f64) -> ControlResult<Self> {
        if tau <= 0.0 {
            return Err(ControlError::InvalidArg {
                what: "tau must be positive",
            });
        }
        if rate_limit <= 0.0 {
            return Err(ControlError::InvalidArg {
                what: "rate_limit must be positive",
            });
        }
        Ok(Self { tau, rate_limit })
    }

    /// Compute position derivative given current position and command.
    ///
    /// Returns `dpdt` clamped to `[-rate_limit, rate_limit]`.
    pub fn dpdt(&self, position: f64, command: f64) -> f64 {
        let raw = (command - position) / self.tau;
        raw.clamp(-self.rate_limit, self.rate_limit)
    }

    /// Advance actuator state by timestep `dt` given command input.
    ///
    /// Uses explicit Euler integration.
    ///
    /// # Arguments
    ///
    /// * `state` - Current actuator state
    /// * `dt` - Timestep in seconds
    /// * `command` - Commanded position [0, 1]
    ///
    /// # Returns
    ///
    /// New actuator state with position clamped to [0, 1].
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
        let cmd = 1.0;
        let dt = 0.01;
        state = act.step(&state, dt, cmd);

        // After one step, should move toward 1.0
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

    #[test]
    fn invalid_parameters() {
        assert!(FirstOrderActuator::new(-0.1, 1.0).is_err());
        assert!(FirstOrderActuator::new(0.1, -1.0).is_err());
        assert!(FirstOrderActuator::new(0.0, 1.0).is_err());
    }
}
