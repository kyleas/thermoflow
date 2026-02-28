//! Control signal graph structure.
//!
//! The control graph is separate from the fluid network graph and consists of:
//! - Signal blocks (sources, processors, sinks)
//! - Signal connections between blocks
//! - Evaluation order for signal propagation

use std::collections::HashMap;

use crate::block::SignalBlock;
use crate::error::{ControlError, ControlResult};
use crate::signal::{SignalId, SignalValue};

/// Unique identifier for a signal block.
#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct BlockId(pub String);

impl BlockId {
    /// Create a new block ID.
    pub fn new(id: impl Into<String>) -> Self {
        Self(id.into())
    }

    /// Get the ID as a string slice.
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl From<String> for BlockId {
    fn from(id: String) -> Self {
        Self(id)
    }
}

impl From<&str> for BlockId {
    fn from(id: &str) -> Self {
        Self(id.to_string())
    }
}

/// Signal edge connects one block's output to another block's input.
#[derive(Debug, Clone, PartialEq)]
pub struct SignalEdge {
    /// Source block ID.
    pub from_block: BlockId,
    /// Destination block ID.
    pub to_block: BlockId,
    /// Signal ID for this connection.
    pub signal_id: SignalId,
}

impl SignalEdge {
    /// Create a new signal edge.
    pub fn new(from_block: BlockId, to_block: BlockId, signal_id: SignalId) -> Self {
        Self {
            from_block,
            to_block,
            signal_id,
        }
    }
}

/// Signal connection represents a wire in the control graph.
#[derive(Debug, Clone)]
pub struct SignalConnection {
    /// Source block.
    pub from: BlockId,
    /// Destination block.
    pub to: BlockId,
    /// Signal carried on this connection.
    pub signal_id: SignalId,
}

/// Control graph contains all signal blocks and their connections.
///
/// The graph is responsible for:
/// - Managing block lifecycle
/// - Validating connections
/// - Computing evaluation order
/// - Propagating signal values
#[derive(Debug, Clone)]
pub struct ControlGraph {
    /// All blocks in the graph, indexed by ID.
    blocks: HashMap<BlockId, SignalBlock>,
    /// Signal edges connecting blocks.
    edges: Vec<SignalEdge>,
    /// Signal values at each edge.
    signals: HashMap<SignalId, SignalValue>,
    /// Next available signal ID.
    next_signal_id: u64,
}

impl ControlGraph {
    /// Create a new empty control graph.
    pub fn new() -> Self {
        Self {
            blocks: HashMap::new(),
            edges: Vec::new(),
            signals: HashMap::new(),
            next_signal_id: 0,
        }
    }

    /// Add a block to the graph.
    pub fn add_block(&mut self, id: BlockId, block: SignalBlock) -> ControlResult<()> {
        if self.blocks.contains_key(&id) {
            return Err(ControlError::TopologyError {
                what: format!("Block '{}' already exists", id.as_str()),
            });
        }
        self.blocks.insert(id, block);
        Ok(())
    }

    /// Get a block by ID.
    pub fn get_block(&self, id: &BlockId) -> Option<&SignalBlock> {
        self.blocks.get(id)
    }

    /// Get a mutable reference to a block by ID.
    pub fn get_block_mut(&mut self, id: &BlockId) -> Option<&mut SignalBlock> {
        self.blocks.get_mut(id)
    }

    /// Add a signal connection between two blocks.
    pub fn connect(&mut self, from: BlockId, to: BlockId) -> ControlResult<SignalId> {
        // Validate that both blocks exist
        if !self.blocks.contains_key(&from) {
            return Err(ControlError::InvalidConnection {
                what: format!("Source block '{}' does not exist", from.as_str()),
            });
        }
        if !self.blocks.contains_key(&to) {
            return Err(ControlError::InvalidConnection {
                what: format!("Destination block '{}' does not exist", to.as_str()),
            });
        }

        // Allocate a new signal ID
        let signal_id = SignalId::new(self.next_signal_id);
        self.next_signal_id += 1;

        // Create the edge
        let edge = SignalEdge::new(from, to, signal_id);
        self.edges.push(edge);

        // Initialize signal value
        self.signals.insert(signal_id, SignalValue::default());

        Ok(signal_id)
    }

    /// Get the current value of a signal.
    pub fn get_signal(&self, id: SignalId) -> Option<SignalValue> {
        self.signals.get(&id).copied()
    }

    /// Set the value of a signal.
    pub fn set_signal(&mut self, id: SignalId, value: SignalValue) {
        self.signals.insert(id, value);
    }

    /// Get all blocks in the graph.
    pub fn blocks(&self) -> &HashMap<BlockId, SignalBlock> {
        &self.blocks
    }

    /// Get all edges in the graph.
    pub fn edges(&self) -> &[SignalEdge] {
        &self.edges
    }

    /// Get all signal values.
    pub fn signals(&self) -> &HashMap<SignalId, SignalValue> {
        &self.signals
    }

