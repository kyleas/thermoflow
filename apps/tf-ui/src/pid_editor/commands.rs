use egui::{Pos2, Vec2};
use tf_project::schema::{
    ComponentDef, ComponentKind, ControlBlockDef, ControlConnectionDef, LayoutDef, NodeDef,
    NodeKind, Project,
};

use super::model::{
    PidControlBlockLayout, PidEdgeRoute, PidLayout, PidNodeLayout, PidSignalConnection, Selection,
};
use super::routing::{autoroute, normalize_orthogonal, polyline_midpoint};

pub fn add_node(
    project: &mut Project,
    system_id: &str,
    kind: NodeKind,
    pos: Pos2,
) -> Option<String> {
    let new_id = {
        let system = project.systems.iter_mut().find(|s| s.id == system_id)?;
        let new_id = next_id("n", system.nodes.iter().map(|n| &n.id));
        let name = format!("Node {}", system.nodes.len() + 1);

        system.nodes.push(NodeDef {
            id: new_id.clone(),
            name,
            kind,
        });

        new_id
    };

    let layout_index = layout_index_for_system(project, system_id);
    let mut pid_layout = PidLayout::from_layout_def(&project.layouts[layout_index]);
    pid_layout.nodes.insert(
        new_id.clone(),
        PidNodeLayout {
            node_id: new_id.clone(),
            pos,
            label_offset: egui::Vec2::ZERO,
        },
    );
    pid_layout.apply_to_layout_def(&mut project.layouts[layout_index]);

    Some(new_id)
}

pub fn delete_node(project: &mut Project, system_id: &str, node_id: &str) -> Vec<String> {
    let mut removed_components = Vec::new();
    {
        let system = match project.systems.iter_mut().find(|s| s.id == system_id) {
            Some(system) => system,
            None => return removed_components,
        };

        system.nodes.retain(|n| n.id != node_id);
        system.boundaries.retain(|b| b.node_id != node_id);

        system.components.retain(|c| {
            let remove = c.from_node_id == node_id || c.to_node_id == node_id;
            if remove {
                removed_components.push(c.id.clone());
            }
            !remove
        });
    }

    let layout_index = layout_index_for_system(project, system_id);
    let mut pid_layout = PidLayout::from_layout_def(&project.layouts[layout_index]);
    pid_layout.nodes.remove(node_id);
    for comp_id in &removed_components {
        pid_layout.edges.remove(comp_id);
    }
    pid_layout.apply_to_layout_def(&mut project.layouts[layout_index]);

    removed_components
}

pub fn add_component(
    project: &mut Project,
    system_id: &str,
    kind: ComponentKind,
    from_node_id: &str,
    to_node_id: &str,
) -> Option<String> {
    let new_id = {
        let system = project.systems.iter_mut().find(|s| s.id == system_id)?;
        let new_id = next_id("c", system.components.iter().map(|c| &c.id));
        let name = format!("Component {}", system.components.len() + 1);

        system.components.push(ComponentDef {
            id: new_id.clone(),
            name,
            kind,
            from_node_id: from_node_id.to_string(),
            to_node_id: to_node_id.to_string(),
        });

        new_id
    };

    let layout_index = layout_index_for_system(project, system_id);
    let mut pid_layout = PidLayout::from_layout_def(&project.layouts[layout_index]);

    if let (Some(from), Some(to)) = (
        pid_layout.nodes.get(from_node_id).map(|n| n.pos),
        pid_layout.nodes.get(to_node_id).map(|n| n.pos),
    ) {
        let points = normalize_orthogonal(&autoroute(from, to));
        let component_pos = polyline_midpoint(&points);
        pid_layout.edges.insert(
            new_id.clone(),
            PidEdgeRoute {
                component_id: new_id.clone(),
                points,
                label_offset: egui::Vec2::ZERO,
                component_pos: Some(component_pos),
            },
        );
    }

    pid_layout.apply_to_layout_def(&mut project.layouts[layout_index]);

    Some(new_id)
}

