//! Result data types.

use serde::{Deserialize, Serialize};

pub type RunId = String;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RunManifest {
    pub run_id: RunId,
    pub system_id: String,
    pub timestamp: String,
    pub run_type: RunType,
    pub solver_version: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type")]
pub enum RunType {
    Steady,
    Transient {
        dt_s: f64,
        t_end_s: f64,
        steps: usize,
    },
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TimeseriesRecord {
    pub time_s: f64,
    pub node_values: Vec<NodeValueSnapshot>,
    pub edge_values: Vec<EdgeValueSnapshot>,
    pub global_values: GlobalValueSnapshot,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NodeValueSnapshot {
    pub node_id: String,
    pub p_pa: Option<f64>,
    pub t_k: Option<f64>,
    pub h_j_per_kg: Option<f64>,
    pub rho_kg_m3: Option<f64>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EdgeValueSnapshot {
    pub component_id: String,
    pub mdot_kg_s: Option<f64>,
    pub delta_p_pa: Option<f64>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct GlobalValueSnapshot {
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub control_values: Vec<ControlValueSnapshot>,
    pub omega_rad_s: Option<f64>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ControlValueSnapshot {
    pub id: String,
    pub kind: String,
    pub value: f64,
}
