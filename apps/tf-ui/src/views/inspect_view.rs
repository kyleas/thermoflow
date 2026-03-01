use tf_project::schema::{
    BoundaryDef, ComponentDef, ComponentKind, ControlBlockDef, ControlBlockKindDef,
    MeasuredVariableDef, NodeDef, NodeKind, OverlaySettingsDef, Project,
};

use crate::views::pid_view::PidView;

#[derive(Default)]
pub struct InspectView {
    overlay: OverlaySettingsDef,
    new_node_kind: NodeKindChoice,
    new_component_kind: ComponentKindChoice,
    new_component_from: Option<String>,
    new_component_to: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub enum NodeKindChoice {
    #[default]
    Junction,
    ControlVolume,
    Atmosphere,
}

#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub enum ComponentKindChoice {
    #[default]
    Orifice,
    Valve,
    Pipe,
    Pump,
    Turbine,
}

#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub enum ControlBlockKindChoice {
    #[default]
    Constant,
    MeasuredVariable,
    PIController,
    PIDController,
    FirstOrderActuator,
    ActuatorCommand,
}

#[derive(Default)]
pub struct InspectActions {
    pub add_node: Option<NodeKindChoice>,
    pub delete_node_id: Option<String>,
    pub add_component: Option<NewComponentSpec>,
    pub delete_component_id: Option<String>,
    pub needs_recompile: bool,
}

pub struct NewComponentSpec {
    pub kind: ComponentKindChoice,
    pub from_node_id: String,
    pub to_node_id: String,
}

impl InspectView {
    pub fn overlay_settings(&self) -> &OverlaySettingsDef {
        &self.overlay
    }

    #[allow(clippy::too_many_arguments)]
    pub fn show(
        &mut self,
        ui: &mut egui::Ui,
        project: &mut Option<Project>,
        selected_system_id: &Option<String>,
        selected_node_id: &Option<String>,
        selected_component_id: &Option<String>,
        selected_control_block_id: &Option<String>,
        pid_view: &mut PidView,
        active_plot_id: Option<&String>,
        plot_workspace: Option<&mut super::super::plot_workspace::PlotWorkspace>,
    ) -> InspectActions {
        let mut actions = InspectActions::default();

        ui.heading("Inspector");

        // Show plot series configuration if viewing plots
        if let (Some(plot_id), Some(plot_ws)) = (active_plot_id, plot_workspace) {
            ui.label("📊 Plot Configuration");
            ui.separator();
            
            if let Some(panel) = plot_ws.panels.get_mut(plot_id) {
                ui.label(format!("Plot: {}", panel.title));
                ui.separator();
                
                ui.label("Series Selection:");
                ui.separator();
                
                egui::ScrollArea::vertical()
                    .max_height(400.0)
                    .show(ui, |ui| {
                        self.show_plot_series_editor(ui, panel, project.as_ref());
                    });
            }
            
            ui.separator();
            return actions;
        }

        if project.is_none() {
            ui.label("No project loaded");
            return actions;
        }

        if selected_system_id.is_none() {
            ui.label("No system selected");
            return actions;
        }

        let proj = project.as_mut().unwrap();
        let sys_id = selected_system_id.as_ref().unwrap();

        if let Some(system) = proj.systems.iter_mut().find(|s| &s.id == sys_id) {
            let node_ids: Vec<String> = system.nodes.iter().map(|n| n.id.clone()).collect();
            let component_ids: Vec<String> =
                system.components.iter().map(|c| c.id.clone()).collect();
            let valve_component_ids: Vec<String> = system
                .components
                .iter()
                .filter(|c| matches!(c.kind, ComponentKind::Valve { .. }))
                .map(|c| c.id.clone())
                .collect();

            // First show selection details
            ui.separator();
            ui.heading("Selection");
            if let Some(node_id) = selected_node_id {
                if let Some(node) = system.nodes.iter_mut().find(|n| &n.id == node_id) {
                    actions.needs_recompile |=
                        self.show_node_inspector(ui, node, &mut system.boundaries, pid_view);
                    ui.separator();
                    if ui.button("Delete Node").clicked() {
                        actions.delete_node_id = Some(node_id.clone());
                    }
                }
            } else if let Some(comp_id) = selected_component_id {
                if let Some(component) = system.components.iter_mut().find(|c| &c.id == comp_id) {
                    actions.needs_recompile |= self.show_component_inspector(ui, component);
                    ui.separator();
                    if ui.button("Delete Component").clicked() {
                        actions.delete_component_id = Some(comp_id.clone());
                    }
                }
            } else if let Some(block_id) = selected_control_block_id {
                if let Some(controls) = system.controls.as_mut() {
                    if let Some(block) = controls.blocks.iter_mut().find(|b| &b.id == block_id) {
                        actions.needs_recompile |= self.show_control_block_inspector(
                            ui,
                            block,
                            &node_ids,
                            &component_ids,
                            &valve_component_ids,
                        );
                    } else {
                        ui.label("Selected control block no longer exists");
                    }
                } else {
                    ui.label("No control system in selected model");
                }
            } else {
                ui.label("No selection - click a node, component, or control block");
            }

            // Add node section
            ui.separator();
            ui.heading("Add Node");
            ui.horizontal(|ui| {
                ui.label("Type:");
                egui::ComboBox::from_id_salt("new_node_kind")
                    .selected_text(self.new_node_kind.label())
                    .show_ui(ui, |ui| {
                        ui.selectable_value(
                            &mut self.new_node_kind,
                            NodeKindChoice::Junction,
                            "Junction",
                        );
                        ui.selectable_value(
                            &mut self.new_node_kind,
                            NodeKindChoice::ControlVolume,
                            "Control Volume",
                        );
                    });
            });
            if ui.button("+ Add New Node").clicked() {
                actions.add_node = Some(self.new_node_kind);
            }

            // Add component section
            ui.separator();
            ui.heading("Add Component");
            if node_ids.len() < 2 {
                ui.label("Add at least two nodes to create components");
            } else {
                if self
                    .new_component_from
                    .as_ref()
                    .map(|id| !node_ids.contains(id))
                    .unwrap_or(true)
                {
                    self.new_component_from = node_ids.first().cloned();
                }

                if self
                    .new_component_to
                    .as_ref()
                    .map(|id| !node_ids.contains(id))
                    .unwrap_or(true)
                {
                    self.new_component_to = node_ids.get(1).cloned();
                }

                ui.horizontal(|ui| {
                    ui.label("Kind:");
                    egui::ComboBox::from_id_salt("new_component_kind")
                        .selected_text(self.new_component_kind.label())
                        .show_ui(ui, |ui| {
                            ui.selectable_value(
                                &mut self.new_component_kind,
                                ComponentKindChoice::Orifice,
                                "Orifice",
                            );
                            ui.selectable_value(
                                &mut self.new_component_kind,
                                ComponentKindChoice::Valve,
                                "Valve",
                            );
                            ui.selectable_value(
                                &mut self.new_component_kind,
                                ComponentKindChoice::Pipe,
                                "Pipe",
                            );
                            ui.selectable_value(
                                &mut self.new_component_kind,
                                ComponentKindChoice::Pump,
                                "Pump",
                            );
                            ui.selectable_value(
                                &mut self.new_component_kind,
                                ComponentKindChoice::Turbine,
                                "Turbine",
                            );
                        });
                });

                ui.horizontal(|ui| {
                    ui.label("From:");
                    egui::ComboBox::from_id_salt("new_component_from")
                        .selected_text(
                            self.new_component_from
                                .clone()
                                .unwrap_or_else(|| "Select".to_string()),
                        )
                        .show_ui(ui, |ui| {
                            for node_id in &node_ids {
                                ui.selectable_value(
                                    &mut self.new_component_from,
                                    Some(node_id.clone()),
                                    node_id,
                                );
                            }
                        });
                });

                ui.horizontal(|ui| {
                    ui.label("To:");
                    egui::ComboBox::from_id_salt("new_component_to")
                        .selected_text(
                            self.new_component_to
                                .clone()
                                .unwrap_or_else(|| "Select".to_string()),
                        )
                        .show_ui(ui, |ui| {
                            for node_id in &node_ids {
                                ui.selectable_value(
                                    &mut self.new_component_to,
                                    Some(node_id.clone()),
                                    node_id,
                                );
                            }
                        });
                });

                let can_add = self.new_component_from.is_some()
                    && self.new_component_to.is_some()
                    && self.new_component_from != self.new_component_to;

                if ui
                    .add_enabled(can_add, egui::Button::new("+ Add New Component"))
                    .clicked()
                {
                    if let (Some(from_id), Some(to_id)) = (
                        self.new_component_from.clone(),
                        self.new_component_to.clone(),
                    ) {
                        actions.add_component = Some(NewComponentSpec {
                            kind: self.new_component_kind,
                            from_node_id: from_id,
                            to_node_id: to_id,
                        });
                    }
                }
            }
        }

        ui.separator();
        ui.heading("Overlay Settings");
        ui.checkbox(&mut self.overlay.show_pressure, "Show Pressure");
        ui.checkbox(&mut self.overlay.show_temperature, "Show Temperature");
        ui.checkbox(&mut self.overlay.show_enthalpy, "Show Enthalpy");
        ui.checkbox(&mut self.overlay.show_density, "Show Density");
        ui.checkbox(&mut self.overlay.show_mass_flow, "Show Mass Flow");

        actions
    }

