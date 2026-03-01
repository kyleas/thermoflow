//! Project schema definitions.

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct Project {
    pub version: u32,
    pub name: String,
    #[serde(default)]
    pub systems: Vec<SystemDef>,
    #[serde(default)]
    pub modules: Vec<ModuleDef>,
    #[serde(default)]
    pub layouts: Vec<LayoutDef>,
    #[serde(default)]
    pub runs: RunLibraryDef,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub plotting_workspace: Option<PlottingWorkspaceDef>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub fluid_workspace: Option<FluidWorkspaceDef>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct SystemDef {
    pub id: String,
    pub name: String,
    pub fluid: FluidDef,
    #[serde(default)]
    pub nodes: Vec<NodeDef>,
    #[serde(default)]
    pub components: Vec<ComponentDef>,
    #[serde(default)]
    pub boundaries: Vec<BoundaryDef>,
    #[serde(default)]
    pub schedules: Vec<ScheduleDef>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub controls: Option<ControlSystemDef>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ControlSystemDef {
    #[serde(default)]
    pub blocks: Vec<ControlBlockDef>,
    #[serde(default)]
    pub connections: Vec<ControlConnectionDef>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ControlBlockDef {
    pub id: String,
    pub name: String,
    pub kind: ControlBlockKindDef,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(tag = "type")]
pub enum ControlBlockKindDef {
    Constant {
        value: f64,
    },
    MeasuredVariable {
        reference: MeasuredVariableDef,
    },
    PIController {
        kp: f64,
        ti_s: f64,
        out_min: f64,
        out_max: f64,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        integral_limit: Option<f64>,
        sample_period_s: f64,
    },
    PIDController {
        kp: f64,
        ti_s: f64,
        td_s: f64,
        td_filter_s: f64,
        out_min: f64,
        out_max: f64,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        integral_limit: Option<f64>,
        sample_period_s: f64,
    },
    FirstOrderActuator {
        tau_s: f64,
        rate_limit_per_s: f64,
        #[serde(default = "default_actuator_initial_position")]
        initial_position: f64,
    },
    ActuatorCommand {
        component_id: String,
    },
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(tag = "type")]
pub enum MeasuredVariableDef {
    NodePressure {
        node_id: String,
    },
    NodeTemperature {
        node_id: String,
    },
    EdgeMassFlow {
        component_id: String,
    },
    PressureDrop {
        from_node_id: String,
        to_node_id: String,
    },
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ControlConnectionDef {
    pub from_block_id: String,
    pub to_block_id: String,
    pub to_input: String,
}

fn default_actuator_initial_position() -> f64 {
    0.0
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct FluidDef {
    pub composition: CompositionDef,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(tag = "type")]
pub enum CompositionDef {
    Pure { species: String },
    Mixture { fractions: Vec<(String, f64)> },
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct NodeDef {
    pub id: String,
    pub name: String,
    pub kind: NodeKind,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(tag = "type")]
pub enum NodeKind {
    Junction,
    ControlVolume {
        volume_m3: f64,
        #[serde(default)]
        initial: InitialCvDef,
    },
    Atmosphere {
        pressure_pa: f64,
        temperature_k: f64,
    },
}

/// Control volume initial condition specification.
///
/// Supports explicit mode-based initialization (preferred) and backward-compatible
/// optional-field syntax (for migration).
///
/// Explicit modes (preferred):
/// ```yaml
/// initial:
///   mode: PT       # or PH, mT, mH
///   p_pa: 3500000.0
///   t_k: 300.0
/// ```
///
/// Backward-compatible syntax (deprecated; requires validation):
/// ```yaml
/// initial:
///   p_pa: 300000.0
///   t_k: 300.0
/// ```
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Default)]
pub struct InitialCvDef {
    /// Explicit initialization mode. If present, only relevant fields for that mode are used.
    /// If absent, the system will attempt to infer the mode from provided optional fields.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub mode: Option<String>, // "PT", "PH", "mT", "mH"

    // Mode-specific parameters (all optional for backward compat)
    pub p_pa: Option<f64>,
    pub t_k: Option<f64>,
    pub h_j_per_kg: Option<f64>,
    pub m_kg: Option<f64>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ComponentDef {
    pub id: String,
    pub name: String,
    pub kind: ComponentKind,
    pub from_node_id: String,
    pub to_node_id: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(tag = "type")]
pub enum ComponentKind {
    Orifice {
        cd: f64,
        area_m2: f64,
        treat_as_gas: bool,
    },
    Valve {
        cd: f64,
        area_max_m2: f64,
        position: f64,
        law: ValveLawDef,
        treat_as_gas: bool,
    },
    Pipe {
        length_m: f64,
        diameter_m: f64,
        roughness_m: f64,
        k_minor: f64,
        mu_pa_s: f64,
    },
    Pump {
        cd: f64,
        area_m2: f64,
        delta_p_pa: f64,
        eta: f64,
        treat_as_liquid: bool,
    },
    Turbine {
        cd: f64,
        area_m2: f64,
        eta: f64,
        treat_as_gas: bool,
    },
    LineVolume {
        volume_m3: f64,
        #[serde(default)]
        cd: f64,
        #[serde(default)]
        area_m2: f64,
    },
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(tag = "type")]
pub enum ValveLawDef {
    Linear,
    Quadratic,
    QuickOpening,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct BoundaryDef {
    pub node_id: String,
    pub pressure_pa: Option<f64>,
    pub temperature_k: Option<f64>,
    pub enthalpy_j_per_kg: Option<f64>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ScheduleDef {
    pub id: String,
    pub name: String,
    pub events: Vec<EventDef>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct EventDef {
    pub time_s: f64,
    pub action: ActionDef,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(tag = "type")]
pub enum ActionDef {
    SetValvePosition { component_id: String, position: f64 },
    SetBoundaryPressure { node_id: String, pressure_pa: f64 },
    SetBoundaryTemperature { node_id: String, temperature_k: f64 },
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ModuleDef {
    pub id: String,
    pub name: String,
    pub interface: ModuleInterfaceDef,
    pub template_system_id: Option<String>,
    #[serde(default)]
    pub exposed_nodes: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ModuleInterfaceDef {
    #[serde(default)]
    pub inputs: Vec<PortDef>,
    #[serde(default)]
    pub outputs: Vec<PortDef>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct PortDef {
    pub name: String,
    pub node_id: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ControlBlockLayout {
    pub block_id: String,
    pub x: f32,
    pub y: f32,
    #[serde(default)]
    pub label_offset_x: f32,
    #[serde(default)]
    pub label_offset_y: f32,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct SignalConnectionRoute {
    #[serde(default)]
    pub from_block_id: String,
    #[serde(default)]
    pub to_block_id: String,
    #[serde(default)]
    pub to_input: String,
    #[serde(default)]
    pub points: Vec<RoutePointDef>,
    #[serde(default)]
    pub label_offset_x: f32,
    #[serde(default)]
    pub label_offset_y: f32,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct LayoutDef {
    pub system_id: String,
    #[serde(default)]
    pub nodes: Vec<NodeLayout>,
    #[serde(default)]
    pub edges: Vec<EdgeLayout>,
    #[serde(default)]
    pub control_blocks: Vec<ControlBlockLayout>,
    #[serde(default)]
    pub signal_connections: Vec<SignalConnectionRoute>,
    #[serde(default)]
    pub overlay: OverlaySettingsDef,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct NodeLayout {
    pub node_id: String,
    pub x: f32,
    pub y: f32,
    #[serde(default)]
    pub label_offset_x: f32,
    #[serde(default)]
    pub label_offset_y: f32,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub overlay: Option<NodeOverlayDef>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Default)]
pub struct NodeOverlayDef {
    #[serde(default)]
    pub show_pressure: bool,
    #[serde(default)]
    pub show_temperature: bool,
    #[serde(default)]
    pub show_enthalpy: bool,
    #[serde(default)]
    pub show_density: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct EdgeLayout {
    pub component_id: String,
    #[serde(default)]
    pub points: Vec<RoutePointDef>,
    pub label_offset_x: f32,
    pub label_offset_y: f32,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub component_x: Option<f32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub component_y: Option<f32>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct RoutePointDef {
    pub x: f32,
    pub y: f32,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Default)]
pub struct OverlaySettingsDef {
    pub show_pressure: bool,
    pub show_temperature: bool,
    pub show_enthalpy: bool,
    pub show_density: bool,
    pub show_mass_flow: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Default)]
pub struct RunLibraryDef {
    #[serde(default)]
    pub runs: Vec<RunMetadataDef>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct RunMetadataDef {
    pub run_id: String,
    pub system_id: String,
    pub timestamp: String,
    pub run_type: RunTypeDef,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(tag = "type")]
pub enum RunTypeDef {
    Steady,
    Transient { dt_s: f64, t_end_s: f64 },
}

/// Persistent plotting workspace configuration.
/// Stores all plot panels, their positions, sizes, and series selections.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Default)]
pub struct PlottingWorkspaceDef {
    #[serde(default)]
    pub panels: Vec<PlotPanelDef>,
    #[serde(default)]
    pub templates: Vec<PlotTemplateDef>,
    /// Width of the plot workspace area in pixels
    #[serde(default)]
    pub workspace_width: f32,
    /// Height of the plot workspace area in pixels
    #[serde(default)]
    pub workspace_height: f32,
}

/// A single plot panel in the workspace.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct PlotPanelDef {
    /// Unique identifier for the panel
    pub id: String,
    /// User-defined name/title for the plot
    pub title: String,
    /// X position in the workspace (in pixels)
    pub x: f32,
    /// Y position in the workspace (in pixels)
    pub y: f32,
    /// Width of the panel (in pixels)
    pub width: f32,
    /// Height of the panel (in pixels)
    pub height: f32,
    /// Which run this plot is displaying data from
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub run_id: Option<String>,
    /// Series selection for this plot
    pub series_selection: PlotSeriesSelectionDef,
}

/// Generic arbitrary curve source  (valve characteristics, actuator responses, fluid property sweeps).
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(tag = "type")]
pub enum ArbitraryCurveSourceDef {
    ValveCharacteristic {
        component_id: String,
        characteristic: ValveCharacteristicKindDef,
        #[serde(default = "default_curve_sample_count")]
        sample_count: usize,
    },
    ActuatorResponse {
        tau_s: f64,
        rate_limit_per_s: f64,
        #[serde(default)]
        initial_position: f64,
        #[serde(default = "default_step_command")]
        command: f64,
        #[serde(default = "default_response_duration")]
        duration_s: f64,
        #[serde(default = "default_curve_sample_count")]
        sample_count: usize,
    },
    FluidPropertySweep {
        x_property: String,
        y_property: String,
        parameters: FluidSweepParametersDef,
    },
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
pub enum ValveCharacteristicKindDef {
    EffectiveArea,
    OpeningFactor,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct FluidSweepParametersDef {
    /// Independent variable being swept (e.g., "Temperature", "Pressure")
    pub sweep_variable: String,
    /// Start value with unit (e.g., "300K", "1bar")
    pub start_value: String,
    /// End value with unit (e.g., "400K", "10bar")
    pub end_value: String,
    /// Number of points to generate in the sweep
    #[serde(default = "default_sweep_points")]
    pub num_points: usize,
    /// Spacing type: "Linear" or "Logarithmic"
    #[serde(default = "default_sweep_type")]
    pub sweep_type: String,
    /// Fixed fluid species for the sweep (e.g., "N2", "H2O")
    pub species: String,
    /// Secondary fixed property (e.g., if sweeping temperature, might fix pressure)
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub fixed_property: Option<FixedPropertyDef>,
}

impl Default for FluidSweepParametersDef {
    fn default() -> Self {
        Self {
            sweep_variable: "Temperature".to_string(),
            start_value: "300K".to_string(),
            end_value: "400K".to_string(),
            num_points: default_sweep_points(),
            sweep_type: default_sweep_type(),
            species: "N2".to_string(),
            fixed_property: Some(FixedPropertyDef {
                property_name: "Pressure".to_string(),
                value: "101325Pa".to_string(),
            }),
        }
    }
}

/// Fixed property definition for sweeps.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct FixedPropertyDef {    /// Property name (e.g., "Pressure", "Temperature")
    pub property_name: String,
    /// Value with unit (e.g., "101325Pa", "300K")
    pub value: String,
}

fn default_sweep_points() -> usize {
    50
}

fn default_sweep_type() -> String {
    "Linear".to_string()
}

fn default_curve_sample_count() -> usize {
    100
}

fn default_step_command() -> f64 {
    1.0
}

fn default_response_duration() -> f64 {
    5.0
}

/// Specifies which series are shown in a plot.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Default)]
pub struct PlotSeriesSelectionDef {
    #[serde(default)]
    pub node_ids_and_variables: Vec<NodePlotVariableDef>,
    #[serde(default)]
    pub component_ids_and_variables: Vec<ComponentPlotVariableDef>,
    #[serde(default)]
    pub control_ids: Vec<String>,
    /// Arbitrary curve sources (valve characteristics, actuator responses, etc.)
    #[serde(default)]
    pub arbitrary_curves: Vec<ArbitraryCurveSourceDef>,
}

/// A node series to plot.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct NodePlotVariableDef {
    pub node_id: String,
    pub variable: String, // "Pressure", "Temperature", "Enthalpy", "Density"
}

/// A component series to plot.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ComponentPlotVariableDef {
    pub component_id: String,
    pub variable: String, // "MassFlow", "PressureDrop"
}

/// A reusable plot template/preset.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct PlotTemplateDef {
    /// Unique identifier for the template
    pub id: String,
    /// User-defined name for the template
    pub name: String,
    /// Optional description
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    /// Series selection that this template defines
    pub series_selection: PlotSeriesSelectionDef,
    /// Default width for plots created from this template
    #[serde(default)]
    pub default_width: f32,
    /// Default height for plots created from this template
    #[serde(default)]
    pub default_height: f32,
}

/// Persistent fluid workspace configuration for multi-column fluid comparison and analysis.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct FluidWorkspaceDef {
    /// Collection of fluid cases for comparison
    #[serde(default)]
    pub cases: Vec<FluidCaseDef>,
}

impl Default for FluidWorkspaceDef {
    fn default() -> Self {
        Self {
            cases: vec![FluidCaseDef::default()],
        }
    }
}

/// Single fluid case definition.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct FluidCaseDef {
    /// Unique identifier for this case
    pub id: String,
    /// Selected fluid species key (e.g. "N2", "H2O").
    pub species: String,
    /// Selected input pair.
    pub input_pair: FluidInputPairDef,
    /// First input value (meaning depends on pair).
    pub input_1: f64,
    /// Second input value (meaning depends on pair).
    pub input_2: f64,
    /// Optional quality for two-phase disambiguation (0.0 = sat. liquid, 1.0 = sat. vapor)
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub quality: Option<f64>,
}

impl Default for FluidCaseDef {
    fn default() -> Self {
        Self {
            id: uuid::Uuid::new_v4().to_string(),
            species: "N2".to_string(),
            input_pair: FluidInputPairDef::PT,
            input_1: 101_325.0,
            input_2: 300.0,
            quality: None,
        }
    }
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Default)]
pub enum FluidInputPairDef {
    #[default]
    PT,
    PH,
    RhoH,
    PS,
}
