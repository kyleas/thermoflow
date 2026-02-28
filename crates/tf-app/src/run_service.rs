//! Run execution and caching service.

use std::path::Path;
use std::time::Instant;
use tf_project::schema::SystemDef;
use tf_results::{
    EdgeValueSnapshot, GlobalValueSnapshot, NodeValueSnapshot, RunManifest, RunStore,
    RunType as ResultsRunType, TimeseriesRecord,
};
use tf_solver::{InitializationStrategy, SolveProgressEvent, SteadyProblem};

use crate::error::AppResult;
use crate::progress::{RunProgressEvent, RunStage, SteadyProgress, TransientProgress};
use crate::project_service;
use crate::runtime_compile::{self, BoundaryCondition, SystemRuntime};

/// Run mode specification.
#[derive(Debug, Clone)]
pub enum RunMode {
    Steady,
    Transient { dt_s: f64, t_end_s: f64 },
}

/// Options for running simulations.
#[derive(Debug, Clone)]
pub struct RunOptions {
    pub use_cache: bool,
    pub solver_version: String,
    pub initialization_strategy: Option<InitializationStrategy>,
}

impl Default for RunOptions {
    fn default() -> Self {
        Self {
            use_cache: true,
            solver_version: "0.1.0".to_string(),
            initialization_strategy: None,
        }
    }
}

/// Request to execute a run.
pub struct RunRequest<'a> {
    pub project_path: &'a Path,
    pub system_id: &'a str,
    pub mode: RunMode,
    pub options: RunOptions,
}

/// Concise timing and execution summary for a run.
#[derive(Debug, Clone, Default)]
pub struct RunTimingSummary {
    pub compile_time_s: f64,
    pub build_time_s: f64,
    pub solve_time_s: f64,
    pub save_time_s: f64,
    pub load_cache_time_s: f64,
    pub total_time_s: f64,
    // Fine-grained solver timing breakdown (Phase 0 instrumentation)
    pub solve_residual_time_s: f64,
    pub solve_jacobian_time_s: f64,
    pub solve_linearch_time_s: f64,
    pub solve_thermo_time_s: f64,
    pub solve_residual_eval_count: usize,
    pub solve_jacobian_eval_count: usize,
    pub solve_linearch_iter_count: usize,
    pub rhs_calls: usize,
    pub rhs_snapshot_time_s: f64,
    pub rhs_plan_check_time_s: f64,
    pub rhs_component_rebuild_time_s: f64,
    pub rhs_snapshot_structure_setup_time_s: f64,
    pub rhs_boundary_hydration_time_s: f64,
    pub rhs_direct_solve_setup_time_s: f64,
    pub rhs_result_unpack_time_s: f64,
    pub rhs_state_reconstruct_time_s: f64,
    pub rhs_buffer_init_time_s: f64,
    pub rhs_flow_routing_time_s: f64,
    pub rhs_cv_derivative_time_s: f64,
    pub rhs_lv_derivative_time_s: f64,
    pub rhs_assembly_time_s: f64,
    pub rhs_surrogate_time_s: f64,
    pub rk4_bookkeeping_time_s: f64,
    pub execution_plan_checks: usize,
    pub execution_plan_unchanged: usize,
    pub component_rebuilds: usize,
    pub component_reuses: usize,
    pub snapshot_setup_rebuilds: usize,
    pub snapshot_setup_reuses: usize,
    pub transient_steps: usize,
    pub transient_cutback_retries: usize,
    pub transient_fallback_uses: usize,
    pub transient_real_fluid_attempts: usize,
    pub transient_real_fluid_successes: usize,
    pub transient_surrogate_populations: usize,
    pub initialization_strategy: Option<String>,
    pub steady_iterations: usize,
    pub steady_residual_norm: f64,
}

/// Response from a run execution.
#[derive(Debug, Clone)]
pub struct RunResponse {
    pub run_id: String,
    pub manifest: RunManifest,
    pub loaded_from_cache: bool,
    pub timing: RunTimingSummary,
}

#[allow(clippy::too_many_arguments)]
fn emit_progress(
    progress_cb: &mut Option<&mut dyn FnMut(RunProgressEvent)>,
    mode: RunMode,
    stage: RunStage,
    started: Instant,
    initialization_strategy: Option<String>,
    message: Option<String>,
    steady: Option<SteadyProgress>,
    transient: Option<TransientProgress>,
) {
    if let Some(cb) = progress_cb.as_deref_mut() {
        cb(RunProgressEvent {
            mode,
            stage,
            elapsed_wall_s: started.elapsed().as_secs_f64(),
            initialization_strategy,
            message,
            steady,
            transient,
        });
    }
}

