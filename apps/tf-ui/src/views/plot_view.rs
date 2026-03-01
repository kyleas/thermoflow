//! Advanced plotting workspace with tabbed interface and split layouts.

use crate::plot_workspace::{PlotPanel, PlotWorkspace};
use egui_plot::{Legend, Line, Plot, PlotPoints};
use tf_results::{RunStore, TimeseriesRecord};

/// Split container ID for identifying containers in the tree
type ContainerId = usize;

/// Drop zone position for drag-to-split
#[derive(Debug, Clone, Copy, PartialEq)]
enum DropZone {
    Left,
    Right,
    Top,
    Bottom,
}

/// Split container - can be a leaf (tabs) or a split (horizontal/vertical)
#[derive(Debug, Clone)]
enum SplitContainer {
    Leaf {
        id: ContainerId,
        panel_ids: Vec<String>,
        active_tab: usize,
    },
    HSplit {
        id: ContainerId,
        left: Box<SplitContainer>,
        right: Box<SplitContainer>,
        ratio: f32, // 0.0 to 1.0, position of divider
    },
    VSplit {
        id: ContainerId,
        top: Box<SplitContainer>,
        bottom: Box<SplitContainer>,
        ratio: f32,
    },
}

impl SplitContainer {
    fn new_leaf(id: ContainerId, panel_ids: Vec<String>) -> Self {
        Self::Leaf {
            id,
            panel_ids,
            active_tab: 0,
        }
    }

    fn get_id(&self) -> ContainerId {
        match self {
            Self::Leaf { id, .. } => *id,
            Self::HSplit { id, .. } => *id,
            Self::VSplit { id, .. } => *id,
        }
    }
}

/// Plot view with tabbed multi-plot interface.
pub struct PlotView {
    pub workspace: PlotWorkspace,
    cached_run_id: Option<String>,
    cached_timeseries: Vec<TimeseriesRecord>,
    rename_target: Option<String>,
    rename_input: String,
    show_template_manager: bool,
    template_rename_target: Option<String>,
    template_rename_input: String,
    // Split layout state
    root_container: SplitContainer,
    next_container_id: ContainerId,
    active_container_id: Option<ContainerId>,
    selected_plot_id: Option<String>, // The plot that drives the inspector panel
    // Drag state
    dragging_panel_id: Option<String>,
    dragging_from_container: Option<ContainerId>,
    drop_target: Option<(ContainerId, DropZone)>,
    dragging_divider_id: Option<ContainerId>, // For resizing splits
    divider_ratio_delta: f32, // Track how much the divider has moved
}

impl Default for PlotView {
    fn default() -> Self {
        Self {
            workspace: PlotWorkspace::new(),
            cached_run_id: None,
            cached_timeseries: Vec::new(),
            rename_target: None,
            rename_input: String::new(),
            show_template_manager: false,
            template_rename_target: None,
            template_rename_input: String::new(),
            root_container: SplitContainer::new_leaf(0, Vec::new()),
            next_container_id: 1,
            active_container_id: Some(0),
            selected_plot_id: None,
            dragging_panel_id: None,
            dragging_from_container: None,
            drop_target: None,
            dragging_divider_id: None,
            divider_ratio_delta: 0.0,
        }
    }
}

impl PlotView {
    /// Find a container by ID in the tree (read-only)
    #[allow(dead_code)]
    fn find_container_ref(
        container: &SplitContainer,
        id: ContainerId,
    ) -> Option<&SplitContainer> {
        if container.get_id() == id {
            return Some(container);
        }
        match container {
            SplitContainer::HSplit { left, right, .. } => {
                Self::find_container_ref(left, id)
                    .or_else(|| Self::find_container_ref(right, id))
            }
            SplitContainer::VSplit { top, bottom, .. } => {
                Self::find_container_ref(top, id)
                    .or_else(|| Self::find_container_ref(bottom, id))
            }
            SplitContainer::Leaf { .. } => None,
        }
    }

    /// Get the currently active plot panel ID
    #[allow(dead_code)]
    pub fn get_active_plot_id(&self) -> Option<String> {
        self.selected_plot_id.clone()
    }

