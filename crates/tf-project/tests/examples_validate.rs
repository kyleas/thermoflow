use std::path::PathBuf;

#[test]
fn examples_validate() {
    let crate_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let root = crate_dir
        .parent()
        .and_then(|p| p.parent())
        .expect("workspace root");

    let examples = [
        "examples/projects/01_orifice_steady.yaml",
        "examples/projects/02_tank_blowdown_transient.yaml",
        "examples/projects/03_turbopump_demo.yaml",
        "examples/projects/09_pressure_controlled_vent.yaml",
        "examples/projects/10_flow_controlled_valve.yaml",
    ];

    for rel in examples {
        let path = root.join(rel);
        let result = tf_project::load_yaml(&path);
        assert!(
            result.is_ok(),
            "example failed validation: {} => {:?}",
            path.display(),
            result.err()
        );
    }
}