pub fn delete_component(project: &mut Project, system_id: &str, component_id: &str) -> bool {
    let before = {
        let system = match project.systems.iter_mut().find(|s| s.id == system_id) {
            Some(system) => system,
            None => return false,
        };

        let before = system.components.len();
        system.components.retain(|c| c.id != component_id);
        before
    };

    let layout_index = layout_index_for_system(project, system_id);
    let mut pid_layout = PidLayout::from_layout_def(&project.layouts[layout_index]);
    pid_layout.edges.remove(component_id);
    pid_layout.apply_to_layout_def(&mut project.layouts[layout_index]);

    before
        != project
            .systems
            .iter()
            .find(|s| s.id == system_id)
            .map(|s| s.components.len())
            .unwrap_or(before)
}

pub fn reconnect_component_endpoint(
    project: &mut Project,
    system_id: &str,
    component_id: &str,
    new_from: Option<String>,
    new_to: Option<String>,
) -> bool {
    let (component_id, from_node_id, to_node_id) = {
        let system = match project.systems.iter_mut().find(|s| s.id == system_id) {
            Some(system) => system,
            None => return false,
        };

        let component = match system.components.iter_mut().find(|c| c.id == component_id) {
            Some(component) => component,
            None => return false,
        };

        if let Some(from) = new_from {
            component.from_node_id = from;
        }
        if let Some(to) = new_to {
            component.to_node_id = to;
        }

        (
            component.id.clone(),
            component.from_node_id.clone(),
            component.to_node_id.clone(),
        )
    };

    let layout_index = layout_index_for_system(project, system_id);
    let mut pid_layout = PidLayout::from_layout_def(&project.layouts[layout_index]);

    if let (Some(from), Some(to)) = (
        pid_layout.nodes.get(&from_node_id).map(|n| n.pos),
        pid_layout.nodes.get(&to_node_id).map(|n| n.pos),
    ) {
        let points = normalize_orthogonal(&autoroute(from, to));
        let component_pos = polyline_midpoint(&points);
        pid_layout.edges.insert(
            component_id.clone(),
            PidEdgeRoute {
                component_id,
                points,
                label_offset: egui::Vec2::ZERO,
                component_pos: Some(component_pos),
            },
        );
    }

    pid_layout.apply_to_layout_def(&mut project.layouts[layout_index]);

    true
}

pub fn insert_node_into_component(
    project: &mut Project,
    system_id: &str,
    component_id: &str,
    node_pos: Pos2,
) -> Option<String> {
    let (from_node, to_node, component_id, new_node_id, new_component_id) = {
        let system = project.systems.iter_mut().find(|s| s.id == system_id)?;
        let component = system
            .components
            .iter_mut()
            .find(|c| c.id == component_id)?;

        let from_node = component.from_node_id.clone();
        let to_node = component.to_node_id.clone();
        let component_kind = component.kind.clone();
        let component_id = component.id.clone();

        let new_node_id = next_id("n", system.nodes.iter().map(|n| &n.id));
        system.nodes.push(NodeDef {
            id: new_node_id.clone(),
            name: format!("Node {}", system.nodes.len() + 1),
            kind: NodeKind::Junction,
        });

        component.to_node_id = new_node_id.clone();

        let new_component_id = next_id("c", system.components.iter().map(|c| &c.id));
        system.components.push(ComponentDef {
            id: new_component_id.clone(),
            name: format!("Component {}", system.components.len() + 1),
            kind: component_kind,
            from_node_id: new_node_id.clone(),
            to_node_id: to_node.clone(),
        });

        (
            from_node,
            to_node,
            component_id,
            new_node_id,
            new_component_id,
        )
    };

    let layout_index = layout_index_for_system(project, system_id);
    let mut pid_layout = PidLayout::from_layout_def(&project.layouts[layout_index]);
    pid_layout.nodes.insert(
        new_node_id.clone(),
        PidNodeLayout {
            node_id: new_node_id.clone(),
            pos: node_pos,
            label_offset: egui::Vec2::ZERO,
        },
    );

    if let (Some(from), Some(to)) = (
        pid_layout.nodes.get(&from_node).map(|n| n.pos),
        pid_layout.nodes.get(&new_node_id).map(|n| n.pos),
    ) {
        let points = normalize_orthogonal(&autoroute(from, to));
        let component_pos = polyline_midpoint(&points);
        pid_layout.edges.insert(
            component_id.clone(),
            PidEdgeRoute {
                component_id: component_id.clone(),
                points,
                label_offset: egui::Vec2::ZERO,
                component_pos: Some(component_pos),
            },
        );
    }
    if let (Some(from), Some(to)) = (
        pid_layout.nodes.get(&new_node_id).map(|n| n.pos),
        pid_layout.nodes.get(&to_node).map(|n| n.pos),
    ) {
        let points = normalize_orthogonal(&autoroute(from, to));
        let component_pos = polyline_midpoint(&points);
        pid_layout.edges.insert(
            new_component_id.clone(),
            PidEdgeRoute {
                component_id: new_component_id.clone(),
                points,
                label_offset: egui::Vec2::ZERO,
                component_pos: Some(component_pos),
            },
        );
    }

    pid_layout.apply_to_layout_def(&mut project.layouts[layout_index]);

    Some(new_node_id)
}

