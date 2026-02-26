//! Graph validation logic.

use std::collections::HashSet;
use tf_core::{NodeId, PortId, TfResult};

use crate::error::GraphError;
use crate::graph::{Component, Node, Port};

/// Validate the graph structure: all references exist, ports are consistent, etc.
pub(crate) fn validate_structure(
    nodes: &[Node],
    components: &[Component],
    ports: &[Port],
) -> TfResult<()> {
    // Check that port IDs are contiguous and match their indices
    for (i, port) in ports.iter().enumerate() {
        if port.id.index() as usize != i {
            return Err(GraphError::InconsistentAdjacency {
                port: port.id,
                node: port.node,
            }
            .into());
        }
    }

    // Check that each port references a valid node
    for port in ports {
        if port.node.index() as usize >= nodes.len() {
            return Err(GraphError::InvalidNodeRef {
                port: port.id,
                node: port.node,
            }
            .into());
        }
    }

    // Check that each port references a valid component
    for port in ports {
        if port.comp.index() as usize >= components.len() {
            return Err(GraphError::InvalidCompRef {
                port: port.id,
                comp: port.comp,
            }
            .into());
        }
    }

    // Check each component
    for comp in components {
        // Must have exactly 2 ports
        if comp.ports.len() != 2 {
            return Err(GraphError::InvalidPortCount {
                comp: comp.id,
                count: comp.ports.len(),
            }
            .into());
        }

        // Ports must be distinct
        if comp.ports[0] == comp.ports[1] {
            return Err(GraphError::DuplicatePorts { comp: comp.id }.into());
        }

        // Each port must reference this component
        for &port_id in &comp.ports {
            if port_id.index() as usize >= ports.len() {
                return Err(GraphError::InvalidCompRef {
                    port: port_id,
                    comp: comp.id,
                }
                .into());
            }
            let port = &ports[port_id.index() as usize];
            if port.comp != comp.id {
                return Err(GraphError::PortCompMismatch {
                    port: port_id,
                    expected: comp.id,
                    actual: port.comp,
                }
                .into());
            }
        }
    }

    Ok(())
}

/// Validate adjacency lists for consistency.
pub(crate) fn validate_adjacency(
    nodes: &[Node],
    ports: &[Port],
    node_port_offsets: &[usize],
    node_ports: &[PortId],
) -> TfResult<()> {
    // Check that offsets array has correct length (nodes.len() + 1)
    if node_port_offsets.len() != nodes.len() + 1 {
        return Err(GraphError::InconsistentAdjacency {
            port: PortId::from_index(0),
            node: nodes.first().map_or(NodeId::from_index(0), |n| n.id),
        }
        .into());
    }

    // For each node, validate its adjacency list
    for node in nodes {
        let idx = node.id.index() as usize;
        let start = node_port_offsets[idx];
        let end = node_port_offsets[idx + 1];

        for &port_id in &node_ports[start..end] {
            // Port must exist
            if port_id.index() as usize >= ports.len() {
                return Err(GraphError::InconsistentAdjacency {
                    port: port_id,
                    node: node.id,
                }
                .into());
            }

            // Port must reference this node
            let port = &ports[port_id.index() as usize];
            if port.node != node.id {
                return Err(GraphError::InconsistentAdjacency {
                    port: port_id,
                    node: node.id,
                }
                .into());
            }
        }
    }

    // Check that all ports appear in exactly one node's adjacency list
    let mut ports_in_adj: HashSet<PortId> = HashSet::new();
    for &port_id in node_ports {
        if !ports_in_adj.insert(port_id) {
            // Duplicate port in adjacency lists
            return Err(GraphError::InconsistentAdjacency {
                port: port_id,
                node: ports[port_id.index() as usize].node,
            }
            .into());
        }
    }

    // Every port should appear
    for port in ports {
        if !ports_in_adj.contains(&port.id) {
            return Err(GraphError::InconsistentAdjacency {
                port: port.id,
                node: port.node,
            }
            .into());
        }
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::graph::PortKind;
    use tf_core::Id;

    #[test]
    fn validate_empty_graph() {
        let nodes = vec![];
        let components = vec![];
        let ports = vec![];
        assert!(validate_structure(&nodes, &components, &ports).is_ok());
    }

    #[test]
    fn validate_invalid_node_ref() {
        let nodes = vec![Node {
            id: Id::from_index(0),
            name: "N1".into(),
        }];
        let components = vec![];
        let ports = vec![Port {
            id: Id::from_index(0),
            comp: Id::from_index(0),
            node: Id::from_index(99), // Invalid!
            kind: PortKind::Inlet,
        }];

        let result = validate_structure(&nodes, &components, &ports);
        assert!(result.is_err());
        assert!(matches!(
            result.unwrap_err(),
            tf_core::TfError::Invariant { .. }
        ));
    }

    #[test]
    fn validate_component_port_mismatch() {
        let nodes = vec![
            Node {
                id: Id::from_index(0),
                name: "N1".into(),
            },
            Node {
                id: Id::from_index(1),
                name: "N2".into(),
            },
        ];
        let ports = vec![
            Port {
                id: Id::from_index(0),
                comp: Id::from_index(0),
                node: Id::from_index(0),
                kind: PortKind::Inlet,
            },
            Port {
                id: Id::from_index(1),
                comp: Id::from_index(999), // Wrong component!
                node: Id::from_index(1),
                kind: PortKind::Outlet,
            },
        ];
        let components = vec![Component {
            id: Id::from_index(0),
            name: "C1".into(),
            ports: [Id::from_index(0), Id::from_index(1)],
        }];

        let result = validate_structure(&nodes, &components, &ports);
        assert!(result.is_err());
    }
}
