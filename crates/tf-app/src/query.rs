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

/// List all control block IDs in a run.
pub fn list_control_ids(records: &[TimeseriesRecord]) -> Vec<String> {
    if let Some(first) = records.first() {
        first
            .global_values
            .control_values
            .iter()
            .map(|cv| cv.id.clone())
            .collect()
    } else {
        Vec::new()
    }
}

/// Extract time series for a control block output.
pub fn extract_control_series(
    records: &[TimeseriesRecord],
    control_id: &str,
) -> AppResult<Vec<(f64, f64)>> {
    let mut series = Vec::new();

    for record in records {
        if let Some(control_val) = record
            .global_values
            .control_values
            .iter()
            .find(|cv| cv.id == control_id)
        {
            series.push((record.time_s, control_val.value));
        }
    }

    Ok(series)
}

/// Attempt to identify and analyze control loops from persisted time-series data.
///
/// Returns a list of detected loops with their computed metrics.
/// A loop detection is heuristic: we look for common patterns like
/// (measured variable, setpoint, controller output, actuator position).
pub fn analyze_control_loops(records: &[TimeseriesRecord]) -> AppResult<Vec<ControlLoopAnalysis>> {
    let control_ids = list_control_ids(records);
    if control_ids.is_empty() {
        return Ok(Vec::new());
    }

    let mut loops = Vec::new();

    // Simple heuristic: identify loop patterns
    // For each measured variable, look for associated setpoint, controller, and actuator
    for control_id in &control_ids {
        if let Some(control_kind) = get_control_kind(records, control_id) {
            if control_kind == "measured" {
                // This might be a measured variable; try to find associated loop
                if let Some(loop_analysis) =
                    analysis_from_measured_variable(records, control_id, &control_ids)
                {
                    loops.push(loop_analysis);
                }
            }
        }
    }

    Ok(loops)
}

/// Summary of a detected control loop with its metrics
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct ControlLoopAnalysis {
    /// ID of the measured variable block
    pub measured_id: String,
    /// ID of the setpoint block (if found)
    pub setpoint_id: Option<String>,
    /// ID of the controller output block (if found)
    pub controller_id: Option<String>,
    /// ID of the actuator block (if found)
    pub actuator_id: Option<String>,
    /// Computed metrics for this loop
    pub metrics: crate::metrics::LoopMetrics,
}

/// Helper: get the kind of a control block
fn get_control_kind(records: &[TimeseriesRecord], control_id: &str) -> Option<String> {
    records.first().and_then(|r| {
        r.global_values
            .control_values
            .iter()
            .find(|cv| cv.id == control_id)
            .map(|cv| cv.kind.clone())
    })
}

/// Attempt to build a loop analysis starting from a measured variable
fn analysis_from_measured_variable(
    records: &[TimeseriesRecord],
    measured_id: &str,
    all_ids: &[String],
) -> Option<ControlLoopAnalysis> {
    use crate::metrics::compute_loop_metrics;

    // Extract measured series
    let measured_series = extract_control_series(records, measured_id).ok()?;
    if measured_series.is_empty() {
        return None;
    }

    // Look for setpoint (usually constant or measured)
    let setpoint_id = find_setpoint_for_measured(all_ids, measured_id);
    let setpoint_series = setpoint_id
        .as_ref()
        .and_then(|id| extract_control_series(records, id).ok())
        .unwrap_or_default();

    if setpoint_series.is_empty() {
        return None; // No setpoint found, can't analyze
    }

    // Look for controller output (PI, PID)
    let controller_id = find_controller_for_measured(all_ids, measured_id);
    let controller_series = controller_id
        .as_ref()
        .and_then(|id| extract_control_series(records, id).ok());

    // Look for actuator
    let actuator_id = find_actuator_for_controller(all_ids, controller_id.as_deref());
    let actuator_series = actuator_id
        .as_ref()
        .and_then(|id| extract_control_series(records, id).ok());

    // Now compute metrics
    let metrics = compute_loop_metrics(
        &measured_series,
        &setpoint_series,
        controller_series.as_deref(),
        actuator_series.as_deref(),
    )
    .unwrap_or_default();

    Some(ControlLoopAnalysis {
        measured_id: measured_id.to_string(),
        setpoint_id,
        controller_id,
        actuator_id,
        metrics,
    })
}

