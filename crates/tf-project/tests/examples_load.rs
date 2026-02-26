use std::path::Path;

#[test]
fn examples_load_and_validate() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("..\\..\\examples\\projects");
    let examples = [
        "01_orifice_steady.yaml",
        "02_tank_blowdown_transient.yaml",
        "03_turbopump_demo.yaml",
    ];

    for name in examples {
        let path = root.join(name);
        let project = tf_project::load_yaml(&path)
            .unwrap_or_else(|e| panic!("Failed to load {}: {}", name, e));
        tf_project::validate_project(&project)
            .unwrap_or_else(|e| panic!("Failed to validate {}: {}", name, e));
    }
}
