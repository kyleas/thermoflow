use egui::{Pos2, Rect};
use std::collections::HashMap;
use tf_project::schema::{
    ComponentKind, ControlBlockDef, ControlBlockKindDef, ControlSystemDef, LayoutDef,
    MeasuredVariableDef, NodeKind, NodeOverlayDef, OverlaySettingsDef, Project, SystemDef,
};
use tf_results::{RunStore, TimeseriesRecord};

use crate::pid_editor::{
    BoxSelection, Clipboard, CommandHistory, GRID_SPACING, PidEditorState, PidLayout,
    SnapshotCommand, autoroute, copy_selection, delete_selection, draw_component_symbol,
    draw_control_block_symbol, draw_node_symbol, is_orthogonal, normalize_orthogonal,
    paste_clipboard, snap_to_grid,
};
use crate::views::{ComponentKindChoice, ControlBlockKindChoice};

#[derive(Clone, Debug)]
pub enum PaletteItemKind {
    FluidComponent(ComponentKindChoice),
    ControlBlock(ControlBlockKindChoice),
    Node(NodeKind),
}

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
    clipboard: Clipboard,
    paste_sequence: usize,
    command_history: CommandHistory,
    drag_before_system: Option<SystemDef>,
    drag_before_layout: Option<LayoutDef>,
    drag_before_selection: Option<crate::pid_editor::Selection>,
    // Camera/viewport state for pan and zoom
    camera_pan_x: f32,
    camera_pan_y: f32,
    camera_zoom: f32,
    camera_frame_just_loaded: bool,
    // Insertion palette state
    insertion_palette_search: String,
    insertion_palette_active: bool,
    pending_insertion_kind: Option<PaletteItemKind>,
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
            clipboard: Clipboard::default(),
            paste_sequence: 0,
            command_history: CommandHistory::new(),
            drag_before_system: None,
            drag_before_layout: None,
            drag_before_selection: None,
            camera_pan_x: 0.0,
            camera_pan_y: 0.0,
            camera_zoom: 1.0,
            camera_frame_just_loaded: true,
            insertion_palette_search: String::new(),
            insertion_palette_active: false,
            pending_insertion_kind: None,
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
        self.command_history.clear();
        self.drag_before_system = None;
        self.drag_before_layout = None;
        self.drag_before_selection = None;
        self.camera_pan_x = 0.0;
        self.camera_pan_y = 0.0;
        self.camera_zoom = 1.0;
        self.camera_frame_just_loaded = true;
        self.insertion_palette_search.clear();
        self.insertion_palette_active = false;
        self.pending_insertion_kind = None;
    }

    pub fn get_node_overlay(&self, node_id: &str) -> Option<&NodeOverlayDef> {
        self.node_overlays.get(node_id)
    }

    pub fn set_node_overlay(&mut self, node_id: String, overlay: NodeOverlayDef) {
        self.node_overlays.insert(node_id, overlay);
    }

    pub fn selected_control_block_id(&self) -> Option<String> {
        if self.editor.selection.control_blocks.len() == 1
            && self.editor.selection.nodes.is_empty()
            && self.editor.selection.components.is_empty()
        {
            return self.editor.selection.control_blocks.iter().next().cloned();
        }
        None
    }

    pub fn selected_node(&self) -> Option<String> {
        if self.editor.selection.nodes.len() == 1
            && self.editor.selection.components.is_empty()
            && self.editor.selection.control_blocks.is_empty()
        {
            return self.editor.selection.nodes.iter().next().cloned();
        }
        None
    }

    pub fn selected_component(&self) -> Option<String> {
        if self.editor.selection.components.len() == 1
            && self.editor.selection.nodes.is_empty()
            && self.editor.selection.control_blocks.is_empty()
        {
            return self.editor.selection.components.iter().next().cloned();
        }
        None
    }

    pub fn clear_selection(&mut self) {
        self.editor.selection.clear();
        self.selected_control_block_id = None;
        self.selected_signal_connection_index = None;
    }

    fn save_selection_to_clipboard(&mut self, project: &Project, system_id: &str) {
        self.clipboard = copy_selection(project, system_id, &self.layout, &self.editor.selection);
        self.paste_sequence = 0;
    }

    fn current_layout_def(&self, system_id: &str) -> LayoutDef {
        let mut layout_def = LayoutDef {
            system_id: system_id.to_string(),
            nodes: Vec::new(),
            edges: Vec::new(),
            control_blocks: Vec::new(),
            signal_connections: Vec::new(),
            overlay: OverlaySettingsDef::default(),
        };
        self.layout.apply_to_layout_def(&mut layout_def);
        for node_layout in &mut layout_def.nodes {
            node_layout.overlay = self.node_overlays.get(&node_layout.node_id).cloned();
        }
        layout_def
    }

    fn paste_from_clipboard(&mut self, project: &mut Project, system_id: &str) -> bool {
        self.paste_from_clipboard_with_description(project, system_id, "Paste")
    }

    fn paste_from_clipboard_with_description(
        &mut self,
        project: &mut Project,
        system_id: &str,
        description: &str,
    ) -> bool {
        if self.clipboard.is_empty() {
            return false;
        }

        self.save_layout(project, system_id);

        let before_system = match project.systems.iter().find(|s| s.id == system_id) {
            Some(system) => system.clone(),
            None => return false,
        };
        let before_layout = match project.layouts.iter().find(|l| l.system_id == system_id) {
            Some(layout) => layout.clone(),
            None => self.current_layout_def(system_id),
        };
        let before_selection = self.editor.selection.clone();

        self.paste_sequence = self.paste_sequence.saturating_add(1);
        let step = self.paste_sequence as f32;
        let offset = egui::vec2(36.0 * step, 28.0 * step);

        let new_selection = paste_clipboard(project, system_id, &self.clipboard, offset);
        if new_selection.is_empty() {
            return false;
        }

        self.load_layout(project, system_id);
        self.editor.selection = new_selection;
        self.selected_control_block_id = self.selected_control_block_id();
        self.selected_signal_connection_index = None;

        let after_system = match project.systems.iter().find(|s| s.id == system_id) {
            Some(system) => system.clone(),
            None => return false,
        };
        let after_layout = match project.layouts.iter().find(|l| l.system_id == system_id) {
            Some(layout) => layout.clone(),
            None => self.current_layout_def(system_id),
        };
        let after_selection = self.editor.selection.clone();

        self.command_history
            .push_executed(Box::new(SnapshotCommand::new(
                description.to_string(),
                before_system,
                before_layout,
                before_selection,
                after_system,
                after_layout,
                after_selection,
            )));

        true
    }

    fn duplicate_selection(&mut self, project: &mut Project, system_id: &str) -> bool {
        if self.editor.selection.is_empty() {
            return false;
        }

        self.save_selection_to_clipboard(project, system_id);
        self.paste_from_clipboard_with_description(project, system_id, "Duplicate")
    }

    fn delete_selected_items(&mut self, project: &mut Project, system_id: &str) -> bool {
        if self.editor.selection.is_empty() {
            return false;
        }

        self.save_layout(project, system_id);

        let before_system = match project.systems.iter().find(|s| s.id == system_id) {
            Some(system) => system.clone(),
            None => return false,
        };
        let before_layout = match project.layouts.iter().find(|l| l.system_id == system_id) {
            Some(layout) => layout.clone(),
            None => self.current_layout_def(system_id),
        };
        let before_selection = self.editor.selection.clone();

        let _ = delete_selection(project, system_id, &self.editor.selection);
        self.load_layout(project, system_id);
        self.clear_selection();

        let after_system = match project.systems.iter().find(|s| s.id == system_id) {
            Some(system) => system.clone(),
            None => return false,
        };
        let after_layout = match project.layouts.iter().find(|l| l.system_id == system_id) {
            Some(layout) => layout.clone(),
            None => self.current_layout_def(system_id),
        };
        let after_selection = self.editor.selection.clone();

        self.command_history
            .push_executed(Box::new(SnapshotCommand::new(
                "Delete selection",
                before_system,
                before_layout,
                before_selection,
                after_system,
                after_layout,
                after_selection,
            )));

        true
    }

    pub fn clear_control_selection(&mut self) {
        self.editor.selection.control_blocks.clear();
        self.selected_control_block_id = None;
        self.selected_signal_connection_index = None;
    }

    #[allow(clippy::too_many_arguments)]
    pub fn show(
        &mut self,
        ui: &mut egui::Ui,
        project: &mut Option<Project>,
        selected_system_id: &Option<String>,
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
            let mut request_undo = false;
            let mut request_redo = false;

            let shortcuts = ui.input(|input| {
                (
                    input.modifiers.command && input.key_pressed(egui::Key::C),
                    input.modifiers.command && input.key_pressed(egui::Key::V),
                    input.modifiers.command && input.key_pressed(egui::Key::D),
                    input.key_pressed(egui::Key::Delete) || input.key_pressed(egui::Key::Backspace),
                    input.modifiers.command
                        && input.key_pressed(egui::Key::Z)
                        && !input.modifiers.shift,
                    (input.modifiers.command && input.key_pressed(egui::Key::Y))
                        || (input.modifiers.command
                            && input.modifiers.shift
                            && input.key_pressed(egui::Key::Z)),
                    input.key_pressed(egui::Key::Escape),
                )
            });

            // Handle escape key to cancel insertion mode
            if shortcuts.6 {
                if self.pending_insertion_kind.is_some() {
                    self.pending_insertion_kind = None;
                    self.pending_from_node = None;
                }
            }

            if shortcuts.0 {
                self.save_selection_to_clipboard(proj, sys_id);
            }
            if shortcuts.1 && self.paste_from_clipboard(proj, sys_id) {
                should_save_layout = true;
            }
            if shortcuts.2 && self.duplicate_selection(proj, sys_id) {
                should_save_layout = true;
            }
            if shortcuts.3 && self.delete_selected_items(proj, sys_id) {
                should_save_layout = true;
            }
            if shortcuts.4
                && self
                    .command_history
                    .undo(proj, sys_id, &mut self.editor.selection)
            {
                self.load_layout(proj, sys_id);
                self.selected_control_block_id = self.selected_control_block_id();
                self.selected_signal_connection_index = self
                    .editor
                    .selection
                    .signal_connections
                    .iter()
                    .next()
                    .copied();
            }
            if shortcuts.5
                && self
                    .command_history
                    .redo(proj, sys_id, &mut self.editor.selection)
            {
                self.load_layout(proj, sys_id);
                self.selected_control_block_id = self.selected_control_block_id();
                self.selected_signal_connection_index = self
                    .editor
                    .selection
                    .signal_connections
                    .iter()
                    .next()
                    .copied();
            }

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
                    if ui
                        .add_enabled(self.command_history.can_undo(), egui::Button::new("Undo"))
                        .clicked()
                    {
                        request_undo = true;
                    }
                    if ui
                        .add_enabled(self.command_history.can_redo(), egui::Button::new("Redo"))
                        .clicked()
                    {
                        request_redo = true;
                    }
                    ui.separator();
                    if ui.button("Auto-route").clicked() {
                        self.autoroute_all(system);
                        should_save_layout = true;
                    }
                    if ui.button("Snap to grid").clicked() {
                        self.snap_all_nodes();
                        should_save_layout = true;
                    }
                    if ui.button("Frame all").clicked() {
                        // Frame all will be applied to the current canvas rect
                        // during drawing, so just set a flag here
                        self.camera_frame_just_loaded = true;
                    }
                    ui.checkbox(&mut self.grid_enabled, "Grid");
                    ui.checkbox(&mut self.hide_junction_names, "Hide junction names");
                    ui.separator();
                    if ui.button("📋 Quick Add").clicked() {
                        self.insertion_palette_active = !self.insertion_palette_active;
                        self.insertion_palette_search.clear();
                    }
                    if self.pending_insertion_kind.is_some() {
                        ui.label("Inserting... (click canvas to place)");
                        if ui.button("Cancel").clicked() {
                            self.pending_insertion_kind = None;
                        }
                    }
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
                        self.editor.selection.clear();
                    }
                    if let Some(kind) = self.pending_component_kind {
                        ui.label(format!("Pick 2 nodes ({})", component_kind_label(kind)));
                    }
                    if let Some(comp_id) = self.selected_component() {
                        if ui.button("Insert component").clicked() {
                            if let Some(new_id) = self.insert_component_on_edge(
                                system,
                                &comp_id,
                                self.add_component_kind,
                            ) {
                                self.editor.selection.clear();
                                self.editor.selection.add_component(new_id);
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
                        self.editor.selection.clear();
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
                        self.editor.selection.clear();
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
                    if let Some(block_id) = self.selected_control_block_id() {
                        if ui.button(format!("Delete Block '{}'", block_id)).clicked() {
                            let before_system = system.clone();
                            let before_layout = self.current_layout_def(sys_id);
                            let before_selection = self.editor.selection.clone();

                            self.delete_control_block(system, &block_id);
                            self.editor.selection.remove_control_block(&block_id);
                            self.selected_control_block_id = None;
                            should_save_layout = true;

                            let after_system = system.clone();
                            let after_layout = self.current_layout_def(sys_id);
                            let after_selection = self.editor.selection.clone();
                            self.command_history
                                .push_executed(Box::new(SnapshotCommand::new(
                                    "Delete control block",
                                    before_system,
                                    before_layout,
                                    before_selection,
                                    after_system,
                                    after_layout,
                                    after_selection,
                                )));
                        }
                    }
                    if let Some(idx) = self.selected_signal_connection_index {
                        if ui.button("Delete Signal Wire").clicked() {
                            let before_system = system.clone();
                            let before_layout = self.current_layout_def(sys_id);
                            let before_selection = self.editor.selection.clone();

                            self.delete_signal_connection(system, idx);
                            self.selected_signal_connection_index = None;
                            self.editor.selection.signal_connections.clear();
                            should_save_layout = true;

                            let after_system = system.clone();
                            let after_layout = self.current_layout_def(sys_id);
                            let after_selection = self.editor.selection.clone();
                            self.command_history
                                .push_executed(Box::new(SnapshotCommand::new(
                                    "Delete signal wire",
                                    before_system,
                                    before_layout,
                                    before_selection,
                                    after_system,
                                    after_layout,
                                    after_selection,
                                )));
                        }
                    }
                });

                if let Some(node_id) = self.selected_node() {
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
                if let Some(comp_id) = self.selected_component() {
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

                // Show insertion palette if active
                if self.insertion_palette_active {
                    self.show_insertion_palette(ui);
                }

                let (response, painter) =
                    ui.allocate_painter(ui.available_size(), egui::Sense::click_and_drag());
                let rect = response.rect;
                painter.rect_filled(rect, 0.0, egui::Color32::from_gray(30));
                if self.grid_enabled {
                    self.draw_grid(&painter, rect);
                }

                self.ensure_layout(system, rect);

                // Apply frame-all if this is the first draw after loading
                if self.camera_frame_just_loaded {
                    self.frame_all(system, rect);
                    self.camera_frame_just_loaded = false;
                }

                let pointer = response.interact_pointer_pos();
                let is_panning = ui.input(|input| {
                    (response.hovered() && input.pointer.button_down(egui::PointerButton::Middle))
                        || (response.hovered()
                            && input.key_down(egui::Key::Space)
                            && input.pointer.button_down(egui::PointerButton::Primary))
                });

                if response.hovered() {
                    let zoom_delta = ui.input(|input| {
                        let wheel_zoom = if input.smooth_scroll_delta.y.abs() > 0.0 {
                            1.0 + input.smooth_scroll_delta.y * 0.0015
                        } else {
                            1.0
                        };
                        wheel_zoom * input.zoom_delta()
                    });

                    if (zoom_delta - 1.0).abs() > f32::EPSILON {
                        let old_zoom = self.camera_zoom;
                        let new_zoom = (old_zoom * zoom_delta).clamp(0.1, 5.0);
                        if (new_zoom - old_zoom).abs() > f32::EPSILON {
                            let anchor_screen = pointer.unwrap_or(rect.center());
                            let anchor_world = self.screen_to_world(anchor_screen, &rect);

                            self.camera_zoom = new_zoom;
                            self.camera_pan_x =
                                (anchor_screen.x - rect.left()) - anchor_world.x * new_zoom;
                            self.camera_pan_y =
                                (anchor_screen.y - rect.top()) - anchor_world.y * new_zoom;
                        }
                    }
                }

                if is_panning {
                    let pan_delta = ui.input(|input| input.pointer.delta());
                    if pan_delta != egui::Vec2::ZERO {
                        self.camera_pan_x += pan_delta.x;
                        self.camera_pan_y += pan_delta.y;
                    }
                    self.editor.drag_state = None;
                    self.editor.box_selection = None;
                }

                if let Some(ref box_sel) = self.editor.box_selection {
                    let selection_rect = Rect::from_two_pos(
                        self.world_to_screen(box_sel.start_pos, &rect),
                        self.world_to_screen(box_sel.current_pos, &rect),
                    );
                    painter.rect_filled(
                        selection_rect,
                        0.0,
                        egui::Color32::from_rgba_unmultiplied(120, 180, 255, 30),
                    );
                    painter.rect_stroke(
                        selection_rect,
                        0.0,
                        egui::Stroke::new(1.0, egui::Color32::from_rgb(120, 180, 255)),
                    );
                }

                if response.drag_started() && !is_panning {
                    if let Some(pos) = pointer {
                        let world_pos = self.screen_to_world(pos, &rect);
                        let shift_pressed = ui.input(|input| input.modifiers.shift);
                        // Try to hit test components first (they're smaller and should take priority)
                        if let Some(comp_id) = self.hit_test_component(system, world_pos) {
                            let already_selected =
                                self.editor.selection.contains_component(&comp_id);
                            if already_selected && !shift_pressed {
                                // Multi-selection drag: calculate offset to the component being grabbed
                                let route = self.layout.edges.get(&comp_id);
                                let component_pos =
                                    route.and_then(|r| r.component_pos).unwrap_or_else(|| {
                                        route
                                            .map(|r| polyline_midpoint(&r.points))
                                            .unwrap_or(world_pos)
                                    });
                                self.editor.drag_state = Some(crate::pid_editor::DragState {
                                    target: crate::pid_editor::DragTarget::MultiSelection,
                                    start_pos: world_pos,
                                    drag_offset: component_pos - world_pos,
                                    free_move: false,
                                });
                            } else if let Some(route) = self.layout.edges.get(&comp_id) {
                                if !shift_pressed {
                                    self.editor.selection.clear();
                                    self.editor.selection.add_component(comp_id.clone());
                                }
                                let component_pos = route
                                    .component_pos
                                    .unwrap_or_else(|| polyline_midpoint(&route.points));
                                self.editor.drag_state = Some(crate::pid_editor::DragState {
                                    target: crate::pid_editor::DragTarget::Component {
                                        component_id: comp_id.clone(),
                                    },
                                    start_pos: component_pos,
                                    drag_offset: component_pos - world_pos,
                                    free_move: false,
                                });
                            }
                        } else if let Some(block_id) =
                            self.hit_test_control_block(system, world_pos)
                        {
                            let already_selected =
                                self.editor.selection.contains_control_block(&block_id);
                            if already_selected && !shift_pressed {
                                // Multi-selection drag: calculate offset to the block being grabbed
                                if let Some(block_layout) =
                                    self.layout.control_blocks.get(&block_id)
                                {
                                    self.editor.drag_state = Some(crate::pid_editor::DragState {
                                        target: crate::pid_editor::DragTarget::MultiSelection,
                                        start_pos: world_pos,
                                        drag_offset: block_layout.pos - world_pos,
                                        free_move: false,
                                    });
                                }
                            } else if let Some(block_layout) =
                                self.layout.control_blocks.get(&block_id)
                            {
                                if !shift_pressed {
                                    self.editor.selection.clear();
                                    self.editor.selection.add_control_block(block_id.clone());
                                }
                                self.editor.drag_state = Some(crate::pid_editor::DragState {
                                    target: crate::pid_editor::DragTarget::ControlBlock {
                                        block_id: block_id.clone(),
                                    },
                                    start_pos: block_layout.pos,
                                    drag_offset: block_layout.pos - world_pos,
                                    free_move: false,
                                });
                            }
                        } else if let Some(node_id) = self.hit_test_node(system, world_pos) {
                            let already_selected = self.editor.selection.contains_node(&node_id);
                            if already_selected && !shift_pressed {
                                // Multi-selection drag: calculate offset to the node being grabbed
                                if let Some(node_layout) = self.layout.nodes.get(&node_id) {
                                    self.editor.drag_state = Some(crate::pid_editor::DragState {
                                        target: crate::pid_editor::DragTarget::MultiSelection,
                                        start_pos: world_pos,
                                        drag_offset: node_layout.pos - world_pos,
                                        free_move: false,
                                    });
                                }
                            } else if let Some(node_layout) = self.layout.nodes.get(&node_id) {
                                if !shift_pressed {
                                    self.editor.selection.clear();
                                    self.editor.selection.add_node(node_id.clone());
                                }
                                self.editor.drag_state = Some(crate::pid_editor::DragState {
                                    target: crate::pid_editor::DragTarget::Node {
                                        node_id: node_id.clone(),
                                    },
                                    start_pos: node_layout.pos,
                                    drag_offset: node_layout.pos - world_pos,
                                    free_move: false,
                                });
                            }
                        } else {
                            self.editor.box_selection = Some(BoxSelection {
                                start_pos: world_pos,
                                current_pos: world_pos,
                            });
                            if !shift_pressed {
                                self.editor.selection.clear();
                            }
                        }

                        if self.editor.drag_state.is_some() {
                            self.drag_before_system = Some(system.clone());
                            self.drag_before_layout = Some(self.current_layout_def(sys_id));
                            self.drag_before_selection = Some(self.editor.selection.clone());
                        }
                    }
                }

                if response.dragged() && !is_panning {
                    if let (Some(pos), Some(drag_state)) = (pointer, self.editor.drag_state.clone())
                    {
                        let world_pos = self.screen_to_world(pos, &rect);
                        match &drag_state.target {
                            crate::pid_editor::DragTarget::Node { node_id } => {
                                let new_pos = world_pos + drag_state.drag_offset;
                                if let Some(node_layout) = self.layout.nodes.get_mut(node_id) {
                                    node_layout.pos = new_pos;
                                }
                                self.update_routes_for_node(system, node_id);
                            }
                            crate::pid_editor::DragTarget::Component { component_id } => {
                                let new_pos = world_pos + drag_state.drag_offset;
                                if let Some(route) = self.layout.edges.get_mut(component_id) {
                                    route.component_pos = Some(new_pos);
                                }
                                self.update_routes_for_component(system, component_id);
                            }
                            crate::pid_editor::DragTarget::ControlBlock { block_id } => {
                                let new_pos = world_pos + drag_state.drag_offset;
                                if let Some(block) = self.layout.control_blocks.get_mut(block_id) {
                                    block.pos = new_pos;
                                }
                            }
                            crate::pid_editor::DragTarget::MultiSelection => {
                                let delta = world_pos - drag_state.start_pos;
                                if delta != egui::Vec2::ZERO {
                                    let selected_nodes: Vec<String> =
                                        self.editor.selection.nodes.iter().cloned().collect();
                                    let selected_components: Vec<String> =
                                        self.editor.selection.components.iter().cloned().collect();
                                    let selected_blocks: Vec<String> = self
                                        .editor
                                        .selection
                                        .control_blocks
                                        .iter()
                                        .cloned()
                                        .collect();

                                    for node_id in &selected_nodes {
                                        if let Some(node_layout) =
                                            self.layout.nodes.get_mut(node_id)
                                        {
                                            node_layout.pos += delta;
                                        }
                                    }
                                    for component_id in &selected_components {
                                        if let Some(route) = self.layout.edges.get_mut(component_id)
                                        {
                                            let current =
                                                route.component_pos.unwrap_or_else(|| {
                                                    polyline_midpoint(&route.points)
                                                });
                                            route.component_pos = Some(current + delta);
                                        }
                                    }
                                    for block_id in &selected_blocks {
                                        if let Some(block) =
                                            self.layout.control_blocks.get_mut(block_id)
                                        {
                                            block.pos += delta;
                                        }
                                    }

                                    for node_id in &selected_nodes {
                                        self.update_routes_for_node(system, node_id);
                                    }
                                    for component_id in &selected_components {
                                        self.update_routes_for_component(system, component_id);
                                    }

                                    if let Some(active_drag_state) = self.editor.drag_state.as_mut()
                                    {
                                        active_drag_state.start_pos = world_pos;
                                    }
                                }
                            }
                        }
                    }

                    if let Some(pos) = pointer {
                        let pointer_world = self.screen_to_world(pos, &rect);
                        if let Some(box_sel) = self.editor.box_selection.as_mut() {
                            box_sel.current_pos = pointer_world;
                        }
                    }
                }

                if response.drag_stopped() && !is_panning {
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
                                    if self.grid_enabled {
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
                            crate::pid_editor::DragTarget::MultiSelection => {
                                let selected_nodes: Vec<String> =
                                    self.editor.selection.nodes.iter().cloned().collect();
                                let selected_components: Vec<String> =
                                    self.editor.selection.components.iter().cloned().collect();
                                let selected_blocks: Vec<String> = self
                                    .editor
                                    .selection
                                    .control_blocks
                                    .iter()
                                    .cloned()
                                    .collect();

                                if self.grid_enabled {
                                    for node_id in &selected_nodes {
                                        if let Some(node_layout) =
                                            self.layout.nodes.get_mut(node_id)
                                        {
                                            node_layout.pos = snap_to_grid(node_layout.pos);
                                        }
                                    }
                                    for component_id in &selected_components {
                                        if let Some(route) = self.layout.edges.get_mut(component_id)
                                        {
                                            if let Some(pos) = route.component_pos {
                                                route.component_pos = Some(snap_to_grid(pos));
                                            }
                                        }
                                    }
                                    for block_id in &selected_blocks {
                                        if let Some(block) =
                                            self.layout.control_blocks.get_mut(block_id)
                                        {
                                            block.pos = snap_to_grid(block.pos);
                                        }
                                    }
                                }

                                for node_id in &selected_nodes {
                                    self.update_routes_for_node(system, node_id);
                                }
                                for component_id in &selected_components {
                                    self.update_routes_for_component(system, component_id);
                                }
                            }
                        }
                        if let (Some(before_system), Some(before_layout), Some(before_selection)) = (
                            self.drag_before_system.take(),
                            self.drag_before_layout.take(),
                            self.drag_before_selection.take(),
                        ) {
                            let after_system = system.clone();
                            let after_layout = self.current_layout_def(sys_id);
                            let after_selection = self.editor.selection.clone();

                            if before_layout != after_layout {
                                let description = match drag_state.target {
                                    crate::pid_editor::DragTarget::MultiSelection => {
                                        "Move selection"
                                    }
                                    _ => "Move",
                                };
                                self.command_history
                                    .push_executed(Box::new(SnapshotCommand::new(
                                        description,
                                        before_system,
                                        before_layout,
                                        before_selection,
                                        after_system,
                                        after_layout,
                                        after_selection,
                                    )));
                            }
                        }
                        should_save_layout = true;
                    }

                    if let Some(box_sel) = self.editor.box_selection.take() {
                        let selection_rect = box_sel.rect();

                        for node in &system.nodes {
                            if let Some(node_layout) = self.layout.nodes.get(&node.id) {
                                if selection_rect.contains(node_layout.pos) {
                                    self.editor.selection.add_node(node.id.clone());
                                }
                            }
                        }

                        for component in &system.components {
                            if let Some(route) = self.layout.edges.get(&component.id) {
                                let component_pos = route
                                    .component_pos
                                    .unwrap_or_else(|| polyline_midpoint(&route.points));
                                if selection_rect.contains(component_pos) {
                                    self.editor.selection.add_component(component.id.clone());
                                }
                            }
                        }

                        if let Some(ref controls) = system.controls {
                            for block in &controls.blocks {
                                if let Some(block_layout) =
                                    self.layout.control_blocks.get(&block.id)
                                {
                                    if selection_rect.contains(block_layout.pos) {
                                        self.editor.selection.add_control_block(block.id.clone());
                                    }
                                }
                            }
                        }
                    }

                    self.selected_control_block_id = self.selected_control_block_id();
                }

                if response.clicked() && !is_panning {
                    if let Some(click_pos_screen) = pointer {
                        let click_pos = self.screen_to_world(click_pos_screen, &rect);
                        let shift_pressed = ui.input(|input| input.modifiers.shift);
                        // Check if we're placing from the insertion palette
                        if let Some(palette_kind) = self.pending_insertion_kind.clone() {
                            match palette_kind {
                                PaletteItemKind::ControlBlock(block_kind) => {
                                    let snapped_pos = if self.grid_enabled {
                                        snap_to_grid(click_pos)
                                    } else {
                                        click_pos
                                    };
                                    let new_block_id =
                                        self.add_control_block(system, block_kind, snapped_pos);
                                    self.pending_insertion_kind = None;
                                    if !shift_pressed {
                                        self.editor.selection.clear();
                                    }
                                    self.editor.selection.add_control_block(new_block_id);
                                    should_save_layout = true;
                                }
                                PaletteItemKind::FluidComponent(comp_kind) => {
                                    // For components, we follow the two-click model
                                    if let Some(node_id) = self.hit_test_node(system, click_pos) {
                                        if let Some(from_id) = self.pending_from_node.clone() {
                                            if from_id != node_id {
                                                let new_id = self.add_component_between_nodes(
                                                    system, comp_kind, &from_id, &node_id,
                                                );
                                                self.pending_insertion_kind = None;
                                                self.pending_from_node = None;
                                                self.editor.selection.clear();
                                                if let Some(new_component_id) = new_id {
                                                    self.editor
                                                        .selection
                                                        .add_component(new_component_id);
                                                }
                                                should_save_layout = true;
                                            }
                                        } else {
                                            self.pending_from_node = Some(node_id.clone());
                                        }
                                    }
                                }
                                PaletteItemKind::Node(node_kind) => {
                                    let snapped_pos = if self.grid_enabled {
                                        snap_to_grid(click_pos)
                                    } else {
                                        click_pos
                                    };
                                    let new_node_id = self.add_node(system, node_kind, snapped_pos);
                                    self.pending_insertion_kind = None;
                                    if !shift_pressed {
                                        self.editor.selection.clear();
                                    }
                                    self.editor.selection.add_node(new_node_id);
                                    should_save_layout = true;
                                }
                            }
                        } else if let Some(kind) = self.pending_control_block_kind {
                            // Place control block at click position
                            let snapped_pos = if self.grid_enabled {
                                snap_to_grid(click_pos)
                            } else {
                                click_pos
                            };
                            let new_block_id = self.add_control_block(system, kind, snapped_pos);
                            self.pending_control_block_kind = None;
                            if !shift_pressed {
                                self.editor.selection.clear();
                            }
                            self.editor.selection.add_control_block(new_block_id);
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
                            } else {
                                if shift_pressed {
                                    self.editor.selection.toggle_control_block(block_id.clone());
                                } else {
                                    self.editor.selection.clear();
                                    self.editor.selection.add_control_block(block_id.clone());
                                }
                                self.selected_control_block_id = self.selected_control_block_id();
                                self.selected_signal_connection_index = None;
                            }
                        } else if let Some(signal_idx) = self.hit_test_signal_connection(click_pos)
                        {
                            if shift_pressed {
                                self.editor.selection.toggle_signal(signal_idx);
                            } else {
                                self.editor.selection.clear();
                                self.editor.selection.add_signal(signal_idx);
                            }

                            self.selected_signal_connection_index =
                                if self.editor.selection.signal_connections.len() == 1 {
                                    self.editor
                                        .selection
                                        .signal_connections
                                        .iter()
                                        .next()
                                        .copied()
                                } else {
                                    None
                                };
                        } else if let Some(node_id) = self.hit_test_node(system, click_pos) {
                            if let Some(kind) = self.pending_component_kind {
                                if let Some(from_id) = self.pending_from_node.clone() {
                                    if from_id != node_id {
                                        let new_id = self.add_component_between_nodes(
                                            system, kind, &from_id, &node_id,
                                        );
                                        self.pending_component_kind = None;
                                        self.pending_from_node = None;
                                        self.editor.selection.clear();
                                        if let Some(new_component_id) = new_id {
                                            self.editor.selection.add_component(new_component_id);
                                        }
                                        should_save_layout = true;
                                    }
                                } else {
                                    self.pending_from_node = Some(node_id.clone());
                                }
                            } else {
                                if shift_pressed {
                                    self.editor.selection.toggle_node(node_id);
                                } else {
                                    self.editor.selection.clear();
                                    self.editor.selection.add_node(node_id);
                                }
                            }
                        } else if let Some(comp_id) = self.hit_test_edge(system, click_pos) {
                            if shift_pressed {
                                self.editor.selection.toggle_component(comp_id);
                            } else {
                                self.editor.selection.clear();
                                self.editor.selection.add_component(comp_id);
                            }
                        } else {
                            self.editor.selection.clear();
                            self.selected_signal_connection_index = None;
                        }

                        self.selected_control_block_id = self.selected_control_block_id();
                    }
                }

                for component in &system.components {
                    let box_hovered = self
                        .editor
                        .box_selection
                        .as_ref()
                        .and_then(|box_sel| {
                            self.layout.edges.get(&component.id).map(|route| {
                                let component_pos = route
                                    .component_pos
                                    .unwrap_or_else(|| polyline_midpoint(&route.points));
                                box_sel.rect().contains(component_pos)
                            })
                        })
                        .unwrap_or(false);
                    let selected =
                        self.editor.selection.contains_component(&component.id) || box_hovered;
                    let color = if selected {
                        egui::Color32::YELLOW
                    } else {
                        egui::Color32::LIGHT_GRAY
                    };

                    if let Some(route) = self.layout.edges.get_mut(&component.id) {
                        if !is_orthogonal(&route.points) {
                            route.points = normalize_orthogonal(&route.points);
                        }

                        // Extract data we need before transforming (to avoid borrow issues)
                        let points_world = route.points.clone();
                        let component_pos_world = route
                            .component_pos
                            .unwrap_or_else(|| polyline_midpoint(&route.points));
                        let label_offset = route.label_offset;

                        // Now we can transform without holding the mutable borrow
                        let screen_points: Vec<Pos2> = points_world
                            .iter()
                            .map(|&p| self.world_to_screen(p, &rect))
                            .collect();
                        for segment in screen_points.windows(2) {
                            painter.line_segment(
                                [segment[0], segment[1]],
                                egui::Stroke::new(2.0, color),
                            );
                        }

                        let component_pos_screen = self.world_to_screen(component_pos_world, &rect);

                        draw_component_symbol(
                            &painter,
                            &component.kind,
                            component_pos_screen,
                            color,
                            self.camera_zoom,
                        );

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
                                            component_pos_screen + egui::vec2(6.0, -18.0),
                                            egui::Align2::LEFT_TOP,
                                            text,
                                            egui::FontId::proportional(10.0 * self.camera_zoom),
                                            egui::Color32::LIGHT_BLUE,
                                        );
                                    }
                                }
                            }
                        }

                        let label_pos_world = component_pos_world + label_offset;
                        let label_pos_screen = self.world_to_screen(label_pos_world, &rect);
                        painter.text(
                            label_pos_screen + egui::vec2(6.0, 10.0),
                            egui::Align2::LEFT_TOP,
                            &component.name,
                            egui::FontId::proportional(10.0 * self.camera_zoom),
                            egui::Color32::WHITE,
                        );
                    }
                }

                let node_radius = 18.0 * self.camera_zoom;
                for node in &system.nodes {
                    if let Some(node_layout) = self.layout.nodes.get(&node.id) {
                        let box_hovered = self
                            .editor
                            .box_selection
                            .as_ref()
                            .map(|box_sel| box_sel.rect().contains(node_layout.pos))
                            .unwrap_or(false);
                        let selected = self.editor.selection.contains_node(&node.id) || box_hovered;
                        let color = if selected {
                            egui::Color32::YELLOW
                        } else {
                            egui::Color32::LIGHT_BLUE
                        };

                        let is_boundary = system.boundaries.iter().any(|b| b.node_id == node.id)
                            || matches!(node.kind, NodeKind::Atmosphere { .. });
                        let node_pos_screen = self.world_to_screen(node_layout.pos, &rect);
                        draw_node_symbol(
                            &painter,
                            &node.kind,
                            node_pos_screen,
                            node_radius,
                            color,
                            is_boundary,
                        );

                        // Draw node name unless it's a junction and hide_junction_names is enabled
                        let should_show_name =
                            !(self.hide_junction_names && matches!(node.kind, NodeKind::Junction));
                        if should_show_name {
                            let label_pos_world = node_layout.pos + node_layout.label_offset;
                            let label_pos_screen = self.world_to_screen(label_pos_world, &rect);
                            painter.text(
                                label_pos_screen + egui::vec2(0.0, node_radius + 6.0),
                                egui::Align2::CENTER_TOP,
                                &node.name,
                                egui::FontId::proportional(12.0 * self.camera_zoom),
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
                                    let node_pos_screen =
                                        self.world_to_screen(node_layout.pos, &rect);
                                    let mut text_pos = node_pos_screen
                                        + egui::vec2(node_radius + 6.0, -node_radius);
                                    text_pos.x =
                                        text_pos.x.clamp(rect.left() + 4.0, rect.right() - 4.0);
                                    text_pos.y =
                                        text_pos.y.clamp(rect.top() + 4.0, rect.bottom() - 4.0);
                                    painter.text(
                                        text_pos,
                                        egui::Align2::LEFT_CENTER,
                                        text,
                                        egui::FontId::proportional(9.0 * self.camera_zoom),
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
                    for (signal_idx, signal_conn) in
                        self.layout.signal_connections.iter().enumerate()
                    {
                        if signal_conn.points.len() >= 2 {
                            // Draw dashed line for signal connections
                            let signal_color = if self.editor.selection.contains_signal(signal_idx)
                            {
                                egui::Color32::YELLOW
                            } else {
                                egui::Color32::from_rgb(100, 255, 100)
                            }; // Green for signals

                            for i in 0..signal_conn.points.len() - 1 {
                                let from_world = signal_conn.points[i];
                                let to_world = signal_conn.points[i + 1];
                                let from = self.world_to_screen(from_world, &rect);
                                let to = self.world_to_screen(to_world, &rect);

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
                            if let (Some(&from_world), Some(&to_world)) = (
                                signal_conn.points.get(signal_conn.points.len() - 2),
                                signal_conn.points.last(),
                            ) {
                                let from = self.world_to_screen(from_world, &rect);
                                let to = self.world_to_screen(to_world, &rect);
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
                            let box_hovered = self
                                .editor
                                .box_selection
                                .as_ref()
                                .map(|box_sel| box_sel.rect().contains(block_layout.pos))
                                .unwrap_or(false);
                            let selected = self.editor.selection.contains_control_block(&block.id)
                                || box_hovered;
                            let color = if selected {
                                egui::Color32::YELLOW
                            } else {
                                egui::Color32::from_rgb(150, 200, 255) // Light cyan for control blocks
                            };

                            let block_pos_screen = self.world_to_screen(block_layout.pos, &rect);
                            draw_control_block_symbol(
                                &painter,
                                &block.kind,
                                block_pos_screen,
                                color,
                                self.camera_zoom,
                            );

                            // Draw block name
                            let label_pos_world = block_layout.pos + block_layout.label_offset;
                            let label_pos_screen = self.world_to_screen(label_pos_world, &rect);
                            // Control blocks are 28px tall (half_h=14), so offset by 14 + 10 = 24
                            let block_half_height = 14.0 * self.camera_zoom;
                            painter.text(
                                label_pos_screen + egui::vec2(0.0, block_half_height + 10.0),
                                egui::Align2::CENTER_TOP,
                                &block.name,
                                egui::FontId::proportional(10.0 * self.camera_zoom),
                                egui::Color32::WHITE,
                            );
                        }
                    }
                }

                // Draw insertion preview if in insertion mode
                if let (Some(palette_kind), Some(pointer_pos)) =
                    (&self.pending_insertion_kind, pointer)
                {
                    let world_pos = self.screen_to_world(pointer_pos, &rect);
                    let snapped_world_pos = if self.grid_enabled {
                        snap_to_grid(world_pos)
                    } else {
                        world_pos
                    };
                    let preview_screen_pos = self.world_to_screen(snapped_world_pos, &rect);

                    // Draw semi-transparent preview based on item type
                    match palette_kind {
                        PaletteItemKind::Node(node_kind) => {
                            let preview_color = egui::Color32::from_rgba_unmultiplied(
                                120, 255, 120, 150, // Semi-transparent green
                            );
                            draw_node_symbol(
                                &painter,
                                node_kind,
                                preview_screen_pos,
                                18.0 * self.camera_zoom,
                                preview_color,
                                false,
                            );
                        }
                        PaletteItemKind::ControlBlock(block_kind) => {
                            let preview_color = egui::Color32::from_rgba_unmultiplied(
                                150, 200, 255, 150, // Semi-transparent cyan
                            );
                            draw_control_block_symbol(
                                &painter,
                                &default_control_block_kind(*block_kind, system),
                                preview_screen_pos,
                                preview_color,
                                self.camera_zoom,
                            );
                        }
                        PaletteItemKind::FluidComponent(comp_kind) => {
                            // For components, show preview only if we have a start node selected
                            if let Some(from_node_id) = &self.pending_from_node {
                                if let Some(from_pos) = self.node_pos(from_node_id) {
                                    let from_screen = self.world_to_screen(from_pos, &rect);
                                    // Draw line from start node to cursor
                                    painter.line_segment(
                                        [from_screen, preview_screen_pos],
                                        egui::Stroke::new(
                                            2.0,
                                            egui::Color32::from_rgba_unmultiplied(
                                                200, 200, 200, 150,
                                            ),
                                        ),
                                    );
                                    // Draw component symbol at cursor
                                    let preview_color =
                                        egui::Color32::from_rgba_unmultiplied(200, 200, 200, 150);
                                    draw_component_symbol(
                                        &painter,
                                        &default_component_kind(*comp_kind),
                                        preview_screen_pos,
                                        preview_color,
                                        self.camera_zoom,
                                    );
                                }
                            }
                        }
                    }
                }
            } else {
                ui.label("Selected system not found");
            }

            if request_undo
                && self
                    .command_history
                    .undo(proj, sys_id, &mut self.editor.selection)
            {
                self.load_layout(proj, sys_id);
                self.selected_control_block_id = self.selected_control_block_id();
                self.selected_signal_connection_index = self
                    .editor
                    .selection
                    .signal_connections
                    .iter()
                    .next()
                    .copied();
            }
            if request_redo
                && self
                    .command_history
                    .redo(proj, sys_id, &mut self.editor.selection)
            {
                self.load_layout(proj, sys_id);
                self.selected_control_block_id = self.selected_control_block_id();
                self.selected_signal_connection_index = self
                    .editor
                    .selection
                    .signal_connections
                    .iter()
                    .next()
                    .copied();
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
        self.camera_frame_just_loaded = true;

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

    #[allow(dead_code)]
    fn constrain_to_rect(&self, pos: Pos2, rect: Rect) -> Pos2 {
        let padding = 20.0;
        Pos2::new(
            pos.x.clamp(rect.left() + padding, rect.right() - padding),
            pos.y.clamp(rect.top() + padding, rect.bottom() - padding),
        )
    }

    fn draw_grid(&self, painter: &egui::Painter, rect: Rect) {
        let color = egui::Color32::from_gray(40);

        let top_left_world = self.screen_to_world(rect.left_top(), &rect);
        let bottom_right_world = self.screen_to_world(rect.right_bottom(), &rect);

        let world_min_x = top_left_world.x.min(bottom_right_world.x);
        let world_max_x = top_left_world.x.max(bottom_right_world.x);
        let world_min_y = top_left_world.y.min(bottom_right_world.y);
        let world_max_y = top_left_world.y.max(bottom_right_world.y);

        let spacing_screen = GRID_SPACING * self.camera_zoom;
        let min_pixels = 10.0;
        let step_multiplier = if spacing_screen > 0.0 {
            (min_pixels / spacing_screen).ceil().max(1.0)
        } else {
            1.0
        };
        let world_step = GRID_SPACING * step_multiplier;

        let mut x = (world_min_x / world_step).floor() * world_step;
        while x <= world_max_x {
            let a = self.world_to_screen(Pos2::new(x, world_min_y), &rect);
            let b = self.world_to_screen(Pos2::new(x, world_max_y), &rect);
            painter.line_segment(
                [Pos2::new(a.x, rect.top()), Pos2::new(b.x, rect.bottom())],
                egui::Stroke::new(1.0, color),
            );
            x += world_step;
        }

        let mut y = (world_min_y / world_step).floor() * world_step;
        while y <= world_max_y {
            let a = self.world_to_screen(Pos2::new(world_min_x, y), &rect);
            let b = self.world_to_screen(Pos2::new(world_max_x, y), &rect);
            painter.line_segment(
                [Pos2::new(rect.left(), a.y), Pos2::new(rect.right(), b.y)],
                egui::Stroke::new(1.0, color),
            );
            y += world_step;
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

    fn hit_test_signal_connection(&self, point: Pos2) -> Option<usize> {
        for (idx, signal_conn) in self.layout.signal_connections.iter().enumerate() {
            if distance_to_polyline(point, &signal_conn.points) < 8.0 {
                return Some(idx);
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

    fn add_node(
        &mut self,
        system: &mut tf_project::schema::SystemDef,
        kind: NodeKind,
        pos: Pos2,
    ) -> String {
        let new_id = next_id("n", system.nodes.iter().map(|n| &n.id));
        let name = format!("{} {}", node_kind_label(&kind), system.nodes.len() + 1);

        system.nodes.push(tf_project::schema::NodeDef {
            id: new_id.clone(),
            name,
            kind,
        });

        // Add to layout
        self.layout.ensure_node(&new_id, pos);

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

    /// Calculate the bounding box of all nodes, components, and control blocks.
    /// Returns (min_x, min_y, max_x, max_y), or (0, 0, 0, 0) if nothing is present.
    fn compute_content_bounds(
        &self,
        system: &tf_project::schema::SystemDef,
    ) -> (f32, f32, f32, f32) {
        const NODE_SIZE: f32 = 20.0;
        const COMPONENT_SIZE: f32 = 16.0;
        const BLOCK_SIZE: f32 = 40.0;

        let mut min_x = f32::MAX;
        let mut min_y = f32::MAX;
        let mut max_x = f32::MIN;
        let mut max_y = f32::MIN;
        let mut has_content = false;

        // Check nodes
        for node in &system.nodes {
            if let Some(node_layout) = self.layout.nodes.get(&node.id) {
                let pos = node_layout.pos;
                min_x = min_x.min(pos.x - NODE_SIZE);
                min_y = min_y.min(pos.y - NODE_SIZE);
                max_x = max_x.max(pos.x + NODE_SIZE);
                max_y = max_y.max(pos.y + NODE_SIZE);
                has_content = true;
            }
        }

        // Check components
        for component in &system.components {
            if let Some(route) = self.layout.edges.get(&component.id) {
                for point in &route.points {
                    min_x = min_x.min(point.x - COMPONENT_SIZE);
                    min_y = min_y.min(point.y - COMPONENT_SIZE);
                    max_x = max_x.max(point.x + COMPONENT_SIZE);
                    max_y = max_y.max(point.y + COMPONENT_SIZE);
                    has_content = true;
                }
            }
        }

        // Check control blocks
        if let Some(ref controls) = system.controls {
            for block in &controls.blocks {
                if let Some(block_layout) = self.layout.control_blocks.get(&block.id) {
                    let pos = block_layout.pos;
                    min_x = min_x.min(pos.x - BLOCK_SIZE);
                    min_y = min_y.min(pos.y - BLOCK_SIZE);
                    max_x = max_x.max(pos.x + BLOCK_SIZE);
                    max_y = max_y.max(pos.y + BLOCK_SIZE);
                    has_content = true;
                }
            }
        }

        if has_content {
            (min_x, min_y, max_x, max_y)
        } else {
            (0.0, 0.0, 0.0, 0.0)
        }
    }

    /// Adjust camera pan and zoom to fit all content in the viewport.
    /// Call this after loading a layout or when the user requests fit-all.
    fn frame_all(&mut self, system: &tf_project::schema::SystemDef, viewport_rect: Rect) {
        let (min_x, min_y, max_x, max_y) = self.compute_content_bounds(system);

        // If there's no content, reset to zero
        if min_x == f32::MAX {
            self.camera_pan_x = 0.0;
            self.camera_pan_y = 0.0;
            self.camera_zoom = 1.0;
            return;
        }

        let content_width = max_x - min_x;
        let content_height = max_y - min_y;

        // Add padding (margins around the content)
        const MARGIN: f32 = 50.0;
        let padded_width = content_width + 2.0 * MARGIN;
        let padded_height = content_height + 2.0 * MARGIN;

        // Calculate zoom to fit content in viewport
        let viewport_width = viewport_rect.width();
        let viewport_height = viewport_rect.height();

        let zoom_x = viewport_width / padded_width;
        let zoom_y = viewport_height / padded_height;
        let new_zoom = zoom_x.min(zoom_y).clamp(0.1, 5.0);

        // Calculate pan to center the content
        // Use rect-relative coordinates (0,0 = top-left of canvas)
        let content_center_x = (min_x + max_x) * 0.5;
        let content_center_y = (min_y + max_y) * 0.5;

        let viewport_center_x = viewport_width * 0.5;
        let viewport_center_y = viewport_height * 0.5;

        self.camera_zoom = new_zoom;
        self.camera_pan_x = viewport_center_x - content_center_x * new_zoom;
        self.camera_pan_y = viewport_center_y - content_center_y * new_zoom;
    }

    fn show_insertion_palette(&mut self, ui: &mut egui::Ui) {
        let mut palette_open = self.insertion_palette_active;

        egui::Window::new("Quick Add")
            .open(&mut palette_open)
            .default_size([300.0, 400.0])
            .vscroll(true)
            .show(ui.ctx(), |ui| {
                // Search box
                ui.label("Search:");
                ui.text_edit_singleline(&mut self.insertion_palette_search);

                ui.separator();

                // Collect available items
                let mut items: Vec<(String, PaletteItemKind)> = Vec::new();

                // Fluid components
                let component_kinds = vec![
                    (ComponentKindChoice::Orifice, "Orifice"),
                    (ComponentKindChoice::Valve, "Valve"),
                    (ComponentKindChoice::Pipe, "Pipe"),
                    (ComponentKindChoice::Pump, "Pump"),
                    (ComponentKindChoice::Turbine, "Turbine"),
                ];

                for (kind, name) in component_kinds {
                    items.push((
                        format!("Component: {}", name),
                        PaletteItemKind::FluidComponent(kind),
                    ));
                }

                // Control blocks
                let block_kinds = vec![
                    (ControlBlockKindChoice::Constant, "Constant"),
                    (
                        ControlBlockKindChoice::MeasuredVariable,
                        "Measured Variable",
                    ),
                    (ControlBlockKindChoice::PIController, "PI Controller"),
                    (ControlBlockKindChoice::PIDController, "PID Controller"),
                    (ControlBlockKindChoice::FirstOrderActuator, "Actuator"),
                    (ControlBlockKindChoice::ActuatorCommand, "Command"),
                ];

                for (kind, name) in block_kinds {
                    items.push((
                        format!("Control Block: {}", name),
                        PaletteItemKind::ControlBlock(kind),
                    ));
                }

                // Nodes
                let node_kinds = vec![
                    (NodeKind::Junction, "Junction"),
                    (
                        NodeKind::ControlVolume {
                            volume_m3: 1.0,
                            initial: Default::default(),
                        },
                        "Control Volume",
                    ),
                    (
                        NodeKind::Atmosphere {
                            pressure_pa: 101325.0,
                            temperature_k: 288.15,
                        },
                        "Atmosphere",
                    ),
                ];

                for (kind, name) in node_kinds {
                    items.push((format!("Node: {}", name), PaletteItemKind::Node(kind)));
                }

                // Filter items by search
                let search_lower = self.insertion_palette_search.to_lowercase();
                let filtered: Vec<_> = items
                    .into_iter()
                    .filter(|(label, _)| label.to_lowercase().contains(&search_lower))
                    .collect();

                // Display palette items
                for (label, kind) in &filtered {
                    if ui.button(label).clicked() {
                        self.pending_insertion_kind = Some(kind.clone());
                    }
                }

                if filtered.is_empty() && !self.insertion_palette_search.is_empty() {
                    ui.label("No items found");
                }
            });

        // Close palette if an item was selected
        if self.pending_insertion_kind.is_some() {
            palette_open = false;
        }
        self.insertion_palette_active = palette_open;
    }

    /// Convert world coordinates to screen coordinates using the camera transform.
    #[allow(dead_code)]
    /// Convert world coordinates to screen coordinates using the camera transform.
    /// The rect parameter is used to convert from rect-relative to absolute coordinates.
    fn world_to_screen(&self, world_pos: Pos2, rect: &Rect) -> Pos2 {
        Pos2::new(
            world_pos.x * self.camera_zoom + self.camera_pan_x + rect.left(),
            world_pos.y * self.camera_zoom + self.camera_pan_y + rect.top(),
        )
    }

    /// Convert screen coordinates to world coordinates using the inverse camera transform.
    /// The rect parameter is used to convert from absolute to rect-relative coordinates.
    fn screen_to_world(&self, screen_pos: Pos2, rect: &Rect) -> Pos2 {
        if self.camera_zoom > 0.0 {
            Pos2::new(
                (screen_pos.x - rect.left() - self.camera_pan_x) / self.camera_zoom,
                (screen_pos.y - rect.top() - self.camera_pan_y) / self.camera_zoom,
            )
        } else {
            Pos2::ZERO
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

fn node_kind_label(kind: &NodeKind) -> &'static str {
    match kind {
        NodeKind::Junction => "Junction",
        NodeKind::ControlVolume { .. } => "Control Volume",
        NodeKind::Atmosphere { .. } => "Atmosphere",
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
