use std::fs;
use std::path::PathBuf;
use std::time::{SystemTime, UNIX_EPOCH};

use tf_results::{RunManifest, RunStore, RunType, TimeseriesRecord};

fn unique_temp_dir(prefix: &str) -> PathBuf {
    let mut dir = std::env::temp_dir();
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_nanos();
    dir.push(format!("{}_{}", prefix, nanos));
    dir
}

#[test]
fn save_list_load_roundtrip() {
    let project_dir = unique_temp_dir("tf_results_project");
    fs::create_dir_all(&project_dir).expect("failed to create temp project dir");
    let project_path = project_dir.join("project.yaml");
    fs::write(&project_path, "version: 1\nname: test\n").expect("failed to write project file");

    let store = RunStore::for_project(&project_path).expect("failed to create run store");

    let manifest = RunManifest {
        run_id: "run-123".to_string(),
        system_id: "s1".to_string(),
        timestamp: "2026-02-26T00:00:00Z".to_string(),
        run_type: RunType::Steady,
        solver_version: "0.1.0".to_string(),
    };

    let records = vec![TimeseriesRecord {
        time_s: 0.0,
        node_values: Vec::new(),
        edge_values: Vec::new(),
        global_values: Default::default(),
    }];

    store
        .save_run(&manifest, &records)
        .expect("failed to save run");

    let runs = store.list_runs("s1").expect("failed to list runs");
    assert_eq!(runs.len(), 1);
    assert_eq!(runs[0].run_id, "run-123");

    let loaded_manifest = store
        .load_manifest("run-123")
        .expect("failed to load manifest");
    assert_eq!(loaded_manifest.system_id, "s1");

    let loaded_records = store
        .load_timeseries("run-123")
        .expect("failed to load records");
    assert_eq!(loaded_records.len(), 1);
}