    /// Find a container by ID in the tree
    fn find_container_mut(
        container: &mut SplitContainer,
        id: ContainerId,
    ) -> Option<&mut SplitContainer> {
        if container.get_id() == id {
            return Some(container);
        }
        match container {
            SplitContainer::HSplit { left, right, .. } => {
                Self::find_container_mut(left, id)
                    .or_else(|| Self::find_container_mut(right, id))
            }
            SplitContainer::VSplit { top, bottom, .. } => {
                Self::find_container_mut(top, id)
                    .or_else(|| Self::find_container_mut(bottom, id))
            }
            SplitContainer::Leaf { .. } => None,
        }
    }

    /// Apply a drop operation
    fn apply_drop(&mut self, target_container_id: ContainerId, zone: DropZone, panel_id: String) {
        // Remove panel from source container first
        if let Some(source_id) = self.dragging_from_container {
            if source_id != target_container_id {
                if let Some(container) = Self::find_container_mut(&mut self.root_container, source_id) {
                    if let SplitContainer::Leaf { panel_ids, active_tab, .. } = container {
                        if let Some(idx) = panel_ids.iter().position(|id| id == &panel_id) {
                            panel_ids.remove(idx);
                            // Adjust active_tab if needed
                            if *active_tab >= panel_ids.len() && !panel_ids.is_empty() {
                                *active_tab = panel_ids.len() - 1;
                            }
                        }
                    }
                }
            }
        }

        let new_id = self.next_container_id;
        self.next_container_id += 1;

        // Create a new leaf with the dragged panel
        let new_leaf = SplitContainer::new_leaf(new_id, vec![panel_id]);

        // Find and split the target container
        Self::split_container_at(&mut self.root_container, target_container_id, zone, new_leaf, &mut self.next_container_id);
    }

    /// Recursively split a container at the given position
    fn split_container_at(
        container: &mut SplitContainer,
        target_id: ContainerId,
        zone: DropZone,
        new_container: SplitContainer,
        next_id: &mut ContainerId,
    ) {
        if container.get_id() == target_id {
            let old_container = std::mem::replace(
                container,
                SplitContainer::new_leaf(0, Vec::new()),
            );
            
            let split_id = *next_id;
            *next_id += 1;

            *container = match zone {
                DropZone::Left => SplitContainer::HSplit {
                    id: split_id,
                    left: Box::new(new_container),
                    right: Box::new(old_container),
                    ratio: 0.5,
                },
                DropZone::Right => SplitContainer::HSplit {
                    id: split_id,
                    left: Box::new(old_container),
                    right: Box::new(new_container),
                    ratio: 0.5,
                },
                DropZone::Top => SplitContainer::VSplit {
                    id: split_id,
                    top: Box::new(new_container),
                    bottom: Box::new(old_container),
                    ratio: 0.5,
                },
                DropZone::Bottom => SplitContainer::VSplit {
                    id: split_id,
                    top: Box::new(old_container),
                    bottom: Box::new(new_container),
                    ratio: 0.5,
                },
            };
            return;
        }

        // Recursively search in sub-containers
        match container {
            SplitContainer::HSplit { left, right, .. } => {
                if left.get_id() == target_id || Self::contains_id(left, target_id) {
                    Self::split_container_at(left, target_id, zone, new_container, next_id);
                } else if right.get_id() == target_id || Self::contains_id(right, target_id) {
                    Self::split_container_at(right, target_id, zone, new_container, next_id);
                }
            }
            SplitContainer::VSplit { top, bottom, .. } => {
                if top.get_id() == target_id || Self::contains_id(top, target_id) {
                    Self::split_container_at(top, target_id, zone, new_container, next_id);
                } else if bottom.get_id() == target_id || Self::contains_id(bottom, target_id) {
                    Self::split_container_at(bottom, target_id, zone, new_container, next_id);
                }
            }
            SplitContainer::Leaf { .. } => {}
        }
    }

    /// Check if a container subtree contains the target ID
    fn contains_id(container: &SplitContainer, target_id: ContainerId) -> bool {
        if container.get_id() == target_id {
            return true;
        }
        match container {
            SplitContainer::HSplit { left, right, .. } => {
                Self::contains_id(left, target_id) || Self::contains_id(right, target_id)
            }
            SplitContainer::VSplit { top, bottom, .. } => {
                Self::contains_id(top, target_id) || Self::contains_id(bottom, target_id)
            }
            SplitContainer::Leaf { .. } => false,
        }
    }

