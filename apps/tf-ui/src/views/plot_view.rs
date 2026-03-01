//! Advanced plotting workspace with tabbed interface.

use crate::plot_workspace::{PlotPanel, PlotWorkspace};
use egui_plot::{Legend, Line, Plot, PlotPoints};
use tf_results::{RunStore, TimeseriesRecord};

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
    // Series config panel state
    show_series_config: bool,
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
            show_series_config: true,
        }
    }
}

impl PlotView {
    /// Show the plotting workspace with tabbed interface.
    pub fn show(
        &mut self,
        ui: &mut egui::Ui,
        run_store: &Option<RunStore>,
        selected_run_id: &Option<String>,
    ) {
        ui.heading("Plotting Workspace");
        ui.separator();

        // Ensure workspace has a default plot if empty
        if self.workspace.panels.is_empty() && selected_run_id.is_some() {
            self.workspace
                .create_panel("Plot 1".to_string(), selected_run_id.clone());
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

        // ===== TAB BAR =====
        ui.horizontal(|ui| {
            // New plot button
            if ui.button("➕").clicked() {
                let title = format!("Plot {}", self.workspace.panels.len() + 1);
                self.workspace.create_panel(title, selected_run_id.clone());
            }

            ui.separator();

            // Tab buttons for each plot
            for panel_id in &self.workspace.panel_order.clone() {
                if let Some(panel) = self.workspace.panels.get(panel_id) {
                    let is_selected = self.workspace.selected_panel_id.as_ref() == Some(panel_id);
                    
                    let tab_response = if is_selected {
                        ui.button(
                            egui::RichText::new(format!("📊 {}", panel.title))
                                .strong()
                                .color(egui::Color32::YELLOW)
                        )
                    } else {
                        ui.button(&panel.title)
                    };

                    if tab_response.clicked() {
                        self.workspace.select_panel(Some(panel_id.clone()));
                    }
                }
            }

            ui.separator();

            // Templates button
            if ui.button("📋 Templates").clicked() {
                self.show_template_manager = !self.show_template_manager;
            }
        });

        ui.separator();

        // ===== TEMPLATE MANAGER DIALOG =====
        if self.show_template_manager {
            self.show_template_manager_panel(ui);
            ui.separator();
        }

        // ===== RENAME DIALOG =====
        if let Some(target_id) = &self.rename_target.clone() {
            ui.horizontal(|ui| {
                ui.label("Rename to:");
                ui.text_edit_singleline(&mut self.rename_input);

                if ui.button("✓").clicked() {
                    self.workspace
                        .rename_panel(target_id, self.rename_input.clone());
                    self.rename_target = None;
                    self.rename_input.clear();
                }

                if ui.button("✗").clicked() {
                    self.rename_target = None;
                    self.rename_input.clear();
                }
            });
            ui.separator();
        }

        // ===== MAIN CONTENT AREA =====
        if let Some(selected_id) = self.workspace.selected_panel_id.clone() {
            if let Some(panel) = self.workspace.panels.get(&selected_id).cloned() {
                // Horizontal split: plot on left, config on right
                ui.horizontal(|ui| {
                    // Left side: plot area (4/5 width)
                    ui.vertical(|ui| {
                        ui.allocate_ui(
                            egui::vec2(ui.available_width() * 0.8, ui.available_height()),
                            |ui| {
                                self.render_plot(ui, &panel, &self.cached_timeseries);
                            },
                        );
                    });

                    ui.separator();

                    // Right side: series configuration (1/5 width)
                    egui::ScrollArea::vertical()
                        .auto_shrink([false; 2])
                        .show(ui, |ui| {
                            ui.vertical(|ui| {
                                ui.horizontal(|ui| {
                                    ui.label(egui::RichText::new("Configure").strong());
                                    if ui.small_button("└⁻").clicked() {
                                        self.show_series_config = !self.show_series_config;
                                    }
                                });

                                if self.show_series_config {
                                    ui.separator();
                                    self.show_panel_editor(ui, &panel, &selected_id);

                                    ui.separator();
                                    ui.horizontal(|ui| {
                                        if ui.button("✏ Rename").clicked() {
                                            self.rename_target = Some(selected_id.clone());
                                            self.rename_input = panel.title.clone();
                                        }

                                        if ui.button("🗑 Delete").clicked() {
                                            self.workspace.delete_panel(&selected_id);
                                        }

                                        if ui.button("💾 Save as Template").clicked() {
                                            self.save_panel_as_template(&selected_id);
                                        }
                                    });
                                }
                            });
                        });
                });
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
