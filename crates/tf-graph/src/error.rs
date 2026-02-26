//! Graph-specific error types.

use tf_core::{CompId, NodeId, PortId, TfError};

/// Graph construction and validation errors.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum GraphError {
    /// A port refers to a node that doesn't exist.
    InvalidNodeRef { port: PortId, node: NodeId },

    /// A port refers to a component that doesn't exist.
    InvalidCompRef { port: PortId, comp: CompId },

    /// A component has an invalid number of ports (expected 2).
    InvalidPortCount { comp: CompId, count: usize },

    /// A component has duplicate port IDs.
    DuplicatePorts { comp: CompId },

    /// A port's component field doesn't match the component containing it.
    PortCompMismatch {
        port: PortId,
        expected: CompId,
        actual: CompId,
    },

    /// Adjacency list is inconsistent (port in node's list but port doesn't reference node).
    InconsistentAdjacency { port: PortId, node: NodeId },

    /// ID not found in index map.
    IdNotFound { what: &'static str },
}

impl std::fmt::Display for GraphError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            GraphError::InvalidNodeRef { port, node } => {
                write!(f, "Port {} refers to non-existent node {}", port, node)
            }
            GraphError::InvalidCompRef { port, comp } => {
                write!(f, "Port {} refers to non-existent component {}", port, comp)
            }
            GraphError::InvalidPortCount { comp, count } => {
                write!(f, "Component {} has {} ports (expected 2)", comp, count)
            }
            GraphError::DuplicatePorts { comp } => {
                write!(f, "Component {} has duplicate port IDs", comp)
            }
            GraphError::PortCompMismatch {
                port,
                expected,
                actual,
            } => {
                write!(
                    f,
                    "Port {} should belong to component {} but references {}",
                    port, expected, actual
                )
            }
            GraphError::InconsistentAdjacency { port, node } => {
                write!(
                    f,
                    "Port {} in node {}'s adjacency list but doesn't reference that node",
                    port, node
                )
            }
            GraphError::IdNotFound { what } => {
                write!(f, "{} not found in index map", what)
            }
        }
    }
}

impl std::error::Error for GraphError {}

impl From<GraphError> for TfError {
    fn from(err: GraphError) -> Self {
        TfError::Invariant {
            what: Box::leak(err.to_string().into_boxed_str()),
        }
    }
}
