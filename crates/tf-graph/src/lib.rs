//! tf-graph: graph/model layer for thermoflow.
//!
//! Provides:
//! - Core graph data structures (Node, Component, Port, Graph)
//! - Incremental graph builder with validation
//! - Stable indexing for solver integration
//!
//! # Example
//!
//! ```
//! use tf_graph::{GraphBuilder};
//!
//! let mut builder = GraphBuilder::new();
//! let n1 = builder.add_node("Inlet");
//! let n2 = builder.add_node("Outlet");
//! let c1 = builder.add_component("Pump", n1, n2);
//! let graph = builder.build().unwrap();
//!
//! assert_eq!(graph.nodes().len(), 2);
//! assert_eq!(graph.components().len(), 1);
//! ```

pub mod builder;
pub mod error;
pub mod graph;
pub mod indexing;
pub(crate) mod validate;

// Re-exports for ergonomics
pub use builder::GraphBuilder;
pub use error::GraphError;
pub use graph::{Component, Graph, Node, Port, PortKind};
pub use indexing::IndexMap;
