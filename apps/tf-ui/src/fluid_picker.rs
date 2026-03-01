use tf_fluids::{Species, filter_practical_coolprop_catalog, practical_coolprop_catalog};

#[derive(Debug, Default)]
pub struct SearchableFluidPicker {
    search_query: String,
}

impl SearchableFluidPicker {
    pub fn show(
        &mut self,
        ui: &mut egui::Ui,
        id_salt: impl std::hash::Hash,
        selected: &mut Species,
    ) -> bool {
        let mut changed = false;
        let selected_label = practical_coolprop_catalog()
            .iter()
            .find(|entry| entry.species == *selected)
            .map(|entry| entry.display_name)
            .unwrap_or_else(|| selected.display_name());

        egui::ComboBox::from_id_salt(id_salt)
            .selected_text(selected_label)
            .width(260.0)
            .show_ui(ui, |ui| {
                ui.horizontal(|ui| {
                    ui.label("Search:");
                    // Request focus on search field to fix click behavior
                    let search_response = ui
                        .text_edit_singleline(&mut self.search_query)
                        .on_hover_text("Type to filter fluids");

                    // Try to take focus if the field hasn't been interacted with yet
                    if search_response.changed() || self.search_query.is_empty() {
                        search_response.request_focus();
                    }

                    if ui.small_button("Clear").clicked() {
                        self.search_query.clear();
                    }
                });

                ui.separator();

                let filtered = filter_practical_coolprop_catalog(&self.search_query);
                if filtered.is_empty() {
                    ui.label("No fluids found");
                    return;
                }

                egui::ScrollArea::vertical()
                    .max_height(280.0)
                    .show(ui, |ui| {
                        for entry in filtered {
                            let label = format!("{} ({})", entry.display_name, entry.canonical_id);
                            changed |= ui
                                .selectable_value(selected, entry.species, label)
                                .changed();
                        }
                    });
            });

        changed
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn selected_species_label_uses_catalog_name() {
        let selected = Species::NitrousOxide;
        let label = practical_coolprop_catalog()
            .iter()
            .find(|entry| entry.species == selected)
            .map(|entry| entry.display_name)
            .unwrap_or_else(|| selected.display_name());

        assert_eq!(label, "Nitrous Oxide");
    }
}