    fn show_node_inspector(
        &mut self,
        ui: &mut egui::Ui,
        node: &mut NodeDef,
        boundaries: &mut Vec<BoundaryDef>,
        pid_view: &mut PidView,
    ) -> bool {
        ui.strong("Node");
        ui.separator();

        ui.horizontal(|ui| {
            ui.label("ID:");
            ui.label(&node.id);
        });

        let mut changed = false;
        changed |= ui
            .horizontal(|ui| {
                ui.label("Name:");
                ui.text_edit_singleline(&mut node.name)
            })
            .inner
            .changed();

        changed |= ui
            .horizontal(|ui| {
                ui.label("Kind:");
                let mut kind_choice = match node.kind {
                    NodeKind::Junction => NodeKindChoice::Junction,
                    NodeKind::ControlVolume { .. } => NodeKindChoice::ControlVolume,
                    NodeKind::Atmosphere { .. } => NodeKindChoice::Atmosphere,
                };

                let old_choice = kind_choice;
                egui::ComboBox::from_id_salt("node_kind")
                    .selected_text(kind_choice.label())
                    .show_ui(ui, |ui| {
                        ui.selectable_value(&mut kind_choice, NodeKindChoice::Junction, "Junction");
                        ui.selectable_value(
                            &mut kind_choice,
                            NodeKindChoice::ControlVolume,
                            "Control Volume",
                        );
                        ui.selectable_value(
                            &mut kind_choice,
                            NodeKindChoice::Atmosphere,
                            "Atmosphere",
                        );
                    });

                if kind_choice != old_choice {
                    node.kind = match kind_choice {
                        NodeKindChoice::Junction => NodeKind::Junction,
                        NodeKindChoice::ControlVolume => NodeKind::ControlVolume {
                            volume_m3: 0.05,
                            initial: Default::default(),
                        },
                        NodeKindChoice::Atmosphere => NodeKind::Atmosphere {
                            pressure_pa: 101_325.0,
                            temperature_k: 300.0,
                        },
                    };
                    if matches!(node.kind, NodeKind::Atmosphere { .. }) {
                        boundaries.retain(|b| b.node_id != node.id);
                    }
                    true
                } else {
                    false
                }
            })
            .inner;

        if let NodeKind::ControlVolume { volume_m3, initial } = &mut node.kind {
            changed |= ui
                .horizontal(|ui| {
                    ui.label("Volume (m3):");
                    ui.add(
                        egui::DragValue::new(volume_m3)
                            .speed(0.01)
                            .range(1e-6..=1e3),
                    )
                })
                .inner
                .changed();

            changed |=
                edit_optional_value(ui, "Initial P (Pa)", &mut initial.p_pa, 1000.0, 1.0..=1e9);
            changed |=
                edit_optional_value(ui, "Initial T (K)", &mut initial.t_k, 1.0, 1.0..=2000.0);
            changed |= edit_optional_value(
                ui,
                "Initial h (J/kg)",
                &mut initial.h_j_per_kg,
                1000.0,
                -1e7..=1e7,
            );
            changed |= edit_optional_value(ui, "Initial m (kg)", &mut initial.m_kg, 0.1, 0.0..=1e6);
        }

        if let NodeKind::Atmosphere {
            pressure_pa,
            temperature_k,
        } = &mut node.kind
        {
            changed |= ui
                .horizontal(|ui| {
                    ui.label("Pressure (Pa):");
                    ui.add(
                        egui::DragValue::new(pressure_pa)
                            .speed(1000.0)
                            .range(0.0..=1e9),
                    )
                })
                .inner
                .changed();

            changed |= ui
                .horizontal(|ui| {
                    ui.label("Temperature (K):");
                    ui.add(
                        egui::DragValue::new(temperature_k)
                            .speed(1.0)
                            .range(0.0..=2000.0),
                    )
                })
                .inner
                .changed();
        }

        if !matches!(node.kind, NodeKind::Atmosphere { .. }) {
            ui.separator();
            ui.strong("Boundary Condition");

            let bc_idx = boundaries.iter().position(|bc| bc.node_id == node.id);

            if let Some(idx) = bc_idx {
                let bc = &mut boundaries[idx];

                changed |= ui
                    .horizontal(|ui| {
                        ui.label("Type:");
                        let current_type =
                            match (bc.pressure_pa, bc.temperature_k, bc.enthalpy_j_per_kg) {
                                (Some(_), Some(_), _) => "P-T",
                                (Some(_), None, Some(_)) => "P-H",
                                _ => "Invalid",
                            };

                        egui::ComboBox::from_id_salt("bc_type")
                            .selected_text(current_type)
                            .show_ui(ui, |ui| {
                                if ui.selectable_label(current_type == "P-T", "P-T").clicked() {
                                    bc.pressure_pa = Some(101325.0);
                                    bc.temperature_k = Some(300.0);
                                    bc.enthalpy_j_per_kg = None;
                                    return true;
                                }
                                if ui.selectable_label(current_type == "P-H", "P-H").clicked() {
                                    bc.pressure_pa = Some(101325.0);
                                    bc.temperature_k = None;
                                    bc.enthalpy_j_per_kg = Some(300000.0);
                                    return true;
                                }
                                false
                            })
                            .inner
                            .unwrap_or(false)
                    })
                    .inner;

                changed |= if let Some(ref mut p) = bc.pressure_pa {
                    ui.horizontal(|ui| {
                        ui.label("Pressure (Pa):");
                        ui.add(egui::DragValue::new(p).speed(1000.0).range(0.0..=1e9))
                    })
                    .inner
                    .changed()
                } else {
                    false
                };

                changed |= if let Some(ref mut t) = bc.temperature_k {
                    ui.horizontal(|ui| {
                        ui.label("Temperature (K):");
                        ui.add(egui::DragValue::new(t).speed(1.0).range(0.0..=1000.0))
                    })
                    .inner
                    .changed()
                } else {
                    false
                };

                changed |= if let Some(ref mut h) = bc.enthalpy_j_per_kg {
                    ui.horizontal(|ui| {
                        ui.label("Enthalpy (J/kg):");
                        ui.add(egui::DragValue::new(h).speed(1000.0).range(-1e7..=1e7))
                    })
                    .inner
                    .changed()
                } else {
                    false
                };

                if ui.button("Remove Boundary Condition").clicked() {
                    boundaries.remove(idx);
                    changed = true;
                }
            } else {
                ui.label("No boundary condition set");
                if ui.button("Add Boundary Condition").clicked() {
                    boundaries.push(BoundaryDef {
                        node_id: node.id.clone(),
                        pressure_pa: Some(101325.0),
                        temperature_k: Some(300.0),
                        enthalpy_j_per_kg: None,
                    });
                    changed = true;
                }
            }
        }

        ui.separator();
        ui.strong("Per-Node Overlay");

        // Get current per-node overlay settings, or create default
        let mut node_overlay = pid_view
            .get_node_overlay(&node.id)
            .cloned()
            .unwrap_or_default();

        let mut overlay_changed = false;
        overlay_changed |= ui
            .checkbox(&mut node_overlay.show_pressure, "Show Pressure")
            .changed();
        overlay_changed |= ui
            .checkbox(&mut node_overlay.show_temperature, "Show Temperature")
            .changed();
        overlay_changed |= ui
            .checkbox(&mut node_overlay.show_enthalpy, "Show Enthalpy")
            .changed();
        overlay_changed |= ui
            .checkbox(&mut node_overlay.show_density, "Show Density")
            .changed();

        if overlay_changed {
            pid_view.set_node_overlay(node.id.clone(), node_overlay);
        }

        changed
    }