    /// Apply a divider ratio delta to a specific split container
    fn apply_divider_delta(&mut self, target_id: ContainerId, delta: f32) {
        Self::apply_divider_delta_recursive(&mut self.root_container, target_id, delta);
    }

    fn apply_divider_delta_recursive(
        container: &mut SplitContainer,
        target_id: ContainerId,
        delta: f32,
    ) {
        match container {
            SplitContainer::HSplit { id, ratio, left, right } => {
                if *id == target_id {
                    *ratio = (*ratio + delta).clamp(0.2, 0.8);
                }
                Self::apply_divider_delta_recursive(left, target_id, delta);
                Self::apply_divider_delta_recursive(right, target_id, delta);
            }
            SplitContainer::VSplit { id, ratio, top, bottom } => {
                if *id == target_id {
                    *ratio = (*ratio + delta).clamp(0.2, 0.8);
                }
                Self::apply_divider_delta_recursive(top, target_id, delta);
                Self::apply_divider_delta_recursive(bottom, target_id, delta);
            }
            SplitContainer::Leaf { .. } => {}
        }
    }

    /// Show the plotting workspace with tabbed interface and drag-to-split.
    pub fn show(
        &mut self,
        ui: &mut egui::Ui,
        run_store: &Option<RunStore>,
        selected_run_id: &Option<String>,
    ) {
        ui.heading("Plotting Workspace");
        ui.separator();

        // Sync workspace panels into root container if needed
        if let SplitContainer::Leaf { panel_ids, .. } = &mut self.root_container {
            if panel_ids.is_empty() && !self.workspace.panel_order.is_empty() {
                *panel_ids = self.workspace.panel_order.clone();
            }
        }

        // Initialize selected_plot_id on first render
        if self.selected_plot_id.is_none() {
            if let SplitContainer::Leaf { panel_ids, .. } = &self.root_container {
                if !panel_ids.is_empty() {
                    self.selected_plot_id = Some(panel_ids[0].clone());
                }
            }
        }

        //Ensure workspace has a default plot if empty
        if self.workspace.panels.is_empty() && selected_run_id.is_some() {
            let panel_id = self.workspace
                .create_panel("Plot 1".to_string(), selected_run_id.clone());
            
            // Add to root container
            if let SplitContainer::Leaf { panel_ids, .. } = &mut self.root_container {
                panel_ids.push(panel_id.clone());
                // Set as selected on creation
                if self.selected_plot_id.is_none() {
                    self.selected_plot_id = Some(panel_id);
                }
            }
        }

        if run_store.is_none() || selected_run_id.is_none() {
            ui.label("Select a run from the Runs panel to visualize data");
            return;
        }

        let store = run_store.as_ref().unwrap();
        let run_id = selected_run_id.as_ref().unwrap();

        // Load timeseries if cache is invalid
        if self.cached_run_id.as_ref() != Some(run_id) {
            match store.load_timeseries(run_id) {
                Ok(data) => {
                    self.cached_timeseries = data;
                    self.cached_run_id = Some(run_id.clone());
                }
                Err(e) => {
                    ui.colored_label(
                        egui::Color32::RED,
                        format!("Error loading timeseries: {}", e),
                    );
                    return;
                }
            }
        }

        if self.cached_timeseries.is_empty() {
            ui.label("No data available in this run");
            return;
        }

        // ===== TOOLBAR =====
        let new_plot_requested = ui.horizontal(|ui| {
            let new_plot = ui.button("➕ New Plot").clicked();

            if ui.button("📋 Templates").clicked() {
                self.show_template_manager = !self.show_template_manager;
            }

            ui.separator();
            ui.label(format!("Drag tabs to split views | {} plot(s)", self.workspace.panels.len()));

            new_plot
        }).inner;

        // Handle new plot creation
        if new_plot_requested {
            let title = format!("Plot {}", self.workspace.panels.len() + 1);
            let new_id = self.workspace.create_panel(title, selected_run_id.clone());
            
            // Add to active container
            if let Some(active_id) = self.active_container_id {
                if let Some(container) = Self::find_container_mut(&mut self.root_container, active_id) {
                    if let SplitContainer::Leaf { panel_ids, active_tab, .. } = container {
                        panel_ids.push(new_id);
                        *active_tab = panel_ids.len() - 1;
                    }
                }
            }
        }

        ui.separator();

        // ===== DIALOGS =====
        if self.show_template_manager {
            self.show_template_manager_panel(ui);
            ui.separator();
        }

        if let Some(target_id) = &self.rename_target.clone() {
            ui.horizontal(|ui| {
                ui.label("Rename to:");
                ui.text_edit_singleline(&mut self.rename_input);
                if ui.button("✓").clicked() {
                    self.workspace
                        .rename_panel(target_id, self.rename_input.clone());
                    self.rename_target = None;
                }
                if ui.button("✗").clicked() {
                    self.rename_target = None;
                }
            });
            ui.separator();
        }

        // Series selection is now handled in the Inspector panel

        // ===== MAIN SPLIT LAYOUT =====
        let available_rect = ui.available_rect_before_wrap();
        let root_container_clone = self.root_container.clone();
        
        self.render_split_container(ui, &root_container_clone, available_rect);
        
        // Apply any pending divider ratio updates
        if let Some(divider_id) = self.dragging_divider_id {
            let delta = self.divider_ratio_delta;
            self.apply_divider_delta(divider_id, delta);
            
            // Stop tracking on release
            if ui.input(|i| i.pointer.any_released()) {
                self.dragging_divider_id = None;
                self.divider_ratio_delta = 0.0;
            }
        }

        // Handle drop if drag released
        if let (Some(dragging_id), Some((target_id, zone))) = 
            (&self.dragging_panel_id.clone(), &self.drop_target) {
            if ui.input(|i| i.pointer.any_released()) {
                self.apply_drop(*target_id, *zone, dragging_id.clone());
                self.dragging_panel_id = None;
                self.drop_target = None;
                self.dragging_from_container = None;
            }
        }
    }

