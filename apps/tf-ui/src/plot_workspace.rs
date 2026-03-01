//! Runtime plot workspace management.
//!
//! Manages a collection of plot panels with drag/resize capabilities,
//! series selection, and templates.

#![allow(dead_code)] // Pre-implemented methods used in future persistence/integration phases

use crate::curve_source::CurveSource;
use std::collections::HashMap;
use tf_project::schema::{
    ComponentPlotVariableDef, NodePlotVariableDef, PlotPanelDef, PlotSeriesSelectionDef,
    PlotTemplateDef, PlottingWorkspaceDef,
};

const DEFAULT_PANEL_WIDTH: f32 = 500.0;
const DEFAULT_PANEL_HEIGHT: f32 = 400.0;

/// Runtime representation of a plot panel.
#[derive(Debug, Clone)]
pub struct PlotPanel {
    pub id: String,
    pub title: String,
    pub x: f32,
    pub y: f32,
    pub width: f32,
    pub height: f32,
    pub run_id: Option<String>,
    pub series_selection: PlotSeriesSelection,
}

/// Runtime representation of series selection for a plot.
#[derive(Debug, Clone, Default)]
pub struct PlotSeriesSelection {
    pub node_ids_and_variables: Vec<(String, String)>, // (node_id, variable)
    pub component_ids_and_variables: Vec<(String, String)>, // (component_id, variable)
    pub control_ids: Vec<String>,
    /// Arbitrary curve sources (valve characteristics, actuator responses, etc.)
    pub arbitrary_curves: Vec<CurveSource>,
}

/// Runtime representation of a plot template.
#[derive(Debug, Clone)]
pub struct PlotTemplate {
    pub id: String,
    pub name: String,
    pub description: Option<String>,
    pub series_selection: PlotSeriesSelection,
    pub default_width: f32,
    pub default_height: f32,
}

/// Runtime plot workspace state.
#[derive(Debug)]
pub struct PlotWorkspace {
    pub panels: HashMap<String, PlotPanel>,
    pub templates: HashMap<String, PlotTemplate>,
    pub workspace_width: f32,
    pub workspace_height: f32,
    pub panel_order: Vec<String>, // Order for rendering/selection
    pub selected_panel_id: Option<String>,
    pub dragging_panel_id: Option<String>,
    pub drag_start_x: f32,
    pub drag_start_y: f32,
    pub resizing_panel_id: Option<String>,
}

impl Default for PlotWorkspace {
    fn default() -> Self {
        Self::new()
    }
}

impl PlotWorkspace {
    pub fn new() -> Self {
        Self {
            panels: HashMap::new(),
            templates: HashMap::new(),
            workspace_width: 1200.0,
            workspace_height: 800.0,
            panel_order: Vec::new(),
            selected_panel_id: None,
            dragging_panel_id: None,
            drag_start_x: 0.0,
            drag_start_y: 0.0,
            resizing_panel_id: None,
        }
    }

    /// Load workspace from persistent definition.
    pub fn from_def(def: &PlottingWorkspaceDef) -> Self {
        let mut workspace = Self::new();
        workspace.workspace_width = def.workspace_width;
        workspace.workspace_height = def.workspace_height;

        // Load panels
        for panel_def in &def.panels {
            let panel = PlotPanel {
                id: panel_def.id.clone(),
                title: panel_def.title.clone(),
                x: panel_def.x,
                y: panel_def.y,
                width: panel_def.width,
                height: panel_def.height,
                run_id: panel_def.run_id.clone(),
                series_selection: PlotSeriesSelection::from_def(&panel_def.series_selection),
            };
            workspace.panel_order.push(panel_def.id.clone());
            workspace.panels.insert(panel_def.id.clone(), panel);
        }

        // Load templates
        for template_def in &def.templates {
            let template = PlotTemplate {
                id: template_def.id.clone(),
                name: template_def.name.clone(),
                description: template_def.description.clone(),
                series_selection: PlotSeriesSelection::from_def(&template_def.series_selection),
                default_width: template_def.default_width,
                default_height: template_def.default_height,
            };
            workspace
                .templates
                .insert(template_def.id.clone(), template);
        }

        workspace
    }

