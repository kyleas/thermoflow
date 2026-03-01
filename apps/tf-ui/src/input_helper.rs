//! Unit-aware numeric input widget for egui.
//!
//! Displays a text input field that parses unit-qualified values (e.g., "70F", "14.7 psia")
//! and stores both the raw text and canonical SI value for use by backends.
//!
//! Designed for:
//! - Temperature/Pressure/Density inputs in fluid workspaces
//! - Sweep parameter bounds
//! - Any engineering value that needs multiple unit options
//!
//! # Usage
//!
//! ```no_run
//! use tf_ui::input_helper::{UnitAwareInput, InputState};
//! use tf_fluids::Quantity;
//! use egui::Context;
//!
//! // During widget setup:
//! let mut input = UnitAwareInput::new(
//!     "temp_1",
//!     "Enter temperature",
//!     Quantity::Temperature,
//! );
//!
//! // In your UI code:
//! if let Some(new_value) = input.show(&ctx, "Temperature 1", 200.0) {
//!     println!("User entered: {} K", new_value);
//! }
//! ```
//!
//! # Expression Support (Future)
//!
//! This widget is designed to support expression evaluation without code changes:
//! - `parse_quantity` in tf-fluids can be replaced with an expression parser
//! - The widget, storage, and schema remain completely stable
//! - Expressions like `1psia + 2bar` can be incrementally added

use egui::{Color32, Ui};
use tf_fluids::Quantity;
use std::collections::HashMap;

/// Tracks persistent state of a unit-aware input across frames.
#[derive(Debug, Clone)]
#[allow(dead_code)]
pub struct InputState {
    /// User-edited text (what they typed)
    pub text: String,
    /// Last successfully parsed canonical SI value
    pub canonical_si: f64,
    /// Parse error from last update (empty if valid)
    pub error: String,
}

impl InputState {
    /// Create a new input state with initial value.
    pub fn new(initial_si: f64, quantity: Quantity) -> Self {
        let text = format_display_value(initial_si, quantity);
        Self {
            text,
            canonical_si: initial_si,
            error: String::new(),
        }
    }

    /// Update from user input text, returning true if value changed and is valid.
    pub fn update_text(&mut self, new_text: String, quantity: Quantity) -> bool {
        self.text = new_text;
        match tf_fluids::parse_quantity(&self.text, quantity) {
            Ok(si_val) => {
                if (si_val - self.canonical_si).abs() > 1e-12 {
                    self.canonical_si = si_val;
                    self.error.clear();
                    true
                } else {
                    self.error.clear();
                    false // Same value, no change
                }
            }
            Err(e) => {
                self.error = e.to_string();
                false // Parse error, keep old value
            }
        }
    }

    /// Get the canonical SI value if valid, otherwise return error.
    pub fn value_or_error(&self) -> Result<f64, &str> {
        if self.error.is_empty() {
            Ok(self.canonical_si)
        } else {
            Err(&self.error)
        }
    }
}

/// Reusable unit-aware input helper for multiple fields.
///
/// Manages the state of several input fields and provides display+edit UI.
#[allow(dead_code)]
pub struct UnitAwareInput {
    /// Per-field state keyed by field ID
    states: HashMap<String, InputState>,
}

impl UnitAwareInput {
    /// Create a new input helper.
    pub fn new() -> Self {
        Self {
            states: HashMap::new(),
        }
    }

    /// Initialize or get state for a field, using initial_si if not yet created.
    pub fn get_or_create(
        &mut self,
        field_id: &str,
        initial_si: f64,
        quantity: Quantity,
    ) -> &mut InputState {
        self.states
            .entry(field_id.to_string())
            .or_insert_with(|| InputState::new(initial_si, quantity))
    }

    /// Get mutable state for a field.
    pub fn get_mut(&mut self, field_id: &str) -> Option<&mut InputState> {
        self.states.get_mut(field_id)
    }

    /// Get immutable state for a field.
    pub fn get(&self, field_id: &str) -> Option<&InputState> {
        self.states.get(field_id)
    }

    /// Show a unit-aware input field in the UI.
    ///
    /// Returns the new canonical SI value if the user committed a valid change.
    pub fn show_field(
        &mut self,
        ui: &mut Ui,
        field_id: &str,
        label: &str,
        quantity: Quantity,
        initial_si: f64,
    ) -> Option<f64> {
        let state = self.get_or_create(field_id, initial_si, quantity);

        ui.horizontal(|ui| {
            ui.label(label);

            let text_response = ui.text_edit_singleline(&mut state.text);

            // Show error indicator if present
            if !state.error.is_empty() {
                ui.label(
                    egui::RichText::new("⚠")
                        .color(Color32::RED)
                        .size(12.0),
                );
                ui.label(
                    egui::RichText::new(&state.error)
                        .color(Color32::RED)
                        .size(10.0),
                );
            } else {
                // Show canonical value for reference
                ui.label(format_display_value(state.canonical_si, quantity));
            }

            // Update on edit or blur
            if text_response.changed() || text_response.lost_focus() {
                let result = state.update_text(state.text.clone(), quantity);
                if result {
                    return Some(state.canonical_si);
                } else if text_response.lost_focus() && state.error.is_empty() {
                    // Valid but unchanged: return old value to trigger update
                    return Some(state.canonical_si);
                }
            }

            None
        })
        .inner
    }