pub fn autoroute_system(project: &mut Project, system_id: &str) {
    let layout_index = layout_index_for_system(project, system_id);
    let mut pid_layout = PidLayout::from_layout_def(&project.layouts[layout_index]);
    for edge in pid_layout.edges.values_mut() {
        if edge.points.len() >= 2 {
            let from = edge.points.first().copied().unwrap_or(Pos2::ZERO);
            let to = edge.points.last().copied().unwrap_or(Pos2::ZERO);
            edge.points = normalize_orthogonal(&autoroute(from, to));
        }
    }
    pid_layout.apply_to_layout_def(&mut project.layouts[layout_index]);
}

fn layout_index_for_system(project: &mut Project, system_id: &str) -> usize {
    if let Some(idx) = project
        .layouts
        .iter()
        .position(|l| l.system_id == system_id)
    {
        return idx;
    }

    project.layouts.push(LayoutDef {
        system_id: system_id.to_string(),
        nodes: Vec::new(),
        edges: Vec::new(),
        control_blocks: Vec::new(),
        signal_connections: Vec::new(),
        overlay: tf_project::schema::OverlaySettingsDef::default(),
    });
    project.layouts.len() - 1
}

fn next_id<'a, I>(prefix: &str, ids: I) -> String
where
    I: Iterator<Item = &'a String>,
{
    let mut max = 0u32;
    for id in ids {
        if let Some(num) = id.strip_prefix(prefix) {
            if let Ok(value) = num.parse::<u32>() {
                if value > max {
                    max = value;
                }
            }
        }
    }
    format!("{}{}", prefix, max + 1)
}

/// Clipboard for copy/paste operations
#[derive(Clone, Debug, Default)]
pub struct Clipboard {
    pub nodes: Vec<NodeDef>,
    pub node_layouts: Vec<PidNodeLayout>,
    pub components: Vec<ComponentDef>,
    pub component_routes: Vec<PidEdgeRoute>,
    pub control_blocks: Vec<ControlBlockDef>,
    pub control_block_layouts: Vec<PidControlBlockLayout>,
    pub control_connections: Vec<ControlConnectionDef>,
    pub signal_connection_routes: Vec<PidSignalConnection>,
}

impl Clipboard {
    pub fn is_empty(&self) -> bool {
        self.nodes.is_empty() && self.components.is_empty() && self.control_blocks.is_empty()
    }
}