/// Execute or load a run based on request.
pub fn ensure_run(request: &RunRequest) -> AppResult<RunResponse> {
    ensure_run_with_progress(request, None)
}

/// Execute or load a run and stream backend progress events.
pub fn ensure_run_with_progress(
    request: &RunRequest,
    mut progress_cb: Option<&mut dyn FnMut(RunProgressEvent)>,
) -> AppResult<RunResponse> {
    let started = Instant::now();
    let mut timing = RunTimingSummary::default();

    emit_progress(
        &mut progress_cb,
        request.mode.clone(),
        RunStage::LoadingProject,
        started,
        None,
        Some("Loading project".to_string()),
        None,
        None,
    );

    // Load project
    let project = project_service::load_project(request.project_path)?;
    let system = project_service::get_system(&project, request.system_id)?;

    emit_progress(
        &mut progress_cb,
        request.mode.clone(),
        RunStage::CheckingCache,
        started,
        None,
        Some("Checking run cache".to_string()),
        None,
        None,
    );

    // Compute run ID
    let result_run_type = match &request.mode {
        RunMode::Steady => ResultsRunType::Steady,
        RunMode::Transient { dt_s, t_end_s } => {
            let steps = ((*t_end_s / *dt_s).ceil() as usize).max(1);
            ResultsRunType::Transient {
                dt_s: *dt_s,
                t_end_s: *t_end_s,
                steps,
            }
        }
    };

    let run_id =
        tf_results::compute_run_id(system, &result_run_type, &request.options.solver_version);

    // Initialize run store
    let store = RunStore::for_project(request.project_path)?;

    // Check cache
    if request.options.use_cache && store.has_run(&run_id) {
        emit_progress(
            &mut progress_cb,
            request.mode.clone(),
            RunStage::LoadingCachedResult,
            started,
            None,
            Some("Loading cached run".to_string()),
            None,
            None,
        );

        let load_started = Instant::now();
        let manifest = store.load_manifest(&run_id)?;
        timing.load_cache_time_s = load_started.elapsed().as_secs_f64();
        timing.total_time_s = started.elapsed().as_secs_f64();

        emit_progress(
            &mut progress_cb,
            request.mode.clone(),
            RunStage::Completed,
            started,
            None,
            Some("Loaded cached run".to_string()),
            None,
            None,
        );

        return Ok(RunResponse {
            run_id,
            manifest,
            loaded_from_cache: true,
            timing,
        });
    }

    // Execute run
    let mut timing = RunTimingSummary::default();

    // Determine initialization strategy
    let strategy = determine_initialization_strategy(
        &request.mode,
        system,
        request.options.initialization_strategy,
    );
    timing.initialization_strategy = Some(strategy.as_str().to_string());

    let manifest = execute_run(
        system,
        request.system_id,
        &request.mode,
        &store,
        &run_id,
        &request.options.solver_version,
        &mut progress_cb,
        started,
        &mut timing,
        strategy,
    )?;

    timing.total_time_s = started.elapsed().as_secs_f64();

    emit_progress(
        &mut progress_cb,
        request.mode.clone(),
        RunStage::Completed,
        started,
        timing.initialization_strategy.clone(),
        Some("Run completed".to_string()),
        None,
        None,
    );

    Ok(RunResponse {
        run_id,
        manifest,
        loaded_from_cache: false,
        timing,
    })
}

/// Determine appropriate initialization strategy based on run context.
///
/// Priority:
/// 1. User-specified strategy (if provided)
/// 2. Auto-select based on run mode and system complexity:
///    - Steady: Strict (simple, well-conditioned)
///    - Transient with single CV: Strict
///    - Transient with multiple CVs or LineVolumes: Relaxed (robust startup)
fn determine_initialization_strategy(
    mode: &RunMode,
    system: &SystemDef,
    user_strategy: Option<InitializationStrategy>,
) -> InitializationStrategy {
    use tf_project::schema::{ComponentKind, NodeKind};

    // User-specified strategy takes precedence
    if let Some(strategy) = user_strategy {
        return strategy;
    }

    // Auto-select based on run mode and system complexity
    match mode {
        RunMode::Steady => InitializationStrategy::Strict,
        RunMode::Transient { .. } => {
            // Count control volumes and LineVolumes
            let cv_count = system
                .nodes
                .iter()
                .filter(|n| matches!(n.kind, NodeKind::ControlVolume { .. }))
                .count();
            let lv_count = system
                .components
                .iter()
                .filter(|c| matches!(c.kind, ComponentKind::LineVolume { .. }))
                .count();

            // Use Relaxed for multi-CV or storage-rich transients
            if cv_count > 1 || lv_count > 0 {
                InitializationStrategy::Relaxed
            } else {
                InitializationStrategy::Strict
            }
        }
    }
}

