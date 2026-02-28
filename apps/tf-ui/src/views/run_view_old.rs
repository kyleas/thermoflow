use tf_app::{analyze_control_loops, ControlLoopAnalysis};
use tf_results::RunStore;

#[derive(Default)]
pub struct RunView {
    show_details: bool,
    show_metrics: bool,
}

impl RunView {
    pub fn show(
        &mut self,
        ui: &mut egui::Ui,
        run_store: &Option<RunStore>,
        selected_system_id: &Option<String>,
        selected_run_id: &mut Option<String>,
    ) {
        ui.heading("Runs");

        if let (Some(store), Some(sys_id)) = (run_store, selected_system_id) {
            match store.list_runs(sys_id) {
                Ok(runs) => {
                    if runs.is_empty() {
                        ui.label("No runs found for this system");
                        ui.separator();
                        ui.label("Run a simulation from the top menu (Simulate → Run Steady)");
                    } else {
                        ui.label(format!("Found {} run(s):", runs.len()));
                        ui.separator();

                        egui::ScrollArea::vertical().show(ui, |ui| {
                            for run in &runs {
                                let is_selected = selected_run_id.as_ref() == Some(&run.run_id);

                                ui.group(|ui| {
                                    if ui
                                        .selectable_label(
                                            is_selected,
                                            format!("📊 Run: {}", run.run_id),
                                        )
                                        .clicked()
                                    {
                                        *selected_run_id = Some(run.run_id.clone());
                                    }

                                    ui.horizontal(|ui| {
                                        ui.label(format!("Type: {:?}", run.run_type));
                                        ui.separator();
                                        ui.label(format!("Time: {}", run.timestamp));
                                    });

                                    if is_selected {
                                        ui.separator();

                                        ui.horizontal(|ui| {
                                            if ui.button("Clear Selection").clicked() {
                                                *selected_run_id = None;
                                            }

                                            if ui
                                                .button(if self.show_details {
                                                    "Hide Details"
                                                } else {
                                                    "Show Details"
                                                })
                                                .clicked()
                                            {
                                                self.show_details = !self.show_details;
                                            }

                                            if ui
                                                .button(if self.show_metrics {
                                                    "Hide Metrics"
                                                } else {
                                                    "Show Metrics"
                                                })
                                                .clicked()
                                            {
                                                self.show_metrics = !self.show_metrics;
                                            }
                                        });

                                        // Show details
                                        if self.show_details {
                                            // Load and show timeseries record count
                                            match store.load_timeseries(&run.run_id) {
                                                Ok(records) => {
                                                    ui.label(format!("Records: {}", records.len()));
                                                }
                                                Err(e) => {
                                                    ui.label(format!("Error loading data: {}", e));
                                                }
                                            }
                                        }

                                        // Show control loop metrics
                                        if self.show_metrics {
                                            match store.load_timeseries(&run.run_id) {
                                                Ok(records) => {
                                                    self.show_loop_metrics(ui, &records);
                                                }
                                                Err(e) => {
                                                    ui.colored_label(
                                                        egui::Color32::RED,
                                                        format!("Error loading metrics: {}", e),
                                                    );
                                                }
                                            }
                                        }
                                    }
                                });

                                ui.add_space(5.0);
                            }
                        });
                    }
                }
                Err(e) => {
                    ui.colored_label(egui::Color32::RED, format!("Error loading runs: {}", e));
                }
            }
        } else {
            ui.label("No system selected or run store not initialized");
        }
    }

    fn show_loop_metrics(&self, ui: &mut egui::Ui, records: &[tf_results::TimeseriesRecord]) {
        ui.separator();
        ui.strong("Control Loop Metrics");

        // Analyze control loops
        let loops = tf_app::analyze_control_loops(records).unwrap_or_default();

        if loops.is_empty() {
            ui.label("No control loops detected in this run");
        } else {
            for loop_analysis in loops {
                ui.group(|ui| {
                    ui.horizontal(|ui| {
                        ui.label(format!("📐 Measured: {}", loop_analysis.measured_id));
                        if let Some(setpoint_id) = &loop_analysis.setpoint_id {
                            ui.label(format!("| Setpoint: {}", setpoint_id));
                        }
                    });

                    // Display metrics in a compact format
                    ui.horizontal(|ui| {
                        ui.vertical(|ui| {
                            if let Some(rise_90) = loop_analysis.metrics.rise_time_90_s {
                                ui.label(format!("Rise time (90%): {:.3} s", rise_90));
                            }
                            if let Some(settle) = loop_analysis.metrics.settling_time_2pct_s {
                                ui.label(format!("Settling time (2%): {:.3} s", settle));
                            }
                        });

                        ui.separator();

                        ui.vertical(|ui| {
                            if let Some(overshoot) = loop_analysis.metrics.overshoot_pct {
                                ui.label(format!("Overshoot: {:.1}%", overshoot));
                            }
                            if let Some(sse) = loop_analysis.metrics.steady_state_error {
                                ui.label(format!("Steady-state error: {:.6}", sse));
                            }
                        });

                        ui.separator();

                        ui.vertical(|ui| {
                            if let Some(max_out) = loop_analysis.metrics.max_controller_output {
                                ui.label(format!("Max controller out: {:.3}", max_out));
                            }
                            if let Some(max_act) = loop_analysis.metrics.max_actuator_pos {
                                ui.label(format!("Max actuator pos: {:.3}", max_act));
                            }
                        });
                    });
                });
            }
        }
    }
