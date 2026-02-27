//! Thermodynamic surrogate models for transient robustness.
//!
//! This module provides temporary, local surrogate models that are used When real-fluid
//! property evaluation (CoolProp) fails during transient simulation. The surrogate allows the
//! simulation to progress through awkward thermodynamic regions where the real-fluid model
//! becomes numerically invalid.
//!
//! The fallback policy:
//! 1. Try real-fluid evaluation (CoolProp via StateInput::PT or StateInput::PH)
//! 2. If it fails, use a temporary local surrogate based on the last valid state
//! 3. When possible, return to real-fluid evaluation automatically
//!
//! The surrogate is NOT a general replacement for CoolProp. It should only be used for
//! small, localized trial steps where real-fluid evaluation would fail.

/// A frozen-thermodynamic-property surrogate model for robustness during transient events.
///
/// This model is built from a known-valid real-fluid state and provides simple,
/// approximately-correct property predictions in a small neighborhood around that state.
///
/// The model uses:
/// - **Frozen specific heat capacity** (cp) from the reference state
/// - **Ideal gas or linearized pressure-enthalpy relationship** for derivatives
/// - Composition is implicitly inherited from the reference state
#[derive(Debug, Clone)]
pub struct FrozenPropertySurrogate {
    /// Reference pressure [Pa], where the surrogate was created
    pub ref_pressure: f64,
    /// Reference temperature [K], where the surrogate was created
    pub ref_temperature: f64,
    /// Reference enthalpy [J/kg], where the surrogate was created
    pub ref_enthalpy: f64,
    /// Reference density [kg/m³], where the surrogate was created
    pub ref_density: f64,
    /// Frozen specific heat at constant pressure [J/(kg·K)] from the reference state
    pub cp_frozen: f64,
    /// Approximate molar mass [kg/kmol] for gas law approximation
    pub molar_mass: f64,
    /// Universal gas constant [J/(kmol·K)]
    const_r_universal: f64,
}

impl FrozenPropertySurrogate {
    /// Gas constant value [J/(kmol·K)]
    const R_UNIVERSAL: f64 = 8314.462618;

    /// Create a new surrogate model from a reference state.
    ///
    /// Parameters:
    /// - `ref_p_pa`: Reference pressure [Pa]
    /// - `ref_t_k`: Reference temperature [K]
    /// - `ref_h`: Reference enthalpy [J/kg]
    /// - `ref_rho`: Reference density [kg/m³]
    /// - `cp_frozen`: Specific heat capacity at constant pressure [J/(kg·K)] from reference state
    /// - `molar_mass_kg_kmol`: Approximate molar mass [kg/kmol] (e.g., 28.014 for N₂)
    pub fn new(
        ref_p_pa: f64,
        ref_t_k: f64,
        ref_h: f64,
        ref_rho: f64,
        cp_frozen: f64,
        molar_mass_kg_kmol: f64,
    ) -> Self {
        Self {
            ref_pressure: ref_p_pa,
            ref_temperature: ref_t_k,
            ref_enthalpy: ref_h,
            ref_density: ref_rho,
            cp_frozen,
            molar_mass: molar_mass_kg_kmol,
            const_r_universal: Self::R_UNIVERSAL,
        }
    }

    /// Estimate enthalpy at a new temperature, assuming constant cp.
    ///
    /// ```text
    /// h(T) ≈ h_ref + cp_frozen * (T - T_ref)
    /// ```
    pub fn estimate_enthalpy_at_t(&self, t_k: f64) -> f64 {
        self.ref_enthalpy + self.cp_frozen * (t_k - self.ref_temperature)
    }

    /// Estimate temperature from enthalpy, assuming constant cp.
    ///
    /// ```text
    /// T(h) ≈ T_ref + (h - h_ref) / cp_frozen
    /// ```
    pub fn estimate_temperature_from_h(&self, h: f64) -> f64 {
        if self.cp_frozen.abs() < 1e-6 {
            return self.ref_temperature;
        }
        self.ref_temperature + (h - self.ref_enthalpy) / self.cp_frozen
    }

    /// Estimate density from pressure, assuming ideal gas with frozen molar mass.
    ///
    /// Uses the ideal gas law with specific gas constant R_specific = R_universal / M:
    ///
    /// ```text
    /// ρ ≈ P / (R_specific * T)
    /// ```
    ///
    /// The temperature is estimated from the enthalpy using constant cp.
    pub fn estimate_density_from_ph(&self, p_pa: f64, h: f64) -> f64 {
        if self.molar_mass < 1e-6 || self.cp_frozen.abs() < 1e-6 {
            return self.ref_density;
        }

        let t_est = self.estimate_temperature_from_h(h);
        let r_specific = self.const_r_universal / self.molar_mass;

        if t_est > 0.0 && p_pa > 0.0 {
            p_pa / (r_specific * t_est)
        } else {
            self.ref_density
        }
    }