/// Execute a run (steady or transient).
#[allow(clippy::too_many_arguments)]
fn execute_run(
    system: &SystemDef,
    system_id: &str,
    mode: &RunMode,
    store: &RunStore,
    run_id: &str,
    solver_version: &str,
    progress_cb: &mut Option<&mut dyn FnMut(RunProgressEvent)>,
    started: Instant,
    timing: &mut RunTimingSummary,
    strategy: InitializationStrategy,
) -> AppResult<RunManifest> {
    match mode {
        RunMode::Steady => execute_steady(
            system,
            system_id,
            store,
            run_id,
            solver_version,
            progress_cb,
            started,
            timing,
            strategy,
        ),
        RunMode::Transient { dt_s, t_end_s } => execute_transient(
            system,
            system_id,
            store,
            run_id,
            *dt_s,
            *t_end_s,
            solver_version,
            progress_cb,
            started,
            timing,
            strategy,
        ),
    }
}

/// Execute steady-state simulation.
#[allow(clippy::too_many_arguments)]
fn execute_steady(
    system: &SystemDef,
    system_id: &str,
    store: &RunStore,
    run_id: &str,
    solver_version: &str,
    progress_cb: &mut Option<&mut dyn FnMut(RunProgressEvent)>,
    started: Instant,
    timing: &mut RunTimingSummary,
    strategy: InitializationStrategy,
) -> AppResult<RunManifest> {
    emit_progress(
        progress_cb,
        RunMode::Steady,
        RunStage::CompilingRuntime,
        started,
        timing.initialization_strategy.clone(),
        Some("Compiling runtime".to_string()),
        None,
        None,
    );

    // Compile runtime
    let compile_started = Instant::now();
    let runtime = runtime_compile::compile_system(system)?;
    let fluid_model = runtime_compile::build_fluid_model(&system.fluid)?;
    let boundaries = runtime_compile::parse_boundaries_with_atmosphere(
        system,
        &system.boundaries,
        &runtime.node_id_map,
    )?;
    let components = runtime_compile::build_components(system, &runtime.comp_id_map)?;
    timing.compile_time_s = compile_started.elapsed().as_secs_f64();

    emit_progress(
        progress_cb,
        RunMode::Steady,
        RunStage::BuildingSteadyProblem,
        started,
        timing.initialization_strategy.clone(),
        Some("Building steady problem".to_string()),
        None,
        None,
    );

    let build_started = Instant::now();

    // Build problem
    let mut problem = SteadyProblem::new(
        &runtime.graph,
        fluid_model.as_ref(),
        runtime.composition.clone(),
    );

    // Add components
    for (comp_id, component) in components {
        problem.add_component(comp_id, component)?;
    }

    // Set boundary conditions
    for (node_id, bc) in boundaries {
        match bc {
            BoundaryCondition::PT { p, t } => {
                problem.set_pressure_bc(node_id, p)?;
                problem.set_temperature_bc(node_id, t)?;
            }
            BoundaryCondition::PH { p, h } => {
                problem.set_pressure_bc(node_id, p)?;
                problem.set_enthalpy_bc(node_id, h)?;
            }
        }
    }

    timing.build_time_s = build_started.elapsed().as_secs_f64();

    emit_progress(
        progress_cb,
        RunMode::Steady,
        RunStage::SolvingSteady,
        started,
        timing.initialization_strategy.clone(),
        Some("Solving steady system".to_string()),
        None,
        None,
    );

    // Solve
    let solve_started = Instant::now();
    let solution =
        tf_solver::solve_with_strategy_and_progress(&mut problem, strategy, None, &mut |event| {
            match event {
                SolveProgressEvent::OuterIterationStarted {
                    outer_iteration,
                    max_outer_iterations,
                } => emit_progress(
                    progress_cb,
                    RunMode::Steady,
                    RunStage::SolvingSteady,
                    started,
                    Some(strategy.as_str().to_string()),
                    Some(format!(
                        "Steady solve outer iteration {}/{}",
                        outer_iteration, max_outer_iterations
                    )),
                    Some(SteadyProgress {
                        outer_iteration: Some(outer_iteration),
                        max_outer_iterations: Some(max_outer_iterations),
                        ..Default::default()
                    }),
                    None,
                ),
                SolveProgressEvent::NewtonIteration {
                    outer_iteration,
                    iteration,
                    residual_norm,
                } => emit_progress(
                    progress_cb,
                    RunMode::Steady,
                    RunStage::SolvingSteady,
                    started,
                    Some(strategy.as_str().to_string()),
                    Some("Steady Newton iteration".to_string()),
                    Some(SteadyProgress {
                        outer_iteration: Some(outer_iteration),
                        iteration: Some(iteration),
                        residual_norm: Some(residual_norm),
                        ..Default::default()
                    }),
                    None,
                ),
                SolveProgressEvent::OuterIterationCompleted {
                    outer_iteration,
                    residual_norm,
                } => emit_progress(
                    progress_cb,
                    RunMode::Steady,
                    RunStage::SolvingSteady,
                    started,
                    Some(strategy.as_str().to_string()),
                    Some(format!("Outer iteration {} completed", outer_iteration)),
                    Some(SteadyProgress {
                        outer_iteration: Some(outer_iteration),
                        residual_norm: Some(residual_norm),
                        ..Default::default()
                    }),
                    None,
                ),
                SolveProgressEvent::Converged {
                    total_iterations,
                    residual_norm,
                } => emit_progress(
                    progress_cb,
                    RunMode::Steady,
                    RunStage::SolvingSteady,
                    started,
                    Some(strategy.as_str().to_string()),
                    Some("Steady solve converged".to_string()),
                    Some(SteadyProgress {
                        iteration: Some(total_iterations),
                        residual_norm: Some(residual_norm),
                        ..Default::default()
                    }),
                    None,
                ),
            }
        })?;
    timing.solve_time_s = solve_started.elapsed().as_secs_f64();
    timing.steady_iterations = solution.iterations;
    timing.steady_residual_norm = solution.residual_norm;
    // Wire up fine-grained solver timing from Phase 0 instrumentation
    timing.solve_residual_time_s = solution.timing_stats.residual_eval_time_s;
    timing.solve_jacobian_time_s = solution.timing_stats.jacobian_eval_time_s;
    timing.solve_linearch_time_s = solution.timing_stats.linearch_time_s;
    timing.solve_thermo_time_s = solution.timing_stats.thermo_createstate_time_s;
    timing.solve_residual_eval_count = solution.timing_stats.residual_eval_count;
    timing.solve_jacobian_eval_count = solution.timing_stats.jacobian_eval_count;
    timing.solve_linearch_iter_count = solution.timing_stats.linearch_iter_count;

    // Convert to timeseries record
    let record = solution_to_timeseries(&solution, &runtime);

    // Build manifest
    let manifest = RunManifest {
        run_id: run_id.to_string(),
        system_id: system_id.to_string(),
        timestamp: chrono::Utc::now().to_rfc3339(),
        run_type: ResultsRunType::Steady,
        solver_version: solver_version.to_string(),
    };

    emit_progress(
        progress_cb,
        RunMode::Steady,
        RunStage::SavingResults,
        started,
        timing.initialization_strategy.clone(),
        Some("Saving run output".to_string()),
        None,
        None,
    );

    // Save
    let save_started = Instant::now();
    store.save_run(&manifest, &[record])?;
    timing.save_time_s = save_started.elapsed().as_secs_f64();

    Ok(manifest)
}

