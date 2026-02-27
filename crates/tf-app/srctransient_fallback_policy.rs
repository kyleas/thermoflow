//! Transient fallback policy for node state creation.
//!
//! Uses surrogate models to recover when CoolProp rejects invalid (P, h) pairs
//! during transient continuation substeps.

use tf_solver::thermo_policy::{ThermoStatePolicy, StateCreationResult};
use tf_fluids::{Composition, FluidModel, ThermoState, FrozenPropertySurrogate};
use tf_core::units::Pressure;
use std::sync::{Arc, Mutex};

/// Transient fallback policy with surrogate models and diagnostics.
pub struct TransientFallbackPolicy {
    /// Surrogate models per node (index matches node id)
    surrogates: Vec<Option<Arc<FrozenPropertySurrogate>>>,
    /// Count of fallback uses for diagnostics
    fallback_count: Arc<Mutex<usize>>,
}

impl TransientFallbackPolicy {
    /// Create a new transient fallback policy.
    pub fn new(num_nodes: usize) -> Self {
        Self {
            surrogates: vec![None; num_nodes],
            fallback_count: Arc::new(Mutex::new(0)),
        }
    }

    /// Update surrogate for a node from a valid (P, T, h, rho) state.
    pub fn update_surrogate(
        &mut self,
        node_id: usize,
        p: Pressure,
        t: f64,
        h: f64,
        rho: f64,
        cp: f64,
        molar_mass: f64,
    ) {
        if node_id < self.surrogates.len() {
            let surrogate = FrozenPropertySurrogate::new(
                p.value,
                t,
                h,
                rho,
                cp,
                molar_mass,
            );
            self.surrogates[node_id] = Some(Arc::new(surrogate));
        }
    }

    /// Get fallback usage count for diagnostics.
    pub fn fallback_count(&self) -> usize {
        *self.fallback_count.lock().unwrap()
    }

    /// Reset fallback counter.
    pub fn reset_fallback_count(&self) {
        *self.fallback_count.lock().unwrap() = 0;
    }
}

impl ThermoStatePolicy for TransientFallbackPolicy {
    fn create_state(
        &self,
        p: Pressure,
        h: f64,
        composition: &Composition,
        fluid: &dyn FluidModel,
        node_id: usize,
    ) -> tf_solver::SolverResult<StateCreationResult> {
        // Try real-fluid first
        match fluid.state(tf_fluids::StateInput::PH { p, h }, composition.clone()) {
            Ok(state) => Ok(StateCreationResult::RealFluid(state)),
            Err(_) => {
                // Real-fluid failed: try fallback surrogate
                if let Some(Some(surrogate)) = self.surrogates.get(node_id) {
                    // Use surrogate to estimate temperature from h
                    let t_est = surrogate.estimate_temperature_from_h(h);
                    
                    // Create a PT state using the estimated temperature
                    match ThermoState::from_pt(
                        p,
                        tf_core::units::k(t_est),
                        composition.clone(),
                    ) {
                        Ok(state) => {
                            *self.fallback_count.lock().unwrap() += 1;
                            eprintln!("[FALLBACK] Node {} using surrogate (P={:.1} Pa, h={:.1} J/kg, T_est={:.1} K)", 
                                     node_id, p.value, h, t_est);
                            Ok(StateCreationResult::Fallback(state))
                        }
                        Err(e) => {
                            Err(tf_solver::SolverError::InvalidState {
                                what: format!(
                                    "Node {} surrogate fallback failed (P={:.1} Pa, T_est={:.1} K): {}",
                                    node_id, p.value, t_est, e
                                ),
                            })
                        }
                    }
                } else {
                    // No surrogate available: fail with descriptive error
                    Err(tf_solver::SolverError::InvalidState {
                        what: format!(
                            "Node {} CoolProp failed and no surrogate available (P={:.1} Pa, h={:.1} J/kg)",
                            node_id, p.value, h
                        ),
                    })
                }
            }
        }
    }

    fn fallback_count(&self) -> usize {
        *self.fallback_count.lock().unwrap()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tf_fluids::CoolPropModel;

    #[test]
    fn policy_succeeds_with_real_fluid() {
        let policy = TransientFallbackPolicy::new(2);
        let fluid = CoolPropModel::new();
        let comp = Composition::pure(tf_fluids::Species::N2);
        let p = Pressure::new::<tf_core::units::uom::si::pressure::pascal>(100000.0);
        let h = 300000.0;

        let result = policy.create_state(p, h, &comp, &fluid, 0);
        assert!(result.is_ok());
        if let Ok(StateCreationResult::RealFluid(_)) = result {
            // Expected
        } else {
            panic!("Expected RealFluid variant");
        }
    }

    #[test]
    fn policy_uses_fallback_when_available() {
        let mut policy = TransientFallbackPolicy::new(2);
        let fluid = CoolPropModel::new();
        let comp = Composition::pure(tf_fluids::Species::N2);

        // Set up a surrogate for node 1
        policy.update_surrogate(1, Pressure::new::<tf_core::units::uom::si::pressure::pascal>(101325.0), 300.0, 300000.0, 1.2, 1039.0, 28.014);

        // Try an invalid state (high h at low p)
        let p = Pressure::new::<tf_core::units::uom::si::pressure::pascal>(101325.0);
        let h = 2313614.8; // Invalid for 1 atm

        let result = policy.create_state(p, h, &comp, &fluid, 1);
        assert!(result.is_ok());
        if let Ok(StateCreationResult::Fallback(_)) = result {
            // Expected
            assert_eq!(policy.fallback_count(), 1);
        } else {
            panic!("Expected Fallback variant");
        }
    }

    #[test]
    fn policy_fails_without_surrogate() {
        let policy = TransientFallbackPolicy::new(2);
        let fluid = CoolPropModel::new();
        let comp = Composition::pure(tf_fluids::Species::N2);

        // Try an invalid state without a surrogate
        let p = Pressure::new::<tf_core::units::uom::si::pressure::pascal>(101325.0);
        let h = 2313614.8;

        let result = policy.create_state(p, h, &comp, &fluid, 1);
        assert!(result.is_err());
        assert_eq!(policy.fallback_count(), 0);
    }
}
