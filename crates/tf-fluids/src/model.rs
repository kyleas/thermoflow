//! Fluid property model trait and validation helpers.

use crate::composition::Composition;
use crate::error::{FluidError, FluidResult};
use crate::state::{SpecEnthalpy, SpecHeatCapacity, StateInput, ThermoState};
use tf_core::units::{Density, Pressure, Temperature, Velocity};

/// Trait for fluid property models.
///
/// Implementations must be thread-safe (Send + Sync) to support parallel evaluation.
/// All methods should validate inputs and outputs for physical plausibility.
pub trait FluidModel: Send + Sync {
    /// Get the model name (for debugging/logging).
    fn name(&self) -> &str;

    /// Check if this model supports the given composition.
    ///
    /// For example, CoolProp backend currently supports pure fluids only.
    fn supports_composition(&self, comp: &Composition) -> bool;

    /// Create a thermodynamic state from input specification.
    ///
    /// For PT input: validates and creates state directly.
    /// For PH input: solves for temperature, then creates state.
    fn state(&self, input: StateInput, comp: Composition) -> FluidResult<ThermoState>;

    /// Compute density [kg/m³] at the given state.
    fn rho(&self, state: &ThermoState) -> FluidResult<Density>;

    /// Compute specific enthalpy [J/kg] at the given state.
    fn h(&self, state: &ThermoState) -> FluidResult<SpecEnthalpy>;

    /// Compute specific heat capacity at constant pressure [J/(kg·K)] at the given state.
    fn cp(&self, state: &ThermoState) -> FluidResult<SpecHeatCapacity>;

    /// Compute heat capacity ratio γ = cp/cv (dimensionless) at the given state.
    fn gamma(&self, state: &ThermoState) -> FluidResult<f64>;

    /// Compute speed of sound [m/s] at the given state.
    fn a(&self, state: &ThermoState) -> FluidResult<Velocity>;
}

/// Validation helpers for fluid properties.
pub(crate) mod validation {
    use super::*;

    /// Ensure pressure is positive and finite.
    pub fn validate_pressure(p: Pressure) -> FluidResult<()> {
        if !p.value.is_finite() || p.value <= 0.0 {
            return Err(FluidError::NonPhysical {
                what: "pressure must be positive and finite",
            });
        }
        Ok(())
    }

    /// Ensure temperature is positive and finite.
    pub fn validate_temperature(t: Temperature) -> FluidResult<()> {
        if !t.value.is_finite() || t.value <= 0.0 {
            return Err(FluidError::NonPhysical {
                what: "temperature must be positive and finite",
            });
        }
        Ok(())
    }

    /// Ensure density is positive and finite.
    pub fn validate_density(rho: Density) -> FluidResult<()> {
        if !rho.value.is_finite() || rho.value <= 0.0 {
            return Err(FluidError::NonPhysical {
                what: "density must be positive and finite",
            });
        }
        Ok(())
    }

    /// Ensure specific heat capacity is positive and finite.
    pub fn validate_cp(cp: f64) -> FluidResult<()> {
        if !cp.is_finite() || cp <= 0.0 {
            return Err(FluidError::NonPhysical {
                what: "cp must be positive and finite",
            });
        }
        Ok(())
    }

    /// Ensure gamma (heat capacity ratio) is physically plausible.
    pub fn validate_gamma(gamma: f64) -> FluidResult<()> {
        if !gamma.is_finite() || gamma < 1.0 {
            return Err(FluidError::NonPhysical {
                what: "gamma must be >= 1 and finite",
            });
        }
        Ok(())
    }

    /// Ensure speed of sound is positive and finite.
    pub fn validate_speed_of_sound(a: Velocity) -> FluidResult<()> {
        if !a.value.is_finite() || a.value <= 0.0 {
            return Err(FluidError::NonPhysical {
                what: "speed of sound must be positive and finite",
            });
        }
        Ok(())
    }

    /// Ensure enthalpy is finite (can be negative).
    pub fn validate_enthalpy(h: f64) -> FluidResult<()> {
        if !h.is_finite() {
            return Err(FluidError::NonPhysical {
                what: "enthalpy must be finite",
            });
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::validation::*;
    use tf_core::units::Density;
    use tf_core::units::{k, pa};

    #[test]
    fn validate_positive_pressure() {
        assert!(validate_pressure(pa(101325.0)).is_ok());
        assert!(validate_pressure(pa(-100.0)).is_err());
        assert!(validate_pressure(pa(0.0)).is_err());
        assert!(validate_pressure(pa(f64::NAN)).is_err());
    }

    #[test]
    fn validate_positive_temperature() {
        assert!(validate_temperature(k(300.0)).is_ok());
        assert!(validate_temperature(k(-10.0)).is_err());
        assert!(validate_temperature(k(0.0)).is_err());
    }

    #[test]
    fn validate_density_positive() {
        use uom::si::mass_density::kilogram_per_cubic_meter;
        let rho_val = 1000.0;
        let rho = Density::new::<kilogram_per_cubic_meter>(rho_val);
        assert!(validate_density(rho).is_ok());

        let rho_neg = Density::new::<kilogram_per_cubic_meter>(-1.0);
        assert!(validate_density(rho_neg).is_err());

        let rho_zero = Density::new::<kilogram_per_cubic_meter>(0.0);
        assert!(validate_density(rho_zero).is_err());
    }

    #[test]
    fn validate_cp_positive() {
        assert!(validate_cp(1000.0).is_ok());
        assert!(validate_cp(-100.0).is_err());
        assert!(validate_cp(0.0).is_err());
    }

    #[test]
    fn validate_gamma_physical() {
        assert!(validate_gamma(1.4).is_ok());
        assert!(validate_gamma(1.0).is_ok());
        assert!(validate_gamma(0.9).is_err());
        assert!(validate_gamma(f64::NAN).is_err());
    }
}
