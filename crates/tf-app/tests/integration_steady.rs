//! Integration tests for steady-state simulation end-to-end

use std::path::Path;
use tf_app::{project_service, query, run_service, RunMode, RunOptions, RunRequest};

fn clear_run_cache(project_path: &Path) {
    if let Some(project_dir) = project_path.parent() {
        let runs_dir = project_dir.join(".thermoflow").join("runs");
        if runs_dir.exists() {
            let _ = std::fs::remove_dir_all(&runs_dir);
        }
    }
}

#[test]
fn test_steady_simulation_orifice() {
    // Path to example project
    let project_path = Path::new("../../examples/projects/01_orifice_steady.yaml");
    clear_run_cache(project_path);

    // Verify project loads and validates
    let project = project_service::load_project(project_path).expect("Failed to load project");
    project_service::validate_project(&project).expect("Project validation failed");

    // Get system
    let systems = project_service::list_systems(&project);
    assert_eq!(systems.len(), 1, "Expected 1 system in orifice example");
    assert_eq!(systems[0].id, "s1");

    // Run steady simulation
    let request = RunRequest {
        project_path,
        system_id: "s1",
        mode: RunMode::Steady,
        options: RunOptions {
            use_cache: true,
            solver_version: "0.1.0".to_string(),
            initialization_strategy: None,
        },
    };

    let response = run_service::ensure_run(&request).expect("Simulation failed");
    // Note: May be from cache if run already exists from previous test runs

    // Verify run was saved
    let run_id = response.run_id.clone();
    let (manifest, records) =
        run_service::load_run(project_path, &run_id).expect("Failed to load run");
    assert_eq!(manifest.system_id, "s1");
    assert_eq!(manifest.solver_version, "0.1.0");

    // Verify results structure
    let summary = query::get_run_summary(&records).expect("Failed to get summary");
    assert_eq!(
        summary.record_count, 1,
        "Steady simulation should have 1 record"
    );
    assert_eq!(summary.node_count, 2, "Orifice example has 2 nodes");
    assert_eq!(
        summary.component_count, 1,
        "Orifice example has 1 component"
    );

    // Verify time is zero for steady
    assert_eq!(summary.time_range.0, 0.0);
    assert_eq!(summary.time_range.1, 0.0);

    // Verify node and component lists
    let node_ids = query::list_node_ids(&records);
    assert_eq!(node_ids.len(), 2);
    assert!(node_ids.contains(&"n1".to_string()));
    assert!(node_ids.contains(&"n2".to_string()));

    let comp_ids = query::list_component_ids(&records);
    assert_eq!(comp_ids.len(), 1);
    assert!(comp_ids.contains(&"c1".to_string()));

    // Extract and verify node pressure
    let n1_pressure = query::extract_node_series(&records, "n1", "pressure")
        .expect("Failed to extract n1 pressure");
    assert_eq!(n1_pressure.len(), 1);
    assert_eq!(n1_pressure[0].0, 0.0); // time = 0
    assert!(n1_pressure[0].1 > 0.0, "Pressure should be positive");

    // Extract and verify component mass flow
    let c1_mdot = query::extract_component_series(&records, "c1", "mass_flow")
        .expect("Failed to extract c1 mass_flow");
    assert_eq!(c1_mdot.len(), 1);
    assert!(c1_mdot[0].1 > 0.0, "Mass flow should be positive");

    // Test caching - second run with same parameters should load from cache
    let response2 = run_service::ensure_run(&request).expect("Second simulation failed");
    assert!(
        response2.loaded_from_cache,
        "Second identical run should always be from cache"
    );
    assert_eq!(
        response2.run_id, run_id,
        "Run ID should match for cached run"
    );

    // Verify runs listing
    let runs = run_service::list_runs(project_path, "s1").expect("Failed to list runs");
    assert!(!runs.is_empty(), "Should have at least one run");
    assert!(
        runs.iter().any(|r| r.run_id == run_id),
        "Run should be in list"
    );
}

#[test]
fn test_steady_simulation_with_no_cache() {
    let project_path = Path::new("../../examples/projects/01_orifice_steady.yaml");
    clear_run_cache(project_path);

    // First run with cache enabled
    let request = RunRequest {
        project_path,
        system_id: "s1",
        mode: RunMode::Steady,
        options: RunOptions {
            use_cache: true,
            solver_version: "0.1.0".to_string(),
            initialization_strategy: None,
        },
    };

    let response1 = run_service::ensure_run(&request).expect("First run failed");
    let run_id = response1.run_id.clone();

    // Second run with cache disabled should NOT load from cache
    // (but will produce same run_id and overwrite)
    let request_no_cache = RunRequest {
        project_path,
        system_id: "s1",
        mode: RunMode::Steady,
        options: RunOptions {
            use_cache: false,
            solver_version: "0.1.0".to_string(),
            initialization_strategy: None,
        },
    };

    let response2 = run_service::ensure_run(&request_no_cache).expect("Second run failed");
    assert!(
        !response2.loaded_from_cache,
        "use_cache=false should force re-run"
    );
    assert_eq!(
        response2.run_id, run_id,
        "Same parameters should produce same run_id"
    );
}
