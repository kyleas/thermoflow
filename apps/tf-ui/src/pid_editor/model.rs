use std::collections::HashMap;

use egui::{Pos2, Vec2};
use tf_project::schema::{
    ControlBlockLayout, EdgeLayout, LayoutDef, NodeLayout, RoutePointDef, SignalConnectionRoute,
};

pub const GRID_SPACING: f32 = 20.0;

#[derive(Clone, Debug)]
pub struct PidNodeLayout {
    pub node_id: String,
    pub pos: Pos2,
    pub label_offset: Vec2,
}

#[derive(Clone, Debug)]
pub struct PidEdgeRoute {
    pub component_id: String,
    pub points: Vec<Pos2>,
    pub label_offset: Vec2,
    pub component_pos: Option<Pos2>,
}

#[derive(Clone, Debug)]
pub struct PidControlBlockLayout {
    pub block_id: String,
    pub pos: Pos2,
    pub label_offset: Vec2,
}

#[derive(Clone, Debug)]
pub struct PidSignalConnection {
    pub from_block_id: String,
    pub to_block_id: String,
    pub to_input: String,
    pub points: Vec<Pos2>,
    pub label_offset: Vec2,
}

#[derive(Default, Clone, Debug)]
pub struct PidLayout {
    pub nodes: HashMap<String, PidNodeLayout>,
    pub edges: HashMap<String, PidEdgeRoute>,
    pub control_blocks: HashMap<String, PidControlBlockLayout>,
    pub signal_connections: Vec<PidSignalConnection>,
}

#[allow(dead_code)]
#[derive(Clone, Debug)]
pub enum DragTarget {
    Node { node_id: String },
    Component { component_id: String },
    ControlBlock { block_id: String },
}

#[allow(dead_code)]
#[derive(Clone, Debug)]
pub struct DragState {
    pub target: DragTarget,
    pub start_pos: Pos2,
    pub drag_offset: Vec2,
    pub free_move: bool,
}

#[allow(dead_code)]
#[derive(Clone, Debug, PartialEq, Eq, Default)]
pub enum EditorMode {
    #[default]
    Select,
    InsertComponent,
    InsertNode,
    InsertControlBlock,
}

#[allow(dead_code)]
#[derive(Default, Clone, Debug)]
pub struct PidEditorState {
    pub selected_node_id: Option<String>,
    pub selected_component_id: Option<String>,
    pub selected_control_block_id: Option<String>,
    pub hover_node_id: Option<String>,
    pub hover_component_id: Option<String>,
    pub hover_control_block_id: Option<String>,
    pub drag_state: Option<DragState>,
    pub mode: EditorMode,
}

impl PidLayout {
    pub fn from_layout_def(layout: &LayoutDef) -> Self {
        let mut nodes = HashMap::new();
        for node in &layout.nodes {
            nodes.insert(
                node.node_id.clone(),
                PidNodeLayout {
                    node_id: node.node_id.clone(),
                    pos: Pos2::new(node.x, node.y),
                    label_offset: Vec2::new(node.label_offset_x, node.label_offset_y),
                },
            );
        }

        let mut edges = HashMap::new();
        for edge in &layout.edges {
            edges.insert(edge.component_id.clone(), edge_route_from_layout(edge));
        }

        let mut control_blocks = HashMap::new();
        for block in &layout.control_blocks {
            control_blocks.insert(
                block.block_id.clone(),
                PidControlBlockLayout {
                    block_id: block.block_id.clone(),
                    pos: Pos2::new(block.x, block.y),
                    label_offset: Vec2::new(block.label_offset_x, block.label_offset_y),
                },
            );
        }

        let signal_connections: Vec<PidSignalConnection> = layout
            .signal_connections
            .iter()
            .map(|conn| PidSignalConnection {
                from_block_id: conn.from_block_id.clone(),
                to_block_id: conn.to_block_id.clone(),
                to_input: conn.to_input.clone(),
                points: conn.points.iter().map(|p| Pos2::new(p.x, p.y)).collect(),
                label_offset: Vec2::new(conn.label_offset_x, conn.label_offset_y),
            })
            .collect();

        Self {
            nodes,
            edges,
            control_blocks,
            signal_connections,
        }
    }

    pub fn apply_to_layout_def(&self, layout: &mut LayoutDef) {
        layout.nodes.clear();
        for node in self.nodes.values() {
            layout.nodes.push(NodeLayout {
                node_id: node.node_id.clone(),
                x: node.pos.x,
                y: node.pos.y,
                label_offset_x: node.label_offset.x,
                label_offset_y: node.label_offset.y,
                overlay: None,
            });
        }

        layout.edges.clear();
        for edge in self.edges.values() {
            layout.edges.push(edge_route_to_layout(edge));
        }

        layout.control_blocks.clear();
        for block in self.control_blocks.values() {
            layout.control_blocks.push(ControlBlockLayout {
                block_id: block.block_id.clone(),
                x: block.pos.x,
                y: block.pos.y,
                label_offset_x: block.label_offset.x,
                label_offset_y: block.label_offset.y,
            });
        }

        layout.signal_connections.clear();
        for conn in &self.signal_connections {
            layout.signal_connections.push(SignalConnectionRoute {
                from_block_id: conn.from_block_id.clone(),
                to_block_id: conn.to_block_id.clone(),
                to_input: conn.to_input.clone(),
                points: conn
                    .points
                    .iter()
                    .map(|p| RoutePointDef { x: p.x, y: p.y })
                    .collect(),
                label_offset_x: conn.label_offset.x,
                label_offset_y: conn.label_offset.y,
            });
        }
    }

    pub fn ensure_node(&mut self, node_id: &str, pos: Pos2) {
        self.nodes
            .entry(node_id.to_string())
            .or_insert(PidNodeLayout {
                node_id: node_id.to_string(),
                pos,
                label_offset: Vec2::ZERO,
            });
    }

    #[allow(dead_code)]
    pub fn ensure_control_block(&mut self, block_id: &str, pos: Pos2) {
        self.control_blocks
            .entry(block_id.to_string())
            .or_insert(PidControlBlockLayout {
                block_id: block_id.to_string(),
                pos,
                label_offset: Vec2::ZERO,
            });
    }
}

pub fn snap_to_grid(pos: Pos2) -> Pos2 {
    let x = (pos.x / GRID_SPACING).round() * GRID_SPACING;
    let y = (pos.y / GRID_SPACING).round() * GRID_SPACING;
    Pos2::new(x, y)
}

pub fn edge_route_from_layout(edge: &EdgeLayout) -> PidEdgeRoute {
    let component_pos = match (edge.component_x, edge.component_y) {
        (Some(x), Some(y)) => Some(Pos2::new(x, y)),
        _ => None,
    };
    PidEdgeRoute {
        component_id: edge.component_id.clone(),
        points: edge.points.iter().map(|p| Pos2::new(p.x, p.y)).collect(),
        label_offset: Vec2::new(edge.label_offset_x, edge.label_offset_y),
        component_pos,
    }
}

pub fn edge_route_to_layout(edge: &PidEdgeRoute) -> EdgeLayout {
    let (component_x, component_y) = edge
        .component_pos
        .map(|p| (Some(p.x), Some(p.y)))
        .unwrap_or((None, None));
    EdgeLayout {
        component_id: edge.component_id.clone(),
        points: edge
            .points
            .iter()
            .map(|p| RoutePointDef { x: p.x, y: p.y })
            .collect(),
        label_offset_x: edge.label_offset.x,
        label_offset_y: edge.label_offset.y,
        component_x,
        component_y,
    }
}