    fn show_component_inspector(
        &mut self,
        ui: &mut egui::Ui,
        component: &mut ComponentDef,
    ) -> bool {
        ui.strong("Component");
        ui.separator();

        ui.horizontal(|ui| {
            ui.label("ID:");
            ui.label(&component.id);
        });

        let mut changed = false;
        ui.horizontal(|ui| {
            ui.label("Name:");
            changed |= ui.text_edit_singleline(&mut component.name).changed();
        });

        ui.horizontal(|ui| {
            ui.label("Type:");
            let type_name = match &component.kind {
                ComponentKind::Orifice { .. } => "Orifice",
                ComponentKind::Valve { .. } => "Valve",
                ComponentKind::Pipe { .. } => "Pipe",
                ComponentKind::Pump { .. } => "Pump",
                ComponentKind::Turbine { .. } => "Turbine",
                ComponentKind::LineVolume { .. } => "LineVolume",
            };
            ui.label(type_name);
        });

        ui.horizontal(|ui| {
            ui.label("From:");
            ui.label(&component.from_node_id);
        });

        ui.horizontal(|ui| {
            ui.label("To:");
            ui.label(&component.to_node_id);
        });

        ui.separator();
        ui.strong("Parameters");

        match &mut component.kind {
            ComponentKind::Orifice {
                cd,
                area_m2,
                treat_as_gas,
            } => {
                changed |= ui
                    .horizontal(|ui| {
                        ui.label("Cd:");
                        ui.add(egui::DragValue::new(cd).speed(0.01).range(0.01..=1.0))
                    })
                    .inner
                    .changed();

                changed |= ui
                    .horizontal(|ui| {
                        ui.label("Area (m²):");
                        ui.add(
                            egui::DragValue::new(area_m2)
                                .speed(0.0001)
                                .range(1e-6..=1.0),
                        )
                    })
                    .inner
                    .changed();

                changed |= ui.checkbox(treat_as_gas, "Treat as Gas").changed();
            }
            ComponentKind::Valve {
                cd,
                area_max_m2,
                position,
                law,
                treat_as_gas,
            } => {
                changed |= ui
                    .horizontal(|ui| {
                        ui.label("Cd:");
                        ui.add(egui::DragValue::new(cd).speed(0.01).range(0.01..=1.0))
                    })
                    .inner
                    .changed();

                changed |= ui
                    .horizontal(|ui| {
                        ui.label("Max Area (m²):");
                        ui.add(
                            egui::DragValue::new(area_max_m2)
                                .speed(0.0001)
                                .range(1e-6..=1.0),
                        )
                    })
                    .inner
                    .changed();

                changed |= ui
                    .horizontal(|ui| {
                        ui.label("Position:");
                        ui.add(egui::DragValue::new(position).speed(0.01).range(0.0..=1.0))
                    })
                    .inner
                    .changed();

                changed |= ui
                    .horizontal(|ui| {
                        ui.label("Law:");
                        egui::ComboBox::from_id_salt("valve_law")
                            .selected_text(format!("{:?}", law))
                            .show_ui(ui, |ui| {
                                ui.selectable_value(
                                    law,
                                    tf_project::schema::ValveLawDef::Linear,
                                    "Linear",
                                )
                                .changed()
                                    || ui
                                        .selectable_value(
                                            law,
                                            tf_project::schema::ValveLawDef::Quadratic,
                                            "Quadratic",
                                        )
                                        .changed()
                                    || ui
                                        .selectable_value(
                                            law,
                                            tf_project::schema::ValveLawDef::QuickOpening,
                                            "QuickOpening",
                                        )
                                        .changed()
                            })
                            .inner
                            .unwrap_or(false)
                    })
                    .inner;

                changed |= ui.checkbox(treat_as_gas, "Treat as Gas").changed();
            }
            ComponentKind::Pipe {
                length_m,
                diameter_m,
                roughness_m,
                k_minor,
                mu_pa_s,
            } => {
                changed |= ui
                    .horizontal(|ui| {
                        ui.label("Length (m):");
                        ui.add(
                            egui::DragValue::new(length_m)
                                .speed(0.1)
                                .range(0.001..=1000.0),
                        )
                    })
                    .inner
                    .changed();

                changed |= ui
                    .horizontal(|ui| {
                        ui.label("Diameter (m):");
                        ui.add(
                            egui::DragValue::new(diameter_m)
                                .speed(0.001)
                                .range(0.001..=10.0),
                        )
                    })
                    .inner
                    .changed();

                changed |= ui
                    .horizontal(|ui| {
                        ui.label("Roughness (m):");
                        ui.add(
                            egui::DragValue::new(roughness_m)
                                .speed(0.00001)
                                .range(0.0..=0.01),
                        )
                    })
                    .inner
                    .changed();

                changed |= ui
                    .horizontal(|ui| {
                        ui.label("K Minor:");
                        ui.add(egui::DragValue::new(k_minor).speed(0.1).range(0.0..=100.0))
                    })
                    .inner
                    .changed();

                changed |= ui
                    .horizontal(|ui| {
                        ui.label("Viscosity (Pa·s):");
                        ui.add(
                            egui::DragValue::new(mu_pa_s)
                                .speed(0.0001)
                                .range(1e-6..=1.0),
                        )
                    })
                    .inner
                    .changed();
            }
            ComponentKind::Pump {
                cd,
                area_m2,
                delta_p_pa,
                eta,
                treat_as_liquid,
            } => {
                changed |= ui
                    .horizontal(|ui| {
                        ui.label("Cd:");
                        ui.add(egui::DragValue::new(cd).speed(0.01).range(0.01..=1.0))
                    })
                    .inner
                    .changed();

                changed |= ui
                    .horizontal(|ui| {
                        ui.label("Area (m²):");
                        ui.add(
                            egui::DragValue::new(area_m2)
                                .speed(0.0001)
                                .range(1e-6..=1.0),
                        )
                    })
                    .inner
                    .changed();

                changed |= ui
                    .horizontal(|ui| {
                        ui.label("ΔP (Pa):");
                        ui.add(
                            egui::DragValue::new(delta_p_pa)
                                .speed(1000.0)
                                .range(0.0..=1e8),
                        )
                    })
                    .inner
                    .changed();

                changed |= ui
                    .horizontal(|ui| {
                        ui.label("Efficiency:");
                        ui.add(egui::DragValue::new(eta).speed(0.01).range(0.01..=1.0))
                    })
                    .inner
                    .changed();

                changed |= ui.checkbox(treat_as_liquid, "Treat as Liquid").changed();
            }
            ComponentKind::Turbine {
                cd,
                area_m2,
                eta,
                treat_as_gas,
            } => {
                changed |= ui
                    .horizontal(|ui| {
                        ui.label("Cd:");
                        ui.add(egui::DragValue::new(cd).speed(0.01).range(0.01..=1.0))
                    })
                    .inner
                    .changed();

                changed |= ui
                    .horizontal(|ui| {
                        ui.label("Area (m²):");
                        ui.add(
                            egui::DragValue::new(area_m2)
                                .speed(0.0001)
                                .range(1e-6..=1.0),
                        )
                    })
                    .inner
                    .changed();

                changed |= ui
                    .horizontal(|ui| {
                        ui.label("Efficiency:");
                        ui.add(egui::DragValue::new(eta).speed(0.01).range(0.01..=1.0))
                    })
                    .inner
                    .changed();

                changed |= ui.checkbox(treat_as_gas, "Treat as Gas").changed();
            }
            ComponentKind::LineVolume {
                volume_m3,
                cd,
                area_m2,
            } => {
                changed |= ui
                    .horizontal(|ui| {
                        ui.label("Volume (m³):");
                        ui.add(
                            egui::DragValue::new(volume_m3)
                                .speed(0.001)
                                .range(1e-6..=100.0),
                        )
                    })
                    .inner
                    .changed();

                changed |= ui
                    .horizontal(|ui| {
                        ui.label("Cd (0 = lossless):");
                        ui.add(egui::DragValue::new(cd).speed(0.01).range(0.0..=1.0))
                    })
                    .inner
                    .changed();

                changed |= ui
                    .horizontal(|ui| {
                        ui.label("Area (m²):");
                        ui.add(egui::DragValue::new(area_m2).speed(0.0001).range(0.0..=1.0))
                    })
                    .inner
                    .changed();
            }
        }

        changed
    }

