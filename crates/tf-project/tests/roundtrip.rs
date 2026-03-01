use tf_project::schema::*;
use tf_project::{load_yaml, save_yaml, validate_project};

#[test]
fn roundtrip_yaml_empty_project() {
    let project = Project {
        version: 2,
        name: "Empty Project".to_string(),
        systems: vec![],
        modules: vec![],
        layouts: vec![],
        runs: RunLibraryDef::default(),
        plotting_workspace: None,
        fluid_workspace: None,
    };

    validate_project(&project).unwrap();

    let temp_dir = std::env::temp_dir();
    let path = temp_dir.join("tf_project_roundtrip_empty.yaml");

    save_yaml(&path, &project).unwrap();
    let loaded = load_yaml(&path).unwrap();

    assert_eq!(project, loaded);
}

#[test]
fn roundtrip_yaml_simple_system() {
    let system = SystemDef {
        id: "sys1".to_string(),
        name: "Simple System".to_string(),
        fluid: FluidDef {
            composition: CompositionDef::Pure {
                species: "N2".to_string(),
            },
        },
        nodes: vec![
            NodeDef {
                id: "n1".to_string(),
                name: "Source".to_string(),
                kind: NodeKind::Junction,
            },
            NodeDef {
                id: "n2".to_string(),
                name: "Atmosphere".to_string(),
                kind: NodeKind::Atmosphere {
                    pressure_pa: 100_000.0,
                    temperature_k: 300.0,
                },
            },
        ],
        components: vec![ComponentDef {
            id: "c1".to_string(),
            name: "Orifice".to_string(),
            kind: ComponentKind::Orifice {
                cd: 0.8,
                area_m2: 0.001,
                treat_as_gas: true,
            },
            from_node_id: "n1".to_string(),
            to_node_id: "n2".to_string(),
        }],
        boundaries: vec![BoundaryDef {
            node_id: "n1".to_string(),
            pressure_pa: Some(200_000.0),
            temperature_k: Some(300.0),
            enthalpy_j_per_kg: None,
        }],
        schedules: vec![],
        controls: None,
    };

    let project = Project {
        version: 2,
        name: "Test Project".to_string(),
        systems: vec![system],
        modules: vec![],
        layouts: vec![],
        runs: RunLibraryDef::default(),
        plotting_workspace: None,
        fluid_workspace: None,
    };

    validate_project(&project).unwrap();

    let temp_dir = std::env::temp_dir();
    let path = temp_dir.join("tf_project_roundtrip_simple.yaml");

    save_yaml(&path, &project).unwrap();
    let loaded = load_yaml(&path).unwrap();

    assert_eq!(project, loaded);
}

#[test]
fn validation_fails_on_missing_node() {
    let system = SystemDef {
        id: "sys1".to_string(),
        name: "Invalid System".to_string(),
        fluid: FluidDef {
            composition: CompositionDef::Pure {
                species: "N2".to_string(),
            },
        },
        nodes: vec![NodeDef {
            id: "n1".to_string(),
            name: "Node1".to_string(),
            kind: NodeKind::Junction,
        }],
        components: vec![ComponentDef {
            id: "c1".to_string(),
            name: "Bad Component".to_string(),
            kind: ComponentKind::Orifice {
                cd: 0.8,
                area_m2: 0.001,
                treat_as_gas: true,
            },
            from_node_id: "n1".to_string(),
            to_node_id: "n999".to_string(),
        }],
        boundaries: vec![],
        schedules: vec![],
        controls: None,
    };

    let project = Project {
        version: 2,
        name: "Test".to_string(),
        systems: vec![system],
        modules: vec![],
        layouts: vec![],
        runs: RunLibraryDef::default(),
        plotting_workspace: None,
        fluid_workspace: None,
    };

    let result = validate_project(&project);
    assert!(result.is_err());
}

#[test]
fn roundtrip_yaml_fluid_workspace() {
    use tf_project::schema::{FluidCaseDef, FluidInputPairDef};

    let project = Project {
        version: 2,
        name: "Fluid Workspace Project".to_string(),
        systems: vec![],
        modules: vec![],
        layouts: vec![],
        runs: RunLibraryDef::default(),
        plotting_workspace: None,
        fluid_workspace: Some(FluidWorkspaceDef {
            cases: vec![FluidCaseDef {
                id: "case1".to_string(),
                species: "N2O".to_string(),
                input_pair: FluidInputPairDef::PS,
                input_1: 101_325.0,
                input_2: 3900.0,
                quality: None,
            }],
        }),
    };

    validate_project(&project).unwrap();

    let temp_dir = std::env::temp_dir();
    let path = temp_dir.join("tf_project_roundtrip_fluid_workspace.yaml");

    save_yaml(&path, &project).unwrap();
    let loaded = load_yaml(&path).unwrap();

    assert_eq!(project, loaded);
}

#[test]
fn load_yaml_without_fluid_workspace_defaults_to_none() {
    let yaml = r#"
version: 2
name: Legacy Project
systems: []
modules: []
layouts: []
runs: { runs: [] }
"#;

    let parsed: Project = serde_yaml::from_str(yaml).expect("legacy yaml should deserialize");
    assert!(parsed.fluid_workspace.is_none());
}
