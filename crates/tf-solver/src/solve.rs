//! High-level solver interface.

use crate::error::{SolverError, SolverResult};
use crate::initialization::InitializationStrategy;
use crate::jacobian::finite_difference_jacobian;
use crate::newton::{NewtonConfig, newton_solve_with_validator};
use crate::problem::SteadyProblem;
use crate::steady::{SolverTimingStats, SteadySolution, compute_residuals, initial_guess};
use crate::thermo_policy::{StrictPolicy, ThermoStatePolicy};
use nalgebra::DVector;
use std::collections::HashSet;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::time::Instant;
use tf_core::CompId;

#[derive(Debug, Clone)]
pub enum SolveProgressEvent {
    OuterIterationStarted {
        outer_iteration: usize,
        max_outer_iterations: usize,
    },
    NewtonIteration {
        outer_iteration: usize,
        iteration: usize,
        residual_norm: f64,
    },
    OuterIterationCompleted {
        outer_iteration: usize,
        residual_norm: f64,
    },
    Converged {
        total_iterations: usize,
        residual_norm: f64,
    },
}

#[derive(Clone, Copy, Debug)]
enum VarKind {
    Pressure,
    Enthalpy(usize),
}

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
    solve_internal(
        problem,
        config,
        initial_guess_solution,
        None,
        &StrictPolicy,
        None,
        None,
    )
}

pub fn solve_with_progress(
    problem: &mut SteadyProblem,
    config: Option<NewtonConfig>,
    initial_guess_solution: Option<&SteadySolution>,
    observer: &mut dyn FnMut(SolveProgressEvent),
) -> SolverResult<SteadySolution> {
    solve_internal(
        problem,
        config,
        initial_guess_solution,
        None,
        &StrictPolicy,
        Some(observer),
        None,
    )
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
        &StrictPolicy,
        None,
        None,
    )
}

/// Solve with an optional fallback policy for node state creation.
///
/// Allows transient/continuation solves to recover from invalid node (P, h) pairs
/// using approximate surrogates instead of failing hard.
///
/// Default behavior (StrictPolicy) remains strict. Provide a custom policy to enable fallback.
pub fn solve_with_policy(
    problem: &mut SteadyProblem,
    config: Option<NewtonConfig>,
    initial_guess_solution: Option<&SteadySolution>,
    policy: &dyn ThermoStatePolicy,
) -> SolverResult<SteadySolution> {
    solve_internal(
        problem,
        config,
        initial_guess_solution,
        None,
        policy,
        None,
        None,
    )
}

/// Solve with active components filter and custom thermo state policy.
pub fn solve_with_active_and_policy(
    problem: &mut SteadyProblem,
    config: Option<NewtonConfig>,
    initial_guess_solution: Option<&SteadySolution>,
    active_components: &HashSet<CompId>,
    policy: &dyn ThermoStatePolicy,
) -> SolverResult<SteadySolution> {
    solve_internal(
        problem,
        config,
        initial_guess_solution,
        Some(active_components),
        policy,
        None,
        None,
    )
}

/// Solve with explicit initialization strategy.
///
/// The strategy controls startup behavior:
/// - Strict: Direct initialization with minimal regularization
/// - Relaxed: Conservative startup with weak-flow regularization and enthalpy clamping
///
/// If both `config` and `strategy` are provided, `config` takes precedence.
/// If neither is provided, defaults to Strict strategy.
pub fn solve_with_strategy(
    problem: &mut SteadyProblem,
    strategy: InitializationStrategy,
    initial_guess_solution: Option<&SteadySolution>,
) -> SolverResult<SteadySolution> {
    solve_internal(
        problem,
        None,
        initial_guess_solution,
        None,
        &StrictPolicy,
        None,
        Some(strategy),
    )
}

/// Solve with strategy and progress reporting.
pub fn solve_with_strategy_and_progress(
    problem: &mut SteadyProblem,
    strategy: InitializationStrategy,
    initial_guess_solution: Option<&SteadySolution>,
    observer: &mut dyn FnMut(SolveProgressEvent),
) -> SolverResult<SteadySolution> {
    solve_internal(
        problem,
        None,
        initial_guess_solution,
        None,
        &StrictPolicy,
        Some(observer),
        Some(strategy),
    )
}

