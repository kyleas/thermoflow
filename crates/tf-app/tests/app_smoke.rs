//! Smoke test for tf-app service layer.

use std::path::PathBuf;
use tf_app::{list_systems, load_project, validate_project};

#[test]
fn test_load_example_project() {
    // Try to load an example project
    let mut project_path = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    project_path.pop(); // go to crates
    project_path.pop(); // go to repo root
    project_path.push("examples");
    project_path.push("projects");
    project_path.push("01_orifice_steady.yaml");

    if !project_path.exists() {
        eprintln!(
            "Skipping test: example project not found at {:?}",
            project_path
        );
        return;
    }

    let project = load_project(&project_path).expect("Failed to load project");
    assert!(!project.systems.is_empty(), "Project should have systems");

    // Validate
    validate_project(&project).expect("Validation should succeed");

    // List systems
    let systems = list_systems(&project);
    assert!(!systems.is_empty(), "Should list systems");

    for sys in &systems {
        println!("System: {} ({})", sys.name, sys.id);
        println!(
            " Nodes: {}, Components: {}",
            sys.node_count, sys.component_count
        );
    }
}
