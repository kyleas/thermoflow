//! Incremental graph builder.

use std::collections::HashMap;
use tf_core::{CompId, NodeId, PortId, TfResult};

use crate::graph::{Component, Graph, Node, Port, PortKind};
use crate::validate;

/// Builder for constructing a graph incrementally.
///
/// Use `add_node` and `add_component` to build up the graph,
/// then call `build()` to validate and freeze it into an immutable `Graph`.
#[derive(Debug, Default)]
pub struct GraphBuilder {
    nodes: Vec<Node>,
    components: Vec<Component>,
    ports: Vec<Port>,
    next_node_id: u32,
    next_comp_id: u32,
    next_port_id: u32,
}

impl GraphBuilder {
    /// Create a new empty builder.
    pub fn new() -> Self {
        Self::default()
    }

    /// Add a node to the graph and return its ID.
    pub fn add_node(&mut self, name: impl Into<String>) -> NodeId {
        let id = NodeId::from_index(self.next_node_id);
        self.next_node_id += 1;
        self.nodes.push(Node {
            id,
            name: name.into(),
        });
        id
    }

    /// Add a component with inlet and outlet nodes.
    ///
    /// Automatically creates two ports (inlet, outlet) and attaches them to the nodes.
    /// Returns the component ID.
    pub fn add_component(
        &mut self,
        name: impl Into<String>,
        inlet_node: NodeId,
        outlet_node: NodeId,
    ) -> CompId {
        let comp_id = CompId::from_index(self.next_comp_id);
        self.next_comp_id += 1;

        // Create inlet port
        let inlet_port_id = PortId::from_index(self.next_port_id);
        self.next_port_id += 1;
        self.ports.push(Port {
            id: inlet_port_id,
            comp: comp_id,
            node: inlet_node,
            kind: PortKind::Inlet,
        });

        // Create outlet port
        let outlet_port_id = PortId::from_index(self.next_port_id);
        self.next_port_id += 1;
        self.ports.push(Port {
            id: outlet_port_id,
            comp: comp_id,
            node: outlet_node,
            kind: PortKind::Outlet,
        });

        // Create component
        self.components.push(Component {
            id: comp_id,
            name: name.into(),
            ports: [inlet_port_id, outlet_port_id],
        });

        comp_id
    }

    /// Rename a node (useful for post-construction adjustments).
    pub fn rename_node(&mut self, node_id: NodeId, new_name: impl Into<String>) {
        if let Some(node) = self.nodes.get_mut(node_id.index() as usize) {
            node.name = new_name.into();
        }
    }

    /// Rename a component (useful for post-construction adjustments).
    pub fn rename_component(&mut self, comp_id: CompId, new_name: impl Into<String>) {
        if let Some(comp) = self.components.get_mut(comp_id.index() as usize) {
            comp.name = new_name.into();
        }
    }

    /// Build and validate the graph, returning an immutable `Graph`.
    ///
    /// This performs validation and constructs compact adjacency lists.
    pub fn build(self) -> TfResult<Graph> {
        // First validate the structure
        validate::validate_structure(&self.nodes, &self.components, &self.ports)?;

        // Build adjacency lists: node -> [ports]
        let (node_port_offsets, node_ports) = Self::build_adjacency(&self.nodes, &self.ports);

        // Validate adjacency consistency
        validate::validate_adjacency(&self.nodes, &self.ports, &node_port_offsets, &node_ports)?;

        Ok(Graph {
            nodes: self.nodes,
            components: self.components,
            ports: self.ports,
            node_port_offsets,
            node_ports,
        })
    }

    /// Build compact adjacency lists: for each node, collect its incident ports.
    fn build_adjacency(nodes: &[Node], ports: &[Port]) -> (Vec<usize>, Vec<PortId>) {
        // Group ports by node
        let mut node_to_ports: HashMap<NodeId, Vec<PortId>> = HashMap::new();
        for port in ports {
            node_to_ports.entry(port.node).or_default().push(port.id);
        }

        // Sort each node's port list for determinism
        for ports_list in node_to_ports.values_mut() {
            ports_list.sort_by_key(|p| p.index());
        }

        // Build offsets and flat list
        let mut offsets = Vec::with_capacity(nodes.len() + 1);
        let mut flat_ports = Vec::new();
        offsets.push(0);

        for node in nodes {
            if let Some(ports_list) = node_to_ports.get(&node.id) {
                flat_ports.extend_from_slice(ports_list);
            }
            offsets.push(flat_ports.len());
        }

        (offsets, flat_ports)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn builder_basic() {
        let mut builder = GraphBuilder::new();
        let n1 = builder.add_node("Node1");
        let n2 = builder.add_node("Node2");
        let c1 = builder.add_component("Comp1", n1, n2);

        assert_eq!(n1.index(), 0);
        assert_eq!(n2.index(), 1);
        assert_eq!(c1.index(), 0);
        assert_eq!(builder.nodes.len(), 2);
        assert_eq!(builder.components.len(), 1);
        assert_eq!(builder.ports.len(), 2);
    }

    #[test]
    fn builder_rename() {
        let mut builder = GraphBuilder::new();
        let n1 = builder.add_node("Old");
        builder.rename_node(n1, "New");
        assert_eq!(builder.nodes[0].name, "New");

        let n2 = builder.add_node("N2");
        let c1 = builder.add_component("OldComp", n1, n2);
        builder.rename_component(c1, "NewComp");
        assert_eq!(builder.components[0].name, "NewComp");
    }

    #[test]
    fn builder_build_simple() {
        let mut builder = GraphBuilder::new();
        let n1 = builder.add_node("N1");
        let n2 = builder.add_node("N2");
        builder.add_component("C1", n1, n2);

        let graph = builder.build().unwrap();
        assert_eq!(graph.nodes().len(), 2);
        assert_eq!(graph.components().len(), 1);
        assert_eq!(graph.ports().len(), 2);

        // Check adjacency
        let n1_ports = graph.node_ports(n1);
        assert_eq!(n1_ports.len(), 1); // inlet
        let n2_ports = graph.node_ports(n2);
        assert_eq!(n2_ports.len(), 1); // outlet
    }
}
