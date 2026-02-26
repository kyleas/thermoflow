//! Stable indexing for solver integration.
//!
//! Provides bidirectional mappings between domain IDs (NodeId, CompId, PortId)
//! and contiguous solver indices (0..N).

use tf_core::{CompId, NodeId, PortId, TfResult};

use crate::error::GraphError;
use crate::graph::Graph;

/// Index map providing stable, contiguous indices for graph objects.
///
/// This is used by solvers to map graph entities to contiguous arrays/vectors.
/// Provides O(1) bidirectional lookup between IDs and indices.
#[derive(Debug, Clone)]
pub struct IndexMap {
    /// Contiguous list of node IDs (index -> NodeId).
    node_ids: Vec<NodeId>,

    /// Contiguous list of component IDs (index -> CompId).
    comp_ids: Vec<CompId>,

    /// Contiguous list of port IDs (index -> PortId).
    port_ids: Vec<PortId>,

    /// Reverse lookup: NodeId -> index.
    /// Sized to max(NodeId.index) + 1; None if that ID doesn't exist.
    node_to_idx: Vec<Option<usize>>,

    /// Reverse lookup: CompId -> index.
    comp_to_idx: Vec<Option<usize>>,

    /// Reverse lookup: PortId -> index.
    port_to_idx: Vec<Option<usize>>,
}

impl IndexMap {
    /// Build an index map from a graph.
    pub fn from_graph(graph: &Graph) -> Self {
        let nodes = graph.nodes();
        let components = graph.components();
        let ports = graph.ports();

        // Forward maps are trivial: IDs are already contiguous by construction
        let node_ids: Vec<NodeId> = nodes.iter().map(|n| n.id).collect();
        let comp_ids: Vec<CompId> = components.iter().map(|c| c.id).collect();
        let port_ids: Vec<PortId> = ports.iter().map(|p| p.id).collect();

        // Reverse maps: size to max ID index + 1
        let max_node_idx = node_ids
            .iter()
            .map(|id| id.index() as usize)
            .max()
            .unwrap_or(0);
        let max_comp_idx = comp_ids
            .iter()
            .map(|id| id.index() as usize)
            .max()
            .unwrap_or(0);
        let max_port_idx = port_ids
            .iter()
            .map(|id| id.index() as usize)
            .max()
            .unwrap_or(0);

        let mut node_to_idx = vec![None; max_node_idx + 1];
        let mut comp_to_idx = vec![None; max_comp_idx + 1];
        let mut port_to_idx = vec![None; max_port_idx + 1];

        for (i, &id) in node_ids.iter().enumerate() {
            node_to_idx[id.index() as usize] = Some(i);
        }
        for (i, &id) in comp_ids.iter().enumerate() {
            comp_to_idx[id.index() as usize] = Some(i);
        }
        for (i, &id) in port_ids.iter().enumerate() {
            port_to_idx[id.index() as usize] = Some(i);
        }

        Self {
            node_ids,
            comp_ids,
            port_ids,
            node_to_idx,
            comp_to_idx,
            port_to_idx,
        }
    }

    /// Number of nodes in the index.
    pub fn node_count(&self) -> usize {
        self.node_ids.len()
    }

    /// Number of components in the index.
    pub fn comp_count(&self) -> usize {
        self.comp_ids.len()
    }

    /// Number of ports in the index.
    pub fn port_count(&self) -> usize {
        self.port_ids.len()
    }

    /// Get the contiguous index for a node ID.
    pub fn node_idx(&self, id: NodeId) -> TfResult<usize> {
        let idx = id.index() as usize;
        self.node_to_idx
            .get(idx)
            .and_then(|&opt| opt)
            .ok_or_else(|| GraphError::IdNotFound { what: "NodeId" }.into())
    }

    /// Get the contiguous index for a component ID.
    pub fn comp_idx(&self, id: CompId) -> TfResult<usize> {
        let idx = id.index() as usize;
        self.comp_to_idx
            .get(idx)
            .and_then(|&opt| opt)
            .ok_or_else(|| GraphError::IdNotFound { what: "CompId" }.into())
    }

    /// Get the contiguous index for a port ID.
    pub fn port_idx(&self, id: PortId) -> TfResult<usize> {
        let idx = id.index() as usize;
        self.port_to_idx
            .get(idx)
            .and_then(|&opt| opt)
            .ok_or_else(|| GraphError::IdNotFound { what: "PortId" }.into())
    }

    /// Get the node ID for a contiguous index (panics if out of bounds).
    pub fn node_id(&self, i: usize) -> NodeId {
        self.node_ids[i]
    }

    /// Get the component ID for a contiguous index (panics if out of bounds).
    pub fn comp_id(&self, i: usize) -> CompId {
        self.comp_ids[i]
    }

    /// Get the port ID for a contiguous index (panics if out of bounds).
    pub fn port_id(&self, i: usize) -> PortId {
        self.port_ids[i]
    }

    /// Iterate over all node IDs in index order.
    pub fn node_ids(&self) -> &[NodeId] {
        &self.node_ids
    }

    /// Iterate over all component IDs in index order.
    pub fn comp_ids(&self) -> &[CompId] {
        &self.comp_ids
    }

    /// Iterate over all port IDs in index order.
    pub fn port_ids(&self) -> &[PortId] {
        &self.port_ids
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::builder::GraphBuilder;

    #[test]
    fn index_map_basic() {
        let mut builder = GraphBuilder::new();
        let n1 = builder.add_node("N1");
        let n2 = builder.add_node("N2");
        let c1 = builder.add_component("C1", n1, n2);
        let graph = builder.build().unwrap();

        let idx_map = IndexMap::from_graph(&graph);

        assert_eq!(idx_map.node_count(), 2);
        assert_eq!(idx_map.comp_count(), 1);
        assert_eq!(idx_map.port_count(), 2);

        // Round-trip node IDs
        let i1 = idx_map.node_idx(n1).unwrap();
        assert_eq!(idx_map.node_id(i1), n1);

        let i2 = idx_map.node_idx(n2).unwrap();
        assert_eq!(idx_map.node_id(i2), n2);

        // Round-trip component ID
        let ic = idx_map.comp_idx(c1).unwrap();
        assert_eq!(idx_map.comp_id(ic), c1);
    }

    #[test]
    fn index_map_invalid_id() {
        let mut builder = GraphBuilder::new();
        builder.add_node("N1");
        let graph = builder.build().unwrap();

        let idx_map = IndexMap::from_graph(&graph);

        // Try to look up a non-existent node
        let bogus_id = NodeId::from_index(999);
        assert!(idx_map.node_idx(bogus_id).is_err());
    }

    #[test]
    fn index_map_contiguous() {
        let mut builder = GraphBuilder::new();
        let n1 = builder.add_node("N1");
        let n2 = builder.add_node("N2");
        let n3 = builder.add_node("N3");
        builder.add_component("C1", n1, n2);
        builder.add_component("C2", n2, n3);
        let graph = builder.build().unwrap();

        let idx_map = IndexMap::from_graph(&graph);

        // All indices should be 0..N
        assert_eq!(idx_map.node_idx(n1).unwrap(), 0);
        assert_eq!(idx_map.node_idx(n2).unwrap(), 1);
        assert_eq!(idx_map.node_idx(n3).unwrap(), 2);

        assert_eq!(idx_map.node_id(0), n1);
        assert_eq!(idx_map.node_id(1), n2);
        assert_eq!(idx_map.node_id(2), n3);
    }
}
