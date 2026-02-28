//! Integration tests for supported fixed-topology multi-control-volume transients.

use std::path::{Path, PathBuf};

use tf_app::{
    ensure_run_with_progress, load_run, RunMode, RunOptions, RunProgressEvent, RunRequest,
};
use tf_results::TimeseriesRecord;

fn run_transient_capture(
    project_path: &Path,
    system_id: &str,
    dt_s: f64,
    t_end_s: f64,
) -> (
    Vec<TimeseriesRecord>,
    Vec<RunProgressEvent>,
    tf_app::RunTimingSummary,
) {
    let request = RunRequest {
        project_path,
        system_id,
        mode: RunMode::Transient { dt_s, t_end_s },
        options: RunOptions {
            use_cache: false,
            solver_version: "0.1.0".to_string(),
            initialization_strategy: None,
        },
    };

    let mut progress = Vec::new();
    let response = ensure_run_with_progress(
        &request,
        Some(&mut |event| {
            progress.push(event);
        }),
    )
    .expect("Transient run failed");

    let (_manifest, records) =
        load_run(project_path, &response.run_id).expect("Failed to load run output");

    (records, progress, response.timing)
}

fn assert_physical(records: &[TimeseriesRecord]) {
    assert!(records.len() >= 2, "Expected at least two records");

    for record in records {
        for node in &record.node_values {
            if let Some(p) = node.p_pa {
                assert!(p.is_finite(), "pressure must be finite");
                assert!(p > 0.0, "pressure must remain positive");
            }
            if let Some(h) = node.h_j_per_kg {
                assert!(h.is_finite(), "enthalpy must be finite");
            }
        }

        for edge in &record.edge_values {
            if let Some(mdot) = edge.mdot_kg_s {
                assert!(mdot.is_finite(), "mass flow must be finite");
            }
        }
    }
}

#[test]
fn multicv_series_vent_runs_and_stays_physical() {
    let project_path =
        PathBuf::from("../../examples/projects/04_two_cv_series_vent_transient.yaml");
    let (records, progress, timing) = run_transient_capture(&project_path, "s1", 0.02, 0.2);

    assert_physical(&records);
    assert!(
        progress
            .iter()
            .any(|e| matches!(e.mode, RunMode::Transient { .. })),
        "Expected transient progress events"
    );
    assert_eq!(
        timing.transient_fallback_uses, 0,
        "Supported benchmark should run on real-fluid path without fallback"
    );
    assert_eq!(timing.initialization_strategy.as_deref(), Some("Relaxed"));
    assert!(timing.transient_surrogate_populations > 0);
    assert!(timing.rhs_calls > 0);
    assert!(timing.rhs_snapshot_time_s > 0.0);
    assert!(timing.rhs_state_reconstruct_time_s > 0.0);
    assert!(timing.transient_surrogate_populations <= 4);
}

#[test]
fn multicv_pipe_vent_runs_and_stays_physical() {
    let project_path = PathBuf::from("../../examples/projects/05_two_cv_pipe_vent_transient.yaml");
    let (records, progress, timing) = run_transient_capture(&project_path, "s1", 0.02, 0.2);

    assert_physical(&records);
    assert!(
        progress
            .iter()
            .any(|e| matches!(e.mode, RunMode::Transient { .. })),
        "Expected transient progress events"
    );
    assert_eq!(
        timing.transient_fallback_uses, 0,
        "Supported benchmark should run on real-fluid path without fallback"
    );
    assert_eq!(timing.initialization_strategy.as_deref(), Some("Relaxed"));
    assert!(timing.transient_surrogate_populations > 0);
    assert!(timing.rhs_calls > 0);
    assert!(timing.rhs_snapshot_time_s > 0.0);
    assert!(timing.rhs_state_reconstruct_time_s > 0.0);
    assert!(timing.transient_surrogate_populations <= 4);
}

#[test]
fn multicv_diagnostics_fallback_counter_is_trustworthy() {
    let project_path =
        PathBuf::from("../../examples/projects/04_two_cv_series_vent_transient.yaml");
    let (_records, progress, timing) = run_transient_capture(&project_path, "s1", 0.02, 0.2);

    let last_progress_fallback = progress
        .iter()
        .rev()
        .find_map(|event| event.transient.as_ref().and_then(|t| t.fallback_uses));

    assert_eq!(
        last_progress_fallback,
        Some(timing.transient_fallback_uses),
        "Progress diagnostics fallback count must match timing summary"
    );
}

#[test]
fn multicv_supported_path_reuses_snapshot_structure() {
    let project_path =
        PathBuf::from("../../examples/projects/04_two_cv_series_vent_transient.yaml");
    let (_records, _progress, timing) = run_transient_capture(&project_path, "s1", 0.02, 0.2);

    assert!(
        timing.rhs_calls > 0,
        "Expected RHS calls for transient solve"
    );
    assert_eq!(
        timing.execution_plan_checks, timing.rhs_calls,
        "Execution plan should be checked once per RHS call"
    );
    assert!(
        timing.execution_plan_unchanged > 0,
        "Execution plan should remain unchanged for fixed-topology supported run"
    );
    assert!(
        timing.component_rebuilds > 0,
        "Component setup should be measured and counted"
    );
    assert_eq!(timing.component_reuses, 0);
    assert_eq!(timing.snapshot_setup_reuses, 0);
}
