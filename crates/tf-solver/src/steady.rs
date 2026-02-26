//! Steady-state network solver with mass and energy balance.

use crate::error::{SolverError, SolverResult};
use crate::problem::SteadyProblem;
use nalgebra::DVector;
use tf_components::PortStates;
use tf_core::CompId;
use tf_core::units::{Pressure, kgps};
use tf_fluids::{SpecEnthalpy, StateInput};

/// Solution state for a steady-state network.
#[derive(Clone, Debug)]
pub struct SteadySolution {
    /// Node pressures (Pa)
    pub pressures: Vec<Pressure>,
    /// Node specific enthalpies (J/kg)
    pub enthalpies: Vec<SpecEnthalpy>,
    /// Component mass flow rates (kg/s)
    pub mass_flows: Vec<(CompId, f64)>,
    /// Residual norm at convergence
    pub residual_norm: f64,
    /// Number of iterations
    pub iterations: usize,
}

/// Unpack solution vector [P_0, h_0, P_1, h_1, ...] for free nodes into full arrays.
fn unpack_solution(
    x: &DVector<f64>,
    problem: &SteadyProblem,
) -> SolverResult<(Vec<Pressure>, Vec<SpecEnthalpy>)> {
    let node_count = problem.graph.nodes().len();
    let mut pressures = vec![Pressure::default(); node_count];
    let mut enthalpies = vec![0.0; node_count];

    let mut idx = 0;
    for i in 0..node_count {
        // Pressure
        if let Some(p_bc) = problem.bc_pressure[i] {
            pressures[i] = p_bc;
        } else {
            pressures[i] = Pressure::new::<uom::si::pressure::pascal>(x[idx]);
            idx += 1;
        }

        // Enthalpy
        if let Some(h_bc) = problem.bc_enthalpy[i] {
            enthalpies[i] = h_bc;
        } else if problem.bc_temperature[i].is_some() {
            // If temperature BC exists, enthalpy should have been converted
            return Err(SolverError::ProblemSetup {
                what: format!("Temperature BC at node {} not converted to enthalpy", i),
            });
        } else {
            enthalpies[i] = x[idx];
            idx += 1;
        }
    }

    Ok((pressures, enthalpies))
}

/// Pack solution arrays into vector [P_0, h_0, P_1, h_1, ...] for free nodes only.
fn pack_solution(
    pressures: &[Pressure],
    enthalpies: &[SpecEnthalpy],
    problem: &SteadyProblem,
) -> DVector<f64> {
    let n = problem.num_free_vars();
    let mut x = DVector::zeros(n);

    let mut idx = 0;
    for i in 0..problem.graph.nodes().len() {
        if problem.bc_pressure[i].is_none() {
            x[idx] = pressures[i].value;
            idx += 1;
        }
        if problem.bc_enthalpy[i].is_none() && problem.bc_temperature[i].is_none() {
            x[idx] = enthalpies[i];
            idx += 1;
        }
    }

    x
}

/// Compute residuals for mass and energy balance at all nodes.
///
/// For each node i:
/// - Mass balance: R_m,i = sum_in(mdot) - sum_out(mdot)
/// - Energy balance: R_e,i = sum_in(mdot*h_stream) - h_i * sum_out(mdot)
///
/// Returns residual vector with alternating [Rm_0, Re_0, Rm_1, Re_1, ...] for free nodes.
pub fn compute_residuals(
    x: &DVector<f64>,
    problem: &SteadyProblem,
    mass_flows: &[(CompId, f64)],
) -> SolverResult<DVector<f64>> {
    let (pressures, enthalpies) = unpack_solution(x, problem)?;
    let node_count = problem.graph.nodes().len();

    // Compute states for all nodes
    let mut states = Vec::new();
    for i in 0..node_count {
        let state = problem.fluid.state(
            StateInput::PH {
                p: pressures[i],
                h: enthalpies[i],
            },
            problem.composition.clone(),
        )?;
        states.push(state);
    }

    // Initialize mass and energy accumulation per node
    let mut mass_in = vec![0.0; node_count];
    let mut mass_out = vec![0.0; node_count];
    let mut energy_in = vec![0.0; node_count];

    // Accumulate contributions from all components
    for (comp_id, mdot) in mass_flows {
        let component =
            problem
                .components
                .get(comp_id)
                .ok_or_else(|| SolverError::ProblemSetup {
                    what: format!("Component {:?} not found", comp_id),
                })?;

        let inlet_node =
            problem
                .graph
                .comp_inlet_node(*comp_id)
                .ok_or_else(|| SolverError::ProblemSetup {
                    what: format!("Component {:?} has no inlet", comp_id),
                })?;
        let outlet_node =
            problem
                .graph
                .comp_outlet_node(*comp_id)
                .ok_or_else(|| SolverError::ProblemSetup {
                    what: format!("Component {:?} has no outlet", comp_id),
                })?;

        let inlet_idx = inlet_node.index() as usize;
        let outlet_idx = outlet_node.index() as usize;

        let inlet_state = &states[inlet_idx];
        let outlet_state = &states[outlet_idx];

        if *mdot >= 0.0 {
            // Flow from inlet to outlet
            let ports = PortStates {
                inlet: inlet_state,
                outlet: outlet_state,
            };
            let h_out = component.outlet_enthalpy(problem.fluid, ports, kgps(*mdot))?;

            // Mass balance
            mass_out[inlet_idx] += *mdot;
            mass_in[outlet_idx] += *mdot;

            // Energy balance: outlet node receives mdot*h_out
            energy_in[outlet_idx] += *mdot * h_out;
        } else {
            // Reverse flow from outlet to inlet
            let ports = PortStates {
                inlet: outlet_state,
                outlet: inlet_state,
            };
            let h_out = component.outlet_enthalpy(problem.fluid, ports, kgps(mdot.abs()))?;

            // Mass balance
            mass_out[outlet_idx] += mdot.abs();
            mass_in[inlet_idx] += mdot.abs();

            // Energy balance: inlet node receives mdot*h_out
            energy_in[inlet_idx] += mdot.abs() * h_out;
        }
    }

    // Compute residuals for free nodes only
    let n = problem.num_free_vars();
    let mut residuals = DVector::zeros(n);
    let mut r_idx = 0;

    for i in 0..node_count {
        let is_p_free = problem.bc_pressure[i].is_none();
        let is_h_free = problem.bc_enthalpy[i].is_none() && problem.bc_temperature[i].is_none();

        if is_p_free {
            // Mass balance residual
            let rm = mass_in[i] - mass_out[i];
            residuals[r_idx] = rm;
            r_idx += 1;
        }

        if is_h_free {
            // Energy balance residual
            let re = energy_in[i] - enthalpies[i] * mass_out[i];
            residuals[r_idx] = re;
            r_idx += 1;
        }
    }

    Ok(residuals)
}

/// Initial guess for unknowns.
pub fn initial_guess(problem: &SteadyProblem) -> SolverResult<DVector<f64>> {
    let node_count = problem.graph.nodes().len();
    let mut pressures = vec![Pressure::new::<uom::si::pressure::pascal>(101325.0); node_count];
    let mut enthalpies = vec![300000.0; node_count];

    // Use boundary conditions where available
    for i in 0..node_count {
        if let Some(p) = problem.bc_pressure[i] {
            pressures[i] = p;
        }
        if let Some(h) = problem.bc_enthalpy[i] {
            enthalpies[i] = h;
        }
    }

    Ok(pack_solution(&pressures, &enthalpies, problem))
}
