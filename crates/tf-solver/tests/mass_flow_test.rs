//! Test mass flow computation in the solver.

use tf_components::Orifice;
use tf_core::units::{k, m, pa};
use tf_fluids::{Composition, CoolPropModel, Species};
use tf_graph::GraphBuilder;
use tf_solver::{SteadyProblem, solve};

#[test]
fn orifice_mass_flow_computed() {
    // Test that mimics the example 1 setup: orifice with specific BCs
    // Inlet: 200 kPa, 300 K
    // Outlet: 101.325 kPa, (same temp initially)

    let mut builder = GraphBuilder::new();
    let n0 = builder.add_node("inlet");
    let n1 = builder.add_node("outlet");
    let c0 = builder.add_component("orifice", n0, n1);
    let graph = builder.build().unwrap();

    let model = CoolPropModel::new();
    let comp = Composition::pure(Species::N2);

    let mut problem = SteadyProblem::new(&graph, &model, comp.clone());

    // Same BCs as example 1
    problem.set_pressure_bc(n0, pa(200_000.0)).unwrap(); // 2 bar
    problem.set_temperature_bc(n0, k(300.0)).unwrap();
    problem.set_pressure_bc(n1, pa(101_325.0)).unwrap(); // 1 atm
    problem.set_temperature_bc(n1, k(300.0)).unwrap();

    // Same orifice parameters as example 1: Cd=0.8, area=0.0001 m²
    let orifice = Orifice::new(
        "orifice".to_string(),
        0.8,
        m(0.01) * m(0.01), // 0.0001 m² = (0.01 m)²
    );
    problem.add_component(c0, Box::new(orifice)).unwrap();

    // Solve
    let solution = solve(&mut problem, None, None).unwrap();

    println!(
        "Orifice example converged in {} iterations",
        solution.iterations
    );
    println!("Residual norm: {}", solution.residual_norm);

    // Verify we got mass flows
    assert_eq!(solution.mass_flows.len(), 1);
    let (comp_id, mdot) = solution.mass_flows[0];
    assert_eq!(comp_id, c0);

    println!(
        "Mass flow through orifice (example 1 conditions): {} kg/s",
        mdot
    );

    // Should be positive flow
    assert!(mdot > 0.0, "Mass flow should be positive");

    // Based on orifice equation with nitrogen gas, the flow should be reasonable
    // Actual computed value is around 0.053 kg/s
    assert!(
        mdot > 0.04 && mdot < 0.07,
        "Mass flow {} kg/s is outside expected range for example 1",
        mdot
    );
}