    /// Recursively render a split container
    fn render_split_container(
        &mut self,
        ui: &mut egui::Ui,
        container: &SplitContainer,
        rect: egui::Rect,
    ) {
        match container {
            SplitContainer::Leaf { id, panel_ids, active_tab } => {
                self.render_leaf_container(ui, *id, panel_ids, *active_tab, rect);
            }
            SplitContainer::HSplit { id, left, right, ratio } => {
                let split_pos = rect.left() + rect.width() * ratio;
                
                let left_rect =egui::Rect::from_min_max(
                    rect.min,
                    egui::pos2(split_pos - 2.0, rect.max.y),
                );
                let right_rect = egui::Rect::from_min_max(
                    egui::pos2(split_pos + 2.0, rect.min.y),
                    rect.max,
                );

                self.render_split_container(ui, left, left_rect);
                
                // Draw and handle divider interaction
                let divider_rect = egui::Rect::from_min_max(
                    egui::pos2(split_pos - 2.0, rect.min.y),
                    egui::pos2(split_pos + 2.0, rect.max.y),
                );
                
                let divider_response = ui.interact(
                    divider_rect,
                    ui.id().with("divider_h").with(*id),
                    egui::Sense::drag(),
                );
                
                // Highlight divider on hover
                let divider_color = if divider_response.hovered() || divider_response.dragged() {
                    egui::Color32::from_rgb(100, 150, 255)
                } else {
                    egui::Color32::from_gray(60)
                };
                ui.painter().rect_filled(divider_rect, 0.0, divider_color);
                
                // Handle dragging
                if divider_response.dragged() {
                    let drag_delta = ui.input(|i| i.pointer.delta()).x;
                    let new_ratio = (ratio + drag_delta / rect.width()).clamp(0.2, 0.8);
                    
                    // Store the update to apply after rendering
                    if self.dragging_divider_id != Some(*id) {
                        self.dragging_divider_id = Some(*id);
                        self.divider_ratio_delta = 0.0;
                    }
                    self.divider_ratio_delta = new_ratio - ratio;
                }

                self.render_split_container(ui, right, right_rect);
            }
            SplitContainer::VSplit { id, top, bottom, ratio } => {
                let split_pos = rect.top() + rect.height() * ratio;
                
                let top_rect = egui::Rect::from_min_max(
                    rect.min,
                    egui::pos2(rect.max.x, split_pos - 2.0),
                );
                let bottom_rect = egui::Rect::from_min_max(
                    egui::pos2(rect.min.x, split_pos + 2.0),
                    rect.max,
                );

                self.render_split_container(ui, top, top_rect);
                
                // Draw and handle divider interaction
                let divider_rect = egui::Rect::from_min_max(
                    egui::pos2(rect.min.x, split_pos - 2.0),
                    egui::pos2(rect.max.x, split_pos + 2.0),
                );
                
                let divider_response = ui.interact(
                    divider_rect,
                    ui.id().with("divider_v").with(*id),
                    egui::Sense::drag(),
                );
                
                // Highlight divider on hover
                let divider_color = if divider_response.hovered() || divider_response.dragged() {
                    egui::Color32::from_rgb(100, 150, 255)
                } else {
                    egui::Color32::from_gray(60)
                };
                ui.painter().rect_filled(divider_rect, 0.0, divider_color);
                
                // Handle dragging
                if divider_response.dragged() {
                    let drag_delta = ui.input(|i| i.pointer.delta()).y;
                    let new_ratio = (ratio + drag_delta / rect.height()).clamp(0.2, 0.8);
                    
                    // Store the update to apply after rendering
                    if self.dragging_divider_id != Some(*id) {
                        self.dragging_divider_id = Some(*id);
                        self.divider_ratio_delta = 0.0;
                    }
                    self.divider_ratio_delta = new_ratio - ratio;
                }

                self.render_split_container(ui, bottom, bottom_rect);
            }
        }
    }

