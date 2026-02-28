use egui::{Pos2, Rect};
use std::collections::HashMap;
use tf_project::schema::{
    ComponentKind, ControlBlockDef, ControlBlockKindDef, ControlSystemDef, MeasuredVariableDef,
    NodeKind, NodeOverlayDef, OverlaySettingsDef, Project,
};
use tf_results::{RunStore, TimeseriesRecord};

use crate::pid_editor::{
    GRID_SPACING, PidEditorState, PidLayout, autoroute, draw_component_symbol,
    draw_control_block_symbol, draw_node_symbol, is_orthogonal, normalize_orthogonal, snap_to_grid,
};
use crate::views::{ComponentKindChoice, ControlBlockKindChoice};

pub struct PidView {
    last_system_id: Option<String>,
    layout: PidLayout,
    // Store per-node overlay settings
    node_overlays: HashMap<String, NodeOverlayDef>,
    editor: PidEditorState,
    add_component_kind: ComponentKindChoice,
    pending_component_kind: Option<ComponentKindChoice>,
    pending_from_node: Option<String>,
    add_control_block_kind: ControlBlockKindChoice,
    pending_control_block_kind: Option<ControlBlockKindChoice>,
    pending_signal_from_block: Option<String>,
    pending_signal_to_input: Option<String>,
    selected_control_block_id: Option<String>,
    selected_signal_connection_index: Option<usize>,
    grid_enabled: bool,
    hide_junction_names: bool,
    cached_run_id: Option<String>,
    cached_records: Vec<TimeseriesRecord>,
    time_s: f64,
    show_steady: bool,
    is_playing: bool,
    play_speed: f64,
}

impl Default for PidView {
    fn default() -> Self {
        Self {
            last_system_id: None,
            layout: PidLayout::default(),
            node_overlays: HashMap::new(),
            editor: PidEditorState::default(),
            add_component_kind: ComponentKindChoice::default(),
            pending_component_kind: None,
            pending_from_node: None,
            add_control_block_kind: ControlBlockKindChoice::default(),
            pending_control_block_kind: None,
            pending_signal_from_block: None,
            pending_signal_to_input: None,
            selected_control_block_id: None,
            selected_signal_connection_index: None,
            grid_enabled: true,
            hide_junction_names: false,
            cached_run_id: None,
            cached_records: Vec::new(),
            time_s: 0.0,
            show_steady: false,
            is_playing: false,
            play_speed: 1.0,
        }
    }
}

impl PidView {
    pub fn invalidate_layout(&mut self) {
        self.last_system_id = None;
        self.layout = PidLayout::default();
        self.node_overlays.clear();
        self.editor = PidEditorState::default();
        self.pending_component_kind = None;
        self.pending_from_node = None;
        self.pending_control_block_kind = None;
        self.pending_signal_from_block = None;
        self.pending_signal_to_input = None;
        self.selected_control_block_id = None;
        self.selected_signal_connection_index = None;
    }

    pub fn get_node_overlay(&self, node_id: &str) -> Option<&NodeOverlayDef> {
        self.node_overlays.get(node_id)
    }

    pub fn set_node_overlay(&mut self, node_id: String, overlay: NodeOverlayDef) {
        self.node_overlays.insert(node_id, overlay);
    }

