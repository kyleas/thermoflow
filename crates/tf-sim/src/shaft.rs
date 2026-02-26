//! Shaft dynamics for rotating machinery.

use crate::error::{SimError, SimResult};

/// State of a rotating shaft.
#[derive(Clone, Debug)]
pub struct ShaftState {
    /// Angular velocity (rad/s)
    pub omega_rad_s: f64,
}

/// Rotating shaft with inertia and friction loss.
///
/// Models the rotational dynamics of a shaft connecting turbomachinery:
///
/// ```text
/// I * dω/dt = Σ(τ_i) - τ_loss
/// ```
///
/// where:
/// - I is the moment of inertia (kg·m²)
/// - τ_i are applied torques from components (pumps, turbines)
/// - τ_loss = loss_coeff * ω (viscous friction model)
///
/// ## Torque Calculation
///
/// Power and torque are related by:
/// ```text
/// τ = P / ω
/// ```
///
/// Sign convention matches component `shaft_power()`:
/// - Positive power (pump) → negative torque on shaft (consumes from shaft)
/// - Negative power (turbine) → positive torque on shaft (adds to shaft)
///
/// At low speeds (ω < ω_min), a small regularization is used to avoid
/// division by zero.
#[derive(Clone, Debug)]
pub struct Shaft {
    /// Moment of inertia (kg·m²)
    pub inertia: f64,
    /// Viscous friction coefficient (N·m·s/rad)
    /// Loss torque = loss_coeff * omega
    pub loss_coeff: f64,
    /// Minimum angular velocity for torque calculation (rad/s)
    /// Used to regularize τ = P/ω near zero speed
    pub omega_min: f64,
}

impl Shaft {
    /// Create a new shaft.
    ///
    /// # Arguments
    /// * `inertia` - Moment of inertia (kg·m²), must be positive
    /// * `loss_coeff` - Viscous friction coefficient (N·m·s/rad), should be >= 0
    ///
    /// # Errors
    /// Returns error if parameters are non-physical.
    pub fn new(inertia: f64, loss_coeff: f64) -> SimResult<Self> {
        if inertia <= 0.0 {
            return Err(SimError::InvalidArg {
                what: "shaft inertia must be positive",
            });
        }
        if loss_coeff < 0.0 {
            return Err(SimError::InvalidArg {
                what: "loss coefficient cannot be negative",
            });
        }

        Ok(Self {
            inertia,
            loss_coeff,
            omega_min: 0.1, // rad/s, ~1 RPM
        })
    }

    /// Convert component power to shaft torque.
    ///
    /// Uses the relation τ = P / ω, with regularization near zero speed.
    ///
    /// # Arguments
    /// * `power` - Component shaft power (W)
    ///   - Positive: power added to fluid (pump consuming shaft power)
    ///   - Negative: power extracted from fluid (turbine producing shaft power)
    /// * `omega` - Current shaft angular velocity (rad/s)
    ///
    /// # Returns
    /// Torque on shaft (N·m):
    /// - For pump (P > 0): negative torque (drains shaft)
    /// - For turbine (P < 0): positive torque (drives shaft)
    pub fn power_to_torque(&self, power: f64, omega: f64) -> f64 {
        let omega_abs = omega.abs().max(self.omega_min);

        // τ = -P / ω
        // Negative sign because positive power (pump) consumes from shaft
        -power / omega_abs
    }

    /// Compute friction torque (always opposes motion).
    ///
    /// # Arguments
    /// * `omega` - Angular velocity (rad/s)
    ///
    /// # Returns
    /// Friction torque (N·m), sign opposite to omega
    pub fn friction_torque(&self, omega: f64) -> f64 {
        -self.loss_coeff * omega
    }

    /// Compute angular acceleration given applied torques.
    ///
    /// # Arguments
    /// * `torques` - Slice of applied torques (N·m) from components
    /// * `omega` - Current angular velocity (rad/s)
    ///
    /// # Returns
    /// Angular acceleration dω/dt (rad/s²)
    pub fn angular_acceleration(&self, torques: &[f64], omega: f64) -> f64 {
        let net_torque: f64 = torques.iter().sum();
        let friction = self.friction_torque(omega);

        (net_torque + friction) / self.inertia
    }

    /// Compute derivative of shaft state.
    ///
    /// Convenience method that wraps angular_acceleration.
    ///
    /// # Arguments
    /// * `state` - Current shaft state
    /// * `applied_torques` - Torques from components (N·m)
    ///
    /// # Returns
    /// State derivative (dω/dt)
    pub fn derivative(&self, state: &ShaftState, applied_torques: &[f64]) -> ShaftState {
        let domega_dt = self.angular_acceleration(applied_torques, state.omega_rad_s);

        ShaftState {
            omega_rad_s: domega_dt,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn shaft_creation() {
        let shaft = Shaft::new(1.0, 0.1);
        assert!(shaft.is_ok());
    }

    #[test]
    fn shaft_invalid_inertia() {
        let shaft = Shaft::new(0.0, 0.1);
        assert!(shaft.is_err());
    }

    #[test]
    fn power_to_torque_pump() {
        let shaft = Shaft::new(1.0, 0.1).unwrap();

        // Pump power (positive) should give negative torque
        let power_pump = 1000.0; // W
        let omega = 100.0; // rad/s
        let torque = shaft.power_to_torque(power_pump, omega);

        assert!(torque < 0.0);
        assert!((torque + 10.0).abs() < 0.1); // τ ≈ -1000/100 = -10
    }

    #[test]
    fn power_to_torque_turbine() {
        let shaft = Shaft::new(1.0, 0.1).unwrap();

        // Turbine power (negative) should give positive torque
        let power_turbine = -1000.0; // W
        let omega = 100.0; // rad/s
        let torque = shaft.power_to_torque(power_turbine, omega);

        assert!(torque > 0.0);
        assert!((torque - 10.0).abs() < 0.1); // τ ≈ -(-1000)/100 = 10
    }

    #[test]
    fn friction_opposes_motion() {
        let shaft = Shaft::new(1.0, 0.5).unwrap();

        // Positive omega should give negative friction
        let friction_pos = shaft.friction_torque(100.0);
        assert!(friction_pos < 0.0);

        // Negative omega should give positive friction
        let friction_neg = shaft.friction_torque(-100.0);
        assert!(friction_neg > 0.0);
    }

    #[test]
    fn acceleration_from_torque() {
        let shaft = Shaft::new(2.0, 0.1).unwrap(); // I = 2 kg·m²

        let torques = vec![10.0]; // 10 N·m applied
        let omega = 50.0; // rad/s

        let alpha = shaft.angular_acceleration(&torques, omega);

        // Net torque = 10 - 0.1*50 = 5 N·m
        // α = 5 / 2 = 2.5 rad/s²
        assert!((alpha - 2.5).abs() < 0.1);
    }

    #[test]
    fn derivative_computation() {
        let shaft = Shaft::new(1.0, 0.05).unwrap();

        let state = ShaftState { omega_rad_s: 100.0 };
        let torques = vec![20.0, -5.0]; // Net = 15 N·m

        let dstate = shaft.derivative(&state, &torques);

        // Net torque = 15 - 0.05*100 = 10 N·m
        // α = 10 / 1 = 10 rad/s²
        assert!((dstate.omega_rad_s - 10.0).abs() < 0.1);
    }

    #[test]
    fn zero_speed_regularization() {
        let shaft = Shaft::new(1.0, 0.1).unwrap();

        // Should not panic at zero speed
        let torque = shaft.power_to_torque(1000.0, 0.0);

        // Should use omega_min for calculation
        assert!(torque.is_finite());
    }
}
