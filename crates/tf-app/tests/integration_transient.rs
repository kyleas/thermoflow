//! Integration test: tf-app transient execution through shared services.

use std::path::{Path, PathBuf};

use tf_app::{ensure_run, load_run, RunMode, RunOptions, RunRequest};
use tf_results::TimeseriesRecord;

fn run_transient(
    project_path: &Path,
    system_id: &str,
    dt_s: f64,
    t_end_s: f64,
) -> Vec<TimeseriesRecord> {
    let request = RunRequest {
        project_path,
        system_id,
        mode: RunMode::Transient { dt_s, t_end_s },
        options: RunOptions {
            use_cache: false,
            solver_version: "0.1.0".to_string(),
        },
    };

    let response = ensure_run(&request).expect("Transient run failed");
    let (_manifest, records) =
        load_run(project_path, &response.run_id).expect("Failed to load run");
    records
}

fn assert_records_physical(records: &[TimeseriesRecord]) {
    assert!(records.len() > 1, "Expected multiple records");

    for record in records {
        for node in &record.node_values {
            if let Some(p) = node.p_pa {
                assert!(p.is_finite(), "Pressure must be finite");
                assert!(p > 0.0, "Pressure must be positive");
            }
            if let Some(t) = node.t_k {
                assert!(t.is_finite(), "Temperature must be finite");
            }
            if let Some(h) = node.h_j_per_kg {
                assert!(h.is_finite(), "Enthalpy must be finite");
            }
            if let Some(rho) = node.rho_kg_m3 {
                assert!(rho.is_finite(), "Density must be finite");
                assert!(rho > 0.0, "Density must be positive");
            }
        }

        for edge in &record.edge_values {
            if let Some(mdot) = edge.mdot_kg_s {
                assert!(mdot.is_finite(), "Mass flow must be finite");
            }
            if let Some(dp) = edge.delta_p_pa {
                assert!(dp.is_finite(), "Delta-p must be finite");
            }
        }
    }
}

#[test]
fn transient_startup_window_t0() {
    let project_path = PathBuf::from("examples/projects/02_tank_blowdown_transient.yaml");
    if !project_path.exists() {
        eprintln!(
            "Warning: tank blowdown example not found at {:?}, skipping transient integration test",
            project_path
        );
        return;
    }

    let records = run_transient(&project_path, "s1", 0.01, 0.1);
    assert_records_physical(&records);

    let p_initial = records
        .first()
        .and_then(|record| {
            record
                .node_values
                .iter()
                .find(|node| node.node_id == "n_tank")
                .and_then(|node| node.p_pa)
        })
        .expect("Missing tank pressure at startup");

    let p_final = records
        .last()
        .and_then(|record| {
            record
                .node_values
                .iter()
                .find(|node| node.node_id == "n_tank")
                .and_then(|node| node.p_pa)
        })
        .expect("Missing tank pressure at end of startup window");

    let rel_change = ((p_final - p_initial) / p_initial).abs();
    assert!(
        rel_change < 1e-3,
        "Tank pressure should remain approximately constant during startup (p0={}, p1={})",
        p_initial,
        p_final
    );
}

#[test]
fn transient_full_blowdown_transition() {
    let project_path = PathBuf::from("examples/projects/02_tank_blowdown_transient.yaml");
    if !project_path.exists() {
        eprintln!(
            "Warning: tank blowdown example not found at {:?}, skipping transient integration test",
            project_path
        );
        return;
    }

    let records = run_transient(&project_path, "s1", 0.01, 3.0);
    assert_records_physical(&records);

    let p_initial = records
        .first()
        .and_then(|record| {
            record
                .node_values
                .iter()
                .find(|node| node.node_id == "n_tank")
                .and_then(|node| node.p_pa)
        })
        .expect("Missing tank pressure at startup");

    let p_after_open = records
        .iter()
        .find(|record| record.time_s >= 2.0)
        .and_then(|record| {
            record
                .node_values
                .iter()
                .find(|node| node.node_id == "n_tank")
                .and_then(|node| node.p_pa)
        })
        .expect("Missing tank pressure after valve opening");

    assert!(
        p_after_open < p_initial,
        "Tank pressure should decrease after opening (p0={}, p2={})",
        p_initial,
        p_after_open
    );

    let max_valve_h = records
        .iter()
        .filter(|record| record.time_s >= 2.0)
        .filter_map(|record| {
            record
                .node_values
                .iter()
                .find(|node| node.node_id == "n_valve")
                .and_then(|node| node.h_j_per_kg)
        })
        .fold(f64::NEG_INFINITY, f64::max);

    assert!(
        max_valve_h.is_finite() && max_valve_h < 1.0e6,
        "Valve junction enthalpy should remain bounded after opening (max_h={})",
        max_valve_h
    );
}
