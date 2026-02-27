use tf_project::schema::*;
use tf_project::validate_project;

fn make_base_system() -> SystemDef {
    SystemDef {
        id: "sys1".to_string(),
        name: "Atmosphere System".to_string(),
        fluid: FluidDef {
            composition: CompositionDef::Pure {
                species: "N2".to_string(),
            },
        },
        nodes: vec![
            NodeDef {
                id: "n1".to_string(),
                name: "Inlet".to_string(),
                kind: NodeKind::Junction,
            },
            NodeDef {
                id: "n_atm".to_string(),
                name: "Atmosphere".to_string(),
                kind: NodeKind::Atmosphere {
                    pressure_pa: 101_325.0,
                    temperature_k: 300.0,
                },
            },
        ],
        components: vec![ComponentDef {
            id: "c1".to_string(),
            name: "Orifice".to_string(),
            kind: ComponentKind::Orifice {
                cd: 0.8,
                area_m2: 1e-4,
                treat_as_gas: true,
            },
            from_node_id: "n1".to_string(),
            to_node_id: "n_atm".to_string(),
        }],
        boundaries: vec![BoundaryDef {
            node_id: "n1".to_string(),
            pressure_pa: Some(200_000.0),
            temperature_k: Some(300.0),
            enthalpy_j_per_kg: None,
        }],
        schedules: vec![],
    }
}

#[test]
fn atmosphere_node_validates() {
    let project = Project {
        version: 2,
        name: "Atmosphere Project".to_string(),
        systems: vec![make_base_system()],
        modules: vec![],
        layouts: vec![],
        runs: RunLibraryDef::default(),
    };

    validate_project(&project).expect("Atmosphere project should validate");
}

#[test]
fn atmosphere_invalid_pressure_fails() {
    let mut system = make_base_system();
    if let Some(node) = system.nodes.iter_mut().find(|n| n.id == "n_atm") {
        node.kind = NodeKind::Atmosphere {
            pressure_pa: -1.0,
            temperature_k: 300.0,
        };
    }

    let project = Project {
        version: 2,
        name: "Bad Atmosphere".to_string(),
        systems: vec![system],
        modules: vec![],
        layouts: vec![],
        runs: RunLibraryDef::default(),
    };

    assert!(validate_project(&project).is_err());
}

#[test]
fn atmosphere_boundary_rejected() {
    let mut system = make_base_system();
    system.boundaries.push(BoundaryDef {
        node_id: "n_atm".to_string(),
        pressure_pa: Some(101_325.0),
        temperature_k: Some(300.0),
        enthalpy_j_per_kg: None,
    });

    let project = Project {
        version: 2,
        name: "Atmosphere Boundary".to_string(),
        systems: vec![system],
        modules: vec![],
        layouts: vec![],
        runs: RunLibraryDef::default(),
    };

    assert!(validate_project(&project).is_err());
}

#[test]
fn atmosphere_schedule_rejected() {
    let mut system = make_base_system();
    system.schedules.push(ScheduleDef {
        id: "s1".to_string(),
        name: "Boundary Schedule".to_string(),
        events: vec![EventDef {
            time_s: 1.0,
            action: ActionDef::SetBoundaryPressure {
                node_id: "n_atm".to_string(),
                pressure_pa: 120_000.0,
            },
        }],
    });

    let project = Project {
        version: 2,
        name: "Atmosphere Schedule".to_string(),
        systems: vec![system],
        modules: vec![],
        layouts: vec![],
        runs: RunLibraryDef::default(),
    };

    assert!(validate_project(&project).is_err());
}