    #[allow(clippy::too_many_arguments)]
    pub fn show(
        &mut self,
        ui: &mut egui::Ui,
        project: &mut Option<Project>,
        selected_system_id: &Option<String>,
        selected_node_id: &mut Option<String>,
        selected_component_id: &mut Option<String>,
        selected_run_id: &Option<String>,
        run_store: &Option<RunStore>,
        overlay: &OverlaySettingsDef,
    ) {
        if let (Some(proj), Some(sys_id)) = (project.as_mut(), selected_system_id) {
            if self.last_system_id.as_ref() != Some(sys_id) {
                self.last_system_id = Some(sys_id.clone());
                self.load_layout(proj, sys_id);
            }

            // Check if we need to save layout (flag set when dragging stops)
            let mut should_save_layout = false;

            let system = proj.systems.iter_mut().find(|s| &s.id == sys_id);
            if let Some(system) = system {
                ui.heading(format!("P&ID: {}", system.name));
                ui.separator();

                // Resolve which run we should display
                let mut run_id_to_show = selected_run_id.clone();
                if self.show_steady {
                    if let Some(store) = run_store {
                        if let Ok(mut runs) = store.list_runs(sys_id) {
                            runs.sort_by(|a, b| a.timestamp.cmp(&b.timestamp));
                            run_id_to_show = runs
                                .iter()
                                .rev()
                                .find(|r| matches!(r.run_type, tf_results::RunType::Steady))
                                .map(|r| r.run_id.clone());
                        }
                    }
                }

                if let (Some(run_id), Some(store)) = (run_id_to_show.as_ref(), run_store) {
                    if self.cached_run_id.as_ref() != Some(run_id) {
                        if let Ok(records) = store.load_timeseries(run_id) {
                            self.cached_records = records;
                            self.cached_run_id = Some(run_id.clone());
                            self.time_s =
                                self.cached_records.last().map(|r| r.time_s).unwrap_or(0.0);
                        }
                    }
                } else {
                    self.cached_run_id = None;
                    self.cached_records.clear();
                }

                let mut run_data: Option<TimeseriesRecord> = None;
                let max_time_s = self.cached_records.last().map(|r| r.time_s).unwrap_or(0.0);

                ui.horizontal(|ui| {
                    if ui.checkbox(&mut self.show_steady, "Show steady").changed() {
                        if self.show_steady {
                            self.is_playing = false;
                        }
                    }
                    if !self.show_steady {
                        if self.is_playing && max_time_s > 0.0 {
                            ui.ctx().request_repaint();
                            let dt = ui.ctx().input(|i| i.unstable_dt).min(0.1) as f64;
                            self.time_s += dt * self.play_speed.max(0.1);
                            if self.time_s >= max_time_s {
                                self.time_s = max_time_s;
                                self.is_playing = false;
                            }
                        }

                        let clamped = self.time_s.clamp(0.0, max_time_s);
                        self.time_s = clamped;

                        if ui
                            .button(if self.is_playing { "Pause" } else { "Play" })
                            .clicked()
                        {
                            if !self.is_playing && self.time_s >= max_time_s && max_time_s > 0.0 {
                                self.time_s = 0.0;
                            }
                            self.is_playing = !self.is_playing;
                            if self.is_playing {
                                ui.ctx().request_repaint();
                            }
                        }
                        ui.add(
                            egui::DragValue::new(&mut self.play_speed)
                                .speed(0.1)
                                .range(0.1..=10.0)
                                .prefix("x"),
                        );
                        ui.add(egui::Slider::new(&mut self.time_s, 0.0..=max_time_s).text("t (s)"));
                        ui.add(
                            egui::DragValue::new(&mut self.time_s)
                                .speed(0.01)
                                .range(0.0..=max_time_s),
                        );
                    }
                });

                if !self.cached_records.is_empty() {
                    run_data = if self.show_steady {
                        self.cached_records.last().cloned()
                    } else {
                        pick_record_at_time(&self.cached_records, self.time_s).cloned()
                    };
                }

                ui.horizontal(|ui| {
                    if ui.button("Auto-route").clicked() {
                        self.autoroute_all(system);
                        should_save_layout = true;
                    }
                    if ui.button("Snap to grid").clicked() {
                        self.snap_all_nodes();
                        should_save_layout = true;
                    }
                    ui.checkbox(&mut self.grid_enabled, "Grid");
                    ui.checkbox(&mut self.hide_junction_names, "Hide junction names");
                    ui.separator();
                    ui.label("Add component:");
                    egui::ComboBox::from_id_salt("pid_add_component")
                        .selected_text(component_kind_label(self.add_component_kind))
                        .show_ui(ui, |ui| {
                            for kind in [
                                ComponentKindChoice::Orifice,
                                ComponentKindChoice::Valve,
                                ComponentKindChoice::Pipe,
                                ComponentKindChoice::Pump,
                                ComponentKindChoice::Turbine,
                            ] {
                                ui.selectable_value(
                                    &mut self.add_component_kind,
                                    kind,
                                    component_kind_label(kind),
                                );
                            }
                        });
                    if ui.button("Pick endpoints").clicked() {
                        self.pending_component_kind = Some(self.add_component_kind);
                        self.pending_from_node = None;
                        *selected_component_id = None;
                        *selected_node_id = None;
                    }
                    if let Some(kind) = self.pending_component_kind {
                        ui.label(format!("Pick 2 nodes ({})", component_kind_label(kind)));
                    }
                    if let Some(comp_id) = selected_component_id.clone() {
                        if ui.button("Insert component").clicked() {
                            if let Some(new_id) = self.insert_component_on_edge(
                                system,
                                &comp_id,
                                self.add_component_kind,
                            ) {
                                *selected_component_id = Some(new_id);
                                *selected_node_id = None;
                                should_save_layout = true;
                            }
                        }
                    }
                });

                // Control block insertion toolbar
                ui.horizontal(|ui| {
                    ui.separator();
                    ui.label("Add control block:");
                    egui::ComboBox::from_id_salt("pid_add_control_block")
                        .selected_text(control_block_kind_label(self.add_control_block_kind))
                        .show_ui(ui, |ui| {
                            for kind in [
                                ControlBlockKindChoice::Constant,
                                ControlBlockKindChoice::MeasuredVariable,
                                ControlBlockKindChoice::PIController,
                                ControlBlockKindChoice::PIDController,
                                ControlBlockKindChoice::FirstOrderActuator,
                                ControlBlockKindChoice::ActuatorCommand,
                            ] {
                                ui.selectable_value(
                                    &mut self.add_control_block_kind,
                                    kind,
                                    control_block_kind_label(kind),
                                );
                            }
                        });
                    if ui.button("Place block").clicked() {
                        self.pending_control_block_kind = Some(self.add_control_block_kind);
                        *selected_component_id = None;
                        *selected_node_id = None;
                    }
                    if let Some(kind) = self.pending_control_block_kind {
                        ui.label(format!(
                            "Click to place ({})",
                            control_block_kind_label(kind)
                        ));
                    }
                });

                // Signal wiring toolbar
                ui.horizontal(|ui| {
                    ui.separator();
                    if ui.button("Wire Signal").clicked() {
                        self.pending_signal_from_block = None;
                        self.pending_signal_to_input = None;
                        // We'll use a flag to indicate wiring mode
                        // For now, just start with no pending blocks
                        *selected_component_id = None;
                        *selected_node_id = None;
                    }
                    if self.pending_signal_from_block.is_some() {
                        if let Some(from_id) = &self.pending_signal_from_block {
                            if self.pending_signal_to_input.is_none() {
                                ui.label(format!(
                                    "Wiring from '{}', click destination block",
                                    from_id
                                ));
                            } else {
                                ui.label("Select input port");
                            }
                        }
                    }
                    if ui.button("Cancel Wire").clicked() {
                        self.pending_signal_from_block = None;
                        self.pending_signal_to_input = None;
                    }
                });

                // Deletion toolbar
                ui.horizontal(|ui| {
                    ui.separator();
                    if let Some(block_id) = self.selected_control_block_id.clone() {
                        if ui.button(format!("Delete Block '{}'", block_id)).clicked() {
                            self.delete_control_block(system, &block_id);
                            self.selected_control_block_id = None;
                            should_save_layout = true;
                        }
                    }
                    if let Some(idx) = self.selected_signal_connection_index {
                        if ui.button("Delete Signal Wire").clicked() {
                            self.delete_signal_connection(system, idx);
                            self.selected_signal_connection_index = None;
                            should_save_layout = true;
                        }
                    }
                });

                if let Some(node_id) = selected_node_id.clone() {
                    if let Some(node_layout) = self.layout.nodes.get_mut(&node_id) {
                        let mut changed = false;
                        ui.horizontal(|ui| {
                            ui.label("Node label offset:");
                            changed |= ui
                                .add(
                                    egui::DragValue::new(&mut node_layout.label_offset.x)
                                        .speed(1.0),
                                )
                                .changed();
                            changed |= ui
                                .add(
                                    egui::DragValue::new(&mut node_layout.label_offset.y)
                                        .speed(1.0),
                                )
                                .changed();
                        });
                        if changed {
                            should_save_layout = true;
                        }
                    }
                }
                if let Some(comp_id) = selected_component_id.clone() {
                    if let Some(edge_layout) = self.layout.edges.get_mut(&comp_id) {
                        let mut changed = false;
                        ui.horizontal(|ui| {
                            ui.label("Component label offset:");
                            changed |= ui
                                .add(
                                    egui::DragValue::new(&mut edge_layout.label_offset.x)
                                        .speed(1.0),
                                )
                                .changed();
                            changed |= ui
                                .add(
                                    egui::DragValue::new(&mut edge_layout.label_offset.y)
                                        .speed(1.0),
                                )
                                .changed();
                        });
                        if changed {
                            should_save_layout = true;
                        }
                    }
                }

                let (response, painter) =
                    ui.allocate_painter(ui.available_size(), egui::Sense::click_and_drag());
                let rect = response.rect;
                painter.rect_filled(rect, 0.0, egui::Color32::from_gray(30));
                if self.grid_enabled {
                    self.draw_grid(&painter, rect);
                }

                self.ensure_layout(system, rect);

                let pointer = response.interact_pointer_pos();

                if response.drag_started() {
                    if let Some(pos) = pointer {
                        // Try to hit test components first (they're smaller and should take priority)
                        if let Some(comp_id) = self.hit_test_component(system, pos) {
                            let free_move = ui.input(|input| input.modifiers.shift);
                            if let Some(route) = self.layout.edges.get(&comp_id) {
                                let component_pos = route
                                    .component_pos
                                    .unwrap_or_else(|| polyline_midpoint(&route.points));
                                self.editor.drag_state = Some(crate::pid_editor::DragState {
                                    target: crate::pid_editor::DragTarget::Component {
                                        component_id: comp_id.clone(),
                                    },
                                    start_pos: component_pos,
                                    drag_offset: component_pos - pos,
                                    free_move,
                                });
                            }
                        } else if let Some(node_id) = self.hit_test_node(system, pos) {
                            if let Some(node_layout) = self.layout.nodes.get(&node_id) {
                                self.editor.drag_state = Some(crate::pid_editor::DragState {
                                    target: crate::pid_editor::DragTarget::Node {
                                        node_id: node_id.clone(),
                                    },
                                    start_pos: node_layout.pos,
                                    drag_offset: node_layout.pos - pos,
                                    free_move: false,
                                });
                            }
                        }
                    }
                }

                if response.dragged() {
                    if let (Some(pos), Some(drag_state)) = (pointer, self.editor.drag_state.clone())
                    {
                        match &drag_state.target {
                            crate::pid_editor::DragTarget::Node { node_id } => {
                                let clamped =
                                    self.constrain_to_rect(pos + drag_state.drag_offset, rect);
                                if let Some(node_layout) = self.layout.nodes.get_mut(node_id) {
                                    node_layout.pos = clamped;
                                }
                                self.update_routes_for_node(system, node_id);
                            }
                            crate::pid_editor::DragTarget::Component { component_id } => {
                                let next_pos = if drag_state.free_move {
                                    pos + drag_state.drag_offset
                                } else {
                                    self.constrain_to_rect(pos + drag_state.drag_offset, rect)
                                };
                                if let Some(route) = self.layout.edges.get_mut(component_id) {
                                    route.component_pos = Some(next_pos);
                                }
                                self.update_routes_for_component(system, component_id);
                            }
                            crate::pid_editor::DragTarget::ControlBlock { block_id } => {
                                let clamped =
                                    self.constrain_to_rect(pos + drag_state.drag_offset, rect);
                                if let Some(block) = self.layout.control_blocks.get_mut(block_id) {
                                    block.pos = clamped;
                                }
                            }
                        }
                    }
                }

                if response.drag_stopped() {
                    if let Some(drag_state) = self.editor.drag_state.take() {
                        match &drag_state.target {
                            crate::pid_editor::DragTarget::Node { node_id } => {
                                if let Some(node_layout) = self.layout.nodes.get_mut(node_id) {
                                    if self.grid_enabled {
                                        node_layout.pos = snap_to_grid(node_layout.pos);
                                    }
                                }
                                self.update_routes_for_node(system, node_id);
                            }
                            crate::pid_editor::DragTarget::Component { component_id } => {
                                if let Some(route) = self.layout.edges.get_mut(component_id) {
                                    if self.grid_enabled && !drag_state.free_move {
                                        if let Some(pos) = route.component_pos {
                                            route.component_pos = Some(snap_to_grid(pos));
                                        }
                                    }
                                }
                                self.update_routes_for_component(system, component_id);
                            }
                            crate::pid_editor::DragTarget::ControlBlock { block_id } => {
                                if let Some(block) = self.layout.control_blocks.get_mut(block_id) {
                                    if self.grid_enabled {
                                        block.pos = snap_to_grid(block.pos);
                                    }
                                }
                            }
                        }
                        should_save_layout = true;
                    }
                }

                if response.clicked() {
                    if let Some(click_pos) = pointer {
                        // Check if we're placing a control block
                        if let Some(kind) = self.pending_control_block_kind {
                            // Place control block at click position
                            let snapped_pos = if self.grid_enabled {
                                snap_to_grid(click_pos)
                            } else {
                                click_pos
                            };
                            let _new_block_id = self.add_control_block(system, kind, snapped_pos);
                            self.pending_control_block_kind = None;
                            // Note: we don't have selected_control_block_id in the signature yet
                            // so we'll skip selection for now
                            *selected_component_id = None;
                            *selected_node_id = None;
                            should_save_layout = true;
                        } else if let Some(block_id) =
                            self.hit_test_control_block(system, click_pos)
                        {
                            // Handle control block click for signal wiring or selection
                            if let Some(from_block) = self.pending_signal_from_block.clone() {
                                // Complete the wire
                                if from_block != block_id {
                                    // Determine default input for the destination block
                                    let to_input = get_default_input_for_block(system, &block_id);
                                    if let Some(input) = to_input {
                                        self.add_signal_connection(
                                            system,
                                            &from_block,
                                            &block_id,
                                            &input,
                                        );
                                        should_save_layout = true;
                                    }
                                }
                                self.pending_signal_from_block = None;
                                self.pending_signal_to_input = None;
                                self.selected_control_block_id = None;
                                self.selected_signal_connection_index = None;
                            } else if self.pending_signal_from_block.is_none() {
                                // Not in wiring mode - check if we should start wiring or just select
                                // For now, just select the block
                                self.selected_control_block_id = Some(block_id);
                                self.selected_signal_connection_index = None;
                            } else {
                                // Start wiring from this block (if it has an output)
                                if block_has_output(system, &block_id) {
                                    self.pending_signal_from_block = Some(block_id);
                                    self.pending_signal_to_input = None;
                                    self.selected_control_block_id = None;
                                    self.selected_signal_connection_index = None;
                                }
                            }
                            *selected_component_id = None;
                            *selected_node_id = None;
                        } else if let Some(node_id) = self.hit_test_node(system, click_pos) {
                            if let Some(kind) = self.pending_component_kind {
                                if let Some(from_id) = self.pending_from_node.clone() {
                                    if from_id != node_id {
                                        let new_id = self.add_component_between_nodes(
                                            system, kind, &from_id, &node_id,
                                        );
                                        self.pending_component_kind = None;
                                        self.pending_from_node = None;
                                        *selected_component_id = new_id;
                                        *selected_node_id = None;
                                        should_save_layout = true;
                                    }
                                } else {
                                    self.pending_from_node = Some(node_id.clone());
                                }
                            } else {
                                *selected_node_id = Some(node_id);
                                *selected_component_id = None;
                            }
                        } else if let Some(comp_id) = self.hit_test_edge(system, click_pos) {
                            *selected_component_id = Some(comp_id);
                            *selected_node_id = None;
                        } else {
                            *selected_component_id = None;
                            *selected_node_id = None;
                        }
                    }
                }

                for component in &system.components {
                    let selected = selected_component_id.as_ref() == Some(&component.id);
                    let color = if selected {
                        egui::Color32::YELLOW
                    } else {
                        egui::Color32::LIGHT_GRAY
                    };

                    if let Some(route) = self.layout.edges.get_mut(&component.id) {
                        if !is_orthogonal(&route.points) {
                            route.points = normalize_orthogonal(&route.points);
                        }

                        draw_route(&painter, route, color);

                        // Use component position if set, otherwise use midpoint
                        let component_pos = route
                            .component_pos
                            .unwrap_or_else(|| polyline_midpoint(&route.points));

                        draw_component_symbol(&painter, &component.kind, component_pos, color);

                        if let Some(ref data) = run_data {
                            if overlay.show_mass_flow {
                                if let Some(edge) = data
                                    .edge_values
                                    .iter()
                                    .find(|e| e.component_id == component.id)
                                {
                                    if let Some(mdot) = edge.mdot_kg_s {
                                        let text = format!("{:.3} kg/s", mdot);
                                        painter.text(
                                            component_pos + egui::vec2(6.0, -18.0),
                                            egui::Align2::LEFT_TOP,
                                            text,
                                            egui::FontId::proportional(10.0),
                                            egui::Color32::LIGHT_BLUE,
                                        );
                                    }
                                }
                            }
                        }

                        painter.text(
                            component_pos + route.label_offset + egui::vec2(6.0, 10.0),
                            egui::Align2::LEFT_TOP,
                            &component.name,
                            egui::FontId::proportional(10.0),
                            egui::Color32::WHITE,
                        );
                    }
                }

                let node_radius = 18.0;
                for node in &system.nodes {
                    if let Some(node_layout) = self.layout.nodes.get(&node.id) {
                        let selected = selected_node_id.as_ref() == Some(&node.id);
                        let color = if selected {
                            egui::Color32::YELLOW
                        } else {
                            egui::Color32::LIGHT_BLUE
                        };

                        let is_boundary = system.boundaries.iter().any(|b| b.node_id == node.id)
                            || matches!(node.kind, NodeKind::Atmosphere { .. });
                        draw_node_symbol(
                            &painter,
                            &node.kind,
                            node_layout.pos,
                            node_radius,
                            color,
                            is_boundary,
                        );

                        // Draw node name unless it's a junction and hide_junction_names is enabled
                        let should_show_name =
                            !(self.hide_junction_names && matches!(node.kind, NodeKind::Junction));
                        if should_show_name {
                            painter.text(
                                node_layout.pos
                                    + node_layout.label_offset
                                    + egui::vec2(0.0, node_radius + 6.0),
                                egui::Align2::CENTER_TOP,
                                &node.name,
                                egui::FontId::proportional(12.0),
                                egui::Color32::WHITE,
                            );
                        }

                        if let Some(ref data) = run_data {
                            if let Some(node_data) =
                                data.node_values.iter().find(|n| n.node_id == node.id)
                            {
                                let node_overlay = self.node_overlays.get(&node.id);
                                let show_pressure = node_overlay
                                    .map(|o| o.show_pressure)
                                    .unwrap_or(overlay.show_pressure);
                                let show_temperature = node_overlay
                                    .map(|o| o.show_temperature)
                                    .unwrap_or(overlay.show_temperature);
                                let show_enthalpy = node_overlay
                                    .map(|o| o.show_enthalpy)
                                    .unwrap_or(overlay.show_enthalpy);
                                let show_density = node_overlay
                                    .map(|o| o.show_density)
                                    .unwrap_or(overlay.show_density);

                                let mut overlay_text = Vec::new();
                                if show_pressure {
                                    if let Some(p) = node_data.p_pa {
                                        overlay_text.push(format!("P: {:.1} Pa", p));
                                    }
                                }
                                if show_temperature {
                                    if let Some(t) = node_data.t_k {
                                        overlay_text.push(format!("T: {:.1} K", t));
                                    }
                                }
                                if show_enthalpy {
                                    if let Some(h) = node_data.h_j_per_kg {
                                        overlay_text.push(format!("h: {:.0} J/kg", h));
                                    }
                                }
                                if show_density {
                                    if let Some(rho) = node_data.rho_kg_m3 {
                                        overlay_text.push(format!("ρ: {:.2} kg/m³", rho));
                                    }
                                }

                                if !overlay_text.is_empty() {
                                    let text = overlay_text.join("\n");
                                    let mut text_pos = node_layout.pos
                                        + egui::vec2(node_radius + 6.0, -node_radius);
                                    text_pos.x =
                                        text_pos.x.clamp(rect.left() + 4.0, rect.right() - 4.0);
                                    text_pos.y =
                                        text_pos.y.clamp(rect.top() + 4.0, rect.bottom() - 4.0);
                                    painter.text(
                                        text_pos,
                                        egui::Align2::LEFT_CENTER,
                                        text,
                                        egui::FontId::proportional(9.0),
                                        egui::Color32::LIGHT_GREEN,
                                    );
                                }
                            }
                        }
                    }
                }

                // Render control blocks if controls exist
                if let Some(ref controls) = system.controls {
                    // First render signal connections (so they appear behind blocks)
                    for signal_conn in &self.layout.signal_connections {
                        if signal_conn.points.len() >= 2 {
                            // Draw dashed line for signal connections
                            let signal_color = egui::Color32::from_rgb(100, 255, 100); // Green for signals

                            for i in 0..signal_conn.points.len() - 1 {
                                let from = signal_conn.points[i];
                                let to = signal_conn.points[i + 1];

                                // Draw dashed line
                                let dash_length = 8.0;
                                let gap_length = 4.0;
                                let total_length = (to - from).length();
                                let direction = (to - from) / total_length;

                                let mut current_pos = from;
                                let mut distance = 0.0;

                                while distance < total_length {
                                    let next_distance = (distance + dash_length).min(total_length);
                                    let next_pos = from + direction * next_distance;

                                    painter.line_segment(
                                        [current_pos, next_pos],
                                        egui::Stroke::new(2.0, signal_color),
                                    );

                                    distance = next_distance + gap_length;
                                    current_pos = from + direction * distance;
                                }
                            }

                            // Draw arrow at the end
                            if let (Some(&from), Some(&to)) = (
                                signal_conn.points.get(signal_conn.points.len() - 2),
                                signal_conn.points.last(),
                            ) {
                                let dir = (to - from).normalized();
                                let arrow_size = 8.0;
                                let arrow_angle = std::f32::consts::PI / 6.0; // 30 degrees

                                let perp = egui::vec2(-dir.y, dir.x);
                                let arrow_left =
                                    to - dir * arrow_size + perp * arrow_size * arrow_angle.tan();
                                let arrow_right =
                                    to - dir * arrow_size - perp * arrow_size * arrow_angle.tan();

                                painter.line_segment(
                                    [to, arrow_left],
                                    egui::Stroke::new(2.0, signal_color),
                                );
                                painter.line_segment(
                                    [to, arrow_right],
                                    egui::Stroke::new(2.0, signal_color),
                                );
                            }
                        }
                    }

                    // Then render blocks on top
                    for block in &controls.blocks {
                        if let Some(block_layout) = self.layout.control_blocks.get(&block.id) {
                            let selected =
                                self.selected_control_block_id.as_ref() == Some(&block.id);
                            let color = if selected {
                                egui::Color32::YELLOW
                            } else {
                                egui::Color32::from_rgb(150, 200, 255) // Light cyan for control blocks
                            };

                            draw_control_block_symbol(
                                &painter,
                                &block.kind,
                                block_layout.pos,
                                color,
                            );

                            // Draw block name
                            painter.text(
                                block_layout.pos
                                    + block_layout.label_offset
                                    + egui::vec2(0.0, 24.0),
                                egui::Align2::CENTER_TOP,
                                &block.name,
                                egui::FontId::proportional(10.0),
                                egui::Color32::WHITE,
                            );
                        }
                    }
                }
            } else {
                ui.label("Selected system not found");
            }

            // Save layout if needed (after system borrow is dropped)
            if should_save_layout {
                self.save_layout(proj, sys_id);
            }
        } else {
            ui.centered_and_justified(|ui| {
                ui.label("No system selected");
            });
        }
    }

