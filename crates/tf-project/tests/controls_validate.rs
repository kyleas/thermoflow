use tf_project::schema::*;
use tf_project::validate_project;

fn base_system() -> SystemDef {
    SystemDef {
        id: "s1".to_string(),
        name: "Control Validation".to_string(),
        fluid: FluidDef {
            composition: CompositionDef::Pure {
                species: "Nitrogen".to_string(),
            },
        },
        nodes: vec![
            NodeDef {
                id: "n1".to_string(),
                name: "Tank".to_string(),
                kind: NodeKind::ControlVolume {
                    volume_m3: 0.02,
                    initial: InitialCvDef {
                        mode: Some("PT".to_string()),
                        p_pa: Some(400000.0),
                        t_k: Some(300.0),
                        h_j_per_kg: None,
                        m_kg: None,
                    },
                },
            },
            NodeDef {
                id: "n2".to_string(),
                name: "Ambient".to_string(),
                kind: NodeKind::Atmosphere {
                    pressure_pa: 101325.0,
                    temperature_k: 300.0,
                },
            },
        ],
        components: vec![ComponentDef {
            id: "v1".to_string(),
            name: "Vent Valve".to_string(),
            kind: ComponentKind::Valve {
                cd: 0.8,
                area_max_m2: 1e-4,
                position: 0.2,
                law: ValveLawDef::Linear,
                treat_as_gas: true,
            },
            from_node_id: "n1".to_string(),
            to_node_id: "n2".to_string(),
        }],
        boundaries: vec![],
        schedules: vec![],
        controls: None,
    }
}

fn base_project_with_controls() -> Project {
    let mut system = base_system();
    system.controls = Some(ControlSystemDef {
        blocks: vec![
            ControlBlockDef {
                id: "sp".to_string(),
                name: "Setpoint".to_string(),
                kind: ControlBlockKindDef::Constant { value: 250000.0 },
            },
            ControlBlockDef {
                id: "pv".to_string(),
                name: "Measured pressure".to_string(),
                kind: ControlBlockKindDef::MeasuredVariable {
                    reference: MeasuredVariableDef::NodePressure {
                        node_id: "n1".to_string(),
                    },
                },
            },
            ControlBlockDef {
                id: "pi".to_string(),
                name: "PI".to_string(),
                kind: ControlBlockKindDef::PIController {
                    kp: 0.00001,
                    ti_s: 1.0,
                    out_min: 0.0,
                    out_max: 1.0,
                    integral_limit: None,
                    sample_period_s: 0.05,
                },
            },
            ControlBlockDef {
                id: "act".to_string(),
                name: "Actuator".to_string(),
                kind: ControlBlockKindDef::FirstOrderActuator {
                    tau_s: 0.2,
                    rate_limit_per_s: 3.0,
                    initial_position: 0.2,
                },
            },
            ControlBlockDef {
                id: "sink".to_string(),
                name: "Valve Target".to_string(),
                kind: ControlBlockKindDef::ActuatorCommand {
                    component_id: "v1".to_string(),
                },
            },
        ],
        connections: vec![
            ControlConnectionDef {
                from_block_id: "sp".to_string(),
                to_block_id: "pi".to_string(),
                to_input: "setpoint".to_string(),
            },
            ControlConnectionDef {
                from_block_id: "pv".to_string(),
                to_block_id: "pi".to_string(),
                to_input: "process".to_string(),
            },
            ControlConnectionDef {
                from_block_id: "pi".to_string(),
                to_block_id: "act".to_string(),
                to_input: "command".to_string(),
            },
            ControlConnectionDef {
                from_block_id: "act".to_string(),
                to_block_id: "sink".to_string(),
                to_input: "position".to_string(),
            },
        ],
    });

    Project {
        version: 2,
        name: "Control validation".to_string(),
        systems: vec![system],
        modules: vec![],
        layouts: vec![],
        runs: RunLibraryDef::default(),
        plotting_workspace: None,
    }
}

#[test]
fn controls_graph_validates() {
    let project = base_project_with_controls();
    validate_project(&project).expect("controls should validate");
}

#[test]
fn controls_cycle_rejected() {
    let mut project = base_project_with_controls();
    let system = project.systems.get_mut(0).expect("system");
    let controls = system.controls.as_mut().expect("controls");

    controls.connections = vec![
        ControlConnectionDef {
            from_block_id: "sp".to_string(),
            to_block_id: "pi".to_string(),
            to_input: "setpoint".to_string(),
        },
        ControlConnectionDef {
            from_block_id: "pi".to_string(),
            to_block_id: "act".to_string(),
            to_input: "command".to_string(),
        },
        ControlConnectionDef {
            from_block_id: "act".to_string(),
            to_block_id: "pi".to_string(),
            to_input: "process".to_string(),
        },
    ];

    let err = validate_project(&project).expect_err("cycle should fail validation");
    assert!(err.to_string().contains("acyclic"));
}

#[test]
fn bad_measured_reference_rejected() {
    let mut project = base_project_with_controls();
    let system = project.systems.get_mut(0).expect("system");
    let controls = system.controls.as_mut().expect("controls");

    controls.blocks[1].kind = ControlBlockKindDef::MeasuredVariable {
        reference: MeasuredVariableDef::NodePressure {
            node_id: "does_not_exist".to_string(),
        },
    };

    let err = validate_project(&project).expect_err("invalid measured reference should fail");
    assert!(err.to_string().contains("Missing reference"));
}