    /// Compute a topological evaluation order for the blocks.
    ///
    /// Returns a list of block IDs in an order such that all inputs to a block
    /// are evaluated before the block itself.
    pub fn evaluation_order(&self) -> ControlResult<Vec<BlockId>> {
        // Build adjacency list
        let mut adj: HashMap<BlockId, Vec<BlockId>> = HashMap::new();
        let mut in_degree: HashMap<BlockId, usize> = HashMap::new();

        // Initialize in-degree for all blocks
        for id in self.blocks.keys() {
            in_degree.insert(id.clone(), 0);
        }

        // Build adjacency and compute in-degrees
        for edge in &self.edges {
            adj.entry(edge.from_block.clone())
                .or_default()
                .push(edge.to_block.clone());
            *in_degree.get_mut(&edge.to_block).unwrap() += 1;
        }

        // Kahn's algorithm for topological sort
        let mut queue: Vec<BlockId> = in_degree
            .iter()
            .filter(|(_, deg)| **deg == 0)
            .map(|(id, _)| id.clone())
            .collect();

        let mut order = Vec::new();

        while let Some(block_id) = queue.pop() {
            order.push(block_id.clone());

            if let Some(neighbors) = adj.get(&block_id) {
                for neighbor in neighbors {
                    let deg = in_degree.get_mut(neighbor).unwrap();
                    *deg -= 1;
                    if *deg == 0 {
                        queue.push(neighbor.clone());
                    }
                }
            }
        }

        // Check for cycles
        if order.len() != self.blocks.len() {
            return Err(ControlError::TopologyError {
                what: "Control graph contains cycles".to_string(),
            });
        }

        Ok(order)
    }

    /// Validate the graph structure.
    pub fn validate(&self) -> ControlResult<()> {
        // Check for dangling edges
        for edge in &self.edges {
            if !self.blocks.contains_key(&edge.from_block) {
                return Err(ControlError::TopologyError {
                    what: format!(
                        "Edge references non-existent source block '{}'",
                        edge.from_block.as_str()
                    ),
                });
            }
            if !self.blocks.contains_key(&edge.to_block) {
                return Err(ControlError::TopologyError {
                    what: format!(
                        "Edge references non-existent destination block '{}'",
                        edge.to_block.as_str()
                    ),
                });
            }
        }

        // Check for cycles by attempting to compute evaluation order
        self.evaluation_order()?;

        Ok(())
    }
}

impl Default for ControlGraph {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::block::SignalBlockKind;

    #[test]
    fn empty_graph() {
        let graph = ControlGraph::new();
        assert_eq!(graph.blocks().len(), 0);
        assert_eq!(graph.edges().len(), 0);
    }

    #[test]
    fn add_blocks() {
        let mut graph = ControlGraph::new();
        let b1 = SignalBlock::new(SignalBlockKind::Constant { value: 1.0 });
        let b2 = SignalBlock::new(SignalBlockKind::Constant { value: 2.0 });

        graph.add_block(BlockId::new("b1"), b1).unwrap();
        graph.add_block(BlockId::new("b2"), b2).unwrap();

        assert_eq!(graph.blocks().len(), 2);
    }

    #[test]
    fn connect_blocks() {
        let mut graph = ControlGraph::new();
        let b1 = SignalBlock::new(SignalBlockKind::Constant { value: 1.0 });
        let b2 = SignalBlock::new(SignalBlockKind::Constant { value: 2.0 });

        graph.add_block(BlockId::new("b1"), b1).unwrap();
        graph.add_block(BlockId::new("b2"), b2).unwrap();

        let sig_id = graph
            .connect(BlockId::new("b1"), BlockId::new("b2"))
            .unwrap();

        assert_eq!(graph.edges().len(), 1);
        assert!(graph.get_signal(sig_id).is_some());
    }

    #[test]
    fn evaluation_order_simple() {
        let mut graph = ControlGraph::new();
        let b1 = SignalBlock::new(SignalBlockKind::Constant { value: 1.0 });
        let b2 = SignalBlock::new(SignalBlockKind::Constant { value: 2.0 });
        let b3 = SignalBlock::new(SignalBlockKind::Constant { value: 3.0 });

        graph.add_block(BlockId::new("b1"), b1).unwrap();
        graph.add_block(BlockId::new("b2"), b2).unwrap();
        graph.add_block(BlockId::new("b3"), b3).unwrap();

        // b1 -> b2 -> b3
        graph
            .connect(BlockId::new("b1"), BlockId::new("b2"))
            .unwrap();
        graph
            .connect(BlockId::new("b2"), BlockId::new("b3"))
            .unwrap();

        let order = graph.evaluation_order().unwrap();
        assert_eq!(order.len(), 3);

        // b1 must come before b2, b2 must come before b3
        let pos_b1 = order.iter().position(|id| id.as_str() == "b1").unwrap();
        let pos_b2 = order.iter().position(|id| id.as_str() == "b2").unwrap();
        let pos_b3 = order.iter().position(|id| id.as_str() == "b3").unwrap();

        assert!(pos_b1 < pos_b2);
        assert!(pos_b2 < pos_b3);
    }
}