    fn load_layout(&mut self, project: &Project, system_id: &str) {
        self.layout = PidLayout::default();
        self.node_overlays.clear();

        if let Some(layout) = project.layouts.iter().find(|l| l.system_id == system_id) {
            self.layout = PidLayout::from_layout_def(layout);
            for node_layout in &layout.nodes {
                if let Some(ref overlay) = node_layout.overlay {
                    self.node_overlays
                        .insert(node_layout.node_id.clone(), overlay.clone());
                }
            }
        }
    }

    fn save_layout(&self, project: &mut Project, system_id: &str) {
        // Find or create layout for this system
        let layout = project
            .layouts
            .iter_mut()
            .find(|l| l.system_id == system_id);

        if let Some(layout) = layout {
            self.layout.apply_to_layout_def(layout);
            for node_layout in &mut layout.nodes {
                node_layout.overlay = self.node_overlays.get(&node_layout.node_id).cloned();
            }
        } else {
            // Create new layout
            let mut layout_def = tf_project::schema::LayoutDef {
                system_id: system_id.to_string(),
                nodes: Vec::new(),
                edges: Vec::new(),
                control_blocks: Vec::new(),
                signal_connections: Vec::new(),
                overlay: tf_project::schema::OverlaySettingsDef::default(),
            };
            self.layout.apply_to_layout_def(&mut layout_def);
            for node_layout in &mut layout_def.nodes {
                node_layout.overlay = self.node_overlays.get(&node_layout.node_id).cloned();
            }
            project.layouts.push(layout_def);
        }
    }