    /// Batch show multiple fields at once.
    ///
    /// Returns a map of field_id -> new_value for fields that changed.
    pub fn show_fields(
        &mut self,
        ui: &mut Ui,
        fields: &[(&str, &str, Quantity, f64)],
    ) -> HashMap<String, f64> {
        let mut changed = HashMap::new();

        for (field_id, label, quantity, initial_si) in fields {
            if let Some(new_value) = self.show_field(ui, field_id, label, *quantity, *initial_si)
            {
                changed.insert(field_id.to_string(), new_value);
            }
        }

        changed
    }
}

impl Default for UnitAwareInput {
    fn default() -> Self {
        Self::new()
    }
}

/// Format a canonical SI value for display to user.
///
/// Shows the value with appropriate precision for the quantity.
#[allow(dead_code)]
fn format_display_value(si_value: f64, quantity: Quantity) -> String {
    match quantity {
        Quantity::Temperature => {
            // Temperature: show with 1 decimal (Kelvin)
            if si_value >= 100.0 {
                format!("{:.1} K", si_value)
            } else {
                format!("{:.3} K", si_value)
            }
        }
        Quantity::Pressure | Quantity::PressureGauge => {
            // Pressure: scientific notation for large ranges
            if si_value >= 1e6 {
                format!("{:.2e} Pa", si_value)
            } else if si_value >= 1e3 {
                format!("{:.1} kPa", si_value / 1e3)
            } else {
                format!("{:.1} Pa", si_value)
            }
        }
        Quantity::Density => {
            // Density: fixed notation
            format!("{:.2} kg/m³", si_value)
        }
        Quantity::SpecificEnthalpy | Quantity::SpecificEntropy => {
            // Energy/entropy: scientific notation for large ranges
            if si_value.abs() >= 1e6 {
                format!("{:.2e}", si_value)
            } else if si_value.abs() >= 1e3 {
                format!("{:.1}k", si_value / 1e3)
            } else {
                format!("{:.1}", si_value)
            }
        }
        Quantity::Quality => {
            // Quality: percentage or fraction
            if si_value > 1.0 {
                format!("{:.1}%", si_value * 100.0)
            } else {
                format!("{:.3}", si_value)
            }
        }
        Quantity::Dimensionless => {
            // Dimensionless: as-is or percentage
            if si_value.abs() > 1.0 {
                format!("{:.1}%", si_value * 100.0)
            } else {
                format!("{:.4}", si_value)
            }
        }
        Quantity::Mass => {
            // Mass
            if si_value >= 1e3 {
                format!("{:.1} t", si_value / 1e3)
            } else {
                format!("{:.2} kg", si_value)
            }
        }
        Quantity::Force => {
            // Force
            if si_value >= 1e6 {
                format!("{:.2e} N", si_value)
            } else {
                format!("{:.1} N", si_value)
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn input_state_creation() {
        let state = InputState::new(300.0, Quantity::Temperature);
        assert_eq!(state.canonical_si, 300.0);
        assert!(state.error.is_empty());
    }

    #[test]
    fn input_state_update_valid() {
        let mut state = InputState::new(300.0, Quantity::Temperature);
        let changed = state.update_text("400K".to_string(), Quantity::Temperature);
        assert!(changed);
        assert_eq!(state.canonical_si, 400.0);
        assert!(state.error.is_empty());
    }

    #[test]
    fn input_state_update_invalid() {
        let mut state = InputState::new(300.0, Quantity::Temperature);
        let changed = state.update_text("invalid".to_string(), Quantity::Temperature);
        assert!(!changed);
        assert!(!state.error.is_empty());
        assert_eq!(state.canonical_si, 300.0); // Original value preserved
    }

    #[test]
    fn format_temperature() {
        let text = format_display_value(300.0, Quantity::Temperature);
        assert!(text.contains("K"));
    }

    #[test]
    fn format_pressure() {
        let text = format_display_value(101_325.0, Quantity::Pressure);
        assert!(text.contains("kPa") || text.contains("Pa"));
    }

    #[test]
    fn format_quality() {
        let text = format_display_value(0.5, Quantity::Quality);
        assert!(text.contains("0.5") || text.contains("50"));
    }
}