    /// Export to persistent definition.
    pub fn to_def(&self) -> PlottingWorkspaceDef {
        PlottingWorkspaceDef {
            panels: self
                .panel_order
                .iter()
                .filter_map(|id| self.panels.get(id).map(|p| p.to_def()))
                .collect(),
            templates: self.templates.values().map(|t| t.to_def()).collect(),
            workspace_width: self.workspace_width,
            workspace_height: self.workspace_height,
        }
    }

    /// Create a new plot panel.
    pub fn create_panel(&mut self, title: String, run_id: Option<String>) -> String {
        let id = format!("panel_{}", uuid::Uuid::new_v4());
        let panel = PlotPanel {
            id: id.clone(),
            title,
            x: 10.0,
            y: 10.0 + (self.panels.len() as f32) * 30.0, // Cascade new panels
            width: DEFAULT_PANEL_WIDTH,
            height: DEFAULT_PANEL_HEIGHT,
            run_id,
            series_selection: PlotSeriesSelection::default(),
        };
        self.panel_order.push(id.clone());
        self.panels.insert(id.clone(), panel);
        id
    }

    /// Delete a plot panel by ID.
    pub fn delete_panel(&mut self, panel_id: &str) -> bool {
        self.panel_order.retain(|id| id != panel_id);
        if self.selected_panel_id.as_deref() == Some(panel_id) {
            self.selected_panel_id = self.panel_order.first().cloned();
        }
        self.panels.remove(panel_id).is_some()
    }

    /// Rename a plot panel.
    pub fn rename_panel(&mut self, panel_id: &str, new_title: String) -> bool {
        if let Some(panel) = self.panels.get_mut(panel_id) {
            panel.title = new_title;
            true
        } else {
            false
        }
    }

    /// Update a panel's position and size.
    pub fn update_panel_rect(
        &mut self,
        panel_id: &str,
        x: f32,
        y: f32,
        width: f32,
        height: f32,
    ) -> bool {
        if let Some(panel) = self.panels.get_mut(panel_id) {
            panel.x = x;
            panel.y = y;
            panel.width = width;
            panel.height = height;
            true
        } else {
            false
        }
    }

    /// Select a panel.
    pub fn select_panel(&mut self, panel_id: Option<String>) {
        self.selected_panel_id = panel_id;
    }

    /// Start dragging a panel.
    pub fn start_drag(&mut self, panel_id: String, mouse_x: f32, mouse_y: f32) {
        self.dragging_panel_id = Some(panel_id);
        self.drag_start_x = mouse_x;
        self.drag_start_y = mouse_y;
    }

    /// Stop dragging.
    pub fn stop_drag(&mut self) {
        self.dragging_panel_id = None;
    }

    /// Start resizing a panel.
    pub fn start_resize(&mut self, panel_id: String) {
        self.resizing_panel_id = Some(panel_id);
    }

    /// Stop resizing.
    pub fn stop_resize(&mut self) {
        self.resizing_panel_id = None;
    }

    /// Create a template from current panel configuration.
    pub fn create_template_from_panel(
        &mut self,
        panel_id: &str,
        template_name: String,
    ) -> Option<String> {
        let panel = self.panels.get(panel_id)?;
        let template_id = format!("template_{}", uuid::Uuid::new_v4());
        let template = PlotTemplate {
            id: template_id.clone(),
            name: template_name,
            description: Some(format!("Template based on panel '{}'", panel.title)),
            series_selection: panel.series_selection.clone(),
            default_width: panel.width,
            default_height: panel.height,
        };
        self.templates.insert(template_id.clone(), template);
        Some(template_id)
    }

    /// Apply a template to a panel.
    pub fn apply_template_to_panel(&mut self, panel_id: &str, template_id: &str) -> bool {
        if let Some(template) = self.templates.get(template_id) {
            if let Some(panel) = self.panels.get_mut(panel_id) {
                panel.series_selection = template.series_selection.clone();
                return true;
            }
        }
        false
    }

