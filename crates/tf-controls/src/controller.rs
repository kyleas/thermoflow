//! Controller block implementations.
//!
//! Provides standard controller types:
//! - **PI (Proportional-Integral)**: Classic feedback controller
//! - **PID (Proportional-Integral-Derivative)**: Enhanced with derivative action
//!
//! All controllers include:
//! - Anti-windup protection
//! - Output clamping
//! - Integral clamping
//! - Sampled/digital operation semantics

use crate::error::{ControlError, ControlResult};
use serde::{Deserialize, Serialize};

/// PI controller configuration.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct PIController {
    /// Proportional gain.
    pub kp: f64,
    /// Integral time constant (seconds). Larger values reduce integral action.
    pub ti: f64,
    /// Minimum output value.
    pub out_min: f64,
    /// Maximum output value.
    pub out_max: f64,
    /// Integral windup limit (optional). If None, uses output limits.
    pub integral_limit: Option<f64>,
}

impl PIController {
    /// Create a new PI controller.
    ///
    /// # Arguments
    ///
    /// * `kp` - Proportional gain
    /// * `ti` - Integral time constant (seconds)
    /// * `out_min` - Minimum output
    /// * `out_max` - Maximum output
    pub fn new(kp: f64, ti: f64, out_min: f64, out_max: f64) -> ControlResult<Self> {
        if ti <= 0.0 {
            return Err(ControlError::InvalidArg {
                what: "ti must be positive",
            });
        }
        if out_min >= out_max {
            return Err(ControlError::InvalidArg {
                what: "out_min must be less than out_max",
            });
        }
        Ok(Self {
            kp,
            ti,
            out_min,
            out_max,
            integral_limit: None,
        })
    }

    /// Set integral windup limit.
    pub fn with_integral_limit(mut self, limit: f64) -> Self {
        self.integral_limit = Some(limit);
        self
    }

    /// Compute controller output given process variable and setpoint.
    ///
    /// # Arguments
    ///
    /// * `state` - Controller state (contains integral)
    /// * `pv` - Process variable (measured value)
    /// * `sp` - Setpoint (desired value)
    /// * `dt` - Time since last update (seconds)
    ///
    /// # Returns
    ///
    /// Updated state and output value.
    pub fn update(
        &self,
        state: &PIControllerState,
        pv: f64,
        sp: f64,
        dt: f64,
    ) -> (PIControllerState, f64) {
        // Error: e = sp - pv (positive error means PV is below setpoint)
        let error = sp - pv;

        // Proportional term
        let p_term = self.kp * error;

        // Integral term with anti-windup
        let ki = self.kp / self.ti;
        let new_integral = state.integral + error * dt;

        // Clamp integral if limit is set
        let clamped_integral = if let Some(limit) = self.integral_limit {
            new_integral.clamp(-limit, limit)
        } else {
            new_integral
        };

        let i_term = ki * clamped_integral;

        // Total output
        let output_raw = p_term + i_term;
        let output = output_raw.clamp(self.out_min, self.out_max);

        // Anti-windup: if output is saturated, don't accumulate integral
        let final_integral = if output == output_raw {
            clamped_integral
        } else {
            state.integral // Keep old integral if saturated
        };

        let new_state = PIControllerState {
            integral: final_integral,
        };

        (new_state, output)
    }
}

/// PI controller state.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct PIControllerState {
    /// Integral accumulator.
    pub integral: f64,
}

impl Default for PIControllerState {
    fn default() -> Self {
        Self { integral: 0.0 }
    }
}

/// PID controller configuration.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct PIDController {
    /// Proportional gain.
    pub kp: f64,
    /// Integral time constant (seconds).
    pub ti: f64,
    /// Derivative time constant (seconds).
    pub td: f64,
    /// Derivative filter time constant (seconds). Prevents noise amplification.
    pub td_filter: f64,
    /// Minimum output value.
    pub out_min: f64,
    /// Maximum output value.
    pub out_max: f64,
    /// Integral windup limit (optional).
    pub integral_limit: Option<f64>,
}

impl PIDController {
    /// Create a new PID controller.
    ///
    /// # Arguments
    ///
    /// * `kp` - Proportional gain
    /// * `ti` - Integral time constant (seconds)
    /// * `td` - Derivative time constant (seconds)
    /// * `td_filter` - Derivative filter time constant (seconds)
    /// * `out_min` - Minimum output
    /// * `out_max` - Maximum output
    pub fn new(
        kp: f64,
        ti: f64,
        td: f64,
        td_filter: f64,
        out_min: f64,
        out_max: f64,
    ) -> ControlResult<Self> {
        if ti <= 0.0 {
            return Err(ControlError::InvalidArg {
                what: "ti must be positive",
            });
        }
        if td < 0.0 {
            return Err(ControlError::InvalidArg {
                what: "td must be non-negative",
            });
        }
        if td_filter <= 0.0 {
            return Err(ControlError::InvalidArg {
                what: "td_filter must be positive",
            });
        }
        if out_min >= out_max {
            return Err(ControlError::InvalidArg {
                what: "out_min must be less than out_max",
            });
        }
        Ok(Self {
            kp,
            ti,
            td,
            td_filter,
            out_min,
            out_max,
            integral_limit: None,
        })
    }

