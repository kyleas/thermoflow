use crate::fluid_picker::SearchableFluidPicker;
use crate::fluid_workspace::{ComputeStatus, FluidWorkspace, StatePoint};
use crate::input_helper::UnitAwareInput;
use std::collections::HashMap;
use tf_fluids::{CoolPropModel, FluidInputPair, Quantity, compute_equilibrium_state};

pub struct FluidView {
    model: CoolPropModel,
    /// Fluid pickers for each state point (keyed by state ID)
    fluid_pickers: HashMap<String, SearchableFluidPicker>,
    /// Unit-aware input helper for managing input fields
    unit_inputs: UnitAwareInput,
}

impl Default for FluidView {
    fn default() -> Self {
        Self {
            model: CoolPropModel::new(),
            fluid_pickers: HashMap::new(),
            unit_inputs: UnitAwareInput::new(),
        }
    }
}

impl FluidView {
    pub fn show(&mut self, ui: &mut egui::Ui, workspace: &mut FluidWorkspace) {
        ui.heading("Fluid Workspace");
        ui.label("Row-based fluid state comparison and property calculator");
        ui.separator();

        // Toolbar
        ui.horizontal(|ui| {
            if ui.button("➕ Add State Point").clicked() {
                workspace.add_state_point();
            }

            ui.label(format!("{} state points", workspace.state_points.len()));
        });

        ui.add_space(8.0);

        // Ensure we have pickers for all state points
        for state in &workspace.state_points {
            self.fluid_pickers
                .entry(state.id.clone())
                .or_insert_with(SearchableFluidPicker::default);
        }

        // Remove pickers for deleted state points
        self.fluid_pickers
            .retain(|id, _| workspace.state_points.iter().any(|s| &s.id == id));

        // Table view
        egui::ScrollArea::both()
            .id_salt("fluid_workspace_table_scroll")
            .show(ui, |ui| {
                self.show_state_table(ui, workspace);
            });

        // Auto-compute any state points that have complete inputs
        self.auto_compute_states(workspace);

        // Handle remove requests (deferred to avoid borrow issues)
        let remove_ids: Vec<String> = workspace
            .state_points
            .iter()
            .filter(|s| {
                s.error_message.as_deref() == Some("REMOVE_REQUESTED")
            })
            .map(|s| s.id.clone())
            .collect();

        for id in remove_ids {
            workspace.remove_state_point(&id);
        }
    }

    fn show_state_table(&mut self, ui: &mut egui::Ui, workspace: &mut FluidWorkspace) {
        use egui_extras::{Column, TableBuilder};

        TableBuilder::new(ui)
            .striped(true)
            .resizable(true)
            .cell_layout(egui::Layout::left_to_right(egui::Align::Center))
            .column(Column::exact(30.0)) // Status indicator
            .column(Column::initial(80.0).at_least(60.0)) // State label
            .column(Column::initial(120.0).at_least(100.0)) // Fluid
            .column(Column::initial(80.0).at_least(60.0)) // Input type  
            .column(Column::initial(120.0).at_least(100.0)) // Input 1
            .column(Column::initial(120.0).at_least(100.0)) // Input 2
            .column(Column::initial(80.0).at_least(60.0)) // Quality (conditional)
            .column(Column::initial(110.0).at_least(90.0)) // Pressure
            .column(Column::initial(110.0).at_least(90.0)) // Temperature
            .column(Column::initial(110.0).at_least(90.0)) // Density
            .column(Column::initial(110.0).at_least(90.0)) // Enthalpy
            .column(Column::initial(110.0).at_least(90.0)) // Entropy
            .column(Column::initial(100.0).at_least(80.0)) // Cp
            .column(Column::initial(100.0).at_least(80.0)) // Cv
            .column(Column::initial(80.0).at_least(60.0)) // γ
            .column(Column::initial(100.0).at_least(80.0)) // Speed of sound
            .column(Column::initial(80.0).at_least(60.0)) // Phase
            .column(Column::exact(40.0)) // Remove button
            .header(22.0, |mut header| {
                header.col(|ui| {
                    ui.strong("●");
                });
                header.col(|ui| {
                    ui.strong("State");
                });
                header.col(|ui| {
                    ui.strong("Fluid");
                });
                header.col(|ui| {
                    ui.strong("Input");
                });
                header.col(|ui| {
                    ui.strong("Value 1");
                });
                header.col(|ui| {
                    ui.strong("Value 2");
                });
                header.col(|ui| {
                    ui.strong("Quality");
                });
                header.col(|ui| {
                    ui.strong("P [Pa]");
                });
                header.col(|ui| {
                    ui.strong("T [K]");
                });
                header.col(|ui| {
                    ui.strong("ρ [kg/m³]");
                });
                header.col(|ui| {
                    ui.strong("h [J/kg]");
                });
                header.col(|ui| {
                    ui.strong("s [J/(kg·K)]");
                });
                header.col(|ui| {
                    ui.strong("cp");
                });
                header.col(|ui| {
                    ui.strong("cv");
                });
                header.col(|ui| {
                    ui.strong("γ");
                });
                header.col(|ui| {
                    ui.strong("a [m/s]");
                });
                header.col(|ui| {
                    ui.strong("Phase");
                });
                header.col(|ui| {
                    ui.strong("");
                });
            })
            .body(|mut body| {
                let state_ids: Vec<String> =
                    workspace.state_points.iter().map(|s| s.id.clone()).collect();

                for state_id in state_ids {
                    if let Some(state) = workspace
                        .state_points
                        .iter_mut()
                        .find(|s| s.id == state_id)
                    {
                        body.row(28.0, |mut row| {
                            self.show_state_row(&mut row, state);
                        });
                    }
                }
            });
    }