    /// Render a leaf container with tabs
    fn render_leaf_container(
        &mut self,
        ui: &mut egui::Ui,
        container_id: ContainerId,
        panel_ids: &[String],
        mut active_tab: usize,
        rect: egui::Rect,
    ) {
        // Guard against invalid active_tab
        if active_tab >= panel_ids.len() && !panel_ids.is_empty() {
            active_tab = 0;
        }

        // Draw container background
        ui.painter().rect_filled(rect, 0.0, egui::Color32::from_gray(20));

        // Tab bar height
        let tab_height = 30.0;
        let tab_bar_rect = egui::Rect::from_min_max(
            rect.min,
            egui::pos2(rect.max.x, rect.min.y + tab_height),
        );

        // Draw tab bar background
        ui.painter().rect_filled(tab_bar_rect, 0.0, egui::Color32::from_gray(30));

        // Render tabs
        let mut tab_x = rect.min.x + 5.0;
        let mut new_active_tab = active_tab;
        let mut dragging_started = false;
        let mut dragging_tab_id = None;

        for (idx, panel_id) in panel_ids.iter().enumerate() {
            if let Some(panel) = self.workspace.panels.get(panel_id) {
                // Show as selected only if this plot is the globally selected one
                let is_selected = self.selected_plot_id.as_ref() == Some(panel_id);
                let tab_text = format!("📊 {}", panel.title);
                let tab_width = 120.0;

                let tab_rect = egui::Rect::from_min_max(
                    egui::pos2(tab_x, rect.min.y + 2.0),
                    egui::pos2(tab_x + tab_width, rect.min.y + tab_height - 2.0),
                );

                // Tab background - only blue for selected plot
                let tab_bg = if is_selected {
                    egui::Color32::from_rgb(40, 60, 100)
                } else {
                    egui::Color32::from_gray(35)
                };
                ui.painter().rect_filled(tab_rect, 2.0, tab_bg);

                // Tab text
                let text_color = if is_selected {
                    egui::Color32::from_rgb(100, 180, 255)
                } else {
                    egui::Color32::LIGHT_GRAY
                };
                ui.painter().text(
                    tab_rect.center(),
                    egui::Align2::CENTER_CENTER,
                    &tab_text,
                    egui::FontId::proportional(12.0),
                    text_color,
                );

                // Handle interaction
                let tab_response = ui.interact(
                    tab_rect,
                    ui.id().with(container_id).with(idx),
                    egui::Sense::click_and_drag(),
                );

                if tab_response.clicked() {
                    new_active_tab = idx;
                    self.active_container_id = Some(container_id);
                    self.selected_plot_id = Some(panel_id.clone());
                }

                if tab_response.drag_started() {
                    dragging_started = true;
                    dragging_tab_id = Some(panel_id.clone());
                    self.dragging_from_container = Some(container_id);
                }

                tab_x += tab_width + 5.0;
            }
        }

        // Update active tab if changed
        if new_active_tab != active_tab {
            if let Some(container) = Self::find_container_mut(&mut self.root_container, container_id) {
                if let SplitContainer::Leaf { active_tab: at, .. } = container {
                    *at = new_active_tab;
                }
            }
        }

        // Start dragging if needed
        if dragging_started {
            if let Some(id) = dragging_tab_id {
                self.dragging_panel_id = Some(id);
            }
        }

        // Draw drop zones if dragging (always show for current container to allow splitting)
        if self.dragging_panel_id.is_some() {
            self.draw_drop_zones(ui, container_id, rect);
        }

        // Render active plot
        if !panel_ids.is_empty() && active_tab < panel_ids.len() {
            let active_panel_id = &panel_ids[active_tab];
            if let Some(panel) = self.workspace.panels.get(active_panel_id).cloned() {
                let plot_rect = egui::Rect::from_min_max(
                    egui::pos2(rect.min.x, rect.min.y + tab_height),
                    rect.max,
                );

                // Check if plot has series
                let has_series = !panel.series_selection.node_ids_and_variables.is_empty()
                    || !panel.series_selection.component_ids_and_variables.is_empty()
                    || !panel.series_selection.control_ids.is_empty();

                if !has_series {
                    // Show hint
                    let center = plot_rect.center();
                    ui.painter().text(
                        center,
                        egui::Align2::CENTER_CENTER,
                        "Configure series in the Inspector panel →",
                        egui::FontId::proportional(14.0),
                        egui::Color32::GRAY,
                    );
                } else {
                    // Render actual plot
                    let mut plot_ui = ui.new_child(
                        egui::UiBuilder::new()
                            .max_rect(plot_rect)
                            .layout(egui::Layout::top_down(egui::Align::Min))
                            .id_salt(active_panel_id),
                    );
                    self.render_plot(&mut plot_ui, &panel, &self.cached_timeseries);
                }
            }
        }
    }