/// Copy selected items to clipboard
pub fn copy_selection(
    project: &Project,
    system_id: &str,
    layout: &PidLayout,
    selection: &Selection,
) -> Clipboard {
    let system = match project.systems.iter().find(|s| s.id == system_id) {
        Some(s) => s,
        None => return Clipboard::default(),
    };

    let mut clipboard = Clipboard::default();

    // Copy nodes
    for node_id in &selection.nodes {
        if let Some(node) = system.nodes.iter().find(|n| &n.id == node_id) {
            clipboard.nodes.push(node.clone());
            if let Some(layout_node) = layout.nodes.get(node_id) {
                clipboard.node_layouts.push(layout_node.clone());
            }
        }
    }

    // Copy components (only if both endpoints are selected)
    for comp_id in &selection.components {
        if let Some(comp) = system.components.iter().find(|c| &c.id == comp_id) {
            if selection.nodes.contains(&comp.from_node_id)
                && selection.nodes.contains(&comp.to_node_id)
            {
                clipboard.components.push(comp.clone());
                if let Some(route) = layout.edges.get(comp_id) {
                    clipboard.component_routes.push(route.clone());
                }
            }
        }
    }

    // Copy control blocks
    for block_id in &selection.control_blocks {
        if let Some(block) = system
            .controls
            .as_ref()
            .and_then(|cs| cs.blocks.iter().find(|b| &b.id == block_id))
        {
            clipboard.control_blocks.push(block.clone());
            if let Some(layout_block) = layout.control_blocks.get(block_id) {
                clipboard.control_block_layouts.push(layout_block.clone());
            }
        }
    }

    // Copy control connections (only if both endpoint blocks are selected)
    if let Some(control_system) = &system.controls {
        for (idx, conn) in control_system.connections.iter().enumerate() {
            if selection.control_blocks.contains(&conn.from_block_id)
                && selection.control_blocks.contains(&conn.to_block_id)
            {
                clipboard.control_connections.push(conn.clone());

                if let Some(route) = layout.signal_connections.get(idx) {
                    clipboard.signal_connection_routes.push(route.clone());
                } else {
                    let from_pos = layout
                        .control_blocks
                        .get(&conn.from_block_id)
                        .map(|b| b.pos)
                        .unwrap_or(Pos2::ZERO);
                    let to_pos = layout
                        .control_blocks
                        .get(&conn.to_block_id)
                        .map(|b| b.pos)
                        .unwrap_or(Pos2::ZERO);
                    clipboard
                        .signal_connection_routes
                        .push(PidSignalConnection {
                            from_block_id: conn.from_block_id.clone(),
                            to_block_id: conn.to_block_id.clone(),
                            to_input: conn.to_input.clone(),
                            points: vec![from_pos, to_pos],
                            label_offset: egui::Vec2::ZERO,
                        });
                }
            }
        }
    }

    clipboard
}

/// Paste clipboard contents at the given offset
pub fn paste_clipboard(
    project: &mut Project,
    system_id: &str,
    clipboard: &Clipboard,
    offset: Vec2,
) -> Selection {
    let mut new_selection = Selection::new();

    if clipboard.is_empty() {
        return new_selection;
    }

    // Get layout first (before any mutable borrows of systems)
    let layout_index = layout_index_for_system(project, system_id);
    let mut pid_layout = PidLayout::from_layout_def(&project.layouts[layout_index]);

    let system = match project.systems.iter_mut().find(|s| s.id == system_id) {
        Some(s) => s,
        None => return new_selection,
    };

    // Map old IDs to new IDs
    let mut node_id_map = std::collections::HashMap::new();
    let mut block_id_map = std::collections::HashMap::new();

    // Paste nodes
    for (node, node_layout) in clipboard.nodes.iter().zip(&clipboard.node_layouts) {
        let new_id = next_id("n", system.nodes.iter().map(|n| &n.id));
        let name = format!("Node {}", system.nodes.len() + 1);

        system.nodes.push(NodeDef {
            id: new_id.clone(),
            name,
            kind: node.kind.clone(),
        });

        pid_layout.nodes.insert(
            new_id.clone(),
            PidNodeLayout {
                node_id: new_id.clone(),
                pos: node_layout.pos + offset,
                label_offset: node_layout.label_offset,
            },
        );

        node_id_map.insert(node.id.clone(), new_id.clone());
        new_selection.add_node(new_id);
    }

    // Paste components (with remapped endpoints)
    for (comp, route) in clipboard.components.iter().zip(&clipboard.component_routes) {
        if let (Some(new_from), Some(new_to)) = (
            node_id_map.get(&comp.from_node_id),
            node_id_map.get(&comp.to_node_id),
        ) {
            let new_id = next_id("c", system.components.iter().map(|c| &c.id));
            let name = format!("Component {}", system.components.len() + 1);

            system.components.push(ComponentDef {
                id: new_id.clone(),
                name,
                kind: comp.kind.clone(),
                from_node_id: new_from.clone(),
                to_node_id: new_to.clone(),
            });

            let new_points: Vec<Pos2> = route.points.iter().map(|p| *p + offset).collect();
            let new_comp_pos = route.component_pos.map(|p| p + offset);

            pid_layout.edges.insert(
                new_id.clone(),
                PidEdgeRoute {
                    component_id: new_id.clone(),
                    points: new_points,
                    label_offset: route.label_offset,
                    component_pos: new_comp_pos,
                },
            );

            new_selection.add_component(new_id);
        }
    }

    // Paste control blocks
    if system.controls.is_none() {
        system.controls = Some(tf_project::schema::ControlSystemDef {
            blocks: Vec::new(),
            connections: Vec::new(),
        });
    }

    if let Some(control_system) = &mut system.controls {
        for (block, block_layout) in clipboard
            .control_blocks
            .iter()
            .zip(&clipboard.control_block_layouts)
        {
            let new_id = next_id("cb", control_system.blocks.iter().map(|b| &b.id));
            let name = format!("Block {}", control_system.blocks.len() + 1);

            control_system.blocks.push(ControlBlockDef {
                id: new_id.clone(),
                name,
                kind: block.kind.clone(),
            });

            pid_layout.control_blocks.insert(
                new_id.clone(),
                PidControlBlockLayout {
                    block_id: new_id.clone(),
                    pos: block_layout.pos + offset,
                    label_offset: block_layout.label_offset,
                },
            );

            block_id_map.insert(block.id.clone(), new_id.clone());
            new_selection.add_control_block(new_id);
        }

        // Paste control connections with remapped block IDs
        for (conn, route) in clipboard
            .control_connections
            .iter()
            .zip(&clipboard.signal_connection_routes)
        {
            if let (Some(new_from), Some(new_to)) = (
                block_id_map.get(&conn.from_block_id),
                block_id_map.get(&conn.to_block_id),
            ) {
                control_system.connections.push(ControlConnectionDef {
                    from_block_id: new_from.clone(),
                    to_block_id: new_to.clone(),
                    to_input: conn.to_input.clone(),
                });

                pid_layout.signal_connections.push(PidSignalConnection {
                    from_block_id: new_from.clone(),
                    to_block_id: new_to.clone(),
                    to_input: conn.to_input.clone(),
                    points: route.points.iter().map(|p| *p + offset).collect(),
                    label_offset: route.label_offset,
                });
            }
        }
    }

    pid_layout.apply_to_layout_def(&mut project.layouts[layout_index]);

    new_selection
}