    fn show_state_row(
        &mut self,
        row: &mut egui_extras::TableRow,
        state: &mut StatePoint,
    ) {
        // Status indicator
        row.col(|ui| {
            let (color, tooltip) = match state.status {
                ComputeStatus::Success => (egui::Color32::from_rgb(0, 200, 0), "Computed successfully"),
                ComputeStatus::Failed => (egui::Color32::from_rgb(200, 0, 0), "Computation failed"),
                ComputeStatus::Computing => (egui::Color32::from_rgb(255, 165, 0), "Computing..."),
                ComputeStatus::NotComputed => (egui::Color32::GRAY, "Not computed"),
            };
            ui.label(egui::RichText::new("●").color(color).size(16.0))
                .on_hover_text(tooltip);
        });

        // State label (editable)
        row.col(|ui| {
            ui.text_edit_singleline(&mut state.label);
        });

        // Fluid picker
        row.col(|ui| {
            if let Some(picker) = self.fluid_pickers.get_mut(&state.id) {
                if picker.show(ui, format!("fluid_species_{}", state.id), &mut state.species) {
                    state.clear_result();
                }
            }
        });

        // Input pair selector
        row.col(|ui| {
            egui::ComboBox::from_id_salt(format!("input_pair_{}", state.id))
                .selected_text(short_pair_label(state.input_pair))
                .width(60.0)
                .show_ui(ui, |ui| {
                    for pair in [
                        FluidInputPair::PT,
                        FluidInputPair::PH,
                        FluidInputPair::RhoH,
                        FluidInputPair::PS,
                    ] {
                        if ui
                            .selectable_value(&mut state.input_pair, pair, short_pair_label(pair))
                            .changed()
                        {
                            state.clear_result();
                        }
                    }
                });
        });

        // Input 1 (unit-aware)
        row.col(|ui| {
            let quantity = input_quantity_for_pair(state.input_pair, true);
            if ui.text_edit_singleline(&mut state.input_1_text).changed() {
                if let Ok(value) = tf_fluids::parse_quantity(&state.input_1_text, quantity) {
                    state.input_1 = value;
                    state.clear_result();
                }
            }
        });

        // Input 2 (unit-aware)
        row.col(|ui| {
            let quantity = input_quantity_for_pair(state.input_pair, false);
            if ui.text_edit_singleline(&mut state.input_2_text).changed() {
                if let Ok(value) = tf_fluids::parse_quantity(&state.input_2_text, quantity) {
                    state.input_2 = value;
                    state.clear_result();
                }
            }
        });

        // Quality (shown conditionally)
        row.col(|ui| {
            if state.needs_disambiguation {
                if ui
                    .add(
                        egui::DragValue::new(state.quality.get_or_insert(0.5))
                            .speed(0.01)
                            .range(0.0..=1.0)
                            .max_decimals(3),
                    )
                    .changed()
                {
                    state.clear_result();
                }
            } else {
                ui.label("-");
            }
        });

        // Computed properties
        if let Some(result) = &state.last_result {
            row.col(|ui| {
                ui.monospace(fmt_value(result.pressure_pa()));
            });
            row.col(|ui| {
                ui.monospace(fmt_value(result.temperature_k()));
            });
            row.col(|ui| {
                ui.monospace(fmt_value(result.density_kg_m3()));
            });
            row.col(|ui| {
                ui.monospace(fmt_value(result.enthalpy_j_per_kg));
            });
            row.col(|ui| {
                ui.monospace(fmt_value(result.entropy_j_per_kg_k));
            });
            row.col(|ui| {
                ui.monospace(fmt_value(result.cp_j_per_kg_k));
            });
            row.col(|ui| {
                ui.monospace(fmt_value(result.cv_j_per_kg_k));
            });
            row.col(|ui| {
                ui.monospace(fmt_value(result.gamma));
            });
            row.col(|ui| {
                ui.monospace(fmt_value(result.speed_of_sound_m_s()));
            });
            row.col(|ui| {
                ui.label(result.phase.as_ref().unwrap_or(&"N/A".to_string()));
            });
        } else {
            // Empty cells for not-yet-computed properties
            for _ in 0..10 {
                row.col(|ui| {
                    ui.label("-");
                });
            }
        }

        // Remove button
        row.col(|ui| {
            if ui.small_button("🗑").on_hover_text("Remove state").clicked() {
                state.error_message = Some("REMOVE_REQUESTED".to_string());
            }
        });
    }

