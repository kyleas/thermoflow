//! Control system and signal graph primitives for Thermoflow.
//!
//! This crate provides a separate signal/control domain that operates alongside
//! the fluid network. The control graph consists of signal sources (measured variables,
//! setpoints), signal processors (controllers), and signal sinks (actuator commands).
//!
//! # Architecture
//!
//! The control system is built on a scalar signal graph where:
//! - Signals are scalar `f64` values
//! - Blocks process signals (sources, processors, sinks)
//! - Controllers operate in sampled/digital mode with configurable update rates
//! - Actuators introduce physical dynamics (lag, rate limiting)
//!
//! # Design Principles
//!
//! - **Separation of Concerns**: Control graph is separate from fluid network
//! - **Backend-First**: All control logic is shared between CLI and GUI
//! - **Physical Realism**: Actuators model real dynamics, controllers are sampled
//! - **Type Safety**: Signal types and block connections are validated

pub mod actuator;
pub mod block;
pub mod controller;
pub mod error;
pub mod graph;
pub mod measured;
pub mod sampled;
pub mod signal;

pub use actuator::{ActuatorState, FirstOrderActuator};
pub use block::{SignalBlock, SignalBlockKind, SignalSink, SignalSource};
pub use controller::{PIController, PIControllerState, PIDController, PIDControllerState};
pub use error::{ControlError, ControlResult};
pub use graph::{ControlGraph, SignalConnection, SignalEdge};
pub use measured::{MeasuredVariable, MeasuredVariableRef};
pub use sampled::{SampleClock, SampleConfig};
pub use signal::{Signal, SignalId, SignalValue};
