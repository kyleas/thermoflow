//! CoolProp-based fluid property model.

use crate::composition::Composition;
use crate::error::{FluidError, FluidResult};
use crate::model::{FluidModel, validation};
use crate::state::{SpecEnthalpy, SpecHeatCapacity, StateInput, ThermoState};
use rfluids::prelude::*;
use tf_core::units::{Density, Velocity, k};

/// CoolProp backend for fluid properties.
///
/// Currently supports pure fluids only. Mixtures will be added in future versions.
///
/// Thread-safe: rfluids Fluid instances are stateless and can be created/used from multiple threads.
pub struct CoolPropModel {
    // Future: could add configuration options here (e.g., backend selection, caching)
}

impl CoolPropModel {
    /// Create a new CoolProp model.
    pub fn new() -> Self {
        Self {}
    }

    /// Create a Fluid instance at given P,T state.
    fn fluid_at_pt(&self, pure: Pure, p_pa: f64, t_k: f64) -> FluidResult<Fluid> {
        Fluid::from(pure)
            .in_state(FluidInput::pressure(p_pa), FluidInput::temperature(t_k))
            .map_err(|e| FluidError::Backend {
                message: format!("rfluids error at P={} Pa, T={} K: {}", p_pa, t_k, e),
            })
    }

    /// Solve for temperature given pressure and enthalpy.
    ///
    /// Uses bisection to robustly find T such that h(P,T) = h_target.
    fn solve_t_from_ph(&self, pure: Pure, p_pa: f64, h_target: f64) -> FluidResult<f64> {
        // Temperature search bounds [K]
        // Use wider bounds to handle various fluids and states
        const T_MIN: f64 = 100.0; // 100 K to avoid near-critical/sublimation issues
        const T_MAX: f64 = 2000.0;
        const MAX_ITER: usize = 100;

        let mut t_low = T_MIN;
        let mut t_high = T_MAX;

        // Evaluate at bounds
        let mut fluid_low = self.fluid_at_pt(pure, p_pa, t_low)?;
        let h_low = fluid_low.enthalpy().map_err(|e| FluidError::Backend {
            message: format!("rfluids error getting enthalpy: {}", e),
        })?;

        let mut fluid_high = self.fluid_at_pt(pure, p_pa, t_high)?;
        let h_high = fluid_high.enthalpy().map_err(|e| FluidError::Backend {
            message: format!("rfluids error getting enthalpy: {}", e),
        })?;

        // Check if target is in brackets
        if h_target < h_low || h_target > h_high {
            return Err(FluidError::OutOfRange {
                what: "enthalpy outside valid range for given pressure",
            });
        }

        // Bisection
        for _ in 0..MAX_ITER {
            let t_mid = 0.5 * (t_low + t_high);
            let mut fluid_mid = self.fluid_at_pt(pure, p_pa, t_mid)?;
            let h_mid = fluid_mid.enthalpy().map_err(|e| FluidError::Backend {
                message: format!("rfluids error getting enthalpy: {}", e),
            })?;

            // Tolerance: absolute or relative
            let tol = 1.0_f64.max(h_target.abs() * 1e-6);
            if (h_mid - h_target).abs() < tol {
                return Ok(t_mid);
            }

            if h_mid < h_target {
                t_low = t_mid;
            } else {
                t_high = t_mid;
            }
        }

        // Return best estimate if we hit max iterations
        Ok(0.5 * (t_low + t_high))
    }
}

impl Default for CoolPropModel {
    fn default() -> Self {
        Self::new()
    }
}

impl FluidModel for CoolPropModel {
    fn name(&self) -> &str {
        "CoolProp"
    }

    fn supports_composition(&self, comp: &Composition) -> bool {
        // Only support pure fluids with rfluids mappings
        if let Some(species) = comp.is_pure() {
            species.rfluids_pure().is_some()
        } else {
            false
        }
    }

    fn state(&self, input: StateInput, comp: Composition) -> FluidResult<ThermoState> {
        // Validate composition
        if !self.supports_composition(&comp) {
            return Err(FluidError::NotSupported {
                what: "composition not supported (mixtures or unsupported species)",
            });
        }

        let species = comp.is_pure().unwrap(); // Already validated above
        let pure = species.rfluids_pure().ok_or(FluidError::NotSupported {
            what: "species not available in rfluids",
        })?;

        match input {
            StateInput::PT { p, t } => {
                // Validate inputs
                validation::validate_pressure(p)?;
                validation::validate_temperature(t)?;

                // Create state directly (validate that rfluids accepts this state)
                let _fluid = self.fluid_at_pt(pure, p.value, t.value)?;

                ThermoState::from_pt(p, t, comp)
            }
            StateInput::PH { p, h } => {
                // Validate inputs
                validation::validate_pressure(p)?;
                validation::validate_enthalpy(h)?;

                // Solve for T
                let p_pa = p.value;
                let t_k = self.solve_t_from_ph(pure, p_pa, h)?;

                // Create state
                let t = k(t_k);
                ThermoState::from_pt(p, t, comp)
            }
        }
    }

