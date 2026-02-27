use std::path::PathBuf;

use tf_app::{query, run_service, RunMode, RunOptions, RunRequest};
use tf_results::RunStore;

#[test]
fn steady_run_persists_in_project_store() {
    let project_path = PathBuf::from("examples/projects/01_orifice_steady.yaml");
    if !project_path.exists() {
        eprintln!(
            "Warning: orifice example not found at {:?}, skipping persistence test",
            project_path
        );
        return;
    }

    let request = RunRequest {
        project_path: &project_path,
        system_id: "s1",
        mode: RunMode::Steady,
        options: RunOptions {
            use_cache: false,
            solver_version: "0.1.0".to_string(),
        },
    };

    let response = run_service::ensure_run(&request).expect("steady run failed");

    let store = RunStore::for_project(&project_path).expect("failed to create run store");
    let runs = store.list_runs("s1").expect("failed to list runs");
    assert!(runs.iter().any(|r| r.run_id == response.run_id));

    let records = store
        .load_timeseries(&response.run_id)
        .expect("failed to load timeseries");
    assert_eq!(records.len(), 1);

    let summary = query::get_run_summary(&records).expect("failed to summarize run");
    assert_eq!(summary.record_count, 1);
    assert_eq!(summary.node_count, 2);
}
