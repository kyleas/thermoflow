//! High-level solver interface.

use crate::error::{SolverError, SolverResult};
use crate::jacobian::finite_difference_jacobian;
use crate::newton::{newton_solve, NewtonConfig};
use crate::problem::SteadyProblem;
use crate::steady::{compute_residuals, initial_guess, SteadySolution};
use nalgebra::DVector;
use std::collections::HashSet;
use tf_core::CompId;

/// Solve a steady-state network problem.
///
/// This function:
/// 1. Validates the problem setup
/// 2. Converts temperature BCs to enthalpy BCs
/// 3. Computes an initial guess for unknowns (or uses provided guess)
/// 4. Iteratively solves for mass flows and node states using nested Newton's method:
///    - Outer loop: compute mass flows from current node states, check convergence
///    - Inner loop: Newton solver for node states given current mass flows
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
    solve_internal(problem, config, initial_guess_solution, None)
}

pub fn solve_with_active(
    problem: &mut SteadyProblem,
    config: Option<NewtonConfig>,
    initial_guess_solution: Option<&SteadySolution>,
    active_components: &HashSet<CompId>,
) -> SolverResult<SteadySolution> {
    solve_internal(
        problem,
        config,
        initial_guess_solution,
        Some(active_components),
    )
}

fn solve_internal(
    problem: &mut SteadyProblem,
    config: Option<NewtonConfig>,
    initial_guess_solution: Option<&SteadySolution>,
    active_components: Option<&HashSet<CompId>>,
) -> SolverResult<SteadySolution> {
    // Validate problem
    problem.validate()?;

    // Convert temperature BCs to enthalpy BCs
    problem.convert_all_temperature_bcs()?;

    if problem.num_free_vars() == 0 {
        let node_count = problem.graph.nodes().len();
        let mut pressures = Vec::with_capacity(node_count);
        let mut enthalpies = Vec::with_capacity(node_count);

        for i in 0..node_count {
            let p = problem.bc_pressure[i].ok_or_else(|| SolverError::ProblemSetup {
                what: format!("Missing pressure BC at node {}", i),
            })?;
            let h = problem.bc_enthalpy[i].ok_or_else(|| SolverError::ProblemSetup {
                what: format!("Missing enthalpy BC at node {}", i),
            })?;
            pressures.push(p);
            enthalpies.push(h);
        }

        let mass_flows = problem
            .graph
            .components()
            .iter()
            .map(|comp_info| (comp_info.id, 0.0))
            .collect();

        return Ok(SteadySolution {
            pressures,
            enthalpies,
            mass_flows,
            residual_norm: 0.0,
            iterations: 0,
        });
    }

    // Get initial guess (from previous solution or compute fresh)
    let mut x = if let Some(prev_sol) = initial_guess_solution {
        // Build initial guess from previous solution
        solution_to_guess_vector(prev_sol, problem)?
    } else {
        initial_guess(problem)?
    };

    let cfg = config.unwrap_or_default();

    // Outer iteration loop for mass flows
    const MAX_OUTER_ITER: usize = 20;
    // Adaptive tolerance: use larger tolerance for very small flows to avoid numerical issues
    const MDOT_TOLERANCE_REL: f64 = 0.01; // 1% relative tolerance
    const MDOT_TOLERANCE_ABS: f64 = 1e-4; // 0.0001 kg/s absolute tolerance
    let mut total_iterations = 0;

    // Initialize mass flows: use values from warm-start if available, otherwise use small non-zero values
    let mut mass_flows = Vec::new();
    for comp_info in problem.graph.components() {
        let comp_id = comp_info.id;
        let is_active = active_components.map_or(true, |set| set.contains(&comp_id));
        let mdot = if is_active {
            initial_guess_solution
                .and_then(|prev_sol| prev_sol.mass_flows.iter().find(|(id, _)| *id == comp_id))
                .map(|(_, mdot)| *mdot)
                .unwrap_or(0.001)
        } else {
            0.0
        };
        mass_flows.push((comp_id, mdot));
    }
    let mut prev_mdots: Vec<f64> = mass_flows.iter().map(|(_, m)| *m).collect();

    for outer_iter in 0..MAX_OUTER_ITER {
        // Solve for node states with current mass flows
        let residual_fn = |x: &DVector<f64>| -> SolverResult<DVector<f64>> {
            compute_residuals(x, problem, &mass_flows)
        };

        let jacobian_fn = |x: &DVector<f64>| -> SolverResult<nalgebra::DMatrix<f64>> {
            finite_difference_jacobian(x, residual_fn, 1e-7)
        };

        let result = newton_solve(x, residual_fn, jacobian_fn, &cfg)?;
        total_iterations += result.iterations;

        if !result.converged {
            return Err(SolverError::ConvergenceFailed {
                what: format!(
                    "Newton solver did not converge at outer iteration {}",
                    outer_iter
                ),
            });
        }

        x = result.x;

        // Recompute mass flows from converged node states
        let (pressures, enthalpies) = unpack_solution(&x, problem)?;
        let mut node_states = Vec::new();
        for (i, (&p, &h)) in pressures.iter().zip(enthalpies.iter()).enumerate() {
            let state = problem
                .fluid
                .state(
                    tf_fluids::StateInput::PH { p, h },
                    problem.composition.clone(),
                )
                .map_err(|e| SolverError::InvalidState {
                    what: format!("Failed to create state for node {}: {}", i, e),
                })?;
            node_states.push(state);
        }

        let mut new_mass_flows = Vec::new();
        for comp_info in problem.graph.components() {
            let comp_id = comp_info.id;
            let is_active = active_components.map_or(true, |set| set.contains(&comp_id));

            if !is_active {
                new_mass_flows.push((comp_id, 0.0));
                continue;
            }

            let inlet_node = problem.graph.comp_inlet_node(comp_id).ok_or_else(|| {
                SolverError::ProblemSetup {
                    what: format!("Component {:?} has no inlet", comp_id),
                }
            })?;
            let outlet_node = problem.graph.comp_outlet_node(comp_id).ok_or_else(|| {
                SolverError::ProblemSetup {
                    what: format!("Component {:?} has no outlet", comp_id),
                }
            })?;

            let from_idx = inlet_node.index() as usize;
            let to_idx = outlet_node.index() as usize;

            let component = problem.components.get(&comp_id).ok_or_else(|| {
                SolverError::ProblemSetup {
                    what: format!("Component {:?} not found", comp_id),
                }
            })?;
            let ports = tf_components::PortStates {
                inlet: &node_states[from_idx],
                outlet: &node_states[to_idx],
            };

            let mdot = component.mdot(problem.fluid, ports)?;
            new_mass_flows.push((comp_id, mdot.value));
        }

        // Check if mass flows have converged using adaptive tolerance
        let mut mdot_converged = true;
        for (i, (_, new_mdot)) in new_mass_flows.iter().enumerate() {
            let mdot_abs = new_mdot.abs().max(prev_mdots[i].abs());
            let tol = MDOT_TOLERANCE_ABS + MDOT_TOLERANCE_REL * mdot_abs;
            if (new_mdot - prev_mdots[i]).abs() > tol {
                mdot_converged = false;
                break;
            }
        }

        // Update for next iteration
        prev_mdots = new_mass_flows.iter().map(|(_, m)| *m).collect();
        mass_flows = new_mass_flows;

        // Check if converged
        if mdot_converged {
            // Already have converged solution - return it
            return Ok(SteadySolution {
                pressures,
                enthalpies,
                mass_flows,
                residual_norm: result.residual_norm,
                iterations: total_iterations,
            });
        }
    }

    Err(SolverError::ConvergenceFailed {
        what: "Outer iteration for mass flow convergence failed".to_string(),
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