/// Delete all items in the selection
pub fn delete_selection(
    project: &mut Project,
    system_id: &str,
    selection: &Selection,
) -> Selection {
    let deleted = selection.clone();

    // Get layout first
    let layout_index = layout_index_for_system(project, system_id);
    let mut pid_layout = PidLayout::from_layout_def(&project.layouts[layout_index]);

    let system = match project.systems.iter_mut().find(|s| s.id == system_id) {
        Some(s) => s,
        None => return Selection::new(),
    };

    // Delete components first (to avoid dangling references)
    for comp_id in &selection.components {
        system.components.retain(|c| &c.id != comp_id);
        pid_layout.edges.remove(comp_id);
    }

    // Delete components attached to selected nodes
    let mut additional_deleted_comps = Vec::new();
    system.components.retain(|c| {
        let should_remove =
            selection.nodes.contains(&c.from_node_id) || selection.nodes.contains(&c.to_node_id);
        if should_remove {
            additional_deleted_comps.push(c.id.clone());
        }
        !should_remove
    });
    for comp_id in additional_deleted_comps {
        pid_layout.edges.remove(&comp_id);
    }

    // Delete nodes
    for node_id in &selection.nodes {
        system.nodes.retain(|n| &n.id != node_id);
        system.boundaries.retain(|b| &b.node_id != node_id);
        pid_layout.nodes.remove(node_id);
    }

    // Delete control blocks and any connected signal wires
    if let Some(control_system) = &mut system.controls {
        control_system
            .blocks
            .retain(|b| !selection.control_blocks.contains(&b.id));
        for block_id in &selection.control_blocks {
            pid_layout.control_blocks.remove(block_id);
        }

        let mut schema_idx = 0usize;
        control_system.connections.retain(|conn| {
            let remove_for_block = selection.control_blocks.contains(&conn.from_block_id)
                || selection.control_blocks.contains(&conn.to_block_id);
            let remove_for_explicit_selection = selection.signal_connections.contains(&schema_idx);
            schema_idx += 1;
            !(remove_for_block || remove_for_explicit_selection)
        });

        let mut layout_idx = 0usize;
        pid_layout.signal_connections.retain(|signal| {
            let remove_for_block = selection.control_blocks.contains(&signal.from_block_id)
                || selection.control_blocks.contains(&signal.to_block_id);
            let remove_for_explicit_selection = selection.signal_connections.contains(&layout_idx);
            layout_idx += 1;
            !(remove_for_block || remove_for_explicit_selection)
        });
    }

    pid_layout.apply_to_layout_def(&mut project.layouts[layout_index]);

    deleted
}