/// Execute transient simulation.
#[allow(clippy::too_many_arguments)]
fn execute_transient(
    system: &SystemDef,
    system_id: &str,
    store: &RunStore,
    run_id: &str,
    dt_s: f64,
    t_end_s: f64,
    solver_version: &str,
    progress_cb: &mut Option<&mut dyn FnMut(RunProgressEvent)>,
    started: Instant,
    timing: &mut RunTimingSummary,
    strategy: InitializationStrategy,
) -> AppResult<RunManifest> {
    use crate::transient_compile::TransientNetworkModel;
    use tf_sim::{run_sim_with_progress, SimOptions};

    emit_progress(
        progress_cb,
        RunMode::Transient { dt_s, t_end_s },
        RunStage::CompilingRuntime,
        started,
        timing.initialization_strategy.clone(),
        Some("Compiling runtime".to_string()),
        None,
        None,
    );

    // Compile runtime
    let compile_started = Instant::now();
    let runtime = runtime_compile::compile_system(system)?;
    timing.compile_time_s = compile_started.elapsed().as_secs_f64();

    // Create transient model (strategy used internally for solver config)
    let build_started = Instant::now();
    let mut model = TransientNetworkModel::new(system, &runtime, strategy)?;
    timing.build_time_s = build_started.elapsed().as_secs_f64();

    emit_progress(
        progress_cb,
        RunMode::Transient { dt_s, t_end_s },
        RunStage::RunningTransient,
        started,
        timing.initialization_strategy.clone(),
        Some("Running transient simulation".to_string()),
        None,
        Some(TransientProgress {
            sim_time_s: 0.0,
            t_end_s,
            fraction_complete: 0.0,
            step: 0,
            cutback_retries: 0,
            fallback_uses: None,
        }),
    );

    // Run transient simulation
    let options = SimOptions {
        dt: dt_s,
        t_end: t_end_s,
        max_steps: 100_000,
        record_every: 1,
        integrator: tf_sim::IntegratorType::RK4,
        min_dt: (dt_s * 0.1).max(1.0e-6),
        max_retries: 8,
        cutback_factor: 0.5,
        grow_factor: 1.5,
    };

    let solve_started = Instant::now();
    let sim_record = run_sim_with_progress(
        &mut model,
        &options,
        Some(&mut |p| {
            emit_progress(
                progress_cb,
                RunMode::Transient { dt_s, t_end_s },
                RunStage::RunningTransient,
                started,
                Some(strategy.as_str().to_string()),
                Some(format!(
                    "Step {} | t={:.4}/{:.4} s | retries={} ",
                    p.step, p.sim_time, p.t_end, p.cutback_retries
                )),
                None,
                Some(TransientProgress {
                    sim_time_s: p.sim_time,
                    t_end_s: p.t_end,
                    fraction_complete: p.fraction_complete,
                    step: p.step,
                    cutback_retries: p.cutback_retries,
                    fallback_uses: None,
                }),
            )
        }),
    )?;
    timing.solve_time_s = solve_started.elapsed().as_secs_f64();
    timing.transient_steps = sim_record.steps;
    timing.transient_cutback_retries = sim_record.cutback_retries;
    timing.transient_real_fluid_attempts = model.real_fluid_attempts();
    timing.transient_real_fluid_successes = model.real_fluid_successes();
    timing.transient_surrogate_populations = model.surrogate_populations();
    // Phase 0 instrumentation: Wire up accumulated solver timing from transient model
    timing.solve_residual_time_s = model.solver_residual_time_s();
    timing.solve_jacobian_time_s = model.solver_jacobian_time_s();
    timing.solve_linearch_time_s = model.solver_linearch_time_s();
    timing.solve_thermo_time_s = model.solver_thermo_time_s();
    timing.solve_residual_eval_count = model.solver_residual_eval_count();
    timing.solve_jacobian_eval_count = model.solver_jacobian_eval_count();
    timing.solve_linearch_iter_count = model.solver_linearch_iter_count();
    timing.rhs_calls = model.rhs_calls();
    timing.rhs_snapshot_time_s = model.rhs_snapshot_time_s();
    timing.rhs_plan_check_time_s = model.rhs_plan_check_time_s();
    timing.rhs_component_rebuild_time_s = model.rhs_component_rebuild_time_s();
    timing.rhs_snapshot_structure_setup_time_s = model.rhs_snapshot_structure_setup_time_s();
    timing.rhs_boundary_hydration_time_s = model.rhs_boundary_hydration_time_s();
    timing.rhs_direct_solve_setup_time_s = model.rhs_direct_solve_setup_time_s();
    timing.rhs_result_unpack_time_s = model.rhs_result_unpack_time_s();
    timing.rhs_state_reconstruct_time_s = model.rhs_state_reconstruct_time_s();
    timing.rhs_buffer_init_time_s = model.rhs_buffer_init_time_s();
    timing.rhs_flow_routing_time_s = model.rhs_flow_routing_time_s();
    timing.rhs_cv_derivative_time_s = model.rhs_cv_derivative_time_s();
    timing.rhs_lv_derivative_time_s = model.rhs_lv_derivative_time_s();
    timing.rhs_assembly_time_s = model.rhs_assembly_time_s();
    timing.rhs_surrogate_time_s = model.rhs_surrogate_time_s();
    timing.execution_plan_checks = model.execution_plan_checks();
    timing.execution_plan_unchanged = model.execution_plan_unchanged();
    timing.component_rebuilds = model.component_rebuilds();
    timing.component_reuses = model.component_reuses();
    timing.snapshot_setup_rebuilds = model.snapshot_setup_rebuilds();
    timing.snapshot_setup_reuses = model.snapshot_setup_reuses();
    let rhs_accounted = timing.rhs_snapshot_time_s
        + timing.rhs_state_reconstruct_time_s
        + timing.rhs_buffer_init_time_s
        + timing.rhs_flow_routing_time_s
        + timing.rhs_cv_derivative_time_s
        + timing.rhs_lv_derivative_time_s
        + timing.rhs_assembly_time_s;
    timing.rk4_bookkeeping_time_s = (timing.solve_time_s - rhs_accounted).max(0.0);

    // Convert simulation records to timeseries records for storage
    let mut timeseries_records = Vec::with_capacity(sim_record.t.len());
    for (time, state) in sim_record.t.iter().zip(sim_record.x.iter()) {
        let ts_record = model.build_timeseries_record(*time, state)?;
        timeseries_records.push(ts_record);
    }

    timing.transient_fallback_uses = model.fallback_uses();

    model.print_diagnostics();

    // Build manifest
    let manifest = RunManifest {
        run_id: run_id.to_string(),
        system_id: system_id.to_string(),
        timestamp: chrono::Utc::now().to_rfc3339(),
        run_type: ResultsRunType::Transient {
            dt_s,
            t_end_s,
            steps: timeseries_records.len(),
        },
        solver_version: solver_version.to_string(),
    };

    emit_progress(
        progress_cb,
        RunMode::Transient { dt_s, t_end_s },
        RunStage::SavingResults,
        started,
        timing.initialization_strategy.clone(),
        Some("Saving run output".to_string()),
        None,
        Some(TransientProgress {
            sim_time_s: t_end_s,
            t_end_s,
            fraction_complete: 1.0,
            step: timing.transient_steps,
            cutback_retries: timing.transient_cutback_retries,
            fallback_uses: Some(timing.transient_fallback_uses),
        }),
    );

    // Save
    let save_started = Instant::now();
    store.save_run(&manifest, &timeseries_records)?;
    timing.save_time_s = save_started.elapsed().as_secs_f64();

    Ok(manifest)
}