    /// Create a new panel from a template.
    pub fn create_panel_from_template(
        &mut self,
        template_id: &str,
        run_id: Option<String>,
    ) -> Option<String> {
        let template = self.templates.get(template_id)?;
        let panel_id = format!("panel_{}", uuid::Uuid::new_v4());
        let panel = PlotPanel {
            id: panel_id.clone(),
            title: template.name.clone(),
            x: 10.0 + (self.panels.len() as f32) * 20.0,
            y: 10.0 + (self.panels.len() as f32) * 20.0,
            width: if template.default_width > 0.0 {
                template.default_width
            } else {
                DEFAULT_PANEL_WIDTH
            },
            height: if template.default_height > 0.0 {
                template.default_height
            } else {
                DEFAULT_PANEL_HEIGHT
            },
            run_id,
            series_selection: template.series_selection.clone(),
        };
        self.panel_order.push(panel_id.clone());
        self.panels.insert(panel_id.clone(), panel);
        Some(panel_id)
    }
}

impl PlotSeriesSelection {
    fn from_def(def: &PlotSeriesSelectionDef) -> Self {
        Self {
            node_ids_and_variables: def
                .node_ids_and_variables
                .iter()
                .map(|nv| (nv.node_id.clone(), nv.variable.clone()))
                .collect(),
            component_ids_and_variables: def
                .component_ids_and_variables
                .iter()
                .map(|cv| (cv.component_id.clone(), cv.variable.clone()))
                .collect(),
            control_ids: def.control_ids.clone(),
            arbitrary_curves: def
                .arbitrary_curves
                .iter()
                .map(Self::curve_from_def)
                .collect(),
        }
    }

    fn to_def(&self) -> PlotSeriesSelectionDef {
        PlotSeriesSelectionDef {
            node_ids_and_variables: self
                .node_ids_and_variables
                .iter()
                .map(|(node_id, variable)| NodePlotVariableDef {
                    node_id: node_id.clone(),
                    variable: variable.clone(),
                })
                .collect(),
            component_ids_and_variables: self
                .component_ids_and_variables
                .iter()
                .map(|(component_id, variable)| ComponentPlotVariableDef {
                    component_id: component_id.clone(),
                    variable: variable.clone(),
                })
                .collect(),
            control_ids: self.control_ids.clone(),
            arbitrary_curves: self
                .arbitrary_curves
                .iter()
                .map(Self::curve_to_def)
                .collect(),
        }
    }

    /// Convert from schema definition to runtime curve source.
    fn curve_from_def(def: &tf_project::schema::ArbitraryCurveSourceDef) -> CurveSource {
        use crate::curve_source::{FluidSweepParameters, ValveCharacteristicKind};
        use tf_project::schema::{ArbitraryCurveSourceDef, ValveCharacteristicKindDef};

        match def {
            ArbitraryCurveSourceDef::ValveCharacteristic {
                component_id,
                characteristic,
                sample_count,
            } => CurveSource::ValveCharacteristic {
                component_id: component_id.clone(),
                characteristic: match characteristic {
                    ValveCharacteristicKindDef::EffectiveArea => {
                        ValveCharacteristicKind::EffectiveArea
                    }
                    ValveCharacteristicKindDef::OpeningFactor => {
                        ValveCharacteristicKind::OpeningFactor
                    }
                },
                sample_count: *sample_count,
            },
            ArbitraryCurveSourceDef::ActuatorResponse {
                tau_s,
                rate_limit_per_s,
                initial_position,
                command,
                duration_s,
                sample_count,
            } => CurveSource::ActuatorResponse {
                tau_s: *tau_s,
                rate_limit_per_s: *rate_limit_per_s,
                initial_position: *initial_position,
                command: *command,
                duration_s: *duration_s,
                sample_count: *sample_count,
            },
            ArbitraryCurveSourceDef::FluidPropertySweep {
                x_property,
                y_property,
                parameters: _,
            } => CurveSource::FluidPropertySweep {
                x_property: x_property.clone(),
                y_property: y_property.clone(),
                parameters: FluidSweepParameters::default(),
            },
        }
    }