    fn ensure_layout(&mut self, system: &tf_project::schema::SystemDef, rect: Rect) {
        let mut missing_nodes = false;
        for node in &system.nodes {
            if !self.layout.nodes.contains_key(&node.id) {
                missing_nodes = true;
                break;
            }
        }

        if missing_nodes {
            self.init_default_positions(system, rect);
        }

        for component in &system.components {
            let needs_route = self
                .layout
                .edges
                .get(&component.id)
                .map(|route| route.points.len() < 2 || route.component_pos.is_none())
                .unwrap_or(true);
            if needs_route {
                self.autoroute_component(system, &component.id);
            }
        }
    }

    fn init_default_positions(&mut self, system: &tf_project::schema::SystemDef, rect: Rect) {
        let padding = 100.0;
        let center = rect.center();
        let max_radius = (rect.width().min(rect.height()) * 0.5) - padding;
        let radius = max_radius.max(120.0) * 0.6;

        for (i, node) in system.nodes.iter().enumerate() {
            let angle = (i as f32) * std::f32::consts::TAU / (system.nodes.len() as f32);
            let pos = center + egui::vec2(angle.cos() * radius, angle.sin() * radius);
            self.layout.ensure_node(&node.id, pos);
        }
    }

    fn autoroute_all(&mut self, system: &tf_project::schema::SystemDef) {
        for component in &system.components {
            self.autoroute_component(system, &component.id);
        }
    }