    fn auto_compute_states(&mut self, workspace: &mut FluidWorkspace) {
        for state in &mut workspace.state_points {
            // Only auto-compute if:
            // 1. Inputs are complete
            // 2. Status is NotComputed (inputs changed or first time)
            if state.inputs_complete() && state.status == ComputeStatus::NotComputed {
                match compute_equilibrium_state(
                    &self.model,
                    state.species,
                    state.input_pair,
                    state.input_1,
                    state.input_2,
                ) {
                    Ok(result) => {
                        state.last_result = Some(result);
                        state.status = ComputeStatus::Success;
                        state.error_message = None;
                        state.needs_disambiguation = false;
                    }
                    Err(e) => {
                        let err_msg = format!("{}", e);
                        // Check if error suggests two-phase ambiguity
                        if err_msg.contains("two-phase")
                            || err_msg.contains("quality")
                            || (state.input_pair == FluidInputPair::PT
                                && err_msg.contains("invalid"))
                        {
                            state.needs_disambiguation = true;
                            state.status = ComputeStatus::Failed;
                            state.error_message = Some("Two-phase: specify quality".to_string());
                        } else {
                            state.last_result = None;
                            state.status = ComputeStatus::Failed;
                            state.error_message = Some(err_msg);
                            state.needs_disambiguation = false;
                        }
                    }
                }
            }
        }
    }
}

fn short_pair_label(pair: FluidInputPair) -> &'static str {
    match pair {
        FluidInputPair::PT => "P-T",
        FluidInputPair::PH => "P-H",
        FluidInputPair::RhoH => "ρ-H",
        FluidInputPair::PS => "P-S",
    }
}

fn input_quantity_for_pair(pair: FluidInputPair, is_first: bool) -> Quantity {
    match (pair, is_first) {
        (FluidInputPair::PT, true) => Quantity::Pressure,
        (FluidInputPair::PT, false) => Quantity::Temperature,
        (FluidInputPair::PH, true) => Quantity::Pressure,
        (FluidInputPair::PH, false) => Quantity::SpecificEnthalpy,
        (FluidInputPair::RhoH, true) => Quantity::Density,
        (FluidInputPair::RhoH, false) => Quantity::SpecificEnthalpy,
        (FluidInputPair::PS, true) => Quantity::Pressure,
        (FluidInputPair::PS, false) => Quantity::SpecificEntropy,
    }
}

fn fmt_value(value: f64) -> String {
    if !value.is_finite() {
        return "NaN".to_string();
    }

    let abs = value.abs();
    if abs >= 1.0e5 || (abs > 0.0 && abs < 1.0e-3) {
        format!("{value:.4e}")
    } else {
        format!("{value:.4}")
    }
}
