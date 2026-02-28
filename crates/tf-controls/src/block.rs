//! Signal block types and abstractions.
//!
//! Signal blocks are the fundamental processing elements in the control graph:
//! - **Sources**: Generate signals (constants, measured variables, setpoints)
//! - **Processors**: Transform signals (controllers, math operations)
//! - **Sinks**: Consume signals (actuator commands, outputs)

use serde::{Deserialize, Serialize};

use crate::controller::{PIController, PIDController};
use crate::measured::MeasuredVariableRef;
use crate::signal::SignalValue;

/// Signal block represents a processing element in the control graph.
#[derive(Debug, Clone, PartialEq)]
pub struct SignalBlock {
    /// Block type and configuration.
    pub kind: SignalBlockKind,
}

impl SignalBlock {
    /// Create a new signal block.
    pub fn new(kind: SignalBlockKind) -> Self {
        Self { kind }
    }

    /// Check if this block is a source (generates signals).
    pub fn is_source(&self) -> bool {
        matches!(
            self.kind,
            SignalBlockKind::Constant { .. } | SignalBlockKind::MeasuredVariable { .. }
        )
    }

    /// Check if this block is a sink (consumes signals).
    pub fn is_sink(&self) -> bool {
        matches!(self.kind, SignalBlockKind::ActuatorCommand { .. })
    }

    /// Check if this block is a processor (transforms signals).
    pub fn is_processor(&self) -> bool {
        matches!(
            self.kind,
            SignalBlockKind::PIController { .. } | SignalBlockKind::PIDController { .. }
        )
    }

    /// Get the number of inputs this block expects.
    pub fn num_inputs(&self) -> usize {
        match &self.kind {
            SignalBlockKind::Constant { .. } => 0,
            SignalBlockKind::MeasuredVariable { .. } => 0,
            SignalBlockKind::PIController { .. } => 2, // process variable, setpoint
            SignalBlockKind::PIDController { .. } => 2, // process variable, setpoint
            SignalBlockKind::ActuatorCommand { .. } => 1,
        }
    }

    /// Get the number of outputs this block produces.
    pub fn num_outputs(&self) -> usize {
        match &self.kind {
            SignalBlockKind::Constant { .. } => 1,
            SignalBlockKind::MeasuredVariable { .. } => 1,
            SignalBlockKind::PIController { .. } => 1,
            SignalBlockKind::PIDController { .. } => 1,
            SignalBlockKind::ActuatorCommand { .. } => 0,
        }
    }
}

/// Signal block kind defines the type and parameters of a block.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type")]
pub enum SignalBlockKind {
    /// Constant signal source.
    Constant {
        /// Constant value.
        value: f64,
    },

    /// Measured variable from fluid system.
    MeasuredVariable {
        /// Reference to the measured quantity.
        reference: MeasuredVariableRef,
    },

    /// Proportional-Integral controller.
    PIController {
        /// Controller configuration.
        controller: PIController,
    },

    /// Proportional-Integral-Derivative controller.
    PIDController {
        /// Controller configuration.
        controller: PIDController,
    },

    /// Actuator command sink.
    ActuatorCommand {
        /// Target actuator identifier.
        actuator_id: String,
    },
}

/// Signal source trait for blocks that generate signals.
pub trait SignalSource {
    /// Compute the output signal value.
    fn output(&self) -> SignalValue;
}

/// Signal processor trait for blocks that transform signals.
pub trait SignalProcessor {
    /// Process input signals and compute output.
    fn process(&mut self, inputs: &[SignalValue]) -> SignalValue;
}

/// Signal sink trait for blocks that consume signals.
pub trait SignalSink {
    /// Consume an input signal.
    fn consume(&mut self, input: SignalValue);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn constant_block() {
        let block = SignalBlock::new(SignalBlockKind::Constant { value: 42.0 });
        assert!(block.is_source());
        assert!(!block.is_sink());
        assert!(!block.is_processor());
        assert_eq!(block.num_inputs(), 0);
        assert_eq!(block.num_outputs(), 1);
    }

    #[test]
    fn actuator_command_block() {
        let block = SignalBlock::new(SignalBlockKind::ActuatorCommand {
            actuator_id: "valve1".to_string(),
        });
        assert!(!block.is_source());
        assert!(block.is_sink());
        assert!(!block.is_processor());
        assert_eq!(block.num_inputs(), 1);
        assert_eq!(block.num_outputs(), 0);
    }
}
