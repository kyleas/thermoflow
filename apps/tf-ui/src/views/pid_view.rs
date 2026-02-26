use egui::Pos2;
use std::collections::HashMap;
use tf_project::schema::{ComponentKind, NodeOverlayDef, OverlaySettingsDef, Project};
use tf_results::{RunStore, TimeseriesRecord};

pub struct PidView {
    last_system_id: Option<String>,
    // Store layout positions
    node_positions: HashMap<String, Pos2>,
    // Store per-node overlay settings
    node_overlays: HashMap<String, NodeOverlayDef>,
    dragging_node: Option<String>,
    drag_offset: egui::Vec2,
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
            node_positions: HashMap::new(),
            node_overlays: HashMap::new(),
            dragging_node: None,
            drag_offset: egui::Vec2::ZERO,
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
        self.node_positions.clear();
        self.node_overlays.clear();
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
                            self.time_s = self
                                .cached_records
                                .last()
                                .map(|r| r.time_s)
                                .unwrap_or(0.0);
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
                            let dt = ui.ctx().input(|i| i.unstable_dt) as f64;
                            self.time_s += dt * self.play_speed.max(0.1);
                            if self.time_s >= max_time_s {
                                self.time_s = max_time_s;
                                self.is_playing = false;
                            }
                        }

                        let clamped = self.time_s.clamp(0.0, max_time_s);
                        self.time_s = clamped;

                        if ui.button(if self.is_playing { "Pause" } else { "Play" }).clicked() {
                            self.is_playing = !self.is_playing;
                        }
                        ui.add(
                            egui::DragValue::new(&mut self.play_speed)
                                .speed(0.1)
                                .range(0.1..=10.0)
                                .prefix("x"),
                        );
                        ui.add(
                            egui::Slider::new(&mut self.time_s, 0.0..=max_time_s)
                                .text("t (s)"),
                        );
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

                // Draw the diagram
                let (response, painter) =
                    ui.allocate_painter(ui.available_size(), egui::Sense::click_and_drag());

                let rect = response.rect;
                painter.rect_filled(rect, 0.0, egui::Color32::from_gray(30));

                // Initialize node positions if needed
                if self.node_positions.is_empty() {
                    self.init_default_positions(&system.nodes, rect);
                } else {
                    self.ensure_node_positions(&system.nodes, rect);
                }

                // Handle dragging
                if response.dragged() {
                    if let Some(node_id) = &self.dragging_node {
                        if let Some(pos) = self.node_positions.get_mut(node_id) {
                            let delta = response.drag_delta();
                            pos.x += delta.x;
                            pos.y += delta.y;
                            // Constrain to canvas with padding
                            let padding = 100.0; // Enough space for node + overlay text
                            pos.x = pos.x.clamp(rect.left() + padding, rect.right() - padding);
                            pos.y = pos.y.clamp(rect.top() + padding, rect.bottom() - padding);
                        }
                    }
                }

                if response.drag_stopped() {
                    if self.dragging_node.is_some() {
                        should_save_layout = true;
                    }
                    self.dragging_node = None;
                }

                // Draw components (edges)
                for component in &system.components {
                    if let (Some(&from_pos), Some(&to_pos)) = (
                        self.node_positions.get(&component.from_node_id),
                        self.node_positions.get(&component.to_node_id),
                    ) {
                        let selected = selected_component_id.as_ref() == Some(&component.id);
                        let color = if selected {
                            egui::Color32::YELLOW
                        } else {
                            egui::Color32::LIGHT_GRAY
                        };

                        let dir = (to_pos - from_pos).normalized();
                        let mid = from_pos + (to_pos - from_pos) * 0.5;
                        let perp = egui::vec2(-dir.y, dir.x);

                        // Draw line segments (not through the symbol)
                        painter.line_segment([from_pos, to_pos], egui::Stroke::new(2.0, color));

                        // Draw component-specific symbol at midpoint
                        match &component.kind {
                            ComponentKind::Pump { .. } => {
                                // Pump: circle with impeller
                                let radius = 12.0;
                                painter.circle_stroke(mid, radius, egui::Stroke::new(2.5, color));
                                // Impeller blades
                                for i in 0..4 {
                                    let angle = (i as f32) * std::f32::consts::FRAC_PI_2;
                                    let blade_dir =
                                        egui::vec2(angle.cos(), angle.sin()).normalized();
                                    painter.line_segment(
                                        [mid, mid + blade_dir * radius * 0.7],
                                        egui::Stroke::new(2.0, color),
                                    );
                                }
                            }
                            ComponentKind::Valve { .. } => {
                                // Valve: bowtie/hourglass shape
                                let size = 12.0;
                                let p1 = mid + perp * size - dir * size * 0.7;
                                let p2 = mid - perp * size - dir * size * 0.7;
                                let p3 = mid - perp * size + dir * size * 0.7;
                                let p4 = mid + perp * size + dir * size * 0.7;

                                painter.line_segment([p1, p3], egui::Stroke::new(2.5, color));
                                painter.line_segment([p2, p4], egui::Stroke::new(2.5, color));
                                painter.line_segment([p1, p2], egui::Stroke::new(2.5, color));
                                painter.line_segment([p3, p4], egui::Stroke::new(2.5, color));
                            }
                            ComponentKind::Orifice { .. } => {
                                // Orifice: thin plate perpendicular to flow
                                let size = 14.0;
                                let p1 = mid + perp * size;
                                let p2 = mid - perp * size;
                                painter.line_segment([p1, p2], egui::Stroke::new(3.0, color));
                                // Small opening in the middle
                                let opening = size * 0.3;
                                let p3 = mid + perp * opening;
                                let p4 = mid - perp * opening;
                                painter.line_segment(
                                    [p3, p4],
                                    egui::Stroke::new(1.5, egui::Color32::from_gray(30)),
                                );
                            }
                            ComponentKind::Turbine { .. } => {
                                // Turbine: circle with angled blades
                                let radius = 12.0;
                                painter.circle_stroke(mid, radius, egui::Stroke::new(2.5, color));
                                // Turbine blades (angled)
                                for i in 0..6 {
                                    let angle = (i as f32) * std::f32::consts::TAU / 6.0;
                                    let blade_start =
                                        egui::vec2(angle.cos(), angle.sin()).normalized();
                                    let blade_end =
                                        egui::vec2((angle + 0.5).cos(), (angle + 0.5).sin())
                                            .normalized();
                                    painter.line_segment(
                                        [
                                            mid + blade_start * radius * 0.5,
                                            mid + blade_end * radius * 0.9,
                                        ],
                                        egui::Stroke::new(2.0, color),
                                    );
                                }
                            }
                            ComponentKind::Pipe { .. } => {
                                // Pipe: just draw a simple arrow (default)
                                painter.line_segment(
                                    [mid, mid - dir * 5.0 + perp * 3.0],
                                    egui::Stroke::new(2.0, color),
                                );
                                painter.line_segment(
                                    [mid, mid - dir * 5.0 - perp * 3.0],
                                    egui::Stroke::new(2.0, color),
                                );
                            }
                        }

                        // Show flow rate if available
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
                                            mid + egui::vec2(5.0, -15.0),
                                            egui::Align2::LEFT_TOP,
                                            text,
                                            egui::FontId::proportional(10.0),
                                            egui::Color32::LIGHT_BLUE,
                                        );
                                    }
                                }
                            }
                        }

                        // Label
                        painter.text(
                            mid + egui::vec2(5.0, 10.0),
                            egui::Align2::LEFT_TOP,
                            &component.name,
                            egui::FontId::proportional(10.0),
                            egui::Color32::WHITE,
                        );
                    }
                }

                // Draw nodes
                let node_radius = 15.0;
                for node in &system.nodes {
                    if let Some(&pos) = self.node_positions.get(&node.id) {
                        let selected = selected_node_id.as_ref() == Some(&node.id);
                        let color = if selected {
                            egui::Color32::YELLOW
                        } else {
                            egui::Color32::LIGHT_BLUE
                        };

                        painter.circle_filled(pos, node_radius, color);
                        painter.circle_stroke(
                            pos,
                            node_radius,
                            egui::Stroke::new(2.0, egui::Color32::WHITE),
                        );

                        // Node label
                        painter.text(
                            pos + egui::vec2(0.0, node_radius + 5.0),
                            egui::Align2::CENTER_TOP,
                            &node.name,
                            egui::FontId::proportional(12.0),
                            egui::Color32::WHITE,
                        );

                        // Show overlay data if available
                        if let Some(ref data) = run_data {
                            if let Some(node_data) =
                                data.node_values.iter().find(|n| n.node_id == node.id)
                            {
                                // Use per-node overlay settings if available, otherwise use global
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
                                    let mut text_pos =
                                        pos + egui::vec2(node_radius + 5.0, -node_radius);
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

                        // Handle click selection
                        let node_rect = egui::Rect::from_center_size(
                            pos,
                            egui::vec2(node_radius * 2.0, node_radius * 2.0),
                        );
                        if response.clicked() {
                            if let Some(click_pos) = response.interact_pointer_pos() {
                                if node_rect.contains(click_pos) {
                                    *selected_node_id = Some(node.id.clone());
                                    *selected_component_id = None;
                                }
                            }
                        }

                        // Start dragging node
                        if response.drag_started() {
                            if let Some(drag_start) = response.interact_pointer_pos() {
                                if node_rect.contains(drag_start) {
                                    self.dragging_node = Some(node.id.clone());
                                    self.drag_offset = pos - drag_start;
                                }
                            }
                        }
                    }
                }

                // Handle component selection (click on edge)
                if response.clicked() {
                    if let Some(click_pos) = response.interact_pointer_pos() {
                        let mut clicked_component = None;
                        for component in &system.components {
                            if let (Some(&from_pos), Some(&to_pos)) = (
                                self.node_positions.get(&component.from_node_id),
                                self.node_positions.get(&component.to_node_id),
                            ) {
                                // Check if click is near the line segment
                                let dist = distance_to_segment(click_pos, from_pos, to_pos);
                                if dist < 5.0 {
                                    clicked_component = Some(component.id.clone());
                                    break;
                                }
                            }
                        }

                        if let Some(comp_id) = clicked_component {
                            *selected_component_id = Some(comp_id);
                            *selected_node_id = None;
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

    fn init_default_positions(&mut self, nodes: &[tf_project::schema::NodeDef], rect: egui::Rect) {
        let padding = 100.0;
        let center = rect.center();
        let max_radius = (rect.width().min(rect.height()) * 0.5) - padding;
        let radius = max_radius * 0.6;

        for (i, node) in nodes.iter().enumerate() {
            let angle = (i as f32) * std::f32::consts::TAU / (nodes.len() as f32);
            let pos = center + egui::vec2(angle.cos() * radius, angle.sin() * radius);
            self.node_positions.insert(node.id.clone(), pos);
        }
    }

    fn ensure_node_positions(&mut self, nodes: &[tf_project::schema::NodeDef], rect: egui::Rect) {
        let padding = 100.0;
        let center = rect.center();
        let max_radius = (rect.width().min(rect.height()) * 0.5) - padding;
        let radius = max_radius * 0.6;
        let count = nodes.len().max(1) as f32;

        for (i, node) in nodes.iter().enumerate() {
            if self.node_positions.contains_key(&node.id) {
                continue;
            }
            let angle = (i as f32) * std::f32::consts::TAU / count;
            let pos = center + egui::vec2(angle.cos() * radius, angle.sin() * radius);
            self.node_positions.insert(node.id.clone(), pos);
        }
    }

    fn load_layout(&mut self, project: &Project, system_id: &str) {
        self.node_positions.clear();
        self.node_overlays.clear();

        if let Some(layout) = project.layouts.iter().find(|l| l.system_id == system_id) {
            for node_layout in &layout.nodes {
                self.node_positions.insert(
                    node_layout.node_id.clone(),
                    Pos2::new(node_layout.x, node_layout.y),
                );
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
            // Update existing layout
            layout.nodes.clear();
            for (node_id, pos) in &self.node_positions {
                let overlay = self.node_overlays.get(node_id).cloned();
                layout.nodes.push(tf_project::schema::NodeLayout {
                    node_id: node_id.clone(),
                    x: pos.x,
                    y: pos.y,
                    overlay,
                });
            }
        } else {
            // Create new layout
            let mut nodes = Vec::new();
            for (node_id, pos) in &self.node_positions {
                let overlay = self.node_overlays.get(node_id).cloned();
                nodes.push(tf_project::schema::NodeLayout {
                    node_id: node_id.clone(),
                    x: pos.x,
                    y: pos.y,
                    overlay,
                });
            }

            project.layouts.push(tf_project::schema::LayoutDef {
                system_id: system_id.to_string(),
                nodes,
                edges: Vec::new(),
                overlay: tf_project::schema::OverlaySettingsDef::default(),
            });
        }
    }
}

fn pick_record_at_time(
    records: &[TimeseriesRecord],
    time_s: f64,
) -> Option<&TimeseriesRecord> {
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
        return records.get(0);
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
    let t = (ap.x * ab.x + ap.y * ab.y) / (ab.x * ab.x + ab.y * ab.y);
    let t = t.clamp(0.0, 1.0);
    let closest = a + ab * t;
    (point - closest).length()
}
