//! Core graph data structures.

use tf_core::{CompId, NodeId, PortId};

/// Direction/kind of a port.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum PortKind {
    /// Inlet port (upstream connection).
    Inlet,
    /// Outlet port (downstream connection).
    Outlet,
}

/// A node in the thermodynamic graph (e.g., a state point).
///
/// Nodes are minimal: they hold no thermodynamic data yet,
/// just an ID and a name for human reference.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Node {
    pub id: NodeId,
    pub name: String,
}

/// A port connects a component to a node.
///
/// Each component has exactly 2 ports (inlet, outlet).
/// Each port references its owning component, its connected node, and its kind.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Port {
    pub id: PortId,
    pub comp: CompId,
    pub node: NodeId,
    pub kind: PortKind,
}

/// A component represents a thermodynamic device or process (e.g., pump, heat exchanger).
///
/// Each component has exactly 2 ports: one inlet and one outlet.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Component {
    pub id: CompId,
    pub name: String,
    /// Exactly 2 ports: [inlet_port_id, outlet_port_id].
    pub ports: [PortId; 2],
}

impl Component {
    /// Get the inlet port ID.
    pub fn inlet(&self) -> PortId {
        self.ports[0]
    }

    /// Get the outlet port ID.
    pub fn outlet(&self) -> PortId {
        self.ports[1]
    }
}

/// The graph: a validated, immutable collection of nodes, components, and ports.
///
/// The graph stores:
/// - All nodes, components, and ports in vectors (indexed by their IDs).
/// - Compact adjacency: for each node, which ports are incident.
///
/// This structure is optimized for parallel iteration and solver indexing.
#[derive(Debug, Clone)]
pub struct Graph {
    pub(crate) nodes: Vec<Node>,
    pub(crate) components: Vec<Component>,
    pub(crate) ports: Vec<Port>,

    /// Offsets for node->port adjacency: node i's ports are in node_ports[node_port_offsets[i]..node_port_offsets[i+1]].
    pub(crate) node_port_offsets: Vec<usize>,

    /// Flat list of port IDs incident to nodes (sorted by node ID then port ID for determinism).
    pub(crate) node_ports: Vec<PortId>,
}

impl Graph {
    /// Return all nodes.
    pub fn nodes(&self) -> &[Node] {
        &self.nodes
    }

    /// Return all components.
    pub fn components(&self) -> &[Component] {
        &self.components
    }

    /// Return all ports.
    pub fn ports(&self) -> &[Port] {
        &self.ports
    }

    /// Get a node by ID (returns None if ID out of bounds).
    pub fn node(&self, id: NodeId) -> Option<&Node> {
        self.nodes.get(id.index() as usize)
    }

    /// Get a component by ID (returns None if ID out of bounds).
    pub fn component(&self, id: CompId) -> Option<&Component> {
        self.components.get(id.index() as usize)
    }

    /// Get a port by ID (returns None if ID out of bounds).
    pub fn port(&self, id: PortId) -> Option<&Port> {
        self.ports.get(id.index() as usize)
    }

    /// Iterate over all port IDs incident to a given node.
    pub fn node_ports(&self, node_id: NodeId) -> &[PortId] {
        let idx = node_id.index() as usize;
        if idx >= self.nodes.len() {
            return &[];
        }
        let start = self.node_port_offsets[idx];
        let end = self.node_port_offsets[idx + 1];
        &self.node_ports[start..end]
    }

    /// Get the inlet node of a component.
    pub fn comp_inlet_node(&self, comp_id: CompId) -> Option<NodeId> {
        let comp = self.component(comp_id)?;
        let inlet_port = self.port(comp.inlet())?;
        Some(inlet_port.node)
    }

    /// Get the outlet node of a component.
    pub fn comp_outlet_node(&self, comp_id: CompId) -> Option<NodeId> {
        let comp = self.component(comp_id)?;
        let outlet_port = self.port(comp.outlet())?;
        Some(outlet_port.node)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tf_core::Id;

    #[test]
    fn port_kind_equality() {
        assert_eq!(PortKind::Inlet, PortKind::Inlet);
        assert_ne!(PortKind::Inlet, PortKind::Outlet);
    }

    #[test]
    fn component_accessors() {
        let comp = Component {
            id: Id::from_index(0),
            name: "Test".into(),
            ports: [Id::from_index(10), Id::from_index(11)],
        };
        assert_eq!(comp.inlet().index(), 10);
        assert_eq!(comp.outlet().index(), 11);
    }
}
