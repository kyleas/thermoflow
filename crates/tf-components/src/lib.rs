//! tf-components: component library for thermodynamic systems.
//!
//! Provides models for common flow elements:
//! - Orifices with compressible/incompressible flow
//! - Valves with position control
//! - Pipes with friction
//!
//! All components implement the `TwoPortComponent` trait and are deterministic
//! functions of state and parameters, suitable for network solving.
//!
//! # Example
//!
//! ```no_run
//! use tf_components::{Orifice, TwoPortComponent, PortStates};
//! use tf_fluids::{CoolPropModel, FluidModel, Composition, Species, StateInput};
//! use tf_core::units::{pa, k};
//! use uom::si::area::square_meter;
//!
//! let model = CoolPropModel::new();
//! let comp = Composition::pure(Species::N2);
//!
//! let state_in = model.state(
//!     StateInput::PT { p: pa(200_000.0), t: k(300.0) },
//!     comp.clone()
//! ).unwrap();
//!
//! let state_out = model.state(
//!     StateInput::PT { p: pa(100_000.0), t: k(300.0) },
//!     comp
//! ).unwrap();
//!
//! let orifice = Orifice::new(
//!     "test".into(),
//!     0.7,
//!     tf_core::units::Area::new::<square_meter>(0.001)
//! );
//!
//! let ports = PortStates {
//!     inlet: &state_in,
//!     outlet: &state_out,
//! };
//!
//! let mdot = orifice.mdot(&model, ports).unwrap();
//! println!("Mass flow: {} kg/s", mdot.value);
//! ```

pub mod common;
pub mod error;
pub mod orifice;
pub mod pipe;
pub mod pump;
pub mod traits;
pub mod turbine;
pub mod valve;

// Re-exports
pub use error::{ComponentError, ComponentResult};
pub use orifice::Orifice;
pub use pipe::Pipe;
pub use pump::Pump;
pub use traits::{PortStates, TwoPortComponent};
pub use turbine::Turbine;
pub use valve::{Valve, ValveLaw};
