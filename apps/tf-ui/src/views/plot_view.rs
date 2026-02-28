use egui_plot::{Legend, Line, Plot, PlotPoints};
use tf_results::{RunStore, TimeseriesRecord};

#[derive(Default)]
pub struct PlotView {
    entity_type: EntityType,
    selected_node_ids: Vec<String>,
    selected_component_ids: Vec<String>,
    selected_control_ids: Vec<String>,
    selected_variable: PlotVariable,
    selected_component_variable: ComponentVariable,
    cached_run_id: Option<String>,
    cached_timeseries: Vec<TimeseriesRecord>,
}

#[derive(Debug, Clone, Copy, PartialEq, Default)]
enum EntityType {
    #[default]
    Nodes,
    Components,
    Controls,
}

#[derive(Debug, Clone, Copy, PartialEq, Default)]
enum PlotVariable {
    #[default]
    Pressure,
    Temperature,
    Enthalpy,
    Density,
}

#[derive(Debug, Clone, Copy, PartialEq, Default)]
enum ComponentVariable {
    #[default]
    MassFlow,
    PressureDrop,
}

impl PlotView {
    pub fn show(
        &mut self,
        ui: &mut egui::Ui,
        run_store: &Option<RunStore>,
        selected_run_id: &Option<String>,
    ) {
        ui.heading("Plot View");
        ui.separator();

        if run_store.is_none() || selected_run_id.is_none() {
            ui.label("Select a run from the Runs panel to visualize data");
            return;
        }

        let store = run_store.as_ref().unwrap();
        let run_id = selected_run_id.as_ref().unwrap();

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

        // Entity type selector
        ui.horizontal(|ui| {
            ui.label("Entity type:");
            ui.selectable_value(&mut self.entity_type, EntityType::Nodes, "Nodes");
            ui.selectable_value(&mut self.entity_type, EntityType::Components, "Components");
            ui.selectable_value(
                &mut self.entity_type,
                EntityType::Controls,
                "Control Blocks",
            );
        });

        ui.separator();

        // Clone timeseries to avoid borrow checker issues
        let timeseries = self.cached_timeseries.clone();

        match self.entity_type {
            EntityType::Nodes => self.show_node_plot(ui, &timeseries),
            EntityType::Components => self.show_component_plot(ui, &timeseries),
            EntityType::Controls => self.show_control_plot(ui, &timeseries),
        }
    }

    fn show_node_plot(&mut self, ui: &mut egui::Ui, timeseries: &[TimeseriesRecord]) {
        // Get available nodes
        let available_nodes: Vec<String> = if let Some(first_record) = timeseries.first() {
            first_record
                .node_values
                .iter()
                .map(|n| n.node_id.clone())
                .collect()
        } else {
            Vec::new()
        };

        // Node selection
        ui.horizontal(|ui| {
            ui.label("Select nodes to plot:");
            egui::ComboBox::from_id_salt("node_selector")
                .selected_text(format!("{} node(s) selected", self.selected_node_ids.len()))
                .show_ui(ui, |ui| {
                    for node_id in &available_nodes {
                        let mut is_selected = self.selected_node_ids.contains(node_id);
                        if ui.checkbox(&mut is_selected, node_id).changed() {
                            if is_selected {
                                self.selected_node_ids.push(node_id.clone());
                            } else {
                                self.selected_node_ids.retain(|id| id != node_id);
                            }
                        }
                    }
                });

            if ui.button("Clear").clicked() {
                self.selected_node_ids.clear();
            }
        });

        // Variable selection
        ui.horizontal(|ui| {
            ui.label("Variable:");
            ui.selectable_value(
                &mut self.selected_variable,
                PlotVariable::Pressure,
                "Pressure (Pa)",
            );
            ui.selectable_value(
                &mut self.selected_variable,
                PlotVariable::Temperature,
                "Temperature (K)",
            );
            ui.selectable_value(
                &mut self.selected_variable,
                PlotVariable::Enthalpy,
                "Enthalpy (J/kg)",
            );
            ui.selectable_value(
                &mut self.selected_variable,
                PlotVariable::Density,
                "Density (kg/m³)",
            );
        });

        ui.separator();

        // Plot data
        if self.selected_node_ids.is_empty() {
            ui.label("Select at least one node to plot");
        } else {
            // Build plot lines
            let mut lines = Vec::new();

            for node_id in &self.selected_node_ids {
                let mut points = Vec::new();

                for record in timeseries {
                    if let Some(node_data) =
                        record.node_values.iter().find(|n| &n.node_id == node_id)
                    {
                        let value = match self.selected_variable {
                            PlotVariable::Pressure => node_data.p_pa,
                            PlotVariable::Temperature => node_data.t_k,
                            PlotVariable::Enthalpy => node_data.h_j_per_kg,
                            PlotVariable::Density => node_data.rho_kg_m3,
                        };

                        if let Some(val) = value {
                            points.push([record.time_s, val]);
                        }
                    }
                }

                if !points.is_empty() {
                    let plot_points: PlotPoints = points.into();
                    let line = Line::new(plot_points).name(node_id);
                    lines.push(line);
                }
            }

            let y_label = match self.selected_variable {
                PlotVariable::Pressure => "Pressure (Pa)",
                PlotVariable::Temperature => "Temperature (K)",
                PlotVariable::Enthalpy => "Enthalpy (J/kg)",
                PlotVariable::Density => "Density (kg/m³)",
            };

            Plot::new("main_plot")
                .legend(Legend::default())
                .x_axis_label("Time (s)")
                .y_axis_label(y_label)
                .show(ui, |plot_ui| {
                    for line in lines {
                        plot_ui.line(line);
                    }
                });
        }
    }