/// Move selected items by the given delta
pub fn move_selection(
    project: &mut Project,
    system_id: &str,
    selection: &Selection,
    delta: Vec2,
    snap_to_grid: bool,
) {
    let layout_index = layout_index_for_system(project, system_id);
    let mut pid_layout = PidLayout::from_layout_def(&project.layouts[layout_index]);

    for node_id in &selection.nodes {
        if let Some(node) = pid_layout.nodes.get_mut(node_id) {
            node.pos += delta;
            if snap_to_grid {
                node.pos = super::model::snap_to_grid(node.pos);
            }
        }
    }

    for comp_id in &selection.components {
        if let Some(edge) = pid_layout.edges.get_mut(comp_id) {
            for point in &mut edge.points {
                *point += delta;
                if snap_to_grid {
                    *point = super::model::snap_to_grid(*point);
                }
            }
            if let Some(comp_pos) = &mut edge.component_pos {
                *comp_pos += delta;
                if snap_to_grid {
                    *comp_pos = super::model::snap_to_grid(*comp_pos);
                }
            }
        }
    }

    for block_id in &selection.control_blocks {
        if let Some(block) = pid_layout.control_blocks.get_mut(block_id) {
            block.pos += delta;
            if snap_to_grid {
                block.pos = super::model::snap_to_grid(block.pos);
            }
        }
    }

    pid_layout.apply_to_layout_def(&mut project.layouts[layout_index]);
}

/// Command trait for undo/redo
pub trait Command {
    fn execute(&mut self, project: &mut Project, system_id: &str);
    fn undo(&mut self, project: &mut Project, system_id: &str);
    fn description(&self) -> String;
}

/// Command history for undo/redo
#[derive(Default)]
pub struct CommandHistory {
    undo_stack: Vec<Box<dyn Command>>,
    redo_stack: Vec<Box<dyn Command>>,
    max_history: usize,
}

impl CommandHistory {
    pub fn new() -> Self {
        Self {
            undo_stack: Vec::new(),
            redo_stack: Vec::new(),
            max_history: 100,
        }
    }

    pub fn execute(&mut self, mut cmd: Box<dyn Command>, project: &mut Project, system_id: &str) {
        cmd.execute(project, system_id);
        self.undo_stack.push(cmd);
        self.redo_stack.clear();

        if self.undo_stack.len() > self.max_history {
            self.undo_stack.remove(0);
        }
    }

    pub fn undo(&mut self, project: &mut Project, system_id: &str) -> bool {
        if let Some(mut cmd) = self.undo_stack.pop() {
            cmd.undo(project, system_id);
            self.redo_stack.push(cmd);
            true
        } else {
            false
        }
    }

    pub fn redo(&mut self, project: &mut Project, system_id: &str) -> bool {
        if let Some(mut cmd) = self.redo_stack.pop() {
            cmd.execute(project, system_id);
            self.undo_stack.push(cmd);
            true
        } else {
            false
        }
    }

    pub fn can_undo(&self) -> bool {
        !self.undo_stack.is_empty()
    }

    pub fn can_redo(&self) -> bool {
        !self.redo_stack.is_empty()
    }

    pub fn clear(&mut self) {
        self.undo_stack.clear();
        self.redo_stack.clear();
    }
}

/// Move command
pub struct MoveCommand {
    selection: Selection,
    delta: Vec2,
    snap_to_grid: bool,
}

impl MoveCommand {
    pub fn new(selection: Selection, delta: Vec2, snap_to_grid: bool) -> Self {
        Self {
            selection,
            delta,
            snap_to_grid,
        }
    }
}

impl Command for MoveCommand {
    fn execute(&mut self, project: &mut Project, system_id: &str) {
        move_selection(
            project,
            system_id,
            &self.selection,
            self.delta,
            self.snap_to_grid,
        );
    }

    fn undo(&mut self, project: &mut Project, system_id: &str) {
        move_selection(
            project,
            system_id,
            &self.selection,
            -self.delta,
            self.snap_to_grid,
        );
    }

    fn description(&self) -> String {
        "Move".to_string()
    }
}