    fn autoroute_component(&mut self, system: &tf_project::schema::SystemDef, component_id: &str) {
        let component = match system.components.iter().find(|c| c.id == component_id) {
            Some(component) => component,
            None => return,
        };
        let from = match self.node_pos(&component.from_node_id) {
            Some(pos) => pos,
            None => return,
        };
        let to = match self.node_pos(&component.to_node_id) {
            Some(pos) => pos,
            None => return,
        };

        let route = self
            .layout
            .edges
            .entry(component.id.clone())
            .or_insert_with(|| crate::pid_editor::PidEdgeRoute {
                component_id: component.id.clone(),
                points: Vec::new(),
                label_offset: egui::Vec2::ZERO,
                component_pos: None,
            });

        // If component has a position, route through it, otherwise route directly
        if let Some(comp_pos) = route.component_pos {
            let from_port = offset_port(from, comp_pos, 18.0);
            let to_port = offset_port(to, comp_pos, 18.0);

            let route_to_component = autoroute(from_port, comp_pos);
            let route_from_component = autoroute(comp_pos, to_port);

            let mut points = Vec::new();
            points.extend(normalize_orthogonal(&route_to_component));
            if !points.is_empty() {
                points.pop();
            }
            points.extend(normalize_orthogonal(&route_from_component));

            route.points = points;
        } else {
            // Direct routing without component position
            let from_port = offset_port(from, to, 18.0);
            let to_port = offset_port(to, from, 18.0);
            route.points = normalize_orthogonal(&autoroute(from_port, to_port));

            // Initialize component_pos at the midpoint for dragging
            route.component_pos = Some(polyline_midpoint(&route.points));
        }
    }