    fn rho(&self, state: &ThermoState) -> FluidResult<Density> {
        let species = state
            .composition()
            .is_pure()
            .ok_or(FluidError::NotSupported {
                what: "mixtures not supported",
            })?;

        let pure = species.rfluids_pure().ok_or(FluidError::NotSupported {
            what: "species not supported by rfluids",
        })?;

        let p_pa = state.pressure().value;
        let t_k = state.temperature().value;

        let mut fluid = self.fluid_at_pt(pure, p_pa, t_k)?;
        let rho_val = fluid.density().map_err(|e| FluidError::Backend {
            message: format!("rfluids error getting density: {}", e),
        })?;

        use uom::si::mass_density::kilogram_per_cubic_meter;
        let rho = Density::new::<kilogram_per_cubic_meter>(rho_val);

        validation::validate_density(rho)?;
        Ok(rho)
    }

    fn h(&self, state: &ThermoState) -> FluidResult<SpecEnthalpy> {
        let species = state
            .composition()
            .is_pure()
            .ok_or(FluidError::NotSupported {
                what: "mixtures not supported",
            })?;

        let pure = species.rfluids_pure().ok_or(FluidError::NotSupported {
            what: "species not supported by rfluids",
        })?;

        let p_pa = state.pressure().value;
        let t_k = state.temperature().value;

        let mut fluid = self.fluid_at_pt(pure, p_pa, t_k)?;
        let h = fluid.enthalpy().map_err(|e| FluidError::Backend {
            message: format!("rfluids error getting enthalpy: {}", e),
        })?;

        validation::validate_enthalpy(h)?;
        Ok(h)
    }

    fn cp(&self, state: &ThermoState) -> FluidResult<SpecHeatCapacity> {
        let species = state
            .composition()
            .is_pure()
            .ok_or(FluidError::NotSupported {
                what: "mixtures not supported",
            })?;

        let pure = species.rfluids_pure().ok_or(FluidError::NotSupported {
            what: "species not supported by rfluids",
        })?;

        let p_pa = state.pressure().value;
        let t_k = state.temperature().value;

        let mut fluid = self.fluid_at_pt(pure, p_pa, t_k)?;
        let cp = fluid.specific_heat().map_err(|e| FluidError::Backend {
            message: format!("rfluids error getting specific heat: {}", e),
        })?;

        validation::validate_cp(cp)?;
        Ok(cp)
    }

    fn gamma(&self, state: &ThermoState) -> FluidResult<f64> {
        let species = state
            .composition()
            .is_pure()
            .ok_or(FluidError::NotSupported {
                what: "mixtures not supported",
            })?;

        let pure = species.rfluids_pure().ok_or(FluidError::NotSupported {
            what: "species not supported by rfluids",
        })?;

        let p_pa = state.pressure().value;
        let t_k = state.temperature().value;

        let mut fluid = self.fluid_at_pt(pure, p_pa, t_k)?;

        // rfluids may or may not expose gamma directly
        // Compute it from cp and cv using thermodynamic relation
        // For real fluids: cv = cp - R_specific, where R_specific = p / (rho * T)
        let cp = fluid.specific_heat().map_err(|e| FluidError::Backend {
            message: format!("rfluids error getting cp: {}", e),
        })?;
        let rho = fluid.density().map_err(|e| FluidError::Backend {
            message: format!("rfluids error getting density: {}", e),
        })?;
        let r_specific = p_pa / (rho * t_k);
        let cv = cp - r_specific;

        if cv <= 0.0 || !cv.is_finite() {
            return Err(FluidError::Backend {
                message: "Failed to compute cv for gamma calculation".into(),
            });
        }

        let gamma = cp / cv;
        validation::validate_gamma(gamma)?;
        Ok(gamma)
    }

    fn a(&self, state: &ThermoState) -> FluidResult<Velocity> {
        let species = state
            .composition()
            .is_pure()
            .ok_or(FluidError::NotSupported {
                what: "mixtures not supported",
            })?;

        let pure = species.rfluids_pure().ok_or(FluidError::NotSupported {
            what: "species not supported by rfluids",
        })?;

        let p_pa = state.pressure().value;
        let t_k = state.temperature().value;

        let mut fluid = self.fluid_at_pt(pure, p_pa, t_k)?;
        let a_val = fluid.sound_speed().map_err(|e| FluidError::Backend {
            message: format!("rfluids error getting sound speed: {}", e),
        })?;

        use uom::si::velocity::meter_per_second;
        let a = Velocity::new::<meter_per_second>(a_val);

        validation::validate_speed_of_sound(a)?;
        Ok(a)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::species::Species;

    #[test]
    fn model_name() {
        let model = CoolPropModel::new();
        assert_eq!(model.name(), "CoolProp");
    }

    #[test]
    fn supports_pure_fluids() {
        let model = CoolPropModel::new();
        let comp = Composition::pure(Species::N2);
        assert!(model.supports_composition(&comp));
    }

    #[test]
    fn does_not_support_rp1() {
        let model = CoolPropModel::new();
        let comp = Composition::pure(Species::RP1);
        assert!(!model.supports_composition(&comp));
    }

    #[test]
    fn does_not_support_mixtures() {
        let model = CoolPropModel::new();
        let comp =
            Composition::new_mole_fractions(vec![(Species::O2, 0.5), (Species::N2, 0.5)]).unwrap();
        assert!(!model.supports_composition(&comp));
    }
}
