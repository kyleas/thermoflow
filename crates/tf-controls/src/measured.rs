//! Measured variable references for control systems.
//!
//! Measured variables extract quantities from the fluid system to be used
//! as inputs to controllers. This module defines the reference types that
//! will be resolved at runtime to actual measurements.

use serde::{Deserialize, Serialize};

/// Reference to a measured quantity in the fluid system.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(tag = "type")]
pub enum MeasuredVariableRef {
    /// Node pressure measurement.
    NodePressure {
        /// Node identifier in the fluid system.
        node_id: String,
    },

    /// Node temperature measurement.
    NodeTemperature {
        /// Node identifier in the fluid system.
        node_id: String,
    },

    /// Node specific enthalpy measurement.
    NodeEnthalpy {
        /// Node identifier in the fluid system.
        node_id: String,
    },

    /// Node density measurement.
    NodeDensity {
        /// Node identifier in the fluid system.
        node_id: String,
    },

    /// Edge mass flow rate measurement.
    EdgeMassFlow {
        /// Component identifier in the fluid system.
        component_id: String,
    },

    /// Derived quantity: pressure drop across a component.
    PressureDrop {
        /// Upstream node identifier.
        from_node_id: String,
        /// Downstream node identifier.
        to_node_id: String,
    },
}

impl MeasuredVariableRef {
    /// Create a node pressure reference.
    pub fn node_pressure(node_id: impl Into<String>) -> Self {
        Self::NodePressure {
            node_id: node_id.into(),
        }
    }

    /// Create a node temperature reference.
    pub fn node_temperature(node_id: impl Into<String>) -> Self {
        Self::NodeTemperature {
            node_id: node_id.into(),
        }
    }

    /// Create a node enthalpy reference.
    pub fn node_enthalpy(node_id: impl Into<String>) -> Self {
        Self::NodeEnthalpy {
            node_id: node_id.into(),
        }
    }

    /// Create a node density reference.
    pub fn node_density(node_id: impl Into<String>) -> Self {
        Self::NodeDensity {
            node_id: node_id.into(),
        }
    }

    /// Create an edge mass flow reference.
    pub fn edge_mass_flow(component_id: impl Into<String>) -> Self {
        Self::EdgeMassFlow {
            component_id: component_id.into(),
        }
    }

    /// Create a pressure drop reference.
    pub fn pressure_drop(from_node_id: impl Into<String>, to_node_id: impl Into<String>) -> Self {
        Self::PressureDrop {
            from_node_id: from_node_id.into(),
            to_node_id: to_node_id.into(),
        }
    }

    /// Get the referenced node ID, if this is a node-based measurement.
    pub fn node_id(&self) -> Option<&str> {
        match self {
            Self::NodePressure { node_id }
            | Self::NodeTemperature { node_id }
            | Self::NodeEnthalpy { node_id }
            | Self::NodeDensity { node_id } => Some(node_id),
            _ => None,
        }
    }

    /// Get the referenced component ID, if this is a component-based measurement.
    pub fn component_id(&self) -> Option<&str> {
        match self {
            Self::EdgeMassFlow { component_id } => Some(component_id),
            _ => None,
        }
    }
}

/// Trait for types that can provide measured variable values.
///
/// This will be implemented by the runtime system to extract measurements
/// from the fluid network simulation.
pub trait MeasuredVariableProvider {
    /// Get the current value of a measured variable.
    ///
    /// Returns `None` if the reference is invalid or the measurement is not available.
    fn get_measurement(&self, reference: &MeasuredVariableRef) -> Option<f64>;
}

/// Placeholder measured variable type for use in signal blocks.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct MeasuredVariable {
    /// Reference to the measurement.
    pub reference: MeasuredVariableRef,
}

impl MeasuredVariable {
    /// Create a new measured variable.
    pub fn new(reference: MeasuredVariableRef) -> Self {
        Self { reference }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn measured_variable_ref_node_pressure() {
        let mv = MeasuredVariableRef::node_pressure("tank1");
        assert_eq!(mv.node_id(), Some("tank1"));
        assert!(mv.component_id().is_none());
    }

    #[test]
    fn measured_variable_ref_edge_mass_flow() {
        let mv = MeasuredVariableRef::edge_mass_flow("valve1");
        assert_eq!(mv.component_id(), Some("valve1"));
        assert!(mv.node_id().is_none());
    }

    #[test]
    fn measured_variable_ref_pressure_drop() {
        let mv = MeasuredVariableRef::pressure_drop("n1", "n2");
        match mv {
            MeasuredVariableRef::PressureDrop {
                from_node_id,
                to_node_id,
            } => {
                assert_eq!(from_node_id, "n1");
                assert_eq!(to_node_id, "n2");
            }
            _ => panic!("Expected PressureDrop variant"),
        }
    }
}