    fn show_control_block_inspector(
        &mut self,
        ui: &mut egui::Ui,
        block: &mut ControlBlockDef,
        node_ids: &[String],
        component_ids: &[String],
        valve_component_ids: &[String],
    ) -> bool {
        ui.strong("Control Block");
        ui.separator();

        ui.horizontal(|ui| {
            ui.label("ID:");
            ui.label(&block.id);
        });

        let mut changed = false;
        ui.horizontal(|ui| {
            ui.label("Name:");
            changed |= ui.text_edit_singleline(&mut block.name).changed();
        });

        ui.horizontal(|ui| {
            ui.label("Type:");
            ui.label(control_block_type_label(&block.kind));
        });

        ui.separator();
        ui.strong("Parameters");

        match &mut block.kind {
            ControlBlockKindDef::Constant { value } => {
                changed |= ui
                    .horizontal(|ui| {
                        ui.label("Value:");
                        ui.add(egui::DragValue::new(value).speed(0.01))
                    })
                    .inner
                    .changed();
            }
            ControlBlockKindDef::MeasuredVariable { reference } => {
                changed |= edit_measured_variable_reference(ui, reference, node_ids, component_ids);
            }
            ControlBlockKindDef::PIController {
                kp,
                ti_s,
                out_min,
                out_max,
                integral_limit,
                sample_period_s,
            } => {
                changed |= ui
                    .horizontal(|ui| {
                        ui.label("Kp:");
                        ui.add(egui::DragValue::new(kp).speed(0.01))
                    })
                    .inner
                    .changed();

                changed |= ui
                    .horizontal(|ui| {
                        ui.label("Ti (s):");
                        ui.add(egui::DragValue::new(ti_s).speed(0.01).range(1e-6..=1e6))
                    })
                    .inner
                    .changed();

                changed |= ui
                    .horizontal(|ui| {
                        ui.label("Output Min:");
                        ui.add(egui::DragValue::new(out_min).speed(0.01))
                    })
                    .inner
                    .changed();

                changed |= ui
                    .horizontal(|ui| {
                        ui.label("Output Max:");
                        ui.add(egui::DragValue::new(out_max).speed(0.01))
                    })
                    .inner
                    .changed();

                changed |=
                    edit_optional_value(ui, "Integral Clamp", integral_limit, 0.01, 1e-6..=1e6);

                changed |= ui
                    .horizontal(|ui| {
                        ui.label("Sample Period (s):");
                        ui.add(
                            egui::DragValue::new(sample_period_s)
                                .speed(0.001)
                                .range(1e-6..=1e6),
                        )
                    })
                    .inner
                    .changed();
            }
            ControlBlockKindDef::PIDController {
                kp,
                ti_s,
                td_s,
                td_filter_s,
                out_min,
                out_max,
                integral_limit,
                sample_period_s,
            } => {
                changed |= ui
                    .horizontal(|ui| {
                        ui.label("Kp:");
                        ui.add(egui::DragValue::new(kp).speed(0.01))
                    })
                    .inner
                    .changed();

                changed |= ui
                    .horizontal(|ui| {
                        ui.label("Ti (s):");
                        ui.add(egui::DragValue::new(ti_s).speed(0.01).range(1e-6..=1e6))
                    })
                    .inner
                    .changed();

                changed |= ui
                    .horizontal(|ui| {
                        ui.label("Td (s):");
                        ui.add(egui::DragValue::new(td_s).speed(0.001).range(0.0..=1e6))
                    })
                    .inner
                    .changed();

                changed |= ui
                    .horizontal(|ui| {
                        ui.label("Td Filter (s):");
                        ui.add(
                            egui::DragValue::new(td_filter_s)
                                .speed(0.001)
                                .range(1e-6..=1e6),
                        )
                    })
                    .inner
                    .changed();

                changed |= ui
                    .horizontal(|ui| {
                        ui.label("Output Min:");
                        ui.add(egui::DragValue::new(out_min).speed(0.01))
                    })
                    .inner
                    .changed();

                changed |= ui
                    .horizontal(|ui| {
                        ui.label("Output Max:");
                        ui.add(egui::DragValue::new(out_max).speed(0.01))
                    })
                    .inner
                    .changed();

                changed |=
                    edit_optional_value(ui, "Integral Clamp", integral_limit, 0.01, 1e-6..=1e6);

                changed |= ui
                    .horizontal(|ui| {
                        ui.label("Sample Period (s):");
                        ui.add(
                            egui::DragValue::new(sample_period_s)
                                .speed(0.001)
                                .range(1e-6..=1e6),
                        )
                    })
                    .inner
                    .changed();
            }
            ControlBlockKindDef::FirstOrderActuator {
                tau_s,
                rate_limit_per_s,
                initial_position,
            } => {
                changed |= ui
                    .horizontal(|ui| {
                        ui.label("Tau (s):");
                        ui.add(egui::DragValue::new(tau_s).speed(0.01).range(1e-6..=1e6))
                    })
                    .inner
                    .changed();

                changed |= ui
                    .horizontal(|ui| {
                        ui.label("Rate Limit (/s):");
                        ui.add(
                            egui::DragValue::new(rate_limit_per_s)
                                .speed(0.01)
                                .range(1e-6..=1e6),
                        )
                    })
                    .inner
                    .changed();

                changed |= ui
                    .horizontal(|ui| {
                        ui.label("Initial Position:");
                        ui.add(
                            egui::DragValue::new(initial_position)
                                .speed(0.01)
                                .range(0.0..=1.0),
                        )
                    })
                    .inner
                    .changed();
            }
            ControlBlockKindDef::ActuatorCommand { component_id } => {
                ui.horizontal(|ui| {
                    ui.label("Target Valve:");
                    if valve_component_ids.is_empty() {
                        ui.label("No valve components available");
                    } else {
                        if !valve_component_ids.contains(component_id) {
                            *component_id = valve_component_ids[0].clone();
                            changed = true;
                        }
                        egui::ComboBox::from_id_salt(format!("ctrl_target_{}", block.id))
                            .selected_text(component_id.clone())
                            .show_ui(ui, |ui| {
                                for id in valve_component_ids {
                                    changed |=
                                        ui.selectable_value(component_id, id.clone(), id).changed();
                                }
                            });
                    }
                });
            }
        }

        let validation = validate_control_block_for_inspector(
            &block.kind,
            node_ids,
            component_ids,
            valve_component_ids,
        );
        if !validation.is_empty() {
            ui.separator();
            ui.colored_label(egui::Color32::LIGHT_RED, "Validation:");
            for issue in validation {
                ui.colored_label(egui::Color32::LIGHT_RED, format!("• {}", issue));
            }
        }

        changed
    }