/// Heuristic: find the setpoint for a given measured variable
/// Usually named to match, or by kind (constant often used as setpoint)
fn find_setpoint_for_measured(all_ids: &[String], _measured_id: &str) -> Option<String> {
    // Simple: look for "sp_" prefix or "setpoint" in name
    for id in all_ids {
        if id.contains("sp_") || id.contains("setpoint") {
            return Some(id.clone());
        }
    }
    None
}

/// Heuristic: find a PI/PID controller that likely controls this measured variable
fn find_controller_for_measured(all_ids: &[String], _measured_id: &str) -> Option<String> {
    // Look for PIController or PIDController
    for id in all_ids {
        if id.contains("pi") || id.contains("pid") || id.contains("ctrl") {
            return Some(id.clone());
        }
    }
    None
}

/// Heuristic: find an actuator that is driven by the controller
fn find_actuator_for_controller(
    all_ids: &[String],
    _controller_id: Option<&str>,
) -> Option<String> {
    // Look for actuator or command block
    for id in all_ids {
        if id.contains("actuator") || id.contains("act_") || id.contains("command") {
            return Some(id.clone());
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;
    use tf_results::{ControlValueSnapshot, GlobalValueSnapshot};

    #[allow(dead_code)]
    fn make_test_record(
        time_s: f64,
        control_values: Vec<ControlValueSnapshot>,
    ) -> TimeseriesRecord {
        TimeseriesRecord {
            time_s,
            node_values: vec![],
            edge_values: vec![],
            global_values: GlobalValueSnapshot {
                control_values,
                omega_rad_s: None,
            },
        }
    }

    #[test]
    fn test_list_control_ids() {
        let records = vec![
            make_test_record(
                0.0,
                vec![
                    ControlValueSnapshot {
                        id: "ctrl1".to_string(),
                        kind: "pi".to_string(),
                        value: 10.0,
                    },
                    ControlValueSnapshot {
                        id: "ctrl2".to_string(),
                        kind: "constant".to_string(),
                        value: 5.0,
                    },
                ],
            ),
            make_test_record(
                1.0,
                vec![
                    ControlValueSnapshot {
                        id: "ctrl1".to_string(),
                        kind: "pi".to_string(),
                        value: 12.0,
                    },
                    ControlValueSnapshot {
                        id: "ctrl2".to_string(),
                        kind: "constant".to_string(),
                        value: 5.0,
                    },
                ],
            ),
        ];

        let ids = list_control_ids(&records);
        assert_eq!(ids, vec!["ctrl1", "ctrl2"]);
    }

    #[test]
    fn test_extract_control_series() {
        let records = vec![
            make_test_record(
                0.0,
                vec![
                    ControlValueSnapshot {
                        id: "ctrl1".to_string(),
                        kind: "pi".to_string(),
                        value: 10.0,
                    },
                    ControlValueSnapshot {
                        id: "ctrl2".to_string(),
                        kind: "constant".to_string(),
                        value: 5.0,
                    },
                ],
            ),
            make_test_record(
                1.0,
                vec![
                    ControlValueSnapshot {
                        id: "ctrl1".to_string(),
                        kind: "pi".to_string(),
                        value: 12.0,
                    },
                    ControlValueSnapshot {
                        id: "ctrl2".to_string(),
                        kind: "constant".to_string(),
                        value: 5.0,
                    },
                ],
            ),
            make_test_record(
                2.0,
                vec![
                    ControlValueSnapshot {
                        id: "ctrl1".to_string(),
                        kind: "pi".to_string(),
                        value: 15.0,
                    },
                    ControlValueSnapshot {
                        id: "ctrl2".to_string(),
                        kind: "constant".to_string(),
                        value: 5.0,
                    },
                ],
            ),
        ];

        let series = extract_control_series(&records, "ctrl1").unwrap();
        assert_eq!(series.len(), 3);
        assert_eq!(series[0], (0.0, 10.0));
        assert_eq!(series[1], (1.0, 12.0));
        assert_eq!(series[2], (2.0, 15.0));

        let series2 = extract_control_series(&records, "ctrl2").unwrap();
        assert_eq!(series2.len(), 3);
        assert_eq!(series2[0], (0.0, 5.0));
        assert_eq!(series2[1], (1.0, 5.0));
        assert_eq!(series2[2], (2.0, 5.0));
    }

    #[test]
    fn test_extract_control_series_missing() {
        let records = vec![make_test_record(
            0.0,
            vec![ControlValueSnapshot {
                id: "ctrl1".to_string(),
                kind: "pi".to_string(),
                value: 10.0,
            }],
        )];

        let series = extract_control_series(&records, "missing").unwrap();
        assert_eq!(series.len(), 0);
    }
}
