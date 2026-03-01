//! Sweep execution for generating fluid property data across parameter ranges.
//!
//! This module connects the sweep definition logic with the fluid calculator to
//! produce arrays of computed properties suitable for plotting and analysis.

use crate::calculator::{EquilibriumState, FluidInputPair, compute_equilibrium_state};
use crate::model::FluidModel;
use crate::species::Species;
use crate::sweeps::{SweepDefinition, SweepType};
use crate::units::Quantity;
use std::fmt;

/// Error in sweep execution.
#[derive(Debug, Clone)]
pub enum SweepError {
    /// Invalid sweep configuration
    InvalidConfiguration(String),
    /// Property computation failed
    ComputationFailed { point_index: usize, error: String },
    /// Too many computation failures
    TooManyFailures { successful: usize, failed: usize },
}

impl fmt::Display for SweepError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::InvalidConfiguration(msg) => write!(f, "Invalid configuration: {}", msg),
            Self::ComputationFailed { point_index, error } => {
                write!(f, "Computation failed at point {}: {}", point_index, error)
            }
            Self::TooManyFailures {
                successful,
                failed,
            } => {
                write!(
                    f,
                    "Too many failures ({} succeeded, {} failed)",
                    successful, failed
                )
            }
        }
    }
}

impl std::error::Error for SweepError {}

/// Result of a fluid property sweep.
#[derive(Debug, Clone)]
pub struct SweepResult {
    /// Species that was swept
    pub species: Species,
    /// Input pair used for calculations
    pub input_pair: FluidInputPair,
    /// Independent variable values (the sweep parameter)
    pub independent_values: Vec<f64>,
    /// Computed equilibrium states (may have None entries for failed points)
    pub states: Vec<Option<EquilibriumState>>,
    /// Number of successful computations
    pub num_successful: usize,
    /// Number of failed computations
    pub num_failed: usize,
}

impl SweepResult {
    /// Get pressure array (excluding failed points)
    pub fn pressure_pa(&self) -> Vec<f64> {
        self.states
            .iter()
            .filter_map(|s| s.as_ref().map(|state| state.pressure_pa()))
            .collect()
    }

    /// Get temperature array (excluding failed points)
    pub fn temperature_k(&self) -> Vec<f64> {
        self.states
            .iter()
            .filter_map(|s| s.as_ref().map(|state| state.temperature_k()))
            .collect()
    }

    /// Get density array (excluding failed points)
    pub fn density_kg_m3(&self) -> Vec<f64> {
        self.states
            .iter()
            .filter_map(|s| s.as_ref().map(|state| state.density_kg_m3()))
            .collect()
    }

    /// Get enthalpy array (excluding failed points)
    pub fn enthalpy_j_per_kg(&self) -> Vec<f64> {
        self.states
            .iter()
            .filter_map(|s| s.as_ref().map(|state| state.enthalpy_j_per_kg))
            .collect()
    }

    /// Get entropy array (excluding failed points)
    pub fn entropy_j_per_kg_k(&self) -> Vec<f64> {
        self.states
            .iter()
            .filter_map(|s| s.as_ref().map(|state| state.entropy_j_per_kg_k))
            .collect()
    }

    /// Get independent values corresponding to successful states
    pub fn successful_independent_values(&self) -> Vec<f64> {
        self.independent_values
            .iter()
            .zip(&self.states)
            .filter_map(|(val, state)| state.as_ref().map(|_| *val))
            .collect()
    }
}

/// Execute a temperature sweep at fixed pressure.
///
/// # Arguments
///
/// - `model`: Fluid property model
/// - `species`: Fluid species
/// - `sweep_def`: Temperature sweep definition
/// - `fixed_pressure_pa`: Fixed pressure in Pa
///
/// # Returns
///
/// SweepResult containing computed states across the temperature range
pub fn execute_temperature_sweep_at_pressure<M: FluidModel>(
    model: &M,
    species: Species,
    sweep_def: &SweepDefinition,
    fixed_pressure_pa: f64,
) -> Result<SweepResult, SweepError> {
    if sweep_def.quantity != Quantity::Temperature {
        return Err(SweepError::InvalidConfiguration(
            "Sweep definition must be for Temperature quantity".to_string(),
        ));
    }

    let temperatures = sweep_def.generate_points();
    let mut states = Vec::with_capacity(temperatures.len());
    let mut num_successful = 0;
    let mut num_failed = 0;

    for temp_k in &temperatures {
        match compute_equilibrium_state(
            model,
            species,
            FluidInputPair::PT,
            fixed_pressure_pa,
            *temp_k,
        ) {
            Ok(state) => {
                states.push(Some(state));
                num_successful += 1;
            }
            Err(_) => {
                states.push(None);
                num_failed += 1;
            }
        }
    }

    Ok(SweepResult {
        species,
        input_pair: FluidInputPair::PT,
        independent_values: temperatures,
        states,
        num_successful,
        num_failed,
    })
}