    fn show_component_plot(&mut self, ui: &mut egui::Ui, timeseries: &[TimeseriesRecord]) {
        // Get available components
        let available_components: Vec<String> = if let Some(first_record) = timeseries.first() {
            first_record
                .edge_values
                .iter()
                .map(|e| e.component_id.clone())
                .collect()
        } else {
            Vec::new()
        };

        // Component selection
        ui.horizontal(|ui| {
            ui.label("Select components to plot:");
            egui::ComboBox::from_id_salt("component_selector")
                .selected_text(format!(
                    "{} component(s) selected",
                    self.selected_component_ids.len()
                ))
                .show_ui(ui, |ui| {
                    for comp_id in &available_components {
                        let mut is_selected = self.selected_component_ids.contains(comp_id);
                        if ui.checkbox(&mut is_selected, comp_id).changed() {
                            if is_selected {
                                self.selected_component_ids.push(comp_id.clone());
                            } else {
                                self.selected_component_ids.retain(|id| id != comp_id);
                            }
                        }
                    }
                });

            if ui.button("Clear").clicked() {
                self.selected_component_ids.clear();
            }
        });

        // Variable selection
        ui.horizontal(|ui| {
            ui.label("Variable:");
            ui.selectable_value(
                &mut self.selected_component_variable,
                ComponentVariable::MassFlow,
                "Mass Flow (kg/s)",
            );
            ui.selectable_value(
                &mut self.selected_component_variable,
                ComponentVariable::PressureDrop,
                "Pressure Drop (Pa)",
            );
        });

        ui.separator();

        // Plot data
        if self.selected_component_ids.is_empty() {
            ui.label("Select at least one component to plot");
        } else {
            // Build plot lines
            let mut lines = Vec::new();

            for comp_id in &self.selected_component_ids {
                let mut points = Vec::new();

                for record in timeseries {
                    if let Some(edge_data) = record
                        .edge_values
                        .iter()
                        .find(|e| &e.component_id == comp_id)
                    {
                        let value = match self.selected_component_variable {
                            ComponentVariable::MassFlow => edge_data.mdot_kg_s,
                            ComponentVariable::PressureDrop => edge_data.delta_p_pa,
                        };

                        if let Some(val) = value {
                            points.push([record.time_s, val]);
                        }
                    }
                }

                if !points.is_empty() {
                    let plot_points: PlotPoints = points.into();
                    let line = Line::new(plot_points).name(comp_id);
                    lines.push(line);
                }
            }

            let y_label = match self.selected_component_variable {
                ComponentVariable::MassFlow => "Mass Flow (kg/s)",
                ComponentVariable::PressureDrop => "Pressure Drop (Pa)",
            };

            Plot::new("main_plot")
                .legend(Legend::default())
                .x_axis_label("Time (s)")
                .y_axis_label(y_label)
                .show(ui, |plot_ui| {
                    for line in lines {
                        plot_ui.line(line);
                    }
                });
        }
    }

    fn show_control_plot(&mut self, ui: &mut egui::Ui, timeseries: &[TimeseriesRecord]) {
        // Get available control blocks
        let available_controls: Vec<String> = if let Some(first_record) = timeseries.first() {
            first_record
                .global_values
                .control_values
                .iter()
                .map(|c| c.id.clone())
                .collect()
        } else {
            Vec::new()
        };

        // Control block selection
        ui.horizontal(|ui| {
            ui.label("Select control blocks to plot:");
            egui::ComboBox::from_id_salt("control_selector")
                .selected_text(format!(
                    "{} control(s) selected",
                    self.selected_control_ids.len()
                ))
                .show_ui(ui, |ui| {
                    for control_id in &available_controls {
                        let mut is_selected = self.selected_control_ids.contains(control_id);
                        if ui.checkbox(&mut is_selected, control_id).changed() {
                            if is_selected {
                                self.selected_control_ids.push(control_id.clone());
                            } else {
                                self.selected_control_ids.retain(|id| id != control_id);
                            }
                        }
                    }
                });

            if ui.button("Clear").clicked() {
                self.selected_control_ids.clear();
            }
        });

        ui.separator();

        // Plot data
        if self.selected_control_ids.is_empty() {
            ui.label("Select at least one control block to plot");
        } else {
            // Build plot lines
            let mut lines = Vec::new();

            for control_id in &self.selected_control_ids {
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

            Plot::new("main_plot")
                .legend(Legend::default())
                .x_axis_label("Time (s)")
                .y_axis_label("Control Output")
                .show(ui, |plot_ui| {
                    for line in lines {
                        plot_ui.line(line);
                    }
                });
        }
    }
}
