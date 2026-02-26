use tf_results::*;

#[test]
fn save_and_load_run() {
    let temp_dir = std::env::temp_dir().join("tf_results_test");
    let _ = std::fs::remove_dir_all(&temp_dir);

    let store = RunStore::new(temp_dir.clone()).unwrap();

    let manifest = RunManifest {
        run_id: "test_run_123".to_string(),
        system_id: "sys1".to_string(),
        timestamp: "2026-02-25T12:00:00Z".to_string(),
        run_type: RunType::Steady,
        solver_version: "v1".to_string(),
    };

    let records = vec![
        TimeseriesRecord {
            time_s: 0.0,
            node_values: vec![NodeValueSnapshot {
                node_id: "n1".to_string(),
                p_pa: Some(200_000.0),
                t_k: Some(300.0),
                h_j_per_kg: None,
                rho_kg_m3: None,
            }],
            edge_values: vec![],
            global_values: GlobalValueSnapshot::default(),
        },
        TimeseriesRecord {
            time_s: 1.0,
            node_values: vec![NodeValueSnapshot {
                node_id: "n1".to_string(),
                p_pa: Some(190_000.0),
                t_k: Some(299.0),
                h_j_per_kg: None,
                rho_kg_m3: None,
            }],
            edge_values: vec![],
            global_values: GlobalValueSnapshot::default(),
        },
    ];

    store.save_run(&manifest, &records).unwrap();

    let loaded_manifest = store.load_manifest("test_run_123").unwrap();
    assert_eq!(loaded_manifest.run_id, manifest.run_id);

    let loaded_records = store.load_timeseries("test_run_123").unwrap();
    assert_eq!(loaded_records.len(), 2);
    assert_eq!(loaded_records[0].time_s, 0.0);
    assert_eq!(loaded_records[1].time_s, 1.0);
}

#[test]
fn list_runs_by_system() {
    let temp_dir = std::env::temp_dir().join("tf_results_test_list");
    let _ = std::fs::remove_dir_all(&temp_dir);

    let store = RunStore::new(temp_dir.clone()).unwrap();

    let manifest1 = RunManifest {
        run_id: "run1".to_string(),
        system_id: "sys1".to_string(),
        timestamp: "2026-02-25T12:00:00Z".to_string(),
        run_type: RunType::Steady,
        solver_version: "v1".to_string(),
    };

    let manifest2 = RunManifest {
        run_id: "run2".to_string(),
        system_id: "sys1".to_string(),
        timestamp: "2026-02-25T13:00:00Z".to_string(),
        run_type: RunType::Steady,
        solver_version: "v1".to_string(),
    };

    let manifest3 = RunManifest {
        run_id: "run3".to_string(),
        system_id: "sys2".to_string(),
        timestamp: "2026-02-25T14:00:00Z".to_string(),
        run_type: RunType::Steady,
        solver_version: "v1".to_string(),
    };

    store.save_run(&manifest1, &[]).unwrap();
    store.save_run(&manifest2, &[]).unwrap();
    store.save_run(&manifest3, &[]).unwrap();

    let sys1_runs = store.list_runs("sys1").unwrap();
    assert_eq!(sys1_runs.len(), 2);

    let sys2_runs = store.list_runs("sys2").unwrap();
    assert_eq!(sys2_runs.len(), 1);
}
