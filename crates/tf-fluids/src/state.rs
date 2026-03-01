//! Thermodynamic state definitions.

use crate::composition::Composition;
use crate::error::{FluidError, FluidResult};
use tf_core::units::{Pressure, Temperature};

/// Specific enthalpy [J/kg].
///
/// Not part of uom's standard set, so we use f64 with clear documentation.
pub type SpecEnthalpy = f64;

/// Specific entropy [J/(kg·K)].
///
/// Not part of uom's standard set, so we use f64 with clear documentation.
pub type SpecEntropy = f64;

/// Specific heat capacity [J/(kg·K)].
pub type SpecHeatCapacity = f64;

/// Input specification for creating a thermodynamic state.
#[derive(Debug, Clone, PartialEq)]
pub enum StateInput {
    /// Pressure and temperature.
    PT { p: Pressure, t: Temperature },
    /// Pressure and specific enthalpy.
    PH { p: Pressure, h: SpecEnthalpy },
    /// Density and specific enthalpy.
    RhoH { rho_kg_m3: f64, h: SpecEnthalpy },
    /// Pressure and specific entropy.
    PS { p: Pressure, s: SpecEntropy },
}

/// Thermodynamic state: pressure, temperature, and composition.
///
/// This is the minimal set of independent properties.
/// Derived properties (density, enthalpy, etc.) are computed on demand
/// via the `FluidModel` trait.
#[derive(Debug, Clone, PartialEq)]
pub struct ThermoState {
    p: Pressure,
    t: Temperature,
    comp: Composition,
}

impl ThermoState {
    /// Create a state from pressure, temperature, and composition.
    ///
    /// Validates that pressure and temperature are positive and finite.
    pub fn from_pt(p: Pressure, t: Temperature, comp: Composition) -> FluidResult<Self> {
        // Validate pressure
        let p_val = p.value;
        if !p_val.is_finite() || p_val <= 0.0 {
            return Err(FluidError::NonPhysical {
                what: "pressure must be positive and finite",
            });
        }

        // Validate temperature
        let t_val = t.value;
        if !t_val.is_finite() || t_val <= 0.0 {
            return Err(FluidError::NonPhysical {
                what: "temperature must be positive and finite",
            });
        }

        Ok(Self { p, t, comp })
    }

    /// Get pressure.
    pub fn pressure(&self) -> Pressure {
        self.p
    }

    /// Get temperature.
    pub fn temperature(&self) -> Temperature {
        self.t
    }

    /// Get composition.
    pub fn composition(&self) -> &Composition {
        &self.comp
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::species::Species;
    use tf_core::units::{k, pa};

    #[test]
    fn create_valid_state() {
        let comp = Composition::pure(Species::N2);
        let p = pa(101325.0);
        let t = k(300.0);

        let state = ThermoState::from_pt(p, t, comp).unwrap();
        assert_eq!(state.pressure().value, 101325.0);
        assert_eq!(state.temperature().value, 300.0);
    }

    #[test]
    fn reject_negative_pressure() {
        let comp = Composition::pure(Species::N2);
        let p = pa(-100.0);
        let t = k(300.0);

        let result = ThermoState::from_pt(p, t, comp);
        assert!(result.is_err());
    }

    #[test]
    fn reject_zero_temperature() {
        let comp = Composition::pure(Species::N2);
        let p = pa(101325.0);
        let t = k(0.0);

        let result = ThermoState::from_pt(p, t, comp);
        assert!(result.is_err());
    }

    #[test]
    fn reject_non_finite() {
        let comp = Composition::pure(Species::N2);
        let p = pa(f64::NAN);
        let t = k(300.0);

        let result = ThermoState::from_pt(p, t, comp);
        assert!(result.is_err());
    }
}
