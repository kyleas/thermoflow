use tf_results::RunStore;

#[derive(Default)]
pub struct RunView {
    show_details: bool,
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
                                        });
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
}