/// Solve with strategy, policy, and progress reporting.
pub fn solve_with_strategy_policy_and_progress(
    problem: &mut SteadyProblem,
    strategy: InitializationStrategy,
    initial_guess_solution: Option<&SteadySolution>,
    policy: &dyn ThermoStatePolicy,
    observer: &mut dyn FnMut(SolveProgressEvent),
) -> SolverResult<SteadySolution> {
    solve_internal(
        problem,
        None,
        initial_guess_solution,
        None,
        policy,
        Some(observer),
        Some(strategy),
    )
}

fn solve_internal(
    problem: &mut SteadyProblem,
    config: Option<NewtonConfig>,
    initial_guess_solution: Option<&SteadySolution>,
    active_components: Option<&HashSet<CompId>>,
    policy: &dyn ThermoStatePolicy,
    mut observer: Option<&mut dyn FnMut(SolveProgressEvent)>,
    initialization_strategy: Option<InitializationStrategy>,
) -> SolverResult<SteadySolution> {
    // Validate problem
    problem.validate()?;

    // Convert temperature BCs to enthalpy BCs
    problem.convert_all_temperature_bcs()?;

    if problem.num_free_vars() == 0 {
        // All nodes have boundary conditions - no unknowns to solve for.
        // Extract pressures and enthalpies from BCs, then compute mass flows directly.
        // Phase 0: Instrument this direct path since transient uses it exclusively
        let thermo_start = std::time::Instant::now();
        
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

        // Compute fluid states for all nodes
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
        let thermo_time_s = thermo_start.elapsed().as_secs_f64();

        // Compute mass flows from component equations
        let mdot_start = std::time::Instant::now();
        let mut mass_flows = Vec::new();
        for comp_info in problem.graph.components() {
            let comp_id = comp_info.id;
            let is_active = active_components.is_none_or(|set| set.contains(&comp_id));

            if !is_active {
                mass_flows.push((comp_id, 0.0));
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

            let component =
                problem
                    .components
                    .get(&comp_id)
                    .ok_or_else(|| SolverError::ProblemSetup {
                        what: format!("Component {:?} not found", comp_id),
                    })?;
            let ports = tf_components::PortStates {
                inlet: &node_states[from_idx],
                outlet: &node_states[to_idx],
            };

            let mdot = component.mdot(problem.fluid, ports)?;
            mass_flows.push((comp_id, mdot.value));
        }
        let mdot_time_s = mdot_start.elapsed().as_secs_f64();

        // Phase 0: Return timing for direct path (no residual/jacobian/linesearch on this path)
        let timing_stats = crate::steady::SolverTimingStats {
            thermo_createstate_time_s: thermo_time_s,
            // For direct path, "residual" time represents mass flow computation
            residual_eval_time_s: mdot_time_s,
            jacobian_eval_time_s: 0.0,
            linearch_time_s: 0.0,
            residual_eval_count: 1, // One "evaluation" = one direct solve
            jacobian_eval_count: 0,
            linearch_iter_count: 0,
        };

        return Ok(SteadySolution {
            pressures,
            enthalpies,
            mass_flows,
            residual_norm: 0.0,
            iterations: 0,
            timing_stats,
        });
    }

    // Get initial guess (from previous solution or compute fresh)
    let mut x = if let Some(prev_sol) = initial_guess_solution {
        // Build initial guess from previous solution
        solution_to_guess_vector(prev_sol, problem)?
    } else {
        initial_guess(problem)?
    };
    // Determine NewtonConfig: explicit config takes precedence, then strategy, then default
    let cfg = if let Some(c) = config {
        c
    } else if let Some(strategy) = initialization_strategy {
        strategy.to_newton_config()
    } else {
        InitializationStrategy::default().to_newton_config()
    };

    let var_kinds = {
        let mut kinds = Vec::with_capacity(problem.num_free_vars());
        for i in 0..problem.graph.nodes().len() {
            if problem.bc_pressure[i].is_none() {
                kinds.push(VarKind::Pressure);
            }
            if problem.bc_enthalpy[i].is_none() && problem.bc_temperature[i].is_none() {
                kinds.push(VarKind::Enthalpy(i));
            }
        }
        kinds
    };

    let prior_enthalpies = unpack_solution(&x, problem)?.1;

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
        let is_active = active_components.is_none_or(|set| set.contains(&comp_id));
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

    // Phase 0: Initialize instrumentation counters and timers
    let residual_eval_count = std::sync::Arc::new(AtomicUsize::new(0));
    let jacobian_eval_count = std::sync::Arc::new(AtomicUsize::new(0));
    let linearch_iter_count = std::sync::Arc::new(AtomicUsize::new(0));
    let residual_time_ns = std::sync::Arc::new(AtomicUsize::new(0));
    let jacobian_time_ns = std::sync::Arc::new(AtomicUsize::new(0));
    let linearch_time_ns = std::sync::Arc::new(AtomicUsize::new(0));
    let thermo_createstate_time_ns = std::sync::Arc::new(AtomicUsize::new(0));

    if cfg.enthalpy_total_abs.is_finite() || cfg.enthalpy_total_rel.is_finite() {
        let node_flow = compute_node_flow_magnitudes(problem, &mass_flows);
        let mut clamp_hits = 0usize;

        for (var_idx, kind) in var_kinds.iter().enumerate() {
            if let VarKind::Enthalpy(node_idx) = kind {
                let h_prior = prior_enthalpies[*node_idx];
                let total_abs_limit = cfg.enthalpy_total_abs;
                let total_rel_limit = cfg.enthalpy_total_rel * h_prior.abs().max(cfg.enthalpy_ref);
                let mut max_total = total_abs_limit.min(total_rel_limit);

                if cfg.weak_flow_mdot > 0.0 && node_flow[*node_idx] < cfg.weak_flow_mdot {
                    max_total *= cfg.weak_flow_enthalpy_scale;
                }

                if max_total.is_finite() {
                    let h_min = h_prior - max_total;
                    let h_max = h_prior + max_total;
                    if x[var_idx] < h_min || x[var_idx] > h_max {
                        if clamp_hits < 5 {
                            eprintln!(
                                "[TRUST] Node {} enthalpy initial clamp: h={:.1} J/kg -> [{:.1}, {:.1}]",
                                node_idx, x[var_idx], h_min, h_max
                            );
                        }
                        clamp_hits += 1;
                        x[var_idx] = x[var_idx].clamp(h_min, h_max);
                    }
                }
            }
        }
    }

    for outer_iter in 0..MAX_OUTER_ITER {
        if let Some(cb) = observer.as_deref_mut() {
            cb(SolveProgressEvent::OuterIterationStarted {
                outer_iteration: outer_iter + 1,
                max_outer_iterations: MAX_OUTER_ITER,
            });
        }

        // Solve for node states with current mass flows
        // Phase 0: Wrap residual and jacobian with timing instrumentation
        let residual_eval_count_clone = residual_eval_count.clone();
        let residual_time_ns_clone = residual_time_ns.clone();
        let thermo_createstate_time_ns_clone = thermo_createstate_time_ns.clone();
        let residual_fn = |x: &DVector<f64>| -> SolverResult<DVector<f64>> {
            let start = Instant::now();
            let thermo_start = Instant::now();
            let result = compute_residuals(
                x,
                problem,
                &mass_flows,
                policy,
                Some(&prior_enthalpies),
                cfg.weak_flow_mdot,
            );
            let thermo_elapsed = thermo_start.elapsed();
            let elapsed = start.elapsed();

            residual_eval_count_clone.fetch_add(1, Ordering::Relaxed);
            residual_time_ns_clone.fetch_add(elapsed.as_nanos() as usize, Ordering::Relaxed);
            thermo_createstate_time_ns_clone
                .fetch_add(thermo_elapsed.as_nanos() as usize, Ordering::Relaxed);
            result
        };

        let jacobian_eval_count_clone = jacobian_eval_count.clone();
        let jacobian_time_ns_clone = jacobian_time_ns.clone();
        let jacobian_fn = |x: &DVector<f64>| -> SolverResult<nalgebra::DMatrix<f64>> {
            let start = Instant::now();
            let result = finite_difference_jacobian(x, residual_fn, 1e-7);
            let elapsed = start.elapsed();

            jacobian_eval_count_clone.fetch_add(1, Ordering::Relaxed);
            jacobian_time_ns_clone.fetch_add(elapsed.as_nanos() as usize, Ordering::Relaxed);
            result
        };

        // Fluid state validator: reject trial states that produce invalid P,h combinations
        // This prevents CoolProp errors during Newton line search for real-fluid solves
        let state_validator = |x: &DVector<f64>| -> bool {
            match unpack_solution(x, problem) {
                Ok((pressures, enthalpies)) => {
                    for (&p, &h) in pressures.iter().zip(enthalpies.iter()) {
                        if problem
                            .fluid
                            .state(
                                tf_fluids::StateInput::PH { p, h },
                                problem.composition.clone(),
                            )
                            .is_err()
                        {
                            return false;
                        }
                    }
                    true
                }
                Err(_) => false,
            }
        };

        let node_flow = compute_node_flow_magnitudes(problem, &mass_flows);
        let trust_hit_count = std::cell::Cell::new(0usize);

        let step_limiter = |x_current: &DVector<f64>, x_candidate: &DVector<f64>| -> DVector<f64> {
            let limits_enabled = cfg.enthalpy_delta_abs.is_finite()
                || cfg.enthalpy_delta_rel.is_finite()
                || cfg.enthalpy_total_abs.is_finite()
                || cfg.enthalpy_total_rel.is_finite();
            if !limits_enabled {
                return x_candidate.clone();
            }

            let mut limited = x_candidate.clone();

            for (var_idx, kind) in var_kinds.iter().enumerate() {
                if let VarKind::Enthalpy(node_idx) = kind {
                    let h_current = x_current[var_idx];
                    let h_prior = prior_enthalpies[*node_idx];

                    let abs_limit = cfg.enthalpy_delta_abs;
                    let rel_limit = cfg.enthalpy_delta_rel * h_current.abs().max(cfg.enthalpy_ref);
                    let mut max_delta = abs_limit.min(rel_limit);

                    let total_abs_limit = cfg.enthalpy_total_abs;
                    let total_rel_limit =
                        cfg.enthalpy_total_rel * h_prior.abs().max(cfg.enthalpy_ref);
                    let mut max_total = total_abs_limit.min(total_rel_limit);

                    if cfg.weak_flow_mdot > 0.0 && node_flow[*node_idx] < cfg.weak_flow_mdot {
                        max_delta *= cfg.weak_flow_enthalpy_scale;
                        max_total *= cfg.weak_flow_enthalpy_scale;
                    }

                    let mut h_min = h_prior - max_total;
                    let mut h_max = h_prior + max_total;

                    if max_delta.is_finite() {
                        let step_min = h_current - max_delta;
                        let step_max = h_current + max_delta;
                        h_min = h_min.max(step_min);
                        h_max = h_max.min(step_max);
                    }

                    if h_min.is_finite() && h_max.is_finite() && h_min <= h_max {
                        let h_candidate = limited[var_idx];
                        if h_candidate < h_min || h_candidate > h_max {
                            let hits = trust_hit_count.get();
                            if hits < 5 {
                                eprintln!(
                                    "[TRUST] Node {} enthalpy clamped: h={:.1} J/kg -> [{:.1}, {:.1}], flow={:.4} kg/s",
                                    node_idx, h_candidate, h_min, h_max, node_flow[*node_idx]
                                );
                            }
                            trust_hit_count.set(hits + 1);
                            limited[var_idx] = h_candidate.clamp(h_min, h_max);
                        }
                    }
                }
            }

            limited
        };

        let mut iter_observer = |iter: usize, residual_norm: f64| {
            if let Some(cb) = observer.as_deref_mut() {
                cb(SolveProgressEvent::NewtonIteration {
                    outer_iteration: outer_iter + 1,
                    iteration: iter,
                    residual_norm,
                });
            }
        };

        let result = if policy.allow_invalid_ph() {
            newton_solve_with_validator(
                x,
                residual_fn,
                jacobian_fn,
                &cfg,
                None::<fn(&DVector<f64>) -> bool>,
                None::<fn(&DVector<f64>, &DVector<f64>) -> bool>,
                Some(step_limiter),
                Some(&mut iter_observer),
            )?
        } else {
            newton_solve_with_validator(
                x,
                residual_fn,
                jacobian_fn,
                &cfg,
                Some(state_validator),
                None::<fn(&DVector<f64>, &DVector<f64>) -> bool>,
                Some(step_limiter),
                Some(&mut iter_observer),
            )?
        };
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
            let is_active = active_components.is_none_or(|set| set.contains(&comp_id));

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

            let component =
                problem
                    .components
                    .get(&comp_id)
                    .ok_or_else(|| SolverError::ProblemSetup {
                        what: format!("Component {:?} not found", comp_id),
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
            if let Some(cb) = observer.as_deref_mut() {
                cb(SolveProgressEvent::Converged {
                    total_iterations,
                    residual_norm: result.residual_norm,
                });
            }
            // Already have converged solution - return it
            // Phase 0: Include fine-grained timing statistics
            let timing_stats = SolverTimingStats {
                residual_eval_time_s: residual_time_ns.load(Ordering::Relaxed) as f64 / 1e9,
                jacobian_eval_time_s: jacobian_time_ns.load(Ordering::Relaxed) as f64 / 1e9,
                linearch_time_s: linearch_time_ns.load(Ordering::Relaxed) as f64 / 1e9,
                thermo_createstate_time_s: thermo_createstate_time_ns.load(Ordering::Relaxed)
                    as f64
                    / 1e9,
                residual_eval_count: residual_eval_count.load(Ordering::Relaxed),
                jacobian_eval_count: jacobian_eval_count.load(Ordering::Relaxed),
                linearch_iter_count: linearch_iter_count.load(Ordering::Relaxed),
            };
            return Ok(SteadySolution {
                pressures,
                enthalpies,
                mass_flows,
                residual_norm: result.residual_norm,
                iterations: total_iterations,
                timing_stats,
            });
        }

        if let Some(cb) = observer.as_deref_mut() {
            cb(SolveProgressEvent::OuterIterationCompleted {
                outer_iteration: outer_iter + 1,
                residual_norm: result.residual_norm,
            });
        }
    }

    Err(SolverError::ConvergenceFailed {
        what: "Outer iteration for mass flow convergence failed".to_string(),
    })
}

fn compute_node_flow_magnitudes(problem: &SteadyProblem, mass_flows: &[(CompId, f64)]) -> Vec<f64> {
    let node_count = problem.graph.nodes().len();
    let mut mass_in = vec![0.0; node_count];
    let mut mass_out = vec![0.0; node_count];

    for (comp_id, mdot) in mass_flows {
        let inlet_node = match problem.graph.comp_inlet_node(*comp_id) {
            Some(node) => node,
            None => continue,
        };
        let outlet_node = match problem.graph.comp_outlet_node(*comp_id) {
            Some(node) => node,
            None => continue,
        };

        let inlet_idx = inlet_node.index() as usize;
        let outlet_idx = outlet_node.index() as usize;

        if *mdot >= 0.0 {
            mass_out[inlet_idx] += *mdot;
            mass_in[outlet_idx] += *mdot;
        } else {
            let mdot_abs = mdot.abs();
            mass_out[outlet_idx] += mdot_abs;
            mass_in[inlet_idx] += mdot_abs;
        }
    }

    mass_in
        .iter()
        .zip(mass_out.iter())
        .map(|(mi, mo)| mi + mo)
        .collect()
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