    fn node_pos(&self, node_id: &str) -> Option<Pos2> {
        self.layout.nodes.get(node_id).map(|n| n.pos)
    }

    fn update_routes_for_node(&mut self, system: &tf_project::schema::SystemDef, node_id: &str) {
        for component in &system.components {
            if component.from_node_id == node_id || component.to_node_id == node_id {
                self.autoroute_component(system, &component.id);
            }
        }
    }

    fn snap_all_nodes(&mut self) {
        for node in self.layout.nodes.values_mut() {
            node.pos = snap_to_grid(node.pos);
        }
    }

    fn constrain_to_rect(&self, pos: Pos2, rect: Rect) -> Pos2 {
        let padding = 20.0;
        Pos2::new(
            pos.x.clamp(rect.left() + padding, rect.right() - padding),
            pos.y.clamp(rect.top() + padding, rect.bottom() - padding),
        )
    }

    fn draw_grid(&self, painter: &egui::Painter, rect: Rect) {
        let color = egui::Color32::from_gray(40);
        let mut x = rect.left() - (rect.left() % GRID_SPACING);
        while x < rect.right() {
            painter.line_segment(
                [Pos2::new(x, rect.top()), Pos2::new(x, rect.bottom())],
                egui::Stroke::new(1.0, color),
            );
            x += GRID_SPACING;
        }

        let mut y = rect.top() - (rect.top() % GRID_SPACING);
        while y < rect.bottom() {
            painter.line_segment(
                [Pos2::new(rect.left(), y), Pos2::new(rect.right(), y)],
                egui::Stroke::new(1.0, color),
            );
            y += GRID_SPACING;
        }
    }

    fn hit_test_node(&self, system: &tf_project::schema::SystemDef, point: Pos2) -> Option<String> {
        let radius = 20.0;
        for node in &system.nodes {
            if let Some(pos) = self.node_pos(&node.id) {
                let rect = Rect::from_center_size(pos, egui::vec2(radius * 2.0, radius * 2.0));
                if rect.contains(point) {
                    return Some(node.id.clone());
                }
            }
        }
        None
    }

    fn hit_test_edge(&self, system: &tf_project::schema::SystemDef, point: Pos2) -> Option<String> {
        for component in &system.components {
            if let Some(route) = self.layout.edges.get(&component.id) {
                if distance_to_polyline(point, &route.points) < 6.0 {
                    return Some(component.id.clone());
                }
            }
        }
        None
    }

    fn hit_test_control_block(
        &self,
        system: &tf_project::schema::SystemDef,
        point: Pos2,
    ) -> Option<String> {
        if let Some(ref controls) = system.controls {
            // Control blocks are drawn as 40x28 rectangles (half_w=20, half_h=14)
            let half_w = 20.0;
            let half_h = 14.0;
            for block in &controls.blocks {
                if let Some(block_layout) = self.layout.control_blocks.get(&block.id) {
                    let rect = Rect::from_center_size(
                        block_layout.pos,
                        egui::vec2(half_w * 2.0, half_h * 2.0),
                    );
                    if rect.contains(point) {
                        return Some(block.id.clone());
                    }
                }
            }
        }
        None
    }

    fn hit_test_component(
        &self,
        system: &tf_project::schema::SystemDef,
        point: Pos2,
    ) -> Option<String> {
        let hit_radius = 12.0;
        for component in &system.components {
            if let Some(route) = self.layout.edges.get(&component.id) {
                let component_pos = route
                    .component_pos
                    .unwrap_or_else(|| polyline_midpoint(&route.points));
                let rect = Rect::from_center_size(
                    component_pos,
                    egui::vec2(hit_radius * 2.0, hit_radius * 2.0),
                );
                if rect.contains(point) {
                    return Some(component.id.clone());
                }
            }
        }
        None
    }

    fn update_routes_for_component(
        &mut self,
        system: &tf_project::schema::SystemDef,
        component_id: &str,
    ) {
        // When a component is moved, update its edge routing to go through its position
        let component = match system.components.iter().find(|c| c.id == component_id) {
            Some(component) => component,
            None => return,
        };

        let component_pos = match self
            .layout
            .edges
            .get(component_id)
            .and_then(|r| r.component_pos)
        {
            Some(pos) => pos,
            None => return, // If no component position, don't update routing
        };

        let from = match self.node_pos(&component.from_node_id) {
            Some(pos) => pos,
            None => return,
        };
        let to = match self.node_pos(&component.to_node_id) {
            Some(pos) => pos,
            None => return,
        };

        let from_port = offset_port(from, component_pos, 18.0);
        let to_port = offset_port(to, component_pos, 18.0);

        // Route from node -> component -> node
        let route_to_component = autoroute(from_port, component_pos);
        let route_from_component = autoroute(component_pos, to_port);

        let mut points = Vec::new();
        points.extend(normalize_orthogonal(&route_to_component));
        // Skip the last point of first route since it overlaps with first point of second route
        if !points.is_empty() {
            points.pop();
        }
        points.extend(normalize_orthogonal(&route_from_component));

        // Now update the route with the computed points
        if let Some(route) = self.layout.edges.get_mut(component_id) {
            route.points = points;
        }
    }

    fn add_component_between_nodes(
        &mut self,
        system: &mut tf_project::schema::SystemDef,
        kind: ComponentKindChoice,
        from_node_id: &str,
        to_node_id: &str,
    ) -> Option<String> {
        let new_id = next_id("c", system.components.iter().map(|c| &c.id));
        let name = format!("Component {}", system.components.len() + 1);
        let component_kind = default_component_kind(kind);

        system.components.push(tf_project::schema::ComponentDef {
            id: new_id.clone(),
            name,
            kind: component_kind,
            from_node_id: from_node_id.to_string(),
            to_node_id: to_node_id.to_string(),
        });

        self.autoroute_component(system, &new_id);
        Some(new_id)
    }

