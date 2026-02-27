//! Integration tests for shared run progress and timing reporting.

use std::path::Path;

use tf_app::{
    ensure_run_with_progress, load_run, query, RunMode, RunOptions, RunProgressEvent, RunRequest,
    RunStage,
};

fn collect_events(request: &RunRequest<'_>) -> (tf_app::RunResponse, Vec<RunProgressEvent>) {
    let mut events = Vec::new();
    let response = ensure_run_with_progress(request, Some(&mut |event| events.push(event)))
        .expect("run with progress should succeed");
    (response, events)
}

#[test]
fn steady_progress_and_timing_are_reported() {
    let project_path = Path::new("../../examples/projects/01_orifice_steady.yaml");
    let request = RunRequest {
        project_path,
        system_id: "s1",
        mode: RunMode::Steady,
        options: RunOptions {
            use_cache: false,
            solver_version: "0.1.0".to_string(),
            initialization_strategy: None,
        },
    };

    let (response, events) = collect_events(&request);

    assert!(!response.loaded_from_cache);
    assert!(
        events
            .iter()
            .any(|e| matches!(e.stage, RunStage::CompilingRuntime)),
        "expected compile stage event"
    );
    assert!(
        events
            .iter()
            .any(|e| matches!(e.stage, RunStage::SolvingSteady)),
        "expected steady solving stage event"
    );
    // Some fully-constrained steady cases converge without Newton iterations,
    // so iteration/residual details may be absent. Stage visibility is still required.
    assert!(response.timing.total_time_s > 0.0);
    assert!(response.timing.solve_time_s > 0.0);
    assert_eq!(
        response.timing.initialization_strategy.as_deref(),
        Some("Strict")
    );

    let (_manifest, records) = load_run(project_path, &response.run_id).expect("run should load");
    let summary = query::get_run_summary(&records).expect("summary should load");
    assert_eq!(summary.record_count, 1);
}

#[test]
fn transient_progress_and_timing_are_reported() {
    let project_path = Path::new("../../examples/projects/03_simple_vent_transient.yaml");
    let request = RunRequest {
        project_path,
        system_id: "s1",
        mode: RunMode::Transient {
            dt_s: 0.1,
            t_end_s: 0.5,
        },
        options: RunOptions {
            use_cache: false,
            solver_version: "0.1.0".to_string(),
            initialization_strategy: None,
        },
    };

    let (response, events) = collect_events(&request);

    assert!(!response.loaded_from_cache);
    assert!(
        events
            .iter()
            .any(|e| matches!(e.stage, RunStage::RunningTransient)),
        "expected transient running stage event"
    );
    assert!(
        events
            .iter()
            .filter_map(|e| e.transient.as_ref())
            .any(|t| t.fraction_complete > 0.0),
        "expected non-zero transient progress fraction"
    );

    assert!(response.timing.total_time_s > 0.0);
    assert!(response.timing.solve_time_s > 0.0);
    assert!(response.timing.transient_steps > 0);
    assert!(response.timing.build_time_s >= 0.0);
    assert_eq!(
        response.timing.initialization_strategy.as_deref(),
        Some("Strict")
    );
    assert!(response.timing.transient_real_fluid_attempts > 0);
    assert_eq!(
        response.timing.transient_real_fluid_attempts,
        response.timing.transient_real_fluid_successes,
        "supported transient should stay on real-fluid path"
    );
    assert!(response.timing.transient_surrogate_populations > 0);
}