    /// Estimate pressure from density and temperature using ideal gas law.
    ///
    /// ```text
    /// P ≈ ρ * R_specific * T
    /// ```
    pub fn estimate_pressure_from_rhot(&self, rho: f64, t_k: f64) -> f64 {
        if self.molar_mass < 1e-6 {
            return self.ref_pressure;
        }

        let r_specific = self.const_r_universal / self.molar_mass;
        rho * r_specific * t_k
    }

    /// Estimate pressure from density and enthalpy using frozen cp + ideal gas approximation.
    ///
    /// This combines two steps:
    /// 1. Estimate temperature from enthalpy: `T ≈ T_ref + (h - h_ref) / cp`
    /// 2. Estimate pressure from density and temperature: `P ≈ ρ * R_specific * T`
    ///
    /// This is the key method for fallback reconstruction of control volume boundary conditions
    /// when the real-fluid backend (CoolProp) rejects a (P, h) pair.
    pub fn estimate_pressure_from_rho_h(&self, rho: f64, h: f64) -> f64 {
        let t_est = self.estimate_temperature_from_h(h);
        self.estimate_pressure_from_rhot(rho, t_est)
    }

    /// Check if a proposed state is "close enough" to the reference to use this surrogate.
    ///
    /// Returns `true` if the state is within reasonable bounds for the surrogate to be
    /// approximately valid. For example, if pressure or temperature are very far from
    /// the reference, the surrogate predictions become inaccurate.
    ///
    /// Current heuristic: Allow up to 50% relative change in pressure or temperature.
    pub fn is_in_valid_range(&self, p_pa: f64, t_k: f64) -> bool {
        if p_pa <= 0.0 || t_k <= 0.0 {
            return false;
        }

        let pressure_ratio = p_pa / self.ref_pressure;
        let temperature_ratio = t_k / self.ref_temperature;

        // Consider the state valid if ratios are in [0.5, 2.0]
        (0.5..=2.0).contains(&pressure_ratio) && (0.5..=2.0).contains(&temperature_ratio)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_surrogate_creation() {
        let surr = FrozenPropertySurrogate::new(
            3500000.0, // P_pa
            300.0,     // T_k
            310755.0,  // h_ref (Nitrogen at 3.5 MPa, 300K from CoolProp)
            39.45,     // rho_kg_m3
            921.0,     // cp_frozen [J/(kg·K)]
            28.014,    // molar_mass_N2
        );
        assert!(surr.ref_pressure > 0.0);
        assert!(surr.cp_frozen > 0.0);
    }

    #[test]
    fn test_enthalpy_from_temperature() {
        let surr = FrozenPropertySurrogate::new(3500000.0, 300.0, 310755.0, 39.45, 921.0, 28.014);

        let h_at_ref = surr.estimate_enthalpy_at_t(300.0);
        assert!((h_at_ref - 310755.0).abs() < 1.0);

        let h_at_310 = surr.estimate_enthalpy_at_t(310.0);
        let expected = 310755.0 + 921.0 * 10.0;
        assert!((h_at_310 - expected).abs() < 1.0);
    }

    #[test]
    fn test_temperature_from_enthalpy() {
        let surr = FrozenPropertySurrogate::new(3500000.0, 300.0, 310755.0, 39.45, 921.0, 28.014);

        let t_at_ref = surr.estimate_temperature_from_h(310755.0);
        assert!((t_at_ref - 300.0).abs() < 0.1);
    }

    #[test]
    fn test_valid_range_check() {
        let surr = FrozenPropertySurrogate::new(3500000.0, 300.0, 310755.0, 39.45, 921.0, 28.014);

        // At reference point
        assert!(surr.is_in_valid_range(3500000.0, 300.0));

        // Within 50% range
        assert!(surr.is_in_valid_range(3500000.0 * 1.4, 300.0 * 1.4));

        // Outside range
        assert!(!surr.is_in_valid_range(3500000.0 * 2.5, 300.0));
        assert!(!surr.is_in_valid_range(3500000.0, 300.0 * 0.3));
    }

    #[test]
    fn test_pressure_from_rho_h() {
        let surr = FrozenPropertySurrogate::new(3500000.0, 300.0, 310755.0, 39.45, 921.0, 28.014);

        // At reference state: rho=39.45 kg/m³, h=310755 J/kg should give P≈3.5 MPa
        let p_est = surr.estimate_pressure_from_rho_h(39.45, 310755.0);
        let rel_error = (p_est - 3500000.0).abs() / 3500000.0;
        assert!(
            rel_error < 0.02,
            "Pressure estimate at reference state should be within 2%"
        );

        // At higher enthalpy: h increases by cp*dT, so T increases, P increases proportionally
        let h_plus_10k = 310755.0 + 921.0 * 10.0; // T → 310K
        let p_est_higher = surr.estimate_pressure_from_rho_h(39.45, h_plus_10k);
        let expected_p = 3500000.0 * (310.0 / 300.0);
        assert!(
            (p_est_higher - expected_p).abs() / expected_p < 0.01,
            "Pressure should scale with temperature under ideal gas approximation"
        );
    }
}