/// Execute a pressure sweep at fixed temperature.
///
/// # Arguments
///
/// - `model`: Fluid property model
/// - `species`: Fluid species
/// - `sweep_def`: Pressure sweep definition
/// - `fixed_temperature_k`: Fixed temperature in K
///
/// # Returns
///
/// SweepResult containing computed states across the pressure range
pub fn execute_pressure_sweep_at_temperature<M: FluidModel>(
    model: &M,
    species: Species,
    sweep_def: &SweepDefinition,
    fixed_temperature_k: f64,
) -> Result<SweepResult, SweepError> {
    if sweep_def.quantity != Quantity::Pressure {
        return Err(SweepError::InvalidConfiguration(
            "Sweep definition must be for Pressure quantity".to_string(),
        ));
    }

    let pressures = sweep_def.generate_points();
    let mut states = Vec::with_capacity(pressures.len());
    let mut num_successful = 0;
    let mut num_failed = 0;

    for pressure_pa in &pressures {
        match compute_equilibrium_state(
            model,
            species,
            FluidInputPair::PT,
            *pressure_pa,
            fixed_temperature_k,
        ) {
            Ok(state) => {
                states.push(Some(state));
                num_successful += 1;
            }
            Err(_) => {
                states.push(None);
                num_failed += 1;
            }
        }
    }

    Ok(SweepResult {
        species,
        input_pair: FluidInputPair::PT,
        independent_values: pressures,
        states,
        num_successful,
        num_failed,
    })
}

/// Execute a generic 2D property sweep (e.g., P-T, P-H, Rho-H, P-S).
///
/// # Arguments
///
/// - `model`: Fluid property model
/// - `species`: Fluid species  
/// - `input_pair`: Input pair to use
/// - `sweep_def_1`: Sweep definition for first input
/// - `fixed_value_2`: Fixed value for second input (SI units)
///
/// # Returns
///
/// SweepResult containing computed states
pub fn execute_generic_sweep<M: FluidModel>(
    model: &M,
    species: Species,
    input_pair: FluidInputPair,
    sweep_def_1: &SweepDefinition,
    fixed_value_2: f64,
) -> Result<SweepResult, SweepError> {
    let values_1 = sweep_def_1.generate_points();
    let mut states = Vec::with_capacity(values_1.len());
    let mut num_successful = 0;
    let mut num_failed = 0;

    for value_1 in &values_1 {
        match compute_equilibrium_state(model, species, input_pair, *value_1, fixed_value_2) {
            Ok(state) => {
                states.push(Some(state));
                num_successful += 1;
            }
            Err(_) => {
                states.push(None);
                num_failed += 1;
            }
        }
    }

    Ok(SweepResult {
        species,
        input_pair,
        independent_values: values_1,
        states,
        num_successful,
        num_failed,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::CoolPropModel;

    #[test]
    fn test_temperature_sweep_nitrogen() {
        let model = CoolPropModel::new();
        let species = Species::N2;

        let sweep_def = SweepDefinition {
            quantity: Quantity::Temperature,
            start_si: 250.0,
            start_raw: "250K".to_string(),
            end_si: 350.0,
            end_raw: "350K".to_string(),
            num_points: 10,
            sweep_type: SweepType::Linear,
        };

        let result =
            execute_temperature_sweep_at_pressure(&model, species, &sweep_def, 101_325.0)
                .unwrap();

        assert_eq!(result.independent_values.len(), 10);
        assert!(result.num_successful > 0);
        assert!(result.pressure_pa().iter().all(|&p| (p - 101_325.0).abs() < 1.0));
    }

    #[test]
    fn test_pressure_sweep_nitrogen() {
        let model = CoolPropModel::new();
        let species = Species::N2;

        let sweep_def = SweepDefinition {
            quantity: Quantity::Pressure,
            start_si: 1e5,
            start_raw: "1bar".to_string(),
            end_si: 5e5,
            end_raw: "5bar".to_string(),
            num_points: 5,
            sweep_type: SweepType::Linear,
        };

        let result =
            execute_pressure_sweep_at_temperature(&model, species, &sweep_def, 300.0).unwrap();

        assert_eq!(result.independent_values.len(), 5);
        assert!(result.num_successful > 0);
        assert!(result
            .temperature_k()
            .iter()
            .all(|&t| (t - 300.0).abs() < 1.0));
    }

    #[test]
    fn test_generic_sweep() {
        let model = CoolPropModel::new();
        let species = Species::N2;

        let sweep_def = SweepDefinition {
            quantity: Quantity::Pressure,
            start_si: 1e5,
            start_raw: "1bar".to_string(),
            end_si: 3e5,
            end_raw: "3bar".to_string(),
            num_points: 3,
            sweep_type: SweepType::Linear,
        };

        let result =
            execute_generic_sweep(&model, species, FluidInputPair::PT, &sweep_def, 300.0)
                .unwrap();

        assert_eq!(result.num_successful, 3);
        assert_eq!(result.temperature_k().len(), 3);
    }
}
