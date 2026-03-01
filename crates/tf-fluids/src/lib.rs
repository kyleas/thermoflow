//! tf-fluids: fluid property calculations for thermoflow.
//!
//! Provides:
//! - Chemical species definitions (O2, CH4, H2, etc.)
//! - Composition handling (pure fluids and mixtures)
//! - Thermodynamic state representation
//! - FluidModel trait for property calculations
//! - CoolProp backend for real fluid properties
//!
//! # Architecture
//!
//! This crate defines a stable API (`FluidModel` trait) that isolates the rest of
//! thermoflow from backend dependencies. Currently, CoolProp (via `rfluids`) is the
//! primary backend, but the architecture allows for future additions:
//! - Ideal gas models
//! - CEA (NASA Chemical Equilibrium with Applications) for combustion products
//! - Custom equation-of-state implementations
//!
//! # Example
//!
//! ```no_run
//! use tf_fluids::{CoolPropModel, Composition, FluidModel, Species, StateInput};
//! use tf_core::units::{pa, k};
//!
//! let model = CoolPropModel::new();
//! let comp = Composition::pure(Species::N2);
//! let input = StateInput::PT {
//!     p: pa(101325.0),
//!     t: k(300.0),
//! };
//!
//! let state = model.state(input, comp).unwrap();
//! let rho = model.rho(&state).unwrap();
//! println!("Density: {} kg/m³", rho.value);
//! ```

pub mod calculator;
pub mod catalog;
pub mod composition;
pub mod coolprop;
pub mod error;
pub mod model;
pub mod species;
pub mod state;
pub mod surrogate;
pub mod sweep_executor;
pub mod sweeps;
pub mod units;

// Re-exports for ergonomics
pub use calculator::{EquilibriumState, FluidInputPair, compute_equilibrium_state};
pub use catalog::{
    FluidCatalogEntry, filter_practical_coolprop_catalog, practical_coolprop_catalog,
};
pub use composition::Composition;
pub use coolprop::CoolPropModel;
pub use error::{FluidError, FluidResult};
pub use model::{FluidModel, ThermoPropertyPack};
pub use species::Species;
pub use state::{SpecEnthalpy, SpecEntropy, SpecHeatCapacity, StateInput, ThermoState};
pub use surrogate::FrozenPropertySurrogate;
pub use sweep_executor::{
    SweepError, SweepResult, execute_generic_sweep, execute_pressure_sweep_at_temperature,
    execute_temperature_sweep_at_pressure,
};
pub use sweeps::{SweepDefinition, SweepType};
pub use units::{parse_quantity, Quantity, UnitError, UnitValue};