    /// Draw drop zones for drag-to-split
    fn draw_drop_zones(&mut self, ui: &mut egui::Ui, container_id: ContainerId, rect: egui::Rect) {
        let zone_size = 50.0;
        let zones = [
            (DropZone::Left, egui::Rect::from_min_size(
                rect.min,
                egui::vec2(zone_size, rect.height()),
            )),
            (DropZone::Right, egui::Rect::from_min_size(
                egui::pos2(rect.max.x - zone_size, rect.min.y),
                egui::vec2(zone_size, rect.height()),
            )),
            (DropZone::Top, egui::Rect::from_min_size(
                rect.min,
                egui::vec2(rect.width(), zone_size),
            )),
            (DropZone::Bottom, egui::Rect::from_min_size(
                egui::pos2(rect.min.x, rect.max.y - zone_size),
                egui::vec2(rect.width(), zone_size),
            )),
        ];

        for (zone, zone_rect) in zones {
            let is_hovered = ui.input(|i| {
                if let Some(pos) = i.pointer.hover_pos() {
                    zone_rect.contains(pos)
                } else {
                    false
                }
            });

            if is_hovered {
                self.drop_target = Some((container_id, zone));
            }

            let alpha = if is_hovered { 120 } else { 40 };
            ui.painter().rect_filled(
                zone_rect,
                4.0,
                egui::Color32::from_rgba_unmultiplied(100, 150, 255, alpha),
            );

            if is_hovered {
                // Draw arrow or indicator
                ui.painter().text(
                    zone_rect.center(),
                    egui::Align2::CENTER_CENTER,
                    match zone {
                        DropZone::Left => "◀",
                        DropZone::Right => "▶",
                        DropZone::Top => "▲",
                        DropZone::Bottom => "▼",
                    },
                    egui::FontId::proportional(24.0),
                    egui::Color32::WHITE,
                );
            }
        }
    }

