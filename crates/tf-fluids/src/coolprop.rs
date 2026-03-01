//! CoolProp-based fluid property model.

use crate::composition::Composition;
use crate::error::{FluidError, FluidResult};
use crate::model::{FluidModel, validation};
use crate::state::{SpecEnthalpy, SpecEntropy, SpecHeatCapacity, StateInput, ThermoState};
use rfluids::prelude::*;
use tf_core::units::{Density, Pressure, Velocity, k};

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

    /// Solve for temperature given pressure and specific entropy.
    fn solve_t_from_ps(&self, pure: Pure, p_pa: f64, s_target: f64) -> FluidResult<f64> {
        const T_MIN: f64 = 100.0;
        const T_MAX: f64 = 2000.0;
        const MAX_ITER: usize = 100;

        let mut t_low = T_MIN;
        let mut t_high = T_MAX;

        let mut fluid_low = self.fluid_at_pt(pure, p_pa, t_low)?;
        let s_low = fluid_low.entropy().map_err(|e| FluidError::Backend {
            message: format!("rfluids error getting entropy: {}", e),
        })?;

        let mut fluid_high = self.fluid_at_pt(pure, p_pa, t_high)?;
        let s_high = fluid_high.entropy().map_err(|e| FluidError::Backend {
            message: format!("rfluids error getting entropy: {}", e),
        })?;

        if s_target < s_low || s_target > s_high {
            return Err(FluidError::OutOfRange {
                what: "entropy outside valid range for given pressure",
            });
        }

        for _ in 0..MAX_ITER {
            let t_mid = 0.5 * (t_low + t_high);
            let mut fluid_mid = self.fluid_at_pt(pure, p_pa, t_mid)?;
            let s_mid = fluid_mid.entropy().map_err(|e| FluidError::Backend {
                message: format!("rfluids error getting entropy: {}", e),
            })?;

            let tol = 1.0_f64.max(s_target.abs() * 1e-6);
            if (s_mid - s_target).abs() < tol {
                return Ok(t_mid);
            }

            if s_mid < s_target {
                t_low = t_mid;
            } else {
                t_high = t_mid;
            }
        }

        Ok(0.5 * (t_low + t_high))
    }

    /// Solve for temperature at fixed density, then extract pressure.
    ///
    /// This is the optimized hot path for control volume pressure inversion.
    /// Given density and enthalpy, solve for T such that h(rho, T) = h_target,
    /// then extract pressure from the same backend state.
    ///
    /// This eliminates the nested bisection (P bisection + PH solve) structure
    /// by working directly in density-temperature space.
    ///
    /// # Arguments
    /// * `pure` - Pure substance
    /// * `rho_kg_m3` - Density in kg/m³
    /// * `h_target` - Target enthalpy in J/kg
    /// * `t_hint` - Optional temperature hint for faster convergence
    ///
    /// # Returns
    /// * `Ok((temperature_k, pressure_pa))` - Temperature in K and pressure in Pa
    pub fn solve_pt_from_rho_h(
        &self,
        pure: Pure,
        rho_kg_m3: f64,
        h_target: f64,
        t_hint: Option<f64>,
    ) -> FluidResult<(f64, f64)> {
        // Temperature search bounds [K]
        const T_MIN: f64 = 100.0;
        const T_MAX: f64 = 2000.0;
        const MAX_ITER: usize = 50;
        const H_TOL: f64 = 1.0; // J/kg - absolute tolerance

        // Initialize bracket
        let mut t_low = T_MIN;
        let mut t_high = T_MAX;

        // If we have a hint, try to use a tighter bracket
        if let Some(t_hint_val) = t_hint
            && t_hint_val > T_MIN
            && t_hint_val < T_MAX
        {
            // Use ±50% bracket around hint
            t_low = (t_hint_val * 0.5).max(T_MIN);
            t_high = (t_hint_val * 1.5).min(T_MAX);
        }

        // Create fluid instance at lower bound
        let mut fluid = Fluid::from(pure)
            .in_state(
                FluidInput::density(rho_kg_m3),
                FluidInput::temperature(t_low),
            )
            .map_err(|e| FluidError::Backend {
                message: format!(
                    "rfluids error at rho={} kg/m³, T={} K: {}",
                    rho_kg_m3, t_low, e
                ),
            })?;

        let h_low = fluid.enthalpy().map_err(|e| FluidError::Backend {
            message: format!("rfluids error getting enthalpy: {}", e),
        })?;

        // Evaluate at upper bound (reuse fluid instance)
        fluid = Fluid::from(pure)
            .in_state(
                FluidInput::density(rho_kg_m3),
                FluidInput::temperature(t_high),
            )
            .map_err(|e| FluidError::Backend {
                message: format!(
                    "rfluids error at rho={} kg/m³, T={} K: {}",
                    rho_kg_m3, t_high, e
                ),
            })?;

        let h_high = fluid.enthalpy().map_err(|e| FluidError::Backend {
            message: format!("rfluids error getting enthalpy: {}", e),
        })?;

        // Check if target is bracketed
        if h_target < h_low || h_target > h_high {
            return Err(FluidError::OutOfRange {
                what: "enthalpy outside valid range for given density",
            });
        }

        // Bisection with persistent fluid reuse
        for _ in 0..MAX_ITER {
            let t_mid = 0.5 * (t_low + t_high);

            // Reuse fluid instance - update state to new temperature
            fluid = Fluid::from(pure)
                .in_state(
                    FluidInput::density(rho_kg_m3),
                    FluidInput::temperature(t_mid),
                )
                .map_err(|e| FluidError::Backend {
                    message: format!(
                        "rfluids error at rho={} kg/m³, T={} K: {}",
                        rho_kg_m3, t_mid, e
                    ),
                })?;

            let h_mid = fluid.enthalpy().map_err(|e| FluidError::Backend {
                message: format!("rfluids error getting enthalpy: {}", e),
            })?;

            // Check convergence
            if (h_mid - h_target).abs() < H_TOL {
                // Extract pressure from converged state
                let p_pa = fluid.pressure().map_err(|e| FluidError::Backend {
                    message: format!("rfluids error getting pressure: {}", e),
                })?;
                return Ok((t_mid, p_pa));
            }

            // Update bracket
            if h_mid < h_target {
                t_low = t_mid;
            } else {
                t_high = t_mid;
            }
        }

        // Convergence failed, but return best estimate
        let t_final = 0.5 * (t_low + t_high);
        fluid = Fluid::from(pure)
            .in_state(
                FluidInput::density(rho_kg_m3),
                FluidInput::temperature(t_final),
            )
            .map_err(|e| FluidError::Backend {
                message: format!(
                    "rfluids error at rho={} kg/m³, T={} K: {}",
                    rho_kg_m3, t_final, e
                ),
            })?;

        let p_pa = fluid.pressure().map_err(|e| FluidError::Backend {
            message: format!("rfluids error getting pressure: {}", e),
        })?;

        Ok((t_final, p_pa))
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
            StateInput::RhoH { rho_kg_m3, h } => {
                if !rho_kg_m3.is_finite() || rho_kg_m3 <= 0.0 {
                    return Err(FluidError::NonPhysical {
                        what: "density must be positive and finite",
                    });
                }
                validation::validate_enthalpy(h)?;

                let (_t_k, p_pa) = self.solve_pt_from_rho_h(pure, rho_kg_m3, h, None)?;
                let p = tf_core::units::pa(p_pa);
                let t_k = self.solve_t_from_ph(pure, p_pa, h)?;
                let t = k(t_k);
                ThermoState::from_pt(p, t, comp)
            }
            StateInput::PS { p, s } => {
                validation::validate_pressure(p)?;
                if !s.is_finite() {
                    return Err(FluidError::NonPhysical {
                        what: "entropy must be finite",
                    });
                }

                let p_pa = p.value;
                let t_k = self.solve_t_from_ps(pure, p_pa, s)?;
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

    fn s(&self, state: &ThermoState) -> FluidResult<SpecEntropy> {
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
        let s = fluid.entropy().map_err(|e| FluidError::Backend {
            message: format!("rfluids error getting entropy: {}", e),
        })?;

        if !s.is_finite() {
            return Err(FluidError::NonPhysical {
                what: "entropy must be finite",
            });
        }
        Ok(s)
    }

    fn cp(&self, state: &ThermoState) -> FluidResult<SpecHeatCapacity> {
        use tf_core::timing;

        let _timer = timing::Timer::start("cp_property_query");
        let start = std::time::Instant::now();

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

        // Record timing
        if timing::is_enabled() {
            timing::thermo_timing::CP_CALLS.record(start.elapsed().as_secs_f64());
        }

        Ok(cp)
    }

    fn cv(&self, state: &ThermoState) -> FluidResult<SpecHeatCapacity> {
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
            message: format!("rfluids error getting cp: {}", e),
        })?;
        let rho = fluid.density().map_err(|e| FluidError::Backend {
            message: format!("rfluids error getting density: {}", e),
        })?;
        let r_specific = p_pa / (rho * t_k);
        let cv = cp - r_specific;
        validation::validate_cp(cv)?;
        Ok(cv)
    }

    fn gamma(&self, state: &ThermoState) -> FluidResult<f64> {
        use tf_core::timing;

        let _timer = timing::Timer::start("gamma_property_query");
        let start = std::time::Instant::now();

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

        // Record timing
        if timing::is_enabled() {
            timing::thermo_timing::GAMMA_CALLS.record(start.elapsed().as_secs_f64());
        }

        Ok(gamma)
    }

    fn a(&self, state: &ThermoState) -> FluidResult<Velocity> {
        use tf_core::timing;

        let _timer = timing::Timer::start("a_property_query");
        let start = std::time::Instant::now();

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

        // Record timing
        if timing::is_enabled() {
            timing::thermo_timing::A_CALLS.record(start.elapsed().as_secs_f64());
        }

        Ok(a)
    }

    fn property_pack(&self, state: &ThermoState) -> FluidResult<crate::model::ThermoPropertyPack> {
        use tf_core::timing;

        let _timer = timing::Timer::start("property_pack_query");
        let start = std::time::Instant::now();

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

        // Create single fluid instance and batch all property queries
        let mut fluid = self.fluid_at_pt(pure, p_pa, t_k)?;

        // Query all properties from the same fluid instance (no redundant creations)
        let rho = fluid.density().map_err(|e| FluidError::Backend {
            message: format!("rfluids error getting density: {}", e),
        })?;

        let h = fluid.enthalpy().map_err(|e| FluidError::Backend {
            message: format!("rfluids error getting enthalpy: {}", e),
        })?;

        let cp = fluid.specific_heat().map_err(|e| FluidError::Backend {
            message: format!("rfluids error getting cp: {}", e),
        })?;

        let a_val = fluid.sound_speed().map_err(|e| FluidError::Backend {
            message: format!("rfluids error getting sound speed: {}", e),
        })?;

        // Compute gamma from cp and density as before
        let r_specific = p_pa / (rho * t_k);
        let cv = cp - r_specific;

        if cv <= 0.0 || !cv.is_finite() {
            return Err(FluidError::Backend {
                message: "Failed to compute cv for gamma calculation in property_pack".into(),
            });
        }

        let gamma = cp / cv;

        use uom::si::velocity::meter_per_second;
        let a = Velocity::new::<meter_per_second>(a_val);

        // Validate all computed properties
        validation::validate_cp(cp)?;
        validation::validate_gamma(gamma)?;
        validation::validate_speed_of_sound(a)?;

        let pack = crate::model::ThermoPropertyPack {
            p: state.pressure(),
            t: state.temperature(),
            rho: {
                use uom::si::mass_density::kilogram_per_cubic_meter;
                Density::new::<kilogram_per_cubic_meter>(rho)
            },
            h,
            cp,
            gamma,
            a,
        };

        // Record timing
        if timing::is_enabled() {
            timing::thermo_timing::PROPERTY_PACK_CALLS.record(start.elapsed().as_secs_f64());
        }

        Ok(pack)
    }

    fn pressure_from_rho_h_direct(
        &self,
        comp: &Composition,
        rho_kg_m3: f64,
        h_j_per_kg: f64,
        t_hint_k: Option<f64>,
    ) -> Option<FluidResult<Pressure>> {
        // Only support pure fluids
        let species = comp.is_pure()?;
        let pure = species.rfluids_pure()?;

        // Call the direct solve method
        let result = self.solve_pt_from_rho_h(pure, rho_kg_m3, h_j_per_kg, t_hint_k);

        match result {
            Ok((_t_k, p_pa)) => {
                use uom::si::pressure::pascal;
                Some(Ok(Pressure::new::<pascal>(p_pa)))
            }
            Err(e) => Some(Err(e)),
        }
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
