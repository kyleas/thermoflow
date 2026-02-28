use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};

use tf_app::{run_service, RunMode, RunOptions, RunRequest};

static TEST_PROJECT_COUNTER: AtomicU64 = AtomicU64::new(0);

fn clear_run_cache(project_path: &Path) {
    if let Some(project_dir) = project_path.parent() {
        let runs_dir = project_dir.join(".thermoflow").join("runs");
        if runs_dir.exists() {
            let _ = std::fs::remove_dir_all(&runs_dir);
        }
    }
}

fn prepare_test_project(source: &Path) -> PathBuf {
    let temp_dir = std::env::temp_dir().join(format!(
        "tf_app_integration_transient_controls_{}",
        std::process::id()
    ));
    let _ = std::fs::create_dir_all(&temp_dir);
    let sequence = TEST_PROJECT_COUNTER.fetch_add(1, Ordering::Relaxed);
    let dest = temp_dir.join(format!(
        "{}_{}_{}.yaml",
        source
            .file_stem()
            .and_then(|s| s.to_str())
            .unwrap_or("project"),
        sequence,
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_nanos())
            .unwrap_or(0)
    ));
    std::fs::copy(source, &dest).expect("copy control example project");
    dest
}

fn node_pressure(records: &[tf_results::TimeseriesRecord], node_id: &str) -> Vec<f64> {
    records
        .iter()
        .filter_map(|record| {
            record
                .node_values
                .iter()
                .find(|n| n.node_id == node_id)
                .and_then(|n| n.p_pa)
        })
        .collect()
}

fn edge_flow(records: &[tf_results::TimeseriesRecord], component_id: &str) -> Vec<f64> {
    records
        .iter()
        .filter_map(|record| {
            record
                .edge_values
                .iter()
                .find(|e| e.component_id == component_id)
                .and_then(|e| e.mdot_kg_s)
        })
        .collect()
}

fn control_values(records: &[tf_results::TimeseriesRecord], control_id: &str) -> Vec<f64> {
    records
        .iter()
        .filter_map(|record| {
            record
                .global_values
                .control_values
                .iter()
                .find(|v| v.id == control_id)
                .map(|v| v.value)
        })
        .collect()
}

#[test]
fn pressure_control_example_runs_closed_loop() {
    let project_path = prepare_test_project(Path::new(
        "../../examples/projects/09_pressure_controlled_vent.yaml",
    ));
    clear_run_cache(&project_path);

    let request = RunRequest {
        project_path: &project_path,
        system_id: "s1",
        mode: RunMode::Transient {
            dt_s: 0.05,
            t_end_s: 1.0,
        },
        options: RunOptions {
            use_cache: false,
            solver_version: "0.1.0".to_string(),
            initialization_strategy: None,
        },
    };

    let response = run_service::ensure_run(&request).expect("pressure control run should succeed");
    let (_manifest, records) =
        run_service::load_run(&project_path, &response.run_id).expect("load run should succeed");

    assert!(records.len() > 3, "expected multiple transient samples");

    let p_tank = node_pressure(&records, "n_tank");
    assert!(p_tank.len() > 3, "tank pressure history missing");

    let p_start = p_tank[0];
    let p_end = *p_tank.last().expect("pressure history");
    assert!(
        p_end < p_start,
        "tank pressure should vent down under control"
    );

    let actuator_pos = control_values(&records, "a_vent");
    assert!(!actuator_pos.is_empty(), "actuator control history missing");
    assert!(
        actuator_pos.iter().all(|v| (0.0..=1.0).contains(v)),
        "actuator position should remain in [0,1]"
    );
}

#[test]
fn flow_control_example_runs_closed_loop() {
    let project_path = prepare_test_project(Path::new(
        "../../examples/projects/10_flow_controlled_valve.yaml",
    ));
    clear_run_cache(&project_path);

    let request = RunRequest {
        project_path: &project_path,
        system_id: "s1",
        mode: RunMode::Transient {
            dt_s: 0.05,
            t_end_s: 1.0,
        },
        options: RunOptions {
            use_cache: false,
            solver_version: "0.1.0".to_string(),
            initialization_strategy: None,
        },
    };

    let response = run_service::ensure_run(&request).expect("flow control run should succeed");
    let (_manifest, records) =
        run_service::load_run(&project_path, &response.run_id).expect("load run should succeed");

    assert!(records.len() > 3, "expected multiple transient samples");

    let flow = edge_flow(&records, "v_control");
    assert!(flow.len() > 3, "controlled edge mass flow history missing");

    let controller_out = control_values(&records, "c_flow_pi");
    assert!(
        !controller_out.is_empty(),
        "controller output history missing"
    );
    assert!(
        controller_out.iter().all(|v| (0.0..=1.0).contains(v)),
        "controller output should stay within configured limits"
    );

    let valve_pos = control_values(&records, "a_control");
    assert!(!valve_pos.is_empty(), "actuator position history missing");

    let flow_start = flow[0].abs();
    let flow_end = flow.last().copied().unwrap_or(flow_start).abs();
    assert!(
        (flow_end - flow_start).abs() > 1e-6,
        "flow should change over transient under closed-loop actuation"
    );
}
