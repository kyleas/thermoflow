use tf_project::schema::Project;

#[derive(Default)]
pub struct ModuleView {
    new_module_name: String,
}

impl ModuleView {
    pub fn show(&mut self, ui: &mut egui::Ui, project: &mut Option<Project>) {
        ui.heading("Module View");
        ui.label("Functional blocks (egui_graph placeholder)");

        if let Some(proj) = project.as_mut() {
            ui.separator();
            ui.horizontal(|ui| {
                ui.label("New module:");
                ui.text_edit_singleline(&mut self.new_module_name);
                if ui.button("Add").clicked() && !self.new_module_name.trim().is_empty() {
                    proj.modules.push(tf_project::schema::ModuleDef {
                        id: format!("mod-{}", proj.modules.len() + 1),
                        name: self.new_module_name.trim().to_string(),
                        interface: tf_project::schema::ModuleInterfaceDef {
                            inputs: vec![],
                            outputs: vec![],
                        },
                        template_system_id: None,
                        exposed_nodes: vec![],
                    });
                    self.new_module_name.clear();
                }
            });

            ui.separator();
            for module in &proj.modules {
                ui.label(format!("Module: {}", module.name));
            }
        } else {
            ui.label("No project loaded");
        }
    }
}
