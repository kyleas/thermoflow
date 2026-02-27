//! Optional thermodynamic state creation policy for solvers.
//!
//! Provides pluggable fallback support for node state creation when the primary
//! real-fluid backend (CoolProp) rejects an invalid (P, h) combination.
//!
//! Default behavior remains strict: fail if the real-fluid state is invalid.
//! With a fallback policy, the solver can recover using approximate surrogates.

use tf_core::units::Pressure;
use tf_fluids::{Composition, FluidModel, ThermoState};

/// Result of attempting to create a thermodynamic state for a node.
#[derive(Debug, Clone)]
pub enum StateCreationResult {
    /// Successfully created a real-fluid state (primary path).
    RealFluid(ThermoState),
    /// Real-fluid failed, but fallback policy provided an approximate state.
    /// Fallback is used when real-fluid (P, h) is outside CoolProp's valid envelope.
    Fallback(ThermoState),
}

impl StateCreationResult {
    /// Extract the ThermoState regardless of method.
    pub fn into_state(self) -> ThermoState {
        match self {
            StateCreationResult::RealFluid(s) => s,
            StateCreationResult::Fallback(s) => s,
        }
    }

    /// Check if this state used fallback.
    pub fn used_fallback(&self) -> bool {
        matches!(self, StateCreationResult::Fallback(_))
    }
}

/// Optional policy for creating node thermodynamic states with fallback support.
///
/// Implementations can provide:
/// - Real-fluid state creation (primary, delegates to FluidModel)
/// - Optional fallback when real-fluid fails (custom surrogate approximations)
///
/// This abstraction allows transient/continuation solves to recover from invalid
/// node (P, h) states by using local approximations, while keeping ordinary
/// steady solves strict and deterministic.
pub trait ThermoStatePolicy: Send + Sync {
    /// Attempt to create a thermodynamic state for a node.
    ///
    /// Tries real-fluid first. If it fails and fallback is available,
    /// may return a surrogate-based approximate state.
    ///
    /// # Arguments
    /// - `p`: Node pressure
    /// - `h`: Node specific enthalpy
    /// - `composition`: Fluid composition
    /// - `fluid`: Real-fluid model (primary backend)
    /// - `node_id`: Node index (for logging/diagnostics)
    ///
    /// # Returns
    /// - `Ok(StateCreationResult::RealFluid(_))` if real-fluid state is valid
    /// - `Ok(StateCreationResult::Fallback(_))` if real-fluid failed but fallback succeeded
    /// - `Err(...)` if both real-fluid and fallback failed (or fallback not available)
    fn create_state(
        &self,
        p: Pressure,
        h: f64,
        composition: &Composition,
        fluid: &dyn FluidModel,
        node_id: usize,
    ) -> crate::error::SolverResult<StateCreationResult>;

    /// Report fallback usage statistics (e.g., for diagnostics).
    fn fallback_count(&self) -> usize {
        0
    }

    /// Whether the policy can recover from invalid (P, h) states without failing.
    ///
    /// If true, the Newton line search should not reject trial states solely
    /// because the real-fluid backend cannot form a PH state.
    fn allow_invalid_ph(&self) -> bool {
        false
    }
}

/// Strict (default) policy: only use real-fluid, fail on invalid states.
pub struct StrictPolicy;

impl ThermoStatePolicy for StrictPolicy {
    fn create_state(
        &self,
        p: Pressure,
        h: f64,
        composition: &Composition,
        fluid: &dyn FluidModel,
        node_id: usize,
    ) -> crate::error::SolverResult<StateCreationResult> {
        let state = fluid
            .state(tf_fluids::StateInput::PH { p, h }, composition.clone())
            .map_err(|e| crate::error::SolverError::InvalidState {
                what: format!(
                    "Node {} (P={:.1} Pa, h={:.1} J/kg): {}",
                    node_id, p.value, h, e
                ),
            })?;
        Ok(StateCreationResult::RealFluid(state))
    }

    fn allow_invalid_ph(&self) -> bool {
        false
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tf_fluids::CoolPropModel;

    #[test]
    fn strict_policy_succeeds_for_valid_state() {
        let policy = StrictPolicy;
        let fluid = CoolPropModel::new();
        let comp = Composition::pure(tf_fluids::Species::N2);
        let p = tf_core::units::pa(100000.0);
        let h = 300000.0;

        // Should succeed for valid state
        let result = policy.create_state(p, h, &comp, &fluid, 0);
        assert!(result.is_ok());
        if let Ok(StateCreationResult::RealFluid(_)) = result {
            // Expected
        } else {
            panic!("Expected RealFluid variant");
        }
    }

    #[test]
    fn strict_policy_fails_for_invalid_state() {
        let policy = StrictPolicy;
        let fluid = CoolPropModel::new();
        let comp = Composition::pure(tf_fluids::Species::N2);
        let p = tf_core::units::pa(101325.0);
        let h = 2313614.8; // Way too high for 1 atm

        // Should fail for invalid state
        let result = policy.create_state(p, h, &comp, &fluid, 1);
        assert!(result.is_err());
    }
}
