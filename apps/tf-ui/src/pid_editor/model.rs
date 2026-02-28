use std::collections::{HashMap, HashSet};

use egui::{Pos2, Rect, Vec2};
use tf_project::schema::{
    ControlBlockLayout, EdgeLayout, LayoutDef, NodeLayout, RoutePointDef, SignalConnectionRoute,
};

pub const GRID_SPACING: f32 = 20.0;

/// Multi-selection model for the P&ID editor
#[derive(Default, Clone, Debug)]
pub struct Selection {
    pub nodes: HashSet<String>,
    pub components: HashSet<String>,
    pub control_blocks: HashSet<String>,
    pub signal_connections: HashSet<usize>,
}

impl Selection {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn clear(&mut self) {
        self.nodes.clear();
        self.components.clear();
        self.control_blocks.clear();
        self.signal_connections.clear();
    }

    pub fn is_empty(&self) -> bool {
        self.nodes.is_empty()
            && self.components.is_empty()
            && self.control_blocks.is_empty()
            && self.signal_connections.is_empty()
    }

    pub fn contains_node(&self, node_id: &str) -> bool {
        self.nodes.contains(node_id)
    }

    pub fn contains_component(&self, comp_id: &str) -> bool {
        self.components.contains(comp_id)
    }

    pub fn contains_control_block(&self, block_id: &str) -> bool {
        self.control_blocks.contains(block_id)
    }

    pub fn contains_signal(&self, idx: usize) -> bool {
        self.signal_connections.contains(&idx)
    }

    pub fn add_node(&mut self, node_id: String) {
        self.nodes.insert(node_id);
    }

    pub fn add_component(&mut self, comp_id: String) {
        self.components.insert(comp_id);
    }

    pub fn add_control_block(&mut self, block_id: String) {
        self.control_blocks.insert(block_id);
    }

    pub fn add_signal(&mut self, idx: usize) {
        self.signal_connections.insert(idx);
    }

    pub fn remove_control_block(&mut self, block_id: &str) {
        self.control_blocks.remove(block_id);
    }

    pub fn toggle_node(&mut self, node_id: String) {
        if !self.nodes.remove(&node_id) {
            self.nodes.insert(node_id);
        }
    }

    pub fn toggle_component(&mut self, comp_id: String) {
        if !self.components.remove(&comp_id) {
            self.components.insert(comp_id);
        }
    }

    pub fn toggle_control_block(&mut self, block_id: String) {
        if !self.control_blocks.remove(&block_id) {
            self.control_blocks.insert(block_id);
        }
    }

    pub fn toggle_signal(&mut self, idx: usize) {
        if !self.signal_connections.remove(&idx) {
            self.signal_connections.insert(idx);
        }
    }
}

/// Box selection state
#[derive(Clone, Debug)]
pub struct BoxSelection {
    pub start_pos: Pos2,
    pub current_pos: Pos2,
}

impl BoxSelection {
    pub fn rect(&self) -> Rect {
        Rect::from_two_pos(self.start_pos, self.current_pos)
    }
}

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
    MultiSelection,
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
    pub selection: Selection,
    pub hover_node_id: Option<String>,
    pub hover_component_id: Option<String>,
    pub hover_control_block_id: Option<String>,
    pub drag_state: Option<DragState>,
    pub box_selection: Option<BoxSelection>,
    pub mode: EditorMode,
    // Legacy single-selection fields - kept for backward compatibility during transition
    pub selected_node_id: Option<String>,
    pub selected_component_id: Option<String>,
    pub selected_control_block_id: Option<String>,
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

    pub fn ensure_control_block(&mut self, block_id: &str, pos: Pos2) {
        self.control_blocks
            .entry(block_id.to_string())
            .or_insert(PidControlBlockLayout {
                block_id: block_id.to_string(),
                pos,
                label_offset: Vec2::ZERO,
            });
    }

    /// Calculate bounding box of all items in the layout
    #[allow(dead_code)]
    pub fn bounding_box(&self) -> Option<Rect> {
        let mut min_x = f32::INFINITY;
        let mut min_y = f32::INFINITY;
        let mut max_x = f32::NEG_INFINITY;
        let mut max_y = f32::NEG_INFINITY;

        let mut has_items = false;

        for node in self.nodes.values() {
            min_x = min_x.min(node.pos.x);
            min_y = min_y.min(node.pos.y);
            max_x = max_x.max(node.pos.x);
            max_y = max_y.max(node.pos.y);
            has_items = true;
        }

        for edge in self.edges.values() {
            for point in &edge.points {
                min_x = min_x.min(point.x);
                min_y = min_y.min(point.y);
                max_x = max_x.max(point.x);
                max_y = max_y.max(point.y);
                has_items = true;
            }
            if let Some(comp_pos) = edge.component_pos {
                min_x = min_x.min(comp_pos.x);
                min_y = min_y.min(comp_pos.y);
                max_x = max_x.max(comp_pos.x);
                max_y = max_y.max(comp_pos.y);
            }
        }

        for block in self.control_blocks.values() {
            min_x = min_x.min(block.pos.x);
            min_y = min_y.min(block.pos.y);
            max_x = max_x.max(block.pos.x);
            max_y = max_y.max(block.pos.y);
            has_items = true;
        }

        if has_items {
            // Add padding
            let padding = 100.0;
            Some(Rect::from_min_max(
                Pos2::new(min_x - padding, min_y - padding),
                Pos2::new(max_x + padding, max_y + padding),
            ))
        } else {
            None
        }
    }

    /// Get bounding box of a selection
    #[allow(dead_code)]
    pub fn selection_bounds(&self, selection: &Selection) -> Option<Rect> {
        let mut min_x = f32::INFINITY;
        let mut min_y = f32::INFINITY;
        let mut max_x = f32::NEG_INFINITY;
        let mut max_y = f32::NEG_INFINITY;

        let mut has_items = false;

        for node_id in &selection.nodes {
            if let Some(node) = self.nodes.get(node_id) {
                min_x = min_x.min(node.pos.x);
                min_y = min_y.min(node.pos.y);
                max_x = max_x.max(node.pos.x);
                max_y = max_y.max(node.pos.y);
                has_items = true;
            }
        }

        for comp_id in &selection.components {
            if let Some(edge) = self.edges.get(comp_id) {
                for point in &edge.points {
                    min_x = min_x.min(point.x);
                    min_y = min_y.min(point.y);
                    max_x = max_x.max(point.x);
                    max_y = max_y.max(point.y);
                    has_items = true;
                }
            }
        }

        for block_id in &selection.control_blocks {
            if let Some(block) = self.control_blocks.get(block_id) {
                min_x = min_x.min(block.pos.x);
                min_y = min_y.min(block.pos.y);
                max_x = max_x.max(block.pos.x);
                max_y = max_y.max(block.pos.y);
                has_items = true;
            }
        }

        if has_items {
            Some(Rect::from_min_max(
                Pos2::new(min_x, min_y),
                Pos2::new(max_x, max_y),
            ))
        } else {
            None
        }
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
