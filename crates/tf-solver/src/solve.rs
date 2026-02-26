//! High-level solver interface.

use crate::error::{SolverError, SolverResult};
use crate::jacobian::finite_difference_jacobian;
use crate::newton::{NewtonConfig, newton_solve};
use crate::problem::SteadyProblem;
use crate::steady::{SteadySolution, compute_residuals, initial_guess};
use nalgebra::DVector;
use tf_core::CompId;

/// Solve a steady-state network problem.
///
/// This function:
/// 1. Validates the problem setup
/// 2. Converts temperature BCs to enthalpy BCs
/// 3. Computes an initial guess for unknowns (or uses provided guess)
/// 4. Iteratively solves for mass flows and node states using Newton's method
///
/// Returns the converged solution with pressures, enthalpies, mass flows, and convergence info.
///
/// # Arguments
/// * `problem` - The steady-state problem to solve
/// * `config` - Optional Newton solver configuration
/// * `initial_guess_solution` - Optional previous solution to use as initial guess (for warm-start)
pub fn solve(
    problem: &mut SteadyProblem,
    config: Option<NewtonConfig>,
    initial_guess_solution: Option<&SteadySolution>,
) -> SolverResult<SteadySolution> {
    // Validate problem
    problem.validate()?;

    // Convert temperature BCs to enthalpy BCs
    problem.convert_all_temperature_bcs()?;

    // Get initial guess (from previous solution or compute fresh)
    let x0 = if let Some(prev_sol) = initial_guess_solution {
        // Build initial guess from previous solution
        solution_to_guess_vector(prev_sol, problem)?
    } else {
        initial_guess(problem)?
    };

    // Initial mass flow guess (zero for all components)
    let mass_flows: Vec<(CompId, f64)> = problem
        .graph
        .components()
        .iter()
        .map(|c| (c.id, 0.0))
        .collect();

    let cfg = config.unwrap_or_default();

    // Newton iteration with fixed mass flows for now (simplified for initial implementation)
    // In a full implementation, mass flows would also be unknowns
    let residual_fn = |x: &DVector<f64>| -> SolverResult<DVector<f64>> {
        compute_residuals(x, problem, &mass_flows)
    };

    let jacobian_fn = |x: &DVector<f64>| -> SolverResult<nalgebra::DMatrix<f64>> {
        finite_difference_jacobian(x, residual_fn, 1e-7)
    };

    let result = newton_solve(x0, residual_fn, jacobian_fn, &cfg)?;

    if !result.converged {
        return Err(SolverError::ConvergenceFailed {
            what: "Newton solver did not converge".to_string(),
        });
    }

    // Unpack solution
    let (pressures, enthalpies) = unpack_solution(&result.x, problem)?;

    // Compute mass flows through each component using the solved node states
    let mut mass_flows_computed = Vec::new();
    
    // Create node states from solved pressure and enthalpy
    let mut node_states = Vec::new();
    for (i, (&p, &h)) in pressures.iter().zip(enthalpies.iter()).enumerate() {
        let state = problem.fluid.state(
            tf_fluids::StateInput::PH { p, h },
            problem.composition.clone(),
        ).map_err(|e| SolverError::InvalidState {
            what: format!("Failed to create state for node {}: {}", i, e),
        })?;
        node_states.push(state);
    }

    // Compute mass flow for each component
    for comp_info in problem.graph.components() {
        let comp_id = comp_info.id;
        
        let inlet_node = problem.graph.comp_inlet_node(comp_id)
            .ok_or_else(|| SolverError::ProblemSetup {
                what: format!("Component {:?} has no inlet", comp_id),
            })?;
        let outlet_node = problem.graph.comp_outlet_node(comp_id)
            .ok_or_else(|| SolverError::ProblemSetup {
                what: format!("Component {:?} has no outlet", comp_id),
            })?;
        
        let from_idx = inlet_node.index() as usize;
        let to_idx = outlet_node.index() as usize;
        
        if let Some(component) = problem.components.get(&comp_id) {
            let ports = tf_components::PortStates {
                inlet: &node_states[from_idx],
                outlet: &node_states[to_idx],
            };
            
            let mdot = component.mdot(problem.fluid, ports)?;
            
            mass_flows_computed.push((comp_id, mdot.value));
        }
    }

    Ok(SteadySolution {
        pressures,
        enthalpies,
        mass_flows: mass_flows_computed,
        residual_norm: result.residual_norm,
        iterations: result.iterations,
    })
}

/// Convert a previous SteadySolution back into a guess vector for warm-start.
fn solution_to_guess_vector(
    sol: &SteadySolution,
    problem: &SteadyProblem,
) -> SolverResult<DVector<f64>> {
    let node_count = problem.graph.nodes().len();

    // Count free variables (nodes without BCs)
    let mut free_count = 0;
    for i in 0..node_count {
        if problem.bc_pressure[i].is_none() {
            free_count += 1;
        }
        if problem.bc_enthalpy[i].is_none() && problem.bc_temperature[i].is_none() {
            free_count += 1;
        }
    }

    let mut x = DVector::zeros(free_count);
    let mut idx = 0;

    for i in 0..node_count {
        // Add pressure if free
        if problem.bc_pressure[i].is_none() {
            x[idx] = sol.pressures[i].value;
            idx += 1;
        }
        // Add enthalpy if free
        if problem.bc_enthalpy[i].is_none() && problem.bc_temperature[i].is_none() {
            x[idx] = sol.enthalpies[i];
            idx += 1;
        }
    }

    Ok(x)
}

/// Unpack solution vector into pressures and enthalpies.
fn unpack_solution(
    x: &DVector<f64>,
    problem: &SteadyProblem,
) -> SolverResult<(Vec<tf_core::units::Pressure>, Vec<tf_fluids::SpecEnthalpy>)> {
    use tf_core::units::Pressure;

    let node_count = problem.graph.nodes().len();
    let mut pressures = vec![Pressure::default(); node_count];
    let mut enthalpies = vec![0.0; node_count];

    let mut idx = 0;
    for i in 0..node_count {
        if let Some(p_bc) = problem.bc_pressure[i] {
            pressures[i] = p_bc;
        } else {
            pressures[i] = Pressure::new::<uom::si::pressure::pascal>(x[idx]);
            idx += 1;
        }

        if let Some(h_bc) = problem.bc_enthalpy[i] {
            enthalpies[i] = h_bc;
        } else {
            enthalpies[i] = x[idx];
            idx += 1;
        }
    }

    Ok((pressures, enthalpies))
}
