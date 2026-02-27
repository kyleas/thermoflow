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
pub struct LayoutDef {
    pub system_id: String,
    #[serde(default)]
    pub nodes: Vec<NodeLayout>,
    #[serde(default)]
    pub edges: Vec<EdgeLayout>,
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