    /// Convert from runtime curve source to schema definition.
    fn curve_to_def(source: &CurveSource) -> tf_project::schema::ArbitraryCurveSourceDef {
        use crate::curve_source::ValveCharacteristicKind;
        use tf_project::schema::{
            ArbitraryCurveSourceDef, FluidSweepParametersDef, ValveCharacteristicKindDef,
        };

        match source {
            CurveSource::ValveCharacteristic {
                component_id,
                characteristic,
                sample_count,
            } => ArbitraryCurveSourceDef::ValveCharacteristic {
                component_id: component_id.clone(),
                characteristic: match characteristic {
                    ValveCharacteristicKind::EffectiveArea => {
                        ValveCharacteristicKindDef::EffectiveArea
                    }
                    ValveCharacteristicKind::OpeningFactor => {
                        ValveCharacteristicKindDef::OpeningFactor
                    }
                },
                sample_count: *sample_count,
            },
            CurveSource::ActuatorResponse {
                tau_s,
                rate_limit_per_s,
                initial_position,
                command,
                duration_s,
                sample_count,
            } => ArbitraryCurveSourceDef::ActuatorResponse {
                tau_s: *tau_s,
                rate_limit_per_s: *rate_limit_per_s,
                initial_position: *initial_position,
                command: *command,
                duration_s: *duration_s,
                sample_count: *sample_count,
            },
            CurveSource::FluidPropertySweep {
                x_property,
                y_property,
                parameters: _,
            } => ArbitraryCurveSourceDef::FluidPropertySweep {
                x_property: x_property.clone(),
                y_property: y_property.clone(),
                parameters: FluidSweepParametersDef::default(),
            },
        }
    }

    /// Add a node variable to plot.
    pub fn add_node_variable(&mut self, node_id: String, variable: String) {
        if !self
            .node_ids_and_variables
            .iter()
            .any(|(n, v)| n == &node_id && v == &variable)
        {
            self.node_ids_and_variables.push((node_id, variable));
        }
    }

    /// Remove a node variable from plot.
    pub fn remove_node_variable(&mut self, node_id: &str, variable: &str) {
        self.node_ids_and_variables
            .retain(|(n, v)| !(n == node_id && v == variable));
    }

    /// Add a component variable to plot.
    pub fn add_component_variable(&mut self, component_id: String, variable: String) {
        if !self
            .component_ids_and_variables
            .iter()
            .any(|(c, v)| c == &component_id && v == &variable)
        {
            self.component_ids_and_variables
                .push((component_id, variable));
        }
    }

    /// Remove a component variable from plot.
    pub fn remove_component_variable(&mut self, component_id: &str, variable: &str) {
        self.component_ids_and_variables
            .retain(|(c, v)| !(c == component_id && v == variable));
    }

    /// Add a control block to plot.
    pub fn add_control_id(&mut self, control_id: String) {
        if !self.control_ids.contains(&control_id) {
            self.control_ids.push(control_id);
        }
    }

    /// Remove a control block from plot.
    pub fn remove_control_id(&mut self, control_id: &str) {
        self.control_ids.retain(|id| id != control_id);
    }

    /// Clear all series selections.
    pub fn clear(&mut self) {
        self.node_ids_and_variables.clear();
        self.component_ids_and_variables.clear();
        self.control_ids.clear();
    }
}

impl PlotPanel {
    fn to_def(&self) -> PlotPanelDef {
        PlotPanelDef {
            id: self.id.clone(),
            title: self.title.clone(),
            x: self.x,
            y: self.y,
            width: self.width,
            height: self.height,
            run_id: self.run_id.clone(),
            series_selection: self.series_selection.to_def(),
        }
    }
}