    fn insert_component_on_edge(
        &mut self,
        system: &mut tf_project::schema::SystemDef,
        component_id: &str,
        new_kind: ComponentKindChoice,
    ) -> Option<String> {
        let (from_node, to_node) = {
            let component = system.components.iter().find(|c| c.id == component_id)?;
            (component.from_node_id.clone(), component.to_node_id.clone())
        };

        let mid_pos = self
            .layout
            .edges
            .get(component_id)
            .map(|route| polyline_midpoint(&route.points))
            .unwrap_or_else(|| {
                let from = self.node_pos(&from_node).unwrap_or(Pos2::ZERO);
                let to = self.node_pos(&to_node).unwrap_or(Pos2::ZERO);
                Pos2::new((from.x + to.x) * 0.5, (from.y + to.y) * 0.5)
            });

        let new_node_id = next_id("n", system.nodes.iter().map(|n| &n.id));
        system.nodes.push(tf_project::schema::NodeDef {
            id: new_node_id.clone(),
            name: format!("Node {}", system.nodes.len() + 1),
            kind: NodeKind::Junction,
        });

        self.layout.ensure_node(&new_node_id, mid_pos);

        let component_id = {
            let component = system
                .components
                .iter_mut()
                .find(|c| c.id == component_id)?;
            component.to_node_id = new_node_id.clone();
            component.id.clone()
        };

        self.autoroute_component(system, &component_id);

        self.add_component_between_nodes(system, new_kind, &new_node_id, &to_node)
    }

    fn add_control_block(
        &mut self,
        system: &mut tf_project::schema::SystemDef,
        kind: ControlBlockKindChoice,
        pos: Pos2,
    ) -> String {
        // Create the block kind first (before mutable borrow)
        let block_kind = default_control_block_kind(kind, system);

        // Ensure controls system exists
        if system.controls.is_none() {
            system.controls = Some(ControlSystemDef {
                blocks: Vec::new(),
                connections: Vec::new(),
            });
        }

        let controls = system.controls.as_mut().unwrap();
        let new_id = next_id("ctrl", controls.blocks.iter().map(|b| &b.id));
        let name = format!(
            "{} {}",
            control_block_kind_label(kind),
            controls.blocks.len() + 1
        );

        controls.blocks.push(ControlBlockDef {
            id: new_id.clone(),
            name,
            kind: block_kind,
        });

        // Add to layout
        self.layout.ensure_control_block(&new_id, pos);

        new_id
    }

    fn add_signal_connection(
        &mut self,
        system: &mut tf_project::schema::SystemDef,
        from_block_id: &str,
        to_block_id: &str,
        to_input: &str,
    ) {
        // Ensure controls system exists
        if system.controls.is_none() {
            return;
        }

        let controls = system.controls.as_mut().unwrap();

        // Check if connection already exists
        let already_exists = controls.connections.iter().any(|c| {
            c.from_block_id == from_block_id
                && c.to_block_id == to_block_id
                && c.to_input == to_input
        });

        if already_exists {
            return; // Don't add duplicate connections
        }

        // Add the connection to the schema
        use tf_project::schema::ControlConnectionDef;
        controls.connections.push(ControlConnectionDef {
            from_block_id: from_block_id.to_string(),
            to_block_id: to_block_id.to_string(),
            to_input: to_input.to_string(),
        });

        // Add simple routing to layout (straight line for now)
        let from_pos = self
            .layout
            .control_blocks
            .get(from_block_id)
            .map(|b| b.pos)
            .unwrap_or(Pos2::ZERO);
        let to_pos = self
            .layout
            .control_blocks
            .get(to_block_id)
            .map(|b| b.pos)
            .unwrap_or(Pos2::ZERO);

        use crate::pid_editor::PidSignalConnection;
        self.layout.signal_connections.push(PidSignalConnection {
            from_block_id: from_block_id.to_string(),
            to_block_id: to_block_id.to_string(),
            to_input: to_input.to_string(),
            points: vec![from_pos, to_pos],
            label_offset: egui::Vec2::ZERO,
        });
    }

    fn delete_control_block(&mut self, system: &mut tf_project::schema::SystemDef, block_id: &str) {
        if let Some(ref mut controls) = system.controls {
            // Remove the block itself
            controls.blocks.retain(|b| b.id != block_id);

            // Remove all connections involving this block
            controls
                .connections
                .retain(|c| c.from_block_id != block_id && c.to_block_id != block_id);

            // Remove from layout
            self.layout.control_blocks.remove(block_id);

            // Remove signal connections from layout
            self.layout
                .signal_connections
                .retain(|sc| sc.from_block_id != block_id && sc.to_block_id != block_id);
        }
    }

    fn delete_signal_connection(
        &mut self,
        system: &mut tf_project::schema::SystemDef,
        index: usize,
    ) {
        // Remove from schema
        if let Some(ref mut controls) = system.controls {
            if index < controls.connections.len() {
                controls.connections.remove(index);
            }
        }

        // Remove from layout
        if index < self.layout.signal_connections.len() {
            self.layout.signal_connections.remove(index);
        }
    }
}

fn pick_record_at_time(records: &[TimeseriesRecord], time_s: f64) -> Option<&TimeseriesRecord> {
    if records.is_empty() {
        return None;
    }

    let mut idx = 0usize;
    for (i, record) in records.iter().enumerate() {
        if record.time_s >= time_s {
            idx = i;
            break;
        }
        idx = i;
    }

    if idx == 0 {
        return records.first();
    }

    let prev = records.get(idx - 1)?;
    let next = records.get(idx)?;
    if (time_s - prev.time_s).abs() <= (next.time_s - time_s).abs() {
        Some(prev)
    } else {
        Some(next)
    }
}

fn distance_to_segment(point: Pos2, a: Pos2, b: Pos2) -> f32 {
    let ab = b - a;
    let ap = point - a;
    let denom = ab.x * ab.x + ab.y * ab.y;
    if denom.abs() < f32::EPSILON {
        return (point - a).length();
    }
    let t = (ap.x * ab.x + ap.y * ab.y) / denom;
    let t = t.clamp(0.0, 1.0);
    let closest = a + ab * t;
    (point - closest).length()
}

fn distance_to_polyline(point: Pos2, points: &[Pos2]) -> f32 {
    if points.len() < 2 {
        return f32::MAX;
    }

    let mut min_dist = f32::MAX;
    for segment in points.windows(2) {
        let dist = distance_to_segment(point, segment[0], segment[1]);
        min_dist = min_dist.min(dist);
    }

    min_dist
}

fn draw_route(
    painter: &egui::Painter,
    route: &crate::pid_editor::PidEdgeRoute,
    color: egui::Color32,
) {
    for segment in route.points.windows(2) {
        let a = segment[0];
        let b = segment[1];
        painter.line_segment([a, b], egui::Stroke::new(2.0, color));
    }
}

