//! Integration tests for tf-graph.

use tf_graph::{GraphBuilder, IndexMap, PortKind};

#[test]
fn build_minimal_graph() {
    // Build: N1 -> [Comp1] -> N2
    let mut builder = GraphBuilder::new();
    let n1 = builder.add_node("Node1");
    let n2 = builder.add_node("Node2");
    let c1 = builder.add_component("Comp1", n1, n2);

    let graph = builder.build().unwrap();

    // Validate structure
    assert_eq!(graph.nodes().len(), 2);
    assert_eq!(graph.components().len(), 1);
    assert_eq!(graph.ports().len(), 2);

    // Check node-port adjacency
    let n1_ports = graph.node_ports(n1);
    assert_eq!(n1_ports.len(), 1);
    let n2_ports = graph.node_ports(n2);
    assert_eq!(n2_ports.len(), 1);

    // Check port endpoints
    let comp = graph.component(c1).unwrap();
    let inlet_port = graph.port(comp.inlet()).unwrap();
    let outlet_port = graph.port(comp.outlet()).unwrap();

    assert_eq!(inlet_port.node, n1);
    assert_eq!(inlet_port.kind, PortKind::Inlet);
    assert_eq!(outlet_port.node, n2);
    assert_eq!(outlet_port.kind, PortKind::Outlet);

    // Check component endpoints
    assert_eq!(graph.comp_inlet_node(c1), Some(n1));
    assert_eq!(graph.comp_outlet_node(c1), Some(n2));
}

#[test]
fn multiple_components_chain() {
    // Build: N1 -> [C1] -> N2 -> [C2] -> N3
    let mut builder = GraphBuilder::new();
    let n1 = builder.add_node("N1");
    let n2 = builder.add_node("N2");
    let n3 = builder.add_node("N3");
    let c1 = builder.add_component("C1", n1, n2);
    let c2 = builder.add_component("C2", n2, n3);

    let graph = builder.build().unwrap();

    assert_eq!(graph.nodes().len(), 3);
    assert_eq!(graph.components().len(), 2);
    assert_eq!(graph.ports().len(), 4); // 2 ports per component

    // N1 should have 1 port (outlet of C1)
    assert_eq!(graph.node_ports(n1).len(), 1);

    // N2 should have 2 ports (outlet of C1, inlet of C2)
    let n2_ports = graph.node_ports(n2);
    assert_eq!(n2_ports.len(), 2);

    // Verify N2's ports are from different components
    let p1 = graph.port(n2_ports[0]).unwrap();
    let p2 = graph.port(n2_ports[1]).unwrap();
    assert_ne!(p1.comp, p2.comp);
    assert!(p1.comp == c1 || p1.comp == c2);
    assert!(p2.comp == c1 || p2.comp == c2);

    // N3 should have 1 port (inlet of C2)
    assert_eq!(graph.node_ports(n3).len(), 1);
}

#[test]
fn index_map_round_trip() {
    let mut builder = GraphBuilder::new();
    let n1 = builder.add_node("N1");
    let n2 = builder.add_node("N2");
    let n3 = builder.add_node("N3");
    let c1 = builder.add_component("C1", n1, n2);
    let c2 = builder.add_component("C2", n2, n3);

    let graph = builder.build().unwrap();
    let idx_map = IndexMap::from_graph(&graph);

    // Test node round-trip
    for node in graph.nodes() {
        let idx = idx_map.node_idx(node.id).unwrap();
        let id_back = idx_map.node_id(idx);
        assert_eq!(id_back, node.id);
    }

    // Test component round-trip
    for comp in graph.components() {
        let idx = idx_map.comp_idx(comp.id).unwrap();
        let id_back = idx_map.comp_id(idx);
        assert_eq!(id_back, comp.id);
    }

    // Test port round-trip
    for port in graph.ports() {
        let idx = idx_map.port_idx(port.id).unwrap();
        let id_back = idx_map.port_id(idx);
        assert_eq!(id_back, port.id);
    }

    // Test counts
    assert_eq!(idx_map.node_count(), 3);
    assert_eq!(idx_map.comp_count(), 2);
    assert_eq!(idx_map.port_count(), 4);

    // Test contiguous indices
    assert_eq!(idx_map.node_idx(n1).unwrap(), 0);
    assert_eq!(idx_map.node_idx(n2).unwrap(), 1);
    assert_eq!(idx_map.node_idx(n3).unwrap(), 2);

    assert_eq!(idx_map.comp_idx(c1).unwrap(), 0);
    assert_eq!(idx_map.comp_idx(c2).unwrap(), 1);
}