    /// Render template manager panel.
    fn show_template_manager_panel(&mut self, ui: &mut egui::Ui) {
        ui.group(|ui| {
            ui.label(egui::RichText::new("📋 Template Manager").strong());
            ui.separator();

            if self.workspace.templates.is_empty() {
                ui.label("No templates saved yet.");
            } else {
                for (_template_id, template) in &self.workspace.templates.clone() {
                    ui.horizontal(|ui| {
                        ui.label(format!("📌 {}", template.name));

                        if ui.button("✓ Apply").clicked() {
                            if let Some(selected_id) = &self.workspace.selected_panel_id.clone()
                            {
                                self.workspace
                                    .apply_template_to_panel(selected_id, _template_id);
                            }
                        }

                        if ui.button("+ Create").clicked() {
                            self.workspace.create_panel_from_template(
                                _template_id,
                                self.cached_run_id.clone(),
                            );
                        }

                        if ui.button("✏").clicked() {
                            self.template_rename_target = Some(_template_id.clone());
                            self.template_rename_input = template.name.clone();
                        }

                        if ui.button("🗑").clicked() {
                            self.workspace.templates.remove(_template_id);
                        }
                    });
                }

                // Rename template dialog
                if let Some(target_id) = &self.template_rename_target.clone() {
                    ui.separator();
                    ui.horizontal(|ui| {
                        ui.text_edit_singleline(&mut self.template_rename_input);

                        if ui.button("✓").clicked() {
                            if let Some(template) = self.workspace.templates.get_mut(target_id) {
                                template.name = self.template_rename_input.clone();
                            }
                            self.template_rename_target = None;
                            self.template_rename_input.clear();
                        }

                        if ui.button("✗").clicked() {
                            self.template_rename_target = None;
                            self.template_rename_input.clear();
                        }
                    });
                }
            }
        });
    }

    /// Save current panel as a template.
    #[allow(dead_code)]
    fn save_panel_as_template(&mut self, panel_id: &str) {
        let template_name = if let Some(panel) = self.workspace.panels.get(panel_id) {
            format!("{} Template", panel.title)
        } else {
            "New Template".to_string()
        };

        if let Some(_template_id) = self
            .workspace
            .create_template_from_panel(panel_id, template_name)
        {
            // Template created successfully - show confirmation
            self.show_template_manager = true;
        }
    }

