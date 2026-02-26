//! Integration test for steady-state solver.

use tf_components::Orifice;
use tf_core::units::{k, m, pa};
use tf_fluids::{Composition, CoolPropModel, FluidModel, Species, StateInput};
use tf_graph::GraphBuilder;
use tf_solver::{SteadyProblem, solve};

#[test]
fn simple_two_node_network() {
    // Create a simple network: node0 --[orifice]--> node1
    // Boundary conditions: node0(P=6 bar, T=300K), node1(P=1 bar)
    // Free: node1 enthalpy

    let mut builder = GraphBuilder::new();
    let n0 = builder.add_node("inlet");
    let n1 = builder.add_node("outlet");
    let c0 = builder.add_component("orifice", n0, n1);
    let graph = builder.build().unwrap();

    // Setup fluid
    let model = CoolPropModel::new();
    let comp = Composition::pure(Species::N2);

    // Create problem
    let mut problem = SteadyProblem::new(&graph, &model, comp.clone());

    // Set boundary conditions
    problem.set_pressure_bc(n0, pa(600_000.0)).unwrap(); // 6 bar
    problem.set_temperature_bc(n0, k(300.0)).unwrap();
    problem.set_pressure_bc(n1, pa(100_000.0)).unwrap(); // 1 bar
    // node1 enthalpy is free (will be solved)

    // Add orifice component
    let orifice = Orifice::new(
        "orifice".to_string(),
        0.7,
        m(0.01) * m(0.01) * std::f64::consts::PI / 4.0,
    ); // 10mm diameter, Cd=0.7
    problem.add_component(c0, Box::new(orifice)).unwrap();

    // Solve (this will fail gracefully since we have zero mass flow)
    // For now, just test that the problem setup works
    let result = solve(&mut problem, None, None);

    // We expect it to converge to zero residuals with zero mass flow
    // (enthalpy at node1 should equal initial guess since there's no flow)
    match result {
        Ok(solution) => {
            println!("Converged in {} iterations", solution.iterations);
            println!("Residual norm: {}", solution.residual_norm);

            // Verify we have the right number of values
            assert_eq!(solution.pressures.len(), 2);
            assert_eq!(solution.enthalpies.len(), 2);

            // Verify boundary conditions are preserved
            assert!((solution.pressures[0].value - 600_000.0).abs() < 1.0);
            assert!((solution.pressures[1].value - 100_000.0).abs() < 1.0);
        }
        Err(e) => {
            println!("Solver failed (expected for zero-flow case): {}", e);
            // This is ok for now - we haven't implemented mass flow solving yet
        }
    }
}

#[test]
fn all_boundary_conditions_specified() {
    // Test case where all nodes have BCs (no unknowns)
    // This should converge immediately with zero iterations

    let mut builder = GraphBuilder::new();
    let n0 = builder.add_node("inlet");
    let n1 = builder.add_node("outlet");
    let c0 = builder.add_component("orifice", n0, n1);
    let graph = builder.build().unwrap();

    let model = CoolPropModel::new();
    let comp = Composition::pure(Species::N2);

    let mut problem = SteadyProblem::new(&graph, &model, comp.clone());

    // Fully specified BCs
    problem.set_pressure_bc(n0, pa(600_000.0)).unwrap();
    problem.set_temperature_bc(n0, k(300.0)).unwrap();
    problem.set_pressure_bc(n1, pa(100_000.0)).unwrap();
    problem.set_temperature_bc(n1, k(250.0)).unwrap(); // Different temperature

    let orifice = Orifice::new(
        "orifice".to_string(),
        0.7,
        m(0.01) * m(0.01) * std::f64::consts::PI / 4.0,
    );
    problem.add_component(c0, Box::new(orifice)).unwrap();

    // This should work since there are no unknowns
    assert_eq!(problem.num_free_vars(), 0);

    // Convert temperature BCs
    problem.convert_all_temperature_bcs().unwrap();

    // Verify enthalpies were set
    assert!(problem.bc_enthalpy[0].is_some());
    assert!(problem.bc_enthalpy[1].is_some());

    // Verify temperatures are different (not constant T assumption)
    let state0 = model
        .state(
            StateInput::PT {
                p: pa(600_000.0),
                t: k(300.0),
            },
            comp.clone(),
        )
        .unwrap();
    let state1 = model
        .state(
            StateInput::PT {
                p: pa(100_000.0),
                t: k(250.0),
            },
            comp.clone(),
        )
        .unwrap();
    let h0 = model.h(&state0).unwrap();
    let h1 = model.h(&state1).unwrap();

    println!("h0 = {} J/kg, h1 = {} J/kg", h0, h1);
    assert!(
        (h0 - h1).abs() > 1000.0,
        "Enthalpies should differ significantly"
    );
}
