use egui_plot::{Legend, Line, Plot, PlotPoints};
use tf_results::{RunStore, TimeseriesRecord};

#[derive(Default)]
pub struct PlotView {
    selected_node_ids: Vec<String>,
    selected_variable: PlotVariable,
    cached_run_id: Option<String>,
    cached_timeseries: Vec<TimeseriesRecord>,
}

#[derive(Debug, Clone, Copy, PartialEq, Default)]
enum PlotVariable {
    #[default]
    Pressure,
    Temperature,
    Enthalpy,
    Density,
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

        let timeseries = &self.cached_timeseries;

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
}