#[test]
fn builder_rename_operations() {
    let mut builder = GraphBuilder::new();
    let n1 = builder.add_node("OldNode");
    let n2 = builder.add_node("N2");
    let c1 = builder.add_component("OldComp", n1, n2);

    builder.rename_node(n1, "NewNode");
    builder.rename_component(c1, "NewComp");

    let graph = builder.build().unwrap();

    assert_eq!(graph.node(n1).unwrap().name, "NewNode");
    assert_eq!(graph.component(c1).unwrap().name, "NewComp");
}

#[test]
fn graph_accessors() {
    let mut builder = GraphBuilder::new();
    let n1 = builder.add_node("N1");
    let n2 = builder.add_node("N2");
    let c1 = builder.add_component("C1", n1, n2);

    let graph = builder.build().unwrap();

    // Test node accessor
    assert!(graph.node(n1).is_some());
    assert_eq!(graph.node(n1).unwrap().name, "N1");

    // Test component accessor
    assert!(graph.component(c1).is_some());
    assert_eq!(graph.component(c1).unwrap().name, "C1");

    // Test invalid IDs
    let bogus_node = tf_core::NodeId::from_index(999);
    assert!(graph.node(bogus_node).is_none());
}

#[test]
fn empty_graph() {
    let builder = GraphBuilder::new();
    let graph = builder.build().unwrap();

    assert_eq!(graph.nodes().len(), 0);
    assert_eq!(graph.components().len(), 0);
    assert_eq!(graph.ports().len(), 0);

    let idx_map = IndexMap::from_graph(&graph);
    assert_eq!(idx_map.node_count(), 0);
    assert_eq!(idx_map.comp_count(), 0);
    assert_eq!(idx_map.port_count(), 0);
}

#[test]
fn large_graph() {
    // Build a larger graph to test scalability
    let mut builder = GraphBuilder::new();

    let mut nodes = Vec::new();
    for i in 0..100 {
        nodes.push(builder.add_node(format!("Node{}", i)));
    }

    for i in 0..99 {
        builder.add_component(format!("Comp{}", i), nodes[i], nodes[i + 1]);
    }

    let graph = builder.build().unwrap();

    assert_eq!(graph.nodes().len(), 100);
    assert_eq!(graph.components().len(), 99);
    assert_eq!(graph.ports().len(), 198); // 2 ports per component

    // Test indexing on large graph
    let idx_map = IndexMap::from_graph(&graph);

    // Spot check a few nodes
    for (i, &node_id) in nodes.iter().take(10).enumerate() {
        let idx = idx_map.node_idx(node_id).unwrap();
        assert_eq!(idx, i);
        assert_eq!(idx_map.node_id(idx), node_id);
    }
}

#[test]
fn parallel_components() {
    // Build a graph with parallel paths: N1 -> C1 -> N2, N1 -> C2 -> N2
    let mut builder = GraphBuilder::new();
    let n1 = builder.add_node("N1");
    let n2 = builder.add_node("N2");
    builder.add_component("C1", n1, n2);
    builder.add_component("C2", n1, n2);

    let graph = builder.build().unwrap();

    assert_eq!(graph.nodes().len(), 2);
    assert_eq!(graph.components().len(), 2);
    assert_eq!(graph.ports().len(), 4);

    // Both nodes should have 2 ports each (inlet and outlet from different components)
    assert_eq!(graph.node_ports(n1).len(), 2);
    assert_eq!(graph.node_ports(n2).len(), 2);
}

#[test]
fn component_port_kinds() {
    let mut builder = GraphBuilder::new();
    let n1 = builder.add_node("N1");
    let n2 = builder.add_node("N2");
    let c1 = builder.add_component("C1", n1, n2);

    let graph = builder.build().unwrap();
    let comp = graph.component(c1).unwrap();

    // First port should be inlet, second should be outlet
    let port0 = graph.port(comp.ports[0]).unwrap();
    let port1 = graph.port(comp.ports[1]).unwrap();

    assert_eq!(port0.kind, PortKind::Inlet);
    assert_eq!(port1.kind, PortKind::Outlet);

    // Helper methods
    assert_eq!(comp.inlet(), comp.ports[0]);
    assert_eq!(comp.outlet(), comp.ports[1]);
}