    /// Set integral windup limit.
    pub fn with_integral_limit(mut self, limit: f64) -> Self {
        self.integral_limit = Some(limit);
        self
    }

    /// Compute controller output.
    ///
    /// Uses filtered derivative to prevent noise amplification.
    pub fn update(
        &self,
        state: &PIDControllerState,
        pv: f64,
        sp: f64,
        dt: f64,
    ) -> (PIDControllerState, f64) {
        // Error
        let error = sp - pv;

        // Proportional term
        let p_term = self.kp * error;

        // Integral term with anti-windup (same as PI)
        let ki = self.kp / self.ti;
        let new_integral = state.integral + error * dt;
        let clamped_integral = if let Some(limit) = self.integral_limit {
            new_integral.clamp(-limit, limit)
        } else {
            new_integral
        };
        let i_term = ki * clamped_integral;

        // Derivative term with filtering
        // Filter: tau * d(filt)/dt + filt = error
        // Discrete: filt[n] = alpha * filt[n-1] + (1-alpha) * error
        let alpha = self.td_filter / (self.td_filter + dt);
        let filtered_error = alpha * state.filtered_error + (1.0 - alpha) * error;
        let kd = self.kp * self.td;
        let d_term = kd * (filtered_error - state.filtered_error) / dt;

        // Total output
        let output_raw = p_term + i_term + d_term;
        let output = output_raw.clamp(self.out_min, self.out_max);

        // Anti-windup
        let final_integral = if output == output_raw {
            clamped_integral
        } else {
            state.integral
        };

        let new_state = PIDControllerState {
            integral: final_integral,
            filtered_error,
        };

        (new_state, output)
    }
}

/// PID controller state.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct PIDControllerState {
    /// Integral accumulator.
    pub integral: f64,
    /// Filtered error for derivative calculation.
    pub filtered_error: f64,
}

impl Default for PIDControllerState {
    fn default() -> Self {
        Self {
            integral: 0.0,
            filtered_error: 0.0,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn pi_controller_creation() {
        let pi = PIController::new(1.0, 2.0, 0.0, 1.0).unwrap();
        assert_eq!(pi.kp, 1.0);
        assert_eq!(pi.ti, 2.0);
    }

    #[test]
    fn pi_controller_proportional_only() {
        let pi = PIController::new(2.0, 1000.0, 0.0, 1.0).unwrap(); // Very large Ti ~ P-only
        let state = PIControllerState::default();

        let (_, output) = pi.update(&state, 0.5, 1.0, 0.1);
        // Error = 1.0 - 0.5 = 0.5, P = 2.0 * 0.5 = 1.0, but clamped to max
        assert!((output - 1.0).abs() < 1e-6);
    }

    #[test]
    fn pi_controller_integral_action() {
        let pi = PIController::new(1.0, 1.0, 0.0, 10.0).unwrap();
        let mut state = PIControllerState::default();

        // Constant error of 1.0
        for _ in 0..10 {
            let (new_state, _) = pi.update(&state, 0.0, 1.0, 0.1);
            state = new_state;
        }

        // Integral should have accumulated
        assert!(state.integral > 0.5);
    }

    #[test]
    fn pi_controller_output_clamping() {
        let pi = PIController::new(10.0, 1.0, 0.0, 1.0).unwrap();
        let state = PIControllerState::default();

        let (_, output) = pi.update(&state, 0.0, 10.0, 0.1);
        // Large error would give huge output, but should be clamped to 1.0
        assert_eq!(output, 1.0);
    }

    #[test]
    fn pid_controller_creation() {
        let pid = PIDController::new(1.0, 2.0, 0.5, 0.1, 0.0, 1.0).unwrap();
        assert_eq!(pid.kp, 1.0);
        assert_eq!(pid.td, 0.5);
    }

    #[test]
    fn pid_controller_basic() {
        let pid = PIDController::new(1.0, 10.0, 0.1, 0.1, 0.0, 10.0).unwrap();
        let state = PIDControllerState::default();

        let (_, output) = pid.update(&state, 5.0, 10.0, 0.1);
        // Should have positive output due to positive error
        assert!(output > 0.0);
    }

    #[test]
    fn invalid_controller_params() {
        // Negative Ti
        assert!(PIController::new(1.0, -1.0, 0.0, 1.0).is_err());
        // out_min >= out_max
        assert!(PIController::new(1.0, 1.0, 1.0, 0.0).is_err());
        // Negative Td
        assert!(PIDController::new(1.0, 1.0, -0.5, 0.1, 0.0, 1.0).is_err());
    }
}