    fn show_plot_series_editor(
        &mut self,
        ui: &mut egui::Ui,
        panel: &mut super::super::plot_workspace::PlotPanel,
        project: Option<&Project>,
    ) {
        let series = &mut panel.series_selection;
        
        if let Some(proj) = project {
            if let Some(system) = proj.systems.first() {
                // Show node variables with toggles
                if !system.nodes.is_empty() {
                    ui.label(egui::RichText::new("Node Variables").strong());
                    ui.separator();
                    
                    for node in &system.nodes {
                        ui.label(format!("  {}", node.id));
                        ui.indent(format!("node_{}", node.id), |ui| {
                            // Pressure toggle
                            let mut has_pressure = series.node_ids_and_variables
                                .iter()
                                .any(|(id, var)| id == &node.id && var == "Pressure");
                            if ui.checkbox(&mut has_pressure, "Pressure (P)").changed() {
                                if has_pressure {
                                    series.node_ids_and_variables.push((node.id.clone(), "Pressure".to_string()));
                                } else {
                                    series.node_ids_and_variables.retain(|(id, var)| !(id == &node.id && var == "Pressure"));
                                }
                            }
                            
                            // Temperature toggle
                            let mut has_temperature = series.node_ids_and_variables
                                .iter()
                                .any(|(id, var)| id == &node.id && var == "Temperature");
                            if ui.checkbox(&mut has_temperature, "Temperature (T)").changed() {
                                if has_temperature {
                                    series.node_ids_and_variables.push((node.id.clone(), "Temperature".to_string()));
                                } else {
                                    series.node_ids_and_variables.retain(|(id, var)| !(id == &node.id && var == "Temperature"));
                                }
                            }
                        });
                    }
                    ui.add_space(8.0);
                }
                
                // Show component variables with toggles
                if !system.components.is_empty() {
                    ui.label(egui::RichText::new("Component Variables").strong());
                    ui.separator();
                    
                    for comp in &system.components {
                        ui.label(format!("  {} ({:?})", comp.id, comp.kind));
                        ui.indent(format!("comp_{}", comp.id), |ui| {
                            // Mass flow toggle
                            let mut has_mass_flow = series.component_ids_and_variables
                                .iter()
                                .any(|(id, var)| id == &comp.id && var == "MassFlow");
                            if ui.checkbox(&mut has_mass_flow, "Mass Flow (ṁ)").changed() {
                                if has_mass_flow {
                                    series.component_ids_and_variables.push((comp.id.clone(), "MassFlow".to_string()));
                                } else {
                                    series.component_ids_and_variables.retain(|(id, var)| !(id == &comp.id && var == "MassFlow"));
                                }
                            }
                            
                            // Pressure drop toggle
                            let mut has_pressure_drop = series.component_ids_and_variables
                                .iter()
                                .any(|(id, var)| id == &comp.id && var == "PressureDrop");
                            if ui.checkbox(&mut has_pressure_drop, "Pressure Drop (ΔP)").changed() {
                                if has_pressure_drop {
                                    series.component_ids_and_variables.push((comp.id.clone(), "PressureDrop".to_string()));
                                } else {
                                    series.component_ids_and_variables.retain(|(id, var)| !(id == &comp.id && var == "PressureDrop"));
                                }
                            }
                        });
                    }
                    ui.add_space(8.0);
                }
                
                // Show control blocks with toggles
                if let Some(controls) = &system.controls {
                    if !controls.blocks.is_empty() {
                        ui.label(egui::RichText::new("Control Blocks").strong());
                        ui.separator();
                        
                        for ctrl in &controls.blocks {
                            let mut has_control = series.control_ids.contains(&ctrl.id);
                            if ui.checkbox(&mut has_control, format!("  {}", ctrl.id)).changed() {
                                if has_control {
                                    series.control_ids.push(ctrl.id.clone());
                                } else {
                                    series.control_ids.retain(|id| id != &ctrl.id);
                                }
                            }
                        }
                    }
                }
            } else {
                ui.label("(No system available)");
            }
        } else {
            ui.label("(No project loaded)");
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum MeasuredRefChoice {
    NodePressure,
    NodeTemperature,
    EdgeMassFlow,
    PressureDrop,
}

fn edit_measured_variable_reference(
    ui: &mut egui::Ui,
    reference: &mut MeasuredVariableDef,
    node_ids: &[String],
    component_ids: &[String],
) -> bool {
    let mut changed = false;

    let mut choice = measured_ref_choice(reference);
    let previous = choice;

    ui.horizontal(|ui| {
        ui.label("Measured:");
        egui::ComboBox::from_id_salt("measured_ref_type")
            .selected_text(measured_ref_label(choice))
            .show_ui(ui, |ui| {
                ui.selectable_value(
                    &mut choice,
                    MeasuredRefChoice::NodePressure,
                    measured_ref_label(MeasuredRefChoice::NodePressure),
                );
                ui.selectable_value(
                    &mut choice,
                    MeasuredRefChoice::NodeTemperature,
                    measured_ref_label(MeasuredRefChoice::NodeTemperature),
                );
                ui.selectable_value(
                    &mut choice,
                    MeasuredRefChoice::EdgeMassFlow,
                    measured_ref_label(MeasuredRefChoice::EdgeMassFlow),
                );
                ui.selectable_value(
                    &mut choice,
                    MeasuredRefChoice::PressureDrop,
                    measured_ref_label(MeasuredRefChoice::PressureDrop),
                );
            });
    });

    if choice != previous {
        *reference = default_measured_reference(choice, node_ids, component_ids);
        changed = true;
    }

    match reference {
        MeasuredVariableDef::NodePressure { node_id }
        | MeasuredVariableDef::NodeTemperature { node_id } => {
            ui.horizontal(|ui| {
                ui.label("Node:");
                if node_ids.is_empty() {
                    ui.label("No nodes available");
                } else {
                    if !node_ids.contains(node_id) {
                        *node_id = node_ids[0].clone();
                        changed = true;
                    }
                    egui::ComboBox::from_id_salt("measured_node")
                        .selected_text(node_id.clone())
                        .show_ui(ui, |ui| {
                            for id in node_ids {
                                changed |= ui.selectable_value(node_id, id.clone(), id).changed();
                            }
                        });
                }
            });
        }
        MeasuredVariableDef::EdgeMassFlow { component_id } => {
            ui.horizontal(|ui| {
                ui.label("Component:");
                if component_ids.is_empty() {
                    ui.label("No components available");
                } else {
                    if !component_ids.contains(component_id) {
                        *component_id = component_ids[0].clone();
                        changed = true;
                    }
                    egui::ComboBox::from_id_salt("measured_component")
                        .selected_text(component_id.clone())
                        .show_ui(ui, |ui| {
                            for id in component_ids {
                                changed |=
                                    ui.selectable_value(component_id, id.clone(), id).changed();
                            }
                        });
                }
            });
        }
        MeasuredVariableDef::PressureDrop {
            from_node_id,
            to_node_id,
        } => {
            ui.horizontal(|ui| {
                ui.label("From Node:");
                if node_ids.is_empty() {
                    ui.label("No nodes available");
                } else {
                    if !node_ids.contains(from_node_id) {
                        *from_node_id = node_ids[0].clone();
                        changed = true;
                    }
                    egui::ComboBox::from_id_salt("measured_pd_from")
                        .selected_text(from_node_id.clone())
                        .show_ui(ui, |ui| {
                            for id in node_ids {
                                changed |=
                                    ui.selectable_value(from_node_id, id.clone(), id).changed();
                            }
                        });
                }
            });
            ui.horizontal(|ui| {
                ui.label("To Node:");
                if node_ids.is_empty() {
                    ui.label("No nodes available");
                } else {
                    if !node_ids.contains(to_node_id) {
                        *to_node_id = node_ids[0].clone();
                        changed = true;
                    }
                    egui::ComboBox::from_id_salt("measured_pd_to")
                        .selected_text(to_node_id.clone())
                        .show_ui(ui, |ui| {
                            for id in node_ids {
                                changed |=
                                    ui.selectable_value(to_node_id, id.clone(), id).changed();
                            }
                        });
                }
            });
        }
    }

    changed
}

fn validate_control_block_for_inspector(
    kind: &ControlBlockKindDef,
    node_ids: &[String],
    component_ids: &[String],
    valve_component_ids: &[String],
) -> Vec<String> {
    let mut issues = Vec::new();

    match kind {
        ControlBlockKindDef::Constant { value } => {
            if !value.is_finite() {
                issues.push("Constant value must be finite".to_string());
            }
        }
        ControlBlockKindDef::MeasuredVariable { reference } => match reference {
            MeasuredVariableDef::NodePressure { node_id }
            | MeasuredVariableDef::NodeTemperature { node_id } => {
                if !node_ids.contains(node_id) {
                    issues.push(format!("Referenced node '{}' does not exist", node_id));
                }
            }
            MeasuredVariableDef::EdgeMassFlow { component_id } => {
                if !component_ids.contains(component_id) {
                    issues.push(format!(
                        "Referenced component '{}' does not exist",
                        component_id
                    ));
                }
            }
            MeasuredVariableDef::PressureDrop {
                from_node_id,
                to_node_id,
            } => {
                if !node_ids.contains(from_node_id) {
                    issues.push(format!("From node '{}' does not exist", from_node_id));
                }
                if !node_ids.contains(to_node_id) {
                    issues.push(format!("To node '{}' does not exist", to_node_id));
                }
            }
        },
        ControlBlockKindDef::PIController {
            kp,
            ti_s,
            out_min,
            out_max,
            integral_limit,
            sample_period_s,
        } => {
            if !kp.is_finite() {
                issues.push("Kp must be finite".to_string());
            }
            if !ti_s.is_finite() || *ti_s <= 0.0 {
                issues.push("Ti must be positive and finite".to_string());
            }
            if !sample_period_s.is_finite() || *sample_period_s <= 0.0 {
                issues.push("Sample period must be positive and finite".to_string());
            }
            if !out_min.is_finite() || !out_max.is_finite() || *out_min >= *out_max {
                issues.push("Output limits must satisfy out_min < out_max".to_string());
            }
            if let Some(limit) = integral_limit
                && (!limit.is_finite() || *limit <= 0.0)
            {
                issues.push("Integral clamp must be positive and finite".to_string());
            }
        }
        ControlBlockKindDef::PIDController {
            kp,
            ti_s,
            td_s,
            td_filter_s,
            out_min,
            out_max,
            integral_limit,
            sample_period_s,
        } => {
            if !kp.is_finite() {
                issues.push("Kp must be finite".to_string());
            }
            if !ti_s.is_finite() || *ti_s <= 0.0 {
                issues.push("Ti must be positive and finite".to_string());
            }
            if !td_s.is_finite() || *td_s < 0.0 {
                issues.push("Td must be non-negative and finite".to_string());
            }
            if !td_filter_s.is_finite() || *td_filter_s <= 0.0 {
                issues.push("Td filter must be positive and finite".to_string());
            }
            if !sample_period_s.is_finite() || *sample_period_s <= 0.0 {
                issues.push("Sample period must be positive and finite".to_string());
            }
            if !out_min.is_finite() || !out_max.is_finite() || *out_min >= *out_max {
                issues.push("Output limits must satisfy out_min < out_max".to_string());
            }
            if let Some(limit) = integral_limit
                && (!limit.is_finite() || *limit <= 0.0)
            {
                issues.push("Integral clamp must be positive and finite".to_string());
            }
        }
        ControlBlockKindDef::FirstOrderActuator {
            tau_s,
            rate_limit_per_s,
            initial_position,
        } => {
            if !tau_s.is_finite() || *tau_s <= 0.0 {
                issues.push("Tau must be positive and finite".to_string());
            }
            if !rate_limit_per_s.is_finite() || *rate_limit_per_s <= 0.0 {
                issues.push("Rate limit must be positive and finite".to_string());
            }
            if !initial_position.is_finite() || !(0.0..=1.0).contains(initial_position) {
                issues.push("Initial position must be in [0, 1]".to_string());
            }
        }
        ControlBlockKindDef::ActuatorCommand { component_id } => {
            if !component_ids.contains(component_id) {
                issues.push(format!(
                    "Target component '{}' does not exist",
                    component_id
                ));
            } else if !valve_component_ids.contains(component_id) {
                issues.push("Actuator command target must be a Valve component".to_string());
            }
        }
    }

    issues
}

fn control_block_type_label(kind: &ControlBlockKindDef) -> &'static str {
    match kind {
        ControlBlockKindDef::Constant { .. } => "Constant / Setpoint",
        ControlBlockKindDef::MeasuredVariable { .. } => "Measured Variable",
        ControlBlockKindDef::PIController { .. } => "PI Controller",
        ControlBlockKindDef::PIDController { .. } => "PID Controller",
        ControlBlockKindDef::FirstOrderActuator { .. } => "First-Order Actuator",
        ControlBlockKindDef::ActuatorCommand { .. } => "Actuator Command",
    }
}

fn measured_ref_choice(reference: &MeasuredVariableDef) -> MeasuredRefChoice {
    match reference {
        MeasuredVariableDef::NodePressure { .. } => MeasuredRefChoice::NodePressure,
        MeasuredVariableDef::NodeTemperature { .. } => MeasuredRefChoice::NodeTemperature,
        MeasuredVariableDef::EdgeMassFlow { .. } => MeasuredRefChoice::EdgeMassFlow,
        MeasuredVariableDef::PressureDrop { .. } => MeasuredRefChoice::PressureDrop,
    }
}

fn measured_ref_label(choice: MeasuredRefChoice) -> &'static str {
    match choice {
        MeasuredRefChoice::NodePressure => "Node Pressure",
        MeasuredRefChoice::NodeTemperature => "Node Temperature",
        MeasuredRefChoice::EdgeMassFlow => "Edge Mass Flow",
        MeasuredRefChoice::PressureDrop => "Pressure Drop",
    }
}

fn default_measured_reference(
    choice: MeasuredRefChoice,
    node_ids: &[String],
    component_ids: &[String],
) -> MeasuredVariableDef {
    match choice {
        MeasuredRefChoice::NodePressure => MeasuredVariableDef::NodePressure {
            node_id: node_ids
                .first()
                .cloned()
                .unwrap_or_else(|| "n1".to_string()),
        },
        MeasuredRefChoice::NodeTemperature => MeasuredVariableDef::NodeTemperature {
            node_id: node_ids
                .first()
                .cloned()
                .unwrap_or_else(|| "n1".to_string()),
        },
        MeasuredRefChoice::EdgeMassFlow => MeasuredVariableDef::EdgeMassFlow {
            component_id: component_ids
                .first()
                .cloned()
                .unwrap_or_else(|| "c1".to_string()),
        },
        MeasuredRefChoice::PressureDrop => MeasuredVariableDef::PressureDrop {
            from_node_id: node_ids
                .first()
                .cloned()
                .unwrap_or_else(|| "n1".to_string()),
            to_node_id: node_ids.get(1).cloned().unwrap_or_else(|| {
                node_ids
                    .first()
                    .cloned()
                    .unwrap_or_else(|| "n1".to_string())
            }),
        },
    }
}

impl NodeKindChoice {
    fn label(self) -> &'static str {
        match self {
            NodeKindChoice::Junction => "Junction",
            NodeKindChoice::ControlVolume => "Control Volume",
            NodeKindChoice::Atmosphere => "Atmosphere",
        }
    }
}

