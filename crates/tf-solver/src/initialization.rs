//! Initialization strategy abstraction for solver startup behavior.
//!
//! This module provides a formal abstraction for controlling how the solver
//! obtains consistent starting states. Different strategies govern:
//! - Initial guess generation
//! - Startup regularization intensity
//! - Enthalpy clamping aggressiveness
//! - Weak-flow handling at junctions
//!
//! The strategy layer makes initialization behavior explicit, configurable,
//! and visible in diagnostics, improving robustness especially for multi-CV
//! and storage-rich transient systems.

use crate::newton::NewtonConfig;

/// Initialization strategy for solver startup.
///
/// Governs how the solver obtains consistent initial states for both
/// steady-state and transient simulations.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum InitializationStrategy {
    /// Strict initialization with minimal special treatment.
    ///
    /// - Direct use of configured initial states
    /// - Standard Newton solver parameters
    /// - No aggressive regularization
    /// - Preferred for well-conditioned systems
    ///
    /// Best for: Simple single-CV systems, well-bounded problems
    Strict,

    /// Relaxed initialization with startup aids.
    ///
    /// - Conservative initial guess propagation
    /// - Weak-flow junction regularization enabled
    /// - Enthalpy clamping during startup iterations
    /// - Surrogate pre-population for transients
    /// - Deferred first-snapshot anchoring
    ///
    /// Best for: Multi-CV networks, storage-rich transients,
    /// systems with complex flow topology
    Relaxed,
}

impl InitializationStrategy {
    /// Convert strategy to human-readable name for diagnostics.
    pub fn as_str(&self) -> &'static str {
        match self {
            InitializationStrategy::Strict => "Strict",
            InitializationStrategy::Relaxed => "Relaxed",
        }
    }

    /// Generate Newton solver configuration appropriate for this strategy.
    ///
    /// Strict: Standard parameters, minimal regularization
    /// Relaxed: Aggressive weak-flow regularization, tighter clamping
    pub fn to_newton_config(&self) -> NewtonConfig {
        match self {
            InitializationStrategy::Strict => NewtonConfig {
                abs_tol: 1.0e-6,
                rel_tol: 1.0e-6,
                min_pressure: 1000.0,
                line_search_beta: 0.5,
                max_line_search_iters: 20,
                max_iterations: 200,
                enthalpy_delta_abs: f64::INFINITY,
                enthalpy_delta_rel: f64::INFINITY,
                enthalpy_total_abs: 5e5,
                enthalpy_total_rel: 0.5,
                enthalpy_ref: 3e5,
                // Minimal weak-flow regularization
                weak_flow_mdot: 5.0e-4,
                weak_flow_enthalpy_scale: 0.5,
            },
            InitializationStrategy::Relaxed => NewtonConfig {
                abs_tol: 1.0e-6,
                rel_tol: 1.0e-6,
                min_pressure: 1000.0,
                line_search_beta: 0.5,
                max_line_search_iters: 20,
                max_iterations: 200,
                enthalpy_delta_abs: f64::INFINITY,
                enthalpy_delta_rel: f64::INFINITY,
                enthalpy_total_abs: 3e5,
                enthalpy_total_rel: 0.3,
                enthalpy_ref: 3e5,
                // Aggressive weak-flow regularization for startup robustness
                weak_flow_mdot: 1.0e-3,
                weak_flow_enthalpy_scale: 0.25,
            },
        }
    }

    /// Whether this strategy uses aggressive startup aids.
    pub fn uses_startup_aids(&self) -> bool {
        matches!(self, InitializationStrategy::Relaxed)
    }
}

impl Default for InitializationStrategy {
    /// Default to Strict for backward compatibility and determinism.
    fn default() -> Self {
        InitializationStrategy::Strict
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn strict_has_minimal_regularization() {
        let config = InitializationStrategy::Strict.to_newton_config();
        assert!(config.weak_flow_mdot < 1.0e-3);
        assert!(config.weak_flow_enthalpy_scale > 0.4);
    }

    #[test]
    fn relaxed_has_aggressive_regularization() {
        let config = InitializationStrategy::Relaxed.to_newton_config();
        assert!(config.weak_flow_mdot >= 1.0e-3);
        assert!(config.weak_flow_enthalpy_scale <= 0.25);
    }

    #[test]
    fn relaxed_has_tighter_enthalpy_bounds() {
        let strict = InitializationStrategy::Strict.to_newton_config();
        let relaxed = InitializationStrategy::Relaxed.to_newton_config();
        assert!(relaxed.enthalpy_total_abs < strict.enthalpy_total_abs);
        assert!(relaxed.enthalpy_total_rel < strict.enthalpy_total_rel);
    }

    #[test]
    fn strategy_names_are_stable() {
        assert_eq!(InitializationStrategy::Strict.as_str(), "Strict");
        assert_eq!(InitializationStrategy::Relaxed.as_str(), "Relaxed");
    }

    #[test]
    fn default_is_strict() {
        assert_eq!(
            InitializationStrategy::default(),
            InitializationStrategy::Strict
        );
    }

    #[test]
    fn uses_startup_aids_only_for_relaxed() {
        assert!(!InitializationStrategy::Strict.uses_startup_aids());
        assert!(InitializationStrategy::Relaxed.uses_startup_aids());
    }
}