    #[allow(dead_code)]
    fn show_panel_editor(&mut self, ui: &mut egui::Ui, panel: &PlotPanel, panel_id: &str) {
        ui.group(|ui| {
            ui.label(format!("Configure: {}", panel.title));
            ui.separator();

            if let Some(mut series) = self
                .workspace
                .panels
                .get(panel_id)
                .map(|p| p.series_selection.clone())
            {
                // Get available entities from cached timeseries
                let available_nodes: Vec<String> =
                    if let Some(first_record) = self.cached_timeseries.first() {
                        first_record
                            .node_values
                            .iter()
                            .map(|n| n.node_id.clone())
                            .collect()
                    } else {
                        Vec::new()
                    };

                let available_components: Vec<String> =
                    if let Some(first_record) = self.cached_timeseries.first() {
                        first_record
                            .edge_values
                            .iter()
                            .map(|e| e.component_id.clone())
                            .collect()
                    } else {
                        Vec::new()
                    };

                let available_controls: Vec<String> =
                    if let Some(first_record) = self.cached_timeseries.first() {
                        first_record
                            .global_values
                            .control_values
                            .iter()
                            .map(|c| c.id.clone())
                            .collect()
                    } else {
                        Vec::new()
                    };

                // Node selection
                ui.collapsing("Nodes", |ui| {
                    ui.horizontal_wrapped(|ui| {
                        for node_id in &available_nodes {
                            let is_selected = series
                                .node_ids_and_variables
                                .iter()
                                .any(|(n, v)| n == node_id && v == "Pressure");

                            if ui.checkbox(&mut is_selected.clone(), node_id).changed() {
                                if is_selected {
                                    series
                                        .add_node_variable(node_id.clone(), "Pressure".to_string());
                                } else {
                                    series.remove_node_variable(node_id, "Pressure");
                                }
                            }
                        }
                    });
                });

                // Component selection
                ui.collapsing("Components", |ui| {
                    ui.horizontal_wrapped(|ui| {
                        for comp_id in &available_components {
                            let is_selected = series
                                .component_ids_and_variables
                                .iter()
                                .any(|(c, v)| c == comp_id && v == "MassFlow");

                            if ui.checkbox(&mut is_selected.clone(), comp_id).changed() {
                                if is_selected {
                                    series.add_component_variable(
                                        comp_id.clone(),
                                        "MassFlow".to_string(),
                                    );
                                } else {
                                    series.remove_component_variable(comp_id, "MassFlow");
                                }
                            }
                        }
                    });
                });

                // Control block selection
                ui.collapsing("Control Blocks", |ui| {
                    ui.horizontal_wrapped(|ui| {
                        for control_id in &available_controls {
                            let is_selected = series.control_ids.contains(control_id);

                            if ui.checkbox(&mut is_selected.clone(), control_id).changed() {
                                if is_selected {
                                    series.add_control_id(control_id.clone());
                                } else {
                                    series.remove_control_id(control_id);
                                }
                            }
                        }
                    });
                });

                // Update workspace with modified series
                if let Some(panel_mut) = self.workspace.panels.get_mut(panel_id) {
                    panel_mut.series_selection = series;
                }
            }
        });
    }

    /// Render a single plot for a panel.
    fn render_plot(&self, ui: &mut egui::Ui, panel: &PlotPanel, timeseries: &[TimeseriesRecord]) {
        let mut lines = Vec::new();

        // Add node series
        for (node_id, variable) in &panel.series_selection.node_ids_and_variables {
            let mut points = Vec::new();

            for record in timeseries {
                if let Some(node_data) = record.node_values.iter().find(|n| &n.node_id == node_id) {
                    let value = match variable.as_str() {
                        "Pressure" => node_data.p_pa,
                        "Temperature" => node_data.t_k,
                        "Enthalpy" => node_data.h_j_per_kg,
                        "Density" => node_data.rho_kg_m3,
                        _ => None,
                    };

                    if let Some(val) = value {
                        points.push([record.time_s, val]);
                    }
                }
            }

            if !points.is_empty() {
                let plot_points: PlotPoints = points.into();
                let line = Line::new(plot_points).name(format!("{} ({})", node_id, variable));
                lines.push(line);
            }
        }

        // Add component series
        for (component_id, variable) in &panel.series_selection.component_ids_and_variables {
            let mut points = Vec::new();

            for record in timeseries {
                if let Some(edge_data) = record
                    .edge_values
                    .iter()
                    .find(|e| &e.component_id == component_id)
                {
                    let value = match variable.as_str() {
                        "MassFlow" => edge_data.mdot_kg_s,
                        "PressureDrop" => edge_data.delta_p_pa,
                        _ => None,
                    };

                    if let Some(val) = value {
                        points.push([record.time_s, val]);
                    }
                }
            }

            if !points.is_empty() {
                let plot_points: PlotPoints = points.into();
                let line = Line::new(plot_points).name(format!("{} ({})", component_id, variable));
                lines.push(line);
            }
        }

        // Add control series
        for control_id in &panel.series_selection.control_ids {
            let mut points = Vec::new();

            for record in timeseries {
                if let Some(control_data) = record
                    .global_values
                    .control_values
                    .iter()
                    .find(|c| &c.id == control_id)
                {
                    points.push([record.time_s, control_data.value]);
                }
            }

            if !points.is_empty() {
                let plot_points: PlotPoints = points.into();
                let line = Line::new(plot_points).name(control_id);
                lines.push(line);
            }
        }

        // Render the plot
        Plot::new(&panel.id)
            .legend(Legend::default())
            .x_axis_label("Time (s)")
            .show(ui, |plot_ui| {
                for line in lines {
                    plot_ui.line(line);
                }
            });
    }
}