impl ComponentKindChoice {
    fn label(self) -> &'static str {
        match self {
            ComponentKindChoice::Orifice => "Orifice",
            ComponentKindChoice::Valve => "Valve",
            ComponentKindChoice::Pipe => "Pipe",
            ComponentKindChoice::Pump => "Pump",
            ComponentKindChoice::Turbine => "Turbine",
        }
    }
}

fn edit_optional_value(
    ui: &mut egui::Ui,
    label: &str,
    value: &mut Option<f64>,
    speed: f64,
    range: std::ops::RangeInclusive<f64>,
) -> bool {
    ui.horizontal(|ui| {
        ui.label(label);
        if let Some(v) = value {
            let drag_changed = ui
                .add(egui::DragValue::new(v).speed(speed).range(range.clone()))
                .changed();
            if ui.button("Clear").clicked() {
                *value = None;
                return true;
            }
            drag_changed
        } else if ui.button("Set").clicked() {
            *value = Some(*range.start());
            true
        } else {
            false
        }
    })
    .inner
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ids(items: &[&str]) -> Vec<String> {
        items.iter().map(|v| (*v).to_string()).collect()
    }

    #[test]
    fn validate_pi_controller_limits_and_period() {
        let kind = ControlBlockKindDef::PIController {
            kp: 1.0,
            ti_s: 0.0,
            out_min: 1.0,
            out_max: 1.0,
            integral_limit: Some(-1.0),
            sample_period_s: -0.01,
        };

        let issues = validate_control_block_for_inspector(&kind, &ids(&["n1"]), &[], &[]);
        assert!(issues.iter().any(|m| m.contains("Ti")));
        assert!(issues.iter().any(|m| m.contains("Sample period")));
        assert!(issues.iter().any(|m| m.contains("Output limits")));
        assert!(issues.iter().any(|m| m.contains("Integral clamp")));
    }

    #[test]
    fn validate_measured_reference_missing_nodes() {
        let kind = ControlBlockKindDef::MeasuredVariable {
            reference: MeasuredVariableDef::PressureDrop {
                from_node_id: "n_missing_a".to_string(),
                to_node_id: "n_missing_b".to_string(),
            },
        };

        let issues = validate_control_block_for_inspector(&kind, &ids(&["n1"]), &[], &[]);
        assert_eq!(issues.len(), 2);
    }

    #[test]
    fn validate_actuator_command_requires_valve_target() {
        let kind = ControlBlockKindDef::ActuatorCommand {
            component_id: "c_pump".to_string(),
        };
        let component_ids = ids(&["c_pump", "c_valve"]);
        let valve_ids = ids(&["c_valve"]);

        let issues = validate_control_block_for_inspector(&kind, &[], &component_ids, &valve_ids);
        assert!(
            issues
                .iter()
                .any(|m| m.contains("must be a Valve component"))
        );
    }

    #[test]
    fn validate_pid_controller_valid_values_pass() {
        let kind = ControlBlockKindDef::PIDController {
            kp: 1.2,
            ti_s: 5.0,
            td_s: 0.5,
            td_filter_s: 0.05,
            out_min: 0.0,
            out_max: 1.0,
            integral_limit: Some(2.0),
            sample_period_s: 0.1,
        };

        let issues = validate_control_block_for_inspector(&kind, &ids(&["n1"]), &[], &[]);
        assert!(issues.is_empty());
    }
}
