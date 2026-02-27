//! Transient simulation framework for thermoflow networks.
//!
//! Provides:
//! - DAE-style time integration with nested steady algebraic solver
//! - Control volume lumped-parameter storage dynamics
//! - Valve actuator model with first-order dynamics
//! - Fixed-step RK4 integrator
//! - Tank blowdown example

pub mod actuator;
pub mod control_volume;
pub mod error;
pub mod integrator;
pub mod junction_thermal;
pub mod model;
pub mod shaft;
pub mod sim;

// Internal modules
mod events;

// Re-exports for public API
pub use actuator::{ActuatorState, FirstOrderActuator};
pub use control_volume::{ControlVolume, ControlVolumeState};
pub use error::{SimError, SimResult};
pub use integrator::{ForwardEuler, Integrator, RK4};
pub use model::TransientModel;
pub use shaft::{Shaft, ShaftState};
pub use sim::{IntegratorType, SimOptions, SimProgress, SimRecord, run_sim, run_sim_with_progress};
