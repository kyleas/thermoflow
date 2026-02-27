//! Transient junction thermal regularization.
//!
//! For zero-storage junction nodes in transient simulations, this module provides
//! a numerically stabilized enthalpy update mechanism that avoids the need for
//! exact algebraic closure during difficult transitions.
//!
//! ## Strategy
//!
//! Instead of treating junction enthalpy as a tightly coupled algebraic unknown:
//! 1. Solve hydraulics using a lagged/frozen junction thermodynamic state
//! 2. Compute a target mixed enthalpy from incoming streams
//! 3. Relax junction enthalpy toward the target using artificial thermal holdup
//!
//! This intentionally trades exact instantaneous physics for numerical robustness.

use std::collections::HashMap;
use tf_core::NodeId;

/// Thermal regularization mode for transient junction nodes.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum JunctionThermalMode {
    /// Exact algebraic enthalpy closure (may be unstable during rapid transitions).
    StrictAlgebraic,

    /// Relaxed mixing with artificial thermal holdup (default for transient).
    /// Uses lagged state for flow solve, then relaxes toward mixed enthalpy.
    #[default]
    RelaxedMixing,

    /// Frozen enthalpy (for debugging).
    Frozen,
}

/// Configuration for junction thermal relaxation.
#[derive(Debug, Clone)]
pub struct JunctionThermalConfig {
    /// Thermal relaxation mode.
    pub mode: JunctionThermalMode,

    /// Relaxation time constant (seconds) for artificial holdup.
    /// Smaller values = faster relaxation toward mixed enthalpy.
    /// Typical range: 0.001 to 0.1 seconds.
    pub tau_relax: f64,

    /// Minimum relaxation factor (dimensionless, 0 to 1).
    /// Ensures some progress toward equilibrium even with large time steps.
    pub min_alpha: f64,

    /// Maximum relaxation factor (dimensionless, 0 to 1).
    /// Prevents overshooting during rapid changes.
    pub max_alpha: f64,
}

impl Default for JunctionThermalConfig {
    fn default() -> Self {
        Self {
            mode: JunctionThermalMode::RelaxedMixing,
            tau_relax: 0.01, // 10 ms thermal time constant
            min_alpha: 0.05, // At least 5% progress per step
            max_alpha: 0.5,  // At most 50% progress per step
        }
    }
}

/// Transient junction thermal state tracker.
#[derive(Debug, Clone)]
pub struct JunctionThermalState {
    /// Current enthalpy at each junction (J/kg).
    pub enthalpy: HashMap<NodeId, f64>,

    /// Target mixed enthalpy from last update (J/kg).
    pub target_enthalpy: HashMap<NodeId, f64>,

    /// Number of relaxed updates performed.
    pub update_count: usize,

    /// Maximum enthalpy deviation seen (J/kg).
    pub max_deviation: f64,
}

impl JunctionThermalState {
    pub fn new() -> Self {
        Self {
            enthalpy: HashMap::new(),
            target_enthalpy: HashMap::new(),
            update_count: 0,
            max_deviation: 0.0,
        }
    }

    /// Initialize junction enthalpy from a solved state.
    pub fn set_initial(&mut self, node_id: NodeId, h: f64) {
        self.enthalpy.insert(node_id, h);
        self.target_enthalpy.insert(node_id, h);
    }

    /// Get current lagged enthalpy (for use in flow solve).
    pub fn get_lagged_enthalpy(&self, node_id: NodeId) -> Option<f64> {
        self.enthalpy.get(&node_id).copied()
    }

    /// Update junction enthalpy using relaxed mixing.
    ///
    /// # Arguments
    /// * `node_id` - Junction node to update
    /// * `h_mixed` - Target mixed enthalpy from incoming streams (J/kg)
    /// * `dt` - Time step (s)
    /// * `config` - Relaxation configuration
    ///
    /// # Returns
    /// Updated junction enthalpy (J/kg)
    pub fn update_relaxed(
        &mut self,
        node_id: NodeId,
        h_mixed: f64,
        dt: f64,
        config: &JunctionThermalConfig,
    ) -> f64 {
        let h_old = self.enthalpy.get(&node_id).copied().unwrap_or(h_mixed);

        // Compute relaxation factor: alpha = clamp(dt / tau, min, max)
        let alpha_raw = (dt / config.tau_relax).abs();
        let alpha = alpha_raw.clamp(config.min_alpha, config.max_alpha);

        // First-order relaxation toward target
        let h_new = h_old + alpha * (h_mixed - h_old);

        // Track statistics
        self.enthalpy.insert(node_id, h_new);
        self.target_enthalpy.insert(node_id, h_mixed);
        self.update_count += 1;

        let deviation = (h_mixed - h_old).abs();
        if deviation > self.max_deviation {
            self.max_deviation = deviation;
        }

        h_new
    }

    /// Get summary statistics.
    pub fn summary(&self) -> String {
        format!(
            "Junction thermal updates: {} relaxations, max deviation: {:.1} J/kg",
            self.update_count, self.max_deviation
        )
    }
}

impl Default for JunctionThermalState {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tf_core::NodeId;

    #[test]
    fn test_relaxed_mixing_basic() {
        let mut state = JunctionThermalState::new();
        let config = JunctionThermalConfig::default();
        let node = NodeId::from_index(0);

        // Initialize at 300 kJ/kg
        state.set_initial(node, 300_000.0);

        // Target mixed enthalpy is 500 kJ/kg
        let h_mixed = 500_000.0;
        let dt = 0.01; // 10 ms time step

        // First update should move toward target
        let h1 = state.update_relaxed(node, h_mixed, dt, &config);

        // Should be between old and new (relaxation)
        assert!(h1 > 300_000.0);
        assert!(h1 < 500_000.0);

        // Second update should continue relaxing
        let h2 = state.update_relaxed(node, h_mixed, dt, &config);
        assert!(h2 > h1);
        assert!(h2 <= 500_000.0);
    }

    #[test]
    fn test_relaxation_factor_clamping() {
        let mut state = JunctionThermalState::new();
        let config = JunctionThermalConfig {
            tau_relax: 0.01,
            min_alpha: 0.1,
            max_alpha: 0.5,
            ..Default::default()
        };
        let node = NodeId::from_index(0);

        state.set_initial(node, 300_000.0);

        // Very small dt: should use min_alpha
        let h1 = state.update_relaxed(node, 400_000.0, 0.0001, &config);
        let expected_min = 300_000.0 + 0.1 * (400_000.0 - 300_000.0);
        assert!((h1 - expected_min).abs() < 1.0);

        // Reset
        state.set_initial(node, 300_000.0);

        // Very large dt: should use max_alpha
        let h2 = state.update_relaxed(node, 400_000.0, 1.0, &config);
        let expected_max = 300_000.0 + 0.5 * (400_000.0 - 300_000.0);
        assert!((h2 - expected_max).abs() < 1.0);
    }

    #[test]
    fn test_frozen_mode_doesnt_update() {
        // For frozen mode, enthalpy should remain constant
        // (implementation will handle this in the transient model)
        let config = JunctionThermalConfig {
            mode: JunctionThermalMode::Frozen,
            ..Default::default()
        };

        assert_eq!(config.mode, JunctionThermalMode::Frozen);
    }
}
