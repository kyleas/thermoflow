//! Run execution and caching service.

use std::path::Path;
use tf_project::schema::SystemDef;
use tf_results::{
    EdgeValueSnapshot, GlobalValueSnapshot, NodeValueSnapshot, RunManifest, RunStore,
    RunType as ResultsRunType, TimeseriesRecord,
};
use tf_solver::SteadyProblem;

use crate::error::{AppError, AppResult};
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
}

impl Default for RunOptions {
    fn default() -> Self {
        Self {
            use_cache: true,
            solver_version: "0.1.0".to_string(),
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

/// Response from a run execution.
#[derive(Debug, Clone)]
pub struct RunResponse {
    pub run_id: String,
    pub manifest: RunManifest,
    pub loaded_from_cache: bool,
}

/// Execute or load a run based on request.
pub fn ensure_run(request: &RunRequest) -> AppResult<RunResponse> {
    // Load project
    let project = project_service::load_project(request.project_path)?;
    let system = project_service::get_system(&project, request.system_id)?;

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
    let project_dir = request
        .project_path
        .parent()
        .ok_or_else(|| AppError::InvalidInput("Invalid project path".to_string()))?;
    let store = RunStore::new(project_dir.to_path_buf())?;

    // Check cache
    if request.options.use_cache && store.has_run(&run_id) {
        let manifest = store.load_manifest(&run_id)?;
        return Ok(RunResponse {
            run_id,
            manifest,
            loaded_from_cache: true,
        });
    }

    // Execute run
    let manifest = execute_run(
        system,
        request.system_id,
        &request.mode,
        &store,
        &run_id,
        &request.options.solver_version,
    )?;

    Ok(RunResponse {
        run_id,
        manifest,
        loaded_from_cache: false,
    })
}

/// Execute a run (steady or transient).
fn execute_run(
    system: &SystemDef,
    system_id: &str,
    mode: &RunMode,
    store: &RunStore,
    run_id: &str,
    solver_version: &str,
) -> AppResult<RunManifest> {
    match mode {
        RunMode::Steady => execute_steady(system, system_id, store, run_id, solver_version),
        RunMode::Transient { dt_s, t_end_s } => execute_transient(
            system,
            system_id,
            store,
            run_id,
            *dt_s,
            *t_end_s,
            solver_version,
        ),
    }
}

/// Execute steady-state simulation.
fn execute_steady(
    system: &SystemDef,
    system_id: &str,
    store: &RunStore,
    run_id: &str,
    solver_version: &str,
) -> AppResult<RunManifest> {
    // Compile runtime
    let runtime = runtime_compile::compile_system(system)?;
    let fluid_model = runtime_compile::build_fluid_model(&system.fluid)?;
    let boundaries = runtime_compile::parse_boundaries(&system.boundaries, &runtime.node_id_map)?;
    let components = runtime_compile::build_components(system, &runtime.comp_id_map)?;

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

    // Solve
    let solution = tf_solver::solve(&mut problem, None, None)?;

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

    // Save
    store.save_run(&manifest, &[record])?;

    Ok(manifest)
}

/// Execute transient simulation.
fn execute_transient(
    system: &SystemDef,
    system_id: &str,
    store: &RunStore,
    run_id: &str,
    dt_s: f64,
    t_end_s: f64,
    solver_version: &str,
) -> AppResult<RunManifest> {
    use crate::transient_compile::TransientNetworkModel;
    use tf_sim::SimOptions;

    // Compile runtime
    let runtime = runtime_compile::compile_system(system)?;

    // Create transient model
    let mut model = TransientNetworkModel::new(system, &runtime)?;

    // Run transient simulation
    let options = SimOptions {
        dt: dt_s,
        t_end: t_end_s,
        max_steps: 100_000,
        record_every: 1,
        integrator: tf_sim::IntegratorType::RK4,
    };

    let sim_record = tf_sim::run_sim(&mut model, &options)?;

    // Convert simulation records to timeseries records for storage
    let mut timeseries_records = Vec::new();
    for (time, state) in sim_record.t.iter().zip(sim_record.x.iter()) {
        let ts_record = model.build_timeseries_record(*time, state)?;
        timeseries_records.push(ts_record);
    }

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

    // Save
    store.save_run(&manifest, &timeseries_records)?;

    Ok(manifest)
}

/// Convert steady solution to timeseries record.
fn solution_to_timeseries(
    solution: &tf_solver::SteadySolution,
    runtime: &SystemRuntime,
) -> TimeseriesRecord {
    let mut node_values = Vec::new();
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

    let mut edge_values = Vec::new();
    for (comp_id_str, &comp_idx) in &runtime.comp_id_map {
        if let Some((_, mdot)) = solution.mass_flows.iter().find(|(id, _)| *id == comp_idx) {
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
    let project_dir = project_path
        .parent()
        .ok_or_else(|| AppError::InvalidInput("Invalid project path".to_string()))?;
    let store = RunStore::new(project_dir.to_path_buf())?;

    let mut runs = store.list_runs(system_id)?;
    runs.sort_by(|a, b| b.timestamp.cmp(&a.timestamp)); // Most recent first
    Ok(runs)
}

/// Load a specific run.
pub fn load_run(
    project_path: &Path,
    run_id: &str,
) -> AppResult<(RunManifest, Vec<TimeseriesRecord>)> {
    let project_dir = project_path
        .parent()
        .ok_or_else(|| AppError::InvalidInput("Invalid project path".to_string()))?;
    let store = RunStore::new(project_dir.to_path_buf())?;

    let manifest = store.load_manifest(run_id)?;
    let records = store.load_timeseries(run_id)?;

    Ok((manifest, records))
}
