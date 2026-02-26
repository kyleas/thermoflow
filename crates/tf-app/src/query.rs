//! Query helpers for extracting data from loaded runs.

use tf_results::TimeseriesRecord;

use crate::error::{AppError, AppResult};

/// Summary of a run's time range and data.
#[derive(Debug, Clone)]
pub struct RunSummary {
    pub time_range: (f64, f64),
    pub record_count: usize,
    pub node_count: usize,
    pub component_count: usize,
}

/// Get run summary from timeseries records.
pub fn get_run_summary(records: &[TimeseriesRecord]) -> AppResult<RunSummary> {
    if records.is_empty() {
        return Err(AppError::InvalidInput("No records in run".to_string()));
    }

    let t_min = records.first().map(|r| r.time_s).unwrap_or(0.0);
    let t_max = records.last().map(|r| r.time_s).unwrap_or(0.0);

    let node_count = records.first().map(|r| r.node_values.len()).unwrap_or(0);
    let component_count = records.first().map(|r| r.edge_values.len()).unwrap_or(0);

    Ok(RunSummary {
        time_range: (t_min, t_max),
        record_count: records.len(),
        node_count,
        component_count,
    })
}

/// List all node IDs in a run.
pub fn list_node_ids(records: &[TimeseriesRecord]) -> Vec<String> {
    if let Some(first) = records.first() {
        first
            .node_values
            .iter()
            .map(|nv| nv.node_id.clone())
            .collect()
    } else {
        Vec::new()
    }
}

/// List all component IDs in a run.
pub fn list_component_ids(records: &[TimeseriesRecord]) -> Vec<String> {
    if let Some(first) = records.first() {
        first
            .edge_values
            .iter()
            .map(|ev| ev.component_id.clone())
            .collect()
    } else {
        Vec::new()
    }
}

/// Extract time series for a node variable.
pub fn extract_node_series(
    records: &[TimeseriesRecord],
    node_id: &str,
    variable: &str,
) -> AppResult<Vec<(f64, f64)>> {
    let mut series = Vec::new();

    for record in records {
        if let Some(node_val) = record.node_values.iter().find(|nv| nv.node_id == node_id) {
            let value = match variable {
                "p_pa" | "pressure" => node_val.p_pa,
                "t_k" | "temperature" => node_val.t_k,
                "h_j_per_kg" | "enthalpy" => node_val.h_j_per_kg,
                "rho_kg_m3" | "density" => node_val.rho_kg_m3,
                _ => {
                    return Err(AppError::InvalidInput(format!(
                        "Unknown node variable: {}",
                        variable
                    )))
                }
            };

            if let Some(v) = value {
                series.push((record.time_s, v));
            }
        }
    }

    Ok(series)
}

/// Extract time series for a component variable.
pub fn extract_component_series(
    records: &[TimeseriesRecord],
    component_id: &str,
    variable: &str,
) -> AppResult<Vec<(f64, f64)>> {
    let mut series = Vec::new();

    for record in records {
        if let Some(edge_val) = record
            .edge_values
            .iter()
            .find(|ev| ev.component_id == component_id)
        {
            let value = match variable {
                "mdot_kg_s" | "mass_flow" => edge_val.mdot_kg_s,
                "delta_p_pa" | "pressure_drop" => edge_val.delta_p_pa,
                _ => {
                    return Err(AppError::InvalidInput(format!(
                        "Unknown component variable: {}",
                        variable
                    )))
                }
            };

            if let Some(v) = value {
                series.push((record.time_s, v));
            }
        }
    }

    Ok(series)
}
