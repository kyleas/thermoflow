use tf_app::runtime_compile::{build_components, build_fluid_model, compile_system};
use tf_app::{parse_boundaries_with_atmosphere, BoundaryCondition};
use tf_project::schema::*;
use tf_solver::SteadyProblem;

fn make_system() -> SystemDef {
    SystemDef {
        id: "s1".to_string(),
        name: "Atmosphere Compile".to_string(),
        fluid: FluidDef {
            composition: CompositionDef::Pure {
                species: "Nitrogen".to_string(),
            },
        },
        nodes: vec![
            NodeDef {
                id: "n_in".to_string(),
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
            from_node_id: "n_in".to_string(),
            to_node_id: "n_atm".to_string(),
        }],
        boundaries: vec![BoundaryDef {
            node_id: "n_in".to_string(),
            pressure_pa: Some(200_000.0),
            temperature_k: Some(300.0),
            enthalpy_j_per_kg: None,
        }],
        schedules: vec![],
    }
}

#[test]
fn atmosphere_compiles_and_sets_fixed_state() {
    let system = make_system();
    let runtime = compile_system(&system).expect("compile_system failed");
    let fluid_model = build_fluid_model(&system.fluid).expect("fluid model failed");

    let boundaries =
        parse_boundaries_with_atmosphere(&system, &system.boundaries, &runtime.node_id_map)
            .expect("parse_boundaries_with_atmosphere failed");

    let inlet_id = runtime.node_id_map.get("n_in").copied().unwrap();
    let atm_id = runtime.node_id_map.get("n_atm").copied().unwrap();
    assert!(boundaries.contains_key(&inlet_id));
    assert!(boundaries.contains_key(&atm_id));

    let components = build_components(&system, &runtime.comp_id_map).expect("components failed");
    let mut problem = SteadyProblem::new(
        &runtime.graph,
        fluid_model.as_ref(),
        runtime.composition.clone(),
    );

    for (comp_id, component) in components {
        problem.add_component(comp_id, component).unwrap();
    }

    for (node_id, bc) in &boundaries {
        match bc {
            BoundaryCondition::PT { p, t } => {
                problem.set_pressure_bc(*node_id, *p).unwrap();
                problem.set_temperature_bc(*node_id, *t).unwrap();
            }
            BoundaryCondition::PH { p, h } => {
                problem.set_pressure_bc(*node_id, *p).unwrap();
                problem.set_enthalpy_bc(*node_id, *h).unwrap();
            }
        }
    }

    problem.convert_all_temperature_bcs().unwrap();

    let atm_idx = atm_id.index() as usize;
    assert!(problem.bc_enthalpy[atm_idx].is_some());
}