/// Convert steady solution to timeseries record.
fn solution_to_timeseries(
    solution: &tf_solver::SteadySolution,
    runtime: &SystemRuntime,
) -> TimeseriesRecord {
    let mut node_values = Vec::with_capacity(runtime.node_id_map.len());
    for (node_id_str, &node_idx) in &runtime.node_id_map {
        if let Some(&p_val) = solution.pressures.get(node_idx.index() as usize) {
            let h_val = solution
                .enthalpies
                .get(node_idx.index() as usize)
                .copied()
                .unwrap_or_default();

            node_values.push(NodeValueSnapshot {
                node_id: node_id_str.clone(),
                p_pa: Some(p_val.value),
                t_k: None, // TODO: compute from P,h
                h_j_per_kg: Some(h_val),
                rho_kg_m3: None, // TODO: compute from P,h
            });
        }
    }

    let mass_flow_by_comp: std::collections::HashMap<_, _> =
        solution.mass_flows.iter().copied().collect();
    let mut edge_values = Vec::with_capacity(runtime.comp_id_map.len());
    for (comp_id_str, &comp_idx) in &runtime.comp_id_map {
        if let Some(mdot) = mass_flow_by_comp.get(&comp_idx) {
            edge_values.push(EdgeValueSnapshot {
                component_id: comp_id_str.clone(),
                mdot_kg_s: Some(*mdot),
                delta_p_pa: None,
            });
        }
    }

    TimeseriesRecord {
        time_s: 0.0,
        node_values,
        edge_values,
        global_values: GlobalValueSnapshot::default(),
    }
}

/// List runs for a system.
pub fn list_runs(project_path: &Path, system_id: &str) -> AppResult<Vec<RunManifest>> {
    let store = RunStore::for_project(project_path)?;

    let mut runs = store.list_runs(system_id)?;
    runs.sort_by(|a, b| b.timestamp.cmp(&a.timestamp)); // Most recent first
    Ok(runs)
}

/// Load a specific run.
pub fn load_run(
    project_path: &Path,
    run_id: &str,
) -> AppResult<(RunManifest, Vec<TimeseriesRecord>)> {
    let store = RunStore::for_project(project_path)?;

    let manifest = store.load_manifest(run_id)?;
    let records = store.load_timeseries(run_id)?;

    Ok((manifest, records))
}