/// Delete command
pub struct DeleteCommand {
    deleted_nodes: Vec<NodeDef>,
    deleted_node_layouts: Vec<PidNodeLayout>,
    deleted_components: Vec<ComponentDef>,
    deleted_component_routes: Vec<PidEdgeRoute>,
    deleted_control_blocks: Vec<ControlBlockDef>,
    deleted_control_block_layouts: Vec<PidControlBlockLayout>,
}

impl DeleteCommand {
    pub fn new(
        project: &Project,
        system_id: &str,
        layout: &PidLayout,
        selection: &Selection,
    ) -> Self {
        let system = project.systems.iter().find(|s| s.id == system_id);

        let mut deleted_nodes = Vec::new();
        let mut deleted_node_layouts = Vec::new();
        let mut deleted_components = Vec::new();
        let mut deleted_component_routes = Vec::new();
        let mut deleted_control_blocks = Vec::new();
        let mut deleted_control_block_layouts = Vec::new();

        if let Some(system) = system {
            for node_id in &selection.nodes {
                if let Some(node) = system.nodes.iter().find(|n| &n.id == node_id) {
                    deleted_nodes.push(node.clone());
                    if let Some(node_layout) = layout.nodes.get(node_id) {
                        deleted_node_layouts.push(node_layout.clone());
                    }
                }
            }

            for comp_id in &selection.components {
                if let Some(comp) = system.components.iter().find(|c| &c.id == comp_id) {
                    deleted_components.push(comp.clone());
                    if let Some(route) = layout.edges.get(comp_id) {
                        deleted_component_routes.push(route.clone());
                    }
                }
            }

            if let Some(control_system) = &system.controls {
                for block_id in &selection.control_blocks {
                    if let Some(block) = control_system.blocks.iter().find(|b| &b.id == block_id) {
                        deleted_control_blocks.push(block.clone());
                        if let Some(block_layout) = layout.control_blocks.get(block_id) {
                            deleted_control_block_layouts.push(block_layout.clone());
                        }
                    }
                }
            }
        }

        Self {
            deleted_nodes,
            deleted_node_layouts,
            deleted_components,
            deleted_component_routes,
            deleted_control_blocks,
            deleted_control_block_layouts,
        }
    }
}

impl Command for DeleteCommand {
    fn execute(&mut self, project: &mut Project, system_id: &str) {
        let selection = Selection {
            nodes: self.deleted_nodes.iter().map(|n| n.id.clone()).collect(),
            components: self
                .deleted_components
                .iter()
                .map(|c| c.id.clone())
                .collect(),
            control_blocks: self
                .deleted_control_blocks
                .iter()
                .map(|b| b.id.clone())
                .collect(),
            signal_connections: Default::default(),
        };
        delete_selection(project, system_id, &selection);
    }

    fn undo(&mut self, project: &mut Project, system_id: &str) {
        // Get layout first
        let layout_index = layout_index_for_system(project, system_id);
        let mut pid_layout = PidLayout::from_layout_def(&project.layouts[layout_index]);

        let system = match project.systems.iter_mut().find(|s| s.id == system_id) {
            Some(s) => s,
            None => return,
        };

        // Restore nodes
        for (node, layout) in self.deleted_nodes.iter().zip(&self.deleted_node_layouts) {
            system.nodes.push(node.clone());
            pid_layout.nodes.insert(node.id.clone(), layout.clone());
        }

        // Restore components
        for (comp, route) in self
            .deleted_components
            .iter()
            .zip(&self.deleted_component_routes)
        {
            system.components.push(comp.clone());
            pid_layout.edges.insert(comp.id.clone(), route.clone());
        }

        // Restore control blocks
        if system.controls.is_none() {
            system.controls = Some(tf_project::schema::ControlSystemDef {
                blocks: Vec::new(),
                connections: Vec::new(),
            });
        }

        if let Some(control_system) = &mut system.controls {
            for (block, layout) in self
                .deleted_control_blocks
                .iter()
                .zip(&self.deleted_control_block_layouts)
            {
                control_system.blocks.push(block.clone());
                pid_layout
                    .control_blocks
                    .insert(block.id.clone(), layout.clone());
            }
        }

        pid_layout.apply_to_layout_def(&mut project.layouts[layout_index]);
    }

    fn description(&self) -> String {
        "Delete".to_string()
    }
}