fn polyline_midpoint(points: &[Pos2]) -> Pos2 {
    if points.len() < 2 {
        return points.first().copied().unwrap_or(Pos2::ZERO);
    }

    let mut total_len = 0.0;
    for segment in points.windows(2) {
        total_len += (segment[1] - segment[0]).length();
    }
    if total_len <= f32::EPSILON {
        return points[0];
    }

    let half = total_len * 0.5;
    let mut accum = 0.0;
    for segment in points.windows(2) {
        let seg_len = (segment[1] - segment[0]).length();
        if accum + seg_len >= half {
            let t = (half - accum) / seg_len;
            return segment[0] + (segment[1] - segment[0]) * t;
        }
        accum += seg_len;
    }

    *points.last().unwrap_or(&Pos2::ZERO)
}

fn offset_port(from: Pos2, to: Pos2, offset: f32) -> Pos2 {
    let dx = to.x - from.x;
    let dy = to.y - from.y;
    if dx.abs() >= dy.abs() {
        Pos2::new(from.x + offset * dx.signum(), from.y)
    } else {
        Pos2::new(from.x, from.y + offset * dy.signum())
    }
}

fn component_kind_label(kind: ComponentKindChoice) -> &'static str {
    match kind {
        ComponentKindChoice::Orifice => "Orifice",
        ComponentKindChoice::Valve => "Valve",
        ComponentKindChoice::Pipe => "Pipe",
        ComponentKindChoice::Pump => "Pump",
        ComponentKindChoice::Turbine => "Turbine",
    }
}

fn control_block_kind_label(kind: ControlBlockKindChoice) -> &'static str {
    match kind {
        ControlBlockKindChoice::Constant => "Constant",
        ControlBlockKindChoice::MeasuredVariable => "Measured Var",
        ControlBlockKindChoice::PIController => "PI Controller",
        ControlBlockKindChoice::PIDController => "PID Controller",
        ControlBlockKindChoice::FirstOrderActuator => "Actuator",
        ControlBlockKindChoice::ActuatorCommand => "Actuator Cmd",
    }
}

fn default_component_kind(kind: ComponentKindChoice) -> ComponentKind {
    match kind {
        ComponentKindChoice::Orifice => ComponentKind::Orifice {
            cd: 0.8,
            area_m2: 0.0001,
            treat_as_gas: false,
        },
        ComponentKindChoice::Valve => ComponentKind::Valve {
            cd: 0.8,
            area_max_m2: 0.0002,
            position: 1.0,
            law: tf_project::schema::ValveLawDef::Linear,
            treat_as_gas: false,
        },
        ComponentKindChoice::Pipe => ComponentKind::Pipe {
            length_m: 1.0,
            diameter_m: 0.05,
            roughness_m: 1e-5,
            k_minor: 0.0,
            mu_pa_s: 1e-5,
        },
        ComponentKindChoice::Pump => ComponentKind::Pump {
            cd: 0.8,
            area_m2: 0.0002,
            delta_p_pa: 200000.0,
            eta: 0.7,
            treat_as_liquid: true,
        },
        ComponentKindChoice::Turbine => ComponentKind::Turbine {
            cd: 0.8,
            area_m2: 0.0002,
            eta: 0.7,
            treat_as_gas: true,
        },
    }
}

fn default_control_block_kind(
    kind: ControlBlockKindChoice,
    system: &tf_project::schema::SystemDef,
) -> ControlBlockKindDef {
    match kind {
        ControlBlockKindChoice::Constant => ControlBlockKindDef::Constant { value: 0.5 },
        ControlBlockKindChoice::MeasuredVariable => {
            // Default to first node's pressure if available
            let node_id = system
                .nodes
                .first()
                .map(|n| n.id.clone())
                .unwrap_or_else(|| "n1".to_string());
            ControlBlockKindDef::MeasuredVariable {
                reference: MeasuredVariableDef::NodePressure { node_id },
            }
        }
        ControlBlockKindChoice::PIController => ControlBlockKindDef::PIController {
            kp: 0.1,
            ti_s: 10.0,
            out_min: 0.0,
            out_max: 1.0,
            integral_limit: Some(10.0),
            sample_period_s: 0.1,
        },
        ControlBlockKindChoice::PIDController => ControlBlockKindDef::PIDController {
            kp: 0.1,
            ti_s: 10.0,
            td_s: 1.0,
            td_filter_s: 0.1,
            out_min: 0.0,
            out_max: 1.0,
            integral_limit: Some(10.0),
            sample_period_s: 0.1,
        },
        ControlBlockKindChoice::FirstOrderActuator => ControlBlockKindDef::FirstOrderActuator {
            tau_s: 1.0,
            rate_limit_per_s: 1.0,
            initial_position: 0.5,
        },
        ControlBlockKindChoice::ActuatorCommand => {
            // Default to first valve component if available
            let component_id = system
                .components
                .iter()
                .find(|c| matches!(c.kind, ComponentKind::Valve { .. }))
                .map(|c| c.id.clone())
                .unwrap_or_else(|| "c1".to_string());
            ControlBlockKindDef::ActuatorCommand { component_id }
        }
    }
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
fn block_has_output(system: &tf_project::schema::SystemDef, block_id: &str) -> bool {
    if let Some(ref controls) = system.controls {
        if let Some(block) = controls.blocks.iter().find(|b| b.id == block_id) {
            // All blocks except ActuatorCommand have outputs
            return !matches!(block.kind, ControlBlockKindDef::ActuatorCommand { .. });
        }
    }
    false
}

fn get_default_input_for_block(
    system: &tf_project::schema::SystemDef,
    block_id: &str,
) -> Option<String> {
    if let Some(ref controls) = system.controls {
        if let Some(block) = controls.blocks.iter().find(|b| b.id == block_id) {
            // Return the default input name for each block type
            return match &block.kind {
                ControlBlockKindDef::Constant { .. } => None, // No inputs
                ControlBlockKindDef::MeasuredVariable { .. } => None, // No inputs
                ControlBlockKindDef::PIController { .. } => Some("pv".to_string()), // process_value
                ControlBlockKindDef::PIDController { .. } => Some("pv".to_string()), // process_value
                ControlBlockKindDef::FirstOrderActuator { .. } => Some("cmd".to_string()), // command
                ControlBlockKindDef::ActuatorCommand { .. } => Some("pos".to_string()), // position
            };
        }
    }
    None
}