impl PlotTemplate {
    fn to_def(&self) -> PlotTemplateDef {
        PlotTemplateDef {
            id: self.id.clone(),
            name: self.name.clone(),
            description: self.description.clone(),
            series_selection: self.series_selection.to_def(),
            default_width: self.default_width,
            default_height: self.default_height,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_create_panel() {
        let mut workspace = PlotWorkspace::new();
        assert_eq!(workspace.panels.len(), 0);

        let panel_id = workspace.create_panel("Test Plot".to_string(), None);
        assert_eq!(workspace.panels.len(), 1);
        assert!(workspace.panels.contains_key(&panel_id));

        let panel = &workspace.panels[&panel_id];
        assert_eq!(panel.title, "Test Plot");
        assert_eq!(panel.x, 10.0);
        assert_eq!(panel.y, 10.0);
        assert_eq!(panel.width, DEFAULT_PANEL_WIDTH);
        assert_eq!(panel.height, DEFAULT_PANEL_HEIGHT);
    }

    #[test]
    fn test_delete_panel() {
        let mut workspace = PlotWorkspace::new();
        let panel_id = workspace.create_panel("Test Plot".to_string(), None);
        assert_eq!(workspace.panels.len(), 1);

        let success = workspace.delete_panel(&panel_id);
        assert!(success);
        assert_eq!(workspace.panels.len(), 0);
        assert!(!workspace.panels.contains_key(&panel_id));
    }

    #[test]
    fn test_rename_panel() {
        let mut workspace = PlotWorkspace::new();
        let panel_id = workspace.create_panel("Original Name".to_string(), None);

        let success = workspace.rename_panel(&panel_id, "New Name".to_string());
        assert!(success);

        let panel = &workspace.panels[&panel_id];
        assert_eq!(panel.title, "New Name");
    }

    #[test]
    fn test_select_panel() {
        let mut workspace = PlotWorkspace::new();
        let panel_id = workspace.create_panel("Test Plot".to_string(), None);

        workspace.select_panel(Some(panel_id.clone()));
        assert_eq!(workspace.selected_panel_id, Some(panel_id));

        workspace.select_panel(None);
        assert_eq!(workspace.selected_panel_id, None);
    }

    #[test]
    fn test_update_panel_rect() {
        let mut workspace = PlotWorkspace::new();
        let panel_id = workspace.create_panel("Test Plot".to_string(), None);

        let success = workspace.update_panel_rect(&panel_id, 50.0, 100.0, 600.0, 500.0);
        assert!(success);

        let panel = &workspace.panels[&panel_id];
        assert_eq!(panel.x, 50.0);
        assert_eq!(panel.y, 100.0);
        assert_eq!(panel.width, 600.0);
        assert_eq!(panel.height, 500.0);
    }

    #[test]
    fn test_create_template_from_panel() {
        let mut workspace = PlotWorkspace::new();
        let panel_id = workspace.create_panel("Test Plot".to_string(), None);

        // Add a series to the panel
        if let Some(panel) = workspace.panels.get_mut(&panel_id) {
            panel
                .series_selection
                .add_node_variable("node1".to_string(), "Pressure".to_string());
        }

        let template_id = workspace
            .create_template_from_panel(&panel_id, "Template 1".to_string())
            .unwrap();

        assert_eq!(workspace.templates.len(), 1);
        let template = &workspace.templates[&template_id];
        assert_eq!(template.name, "Template 1");
        assert!(!template.series_selection.node_ids_and_variables.is_empty());
    }

    #[test]
    fn test_apply_template_to_panel() {
        let mut workspace = PlotWorkspace::new();
        let panel_id = workspace.create_panel("Test Plot".to_string(), None);

        // Create a template with specific series
        workspace.panels.get_mut(&panel_id).unwrap();
        let template_id = format!("template_{}", uuid::Uuid::new_v4());
        let mut series = PlotSeriesSelection::default();
        series.add_node_variable("node2".to_string(), "Temperature".to_string());

        workspace.templates.insert(
            template_id.clone(),
            PlotTemplate {
                id: template_id.clone(),
                name: "Test Template".to_string(),
                description: None,
                series_selection: series,
                default_width: 500.0,
                default_height: 400.0,
            },
        );

        // Apply template to panel
        let success = workspace.apply_template_to_panel(&panel_id, &template_id);
        assert!(success);

        let panel = &workspace.panels[&panel_id];
        assert!(!panel.series_selection.node_ids_and_variables.is_empty());
    }

    #[test]
    fn test_create_panel_from_template() {
        let mut workspace = PlotWorkspace::new();

        // Create template directly
        let template_id = format!("template_{}", uuid::Uuid::new_v4());
        let series = PlotSeriesSelection::default();
        workspace.templates.insert(
            template_id.clone(),
            PlotTemplate {
                id: template_id.clone(),
                name: "Test Template".to_string(),
                description: Some("A test template".to_string()),
                series_selection: series,
                default_width: 600.0,
                default_height: 450.0,
            },
        );

        // Create panel from template
        let panel_id = workspace
            .create_panel_from_template(&template_id, None)
            .unwrap();

        assert_eq!(workspace.panels.len(), 1);
        let panel = &workspace.panels[&panel_id];
        assert_eq!(panel.title, "Test Template");
        assert_eq!(panel.width, 600.0);
        assert_eq!(panel.height, 450.0);
    }

    #[test]
    fn test_persistence_round_trip() {
        // Create workspace with panels and templates
        let mut workspace1 = PlotWorkspace::new();
        let panel_id = workspace1.create_panel("Plot 1".to_string(), Some("run1".to_string()));

        // Add series to the panel
        if let Some(panel) = workspace1.panels.get_mut(&panel_id) {
            panel
                .series_selection
                .add_node_variable("node1".to_string(), "Pressure".to_string());
        }

        workspace1.create_template_from_panel(&panel_id, "Template 1".to_string());

        // Export to definition
        let def = workspace1.to_def();

        // Import from definition
        let workspace2 = PlotWorkspace::from_def(&def);

        // Verify round-trip
        assert_eq!(workspace2.panels.len(), workspace1.panels.len());
        assert_eq!(workspace2.templates.len(), workspace1.templates.len());
        assert_eq!(workspace2.workspace_width, workspace1.workspace_width);
        assert_eq!(workspace2.workspace_height, workspace1.workspace_height);
    }

    #[test]
    fn test_drag_start_stop() {
        let mut workspace = PlotWorkspace::new();
        let panel_id = workspace.create_panel("Test Plot".to_string(), None);

        workspace.start_drag(panel_id.clone(), 100.0, 200.0);
        assert_eq!(workspace.dragging_panel_id, Some(panel_id.clone()));
        assert_eq!(workspace.drag_start_x, 100.0);
        assert_eq!(workspace.drag_start_y, 200.0);

        workspace.stop_drag();
        assert_eq!(workspace.dragging_panel_id, None);
    }

    #[test]
    fn test_resize_start_stop() {
        let mut workspace = PlotWorkspace::new();
        let panel_id = workspace.create_panel("Test Plot".to_string(), None);

        workspace.start_resize(panel_id.clone());
        assert_eq!(workspace.resizing_panel_id, Some(panel_id.clone()));

        workspace.stop_resize();
        assert_eq!(workspace.resizing_panel_id, None);
    }

    #[test]
    fn test_series_selection_operations() {
        let mut selection = PlotSeriesSelection::default();

        selection.add_node_variable("node1".to_string(), "Pressure".to_string());
        assert_eq!(selection.node_ids_and_variables.len(), 1);

        selection.add_component_variable("comp1".to_string(), "MassFlow".to_string());
        assert_eq!(selection.component_ids_and_variables.len(), 1);

        selection.add_control_id("control1".to_string());
        assert_eq!(selection.control_ids.len(), 1);

        selection.remove_node_variable("node1", "Pressure");
        assert_eq!(selection.node_ids_and_variables.len(), 0);

        selection.remove_component_variable("comp1", "MassFlow");
        assert_eq!(selection.component_ids_and_variables.len(), 0);

        selection.remove_control_id("control1");
        assert_eq!(selection.control_ids.len(), 0);
    }

    #[test]
    fn test_panel_cascading_position() {
        let mut workspace = PlotWorkspace::new();

        let panel1_id = workspace.create_panel("Plot 1".to_string(), None);
        let panel2_id = workspace.create_panel("Plot 2".to_string(), None);
        let panel3_id = workspace.create_panel("Plot 3".to_string(), None);

        let panel1 = &workspace.panels[&panel1_id];
        let panel2 = &workspace.panels[&panel2_id];
        let panel3 = &workspace.panels[&panel3_id];

        // Panels should cascade (y increase with each new panel)
        assert!(panel1.y < panel2.y);
        assert!(panel2.y < panel3.y);
    }

    #[test]
    fn test_arbitrary_curves_persistence() {
        use crate::curve_source::{CurveSource, ValveCharacteristicKind};

        let mut selection = PlotSeriesSelection::default();

        // Add valve characteristic curve
        selection
            .arbitrary_curves
            .push(CurveSource::ValveCharacteristic {
                component_id: "valve1".to_string(),
                characteristic: ValveCharacteristicKind::EffectiveArea,
                sample_count: 100,
            });

        // Add actuator response curve
        selection
            .arbitrary_curves
            .push(CurveSource::ActuatorResponse {
                tau_s: 1.5,
                rate_limit_per_s: 2.0,
                initial_position: 0.0,
                command: 1.0,
                duration_s: 5.0,
                sample_count: 150,
            });

        // Convert to def and back
        let def = selection.to_def();
        let selection2 = PlotSeriesSelection::from_def(&def);

        // Verify round-trip
        assert_eq!(selection2.arbitrary_curves.len(), 2);

        // Check first curve (valve)
        match &selection2.arbitrary_curves[0] {
            CurveSource::ValveCharacteristic {
                component_id,
                characteristic,
                sample_count,
            } => {
                assert_eq!(component_id, "valve1");
                assert_eq!(*characteristic, ValveCharacteristicKind::EffectiveArea);
                assert_eq!(*sample_count, 100);
            }
            _ => panic!("Expected ValveCharacteristic"),
        }

        // Check second curve (actuator)
        match &selection2.arbitrary_curves[1] {
            CurveSource::ActuatorResponse {
                tau_s,
                rate_limit_per_s,
                command,
                duration_s,
                sample_count,
                ..
            } => {
                assert_eq!(*tau_s, 1.5);
                assert_eq!(*rate_limit_per_s, 2.0);
                assert_eq!(*command, 1.0);
                assert_eq!(*duration_s, 5.0);
                assert_eq!(*sample_count, 150);
            }
            _ => panic!("Expected ActuatorResponse"),
        }
    }

    #[test]
    fn test_workspace_with_arbitrary_curves_persistence() {
        use crate::curve_source::{CurveSource, ValveCharacteristicKind};

        let mut workspace1 = PlotWorkspace::new();
        let panel_id = workspace1.create_panel("Plot with curves".to_string(), None);

        // Add arbitrary curves to the panel
        if let Some(panel) = workspace1.panels.get_mut(&panel_id) {
            panel
                .series_selection
                .arbitrary_curves
                .push(CurveSource::ValveCharacteristic {
                    component_id: "valve1".to_string(),
                    characteristic: ValveCharacteristicKind::OpeningFactor,
                    sample_count: 50,
                });
            panel
                .series_selection
                .arbitrary_curves
                .push(CurveSource::ActuatorResponse {
                    tau_s: 0.5,
                    rate_limit_per_s: 1.0,
                    initial_position: 0.5,
                    command: 0.0,
                    duration_s: 3.0,
                    sample_count: 75,
                });
        }

        // Export to definition and re-import
        let def = workspace1.to_def();
        let workspace2 = PlotWorkspace::from_def(&def);

        // Verify arbitrary curves persisted
        assert_eq!(workspace2.panels.len(), 1);
        let restored_panel = workspace2.panels.values().next().unwrap();
        assert_eq!(restored_panel.series_selection.arbitrary_curves.len(), 2);
    }
}
