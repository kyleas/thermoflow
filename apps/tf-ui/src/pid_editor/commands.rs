use egui::Pos2;
use tf_project::schema::{ComponentDef, ComponentKind, LayoutDef, NodeDef, NodeKind, Project};

use super::model::{PidEdgeRoute, PidLayout, PidNodeLayout};
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
