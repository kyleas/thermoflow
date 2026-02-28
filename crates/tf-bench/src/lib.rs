//! Benchmark framework for Thermoflow supported workflows.

use serde::{Deserialize, Serialize};
use std::path::Path;
use std::time::Instant;
use tf_app::{ensure_run_with_progress, RunMode, RunOptions, RunRequest};

/// A benchmark scenario definition.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BenchmarkScenario {
    /// Unique identifier for this benchmark.
    pub id: String,
    /// Human-readable name.
    pub name: String,
    /// Path to the project YAML file (relative to repo root).
    pub project_path: String,
    /// System ID within the project.
    pub system_id: String,
    /// Run mode (steady or transient).
    pub mode: BenchmarkMode,
    /// Whether this example is officially supported.
    pub supported: bool,
    /// Notes about this benchmark.
    pub notes: Option<String>,
}

/// Run mode for benchmarks.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq)]
pub enum BenchmarkMode {
    Steady,
    Transient { dt_s: f64, t_end_s: f64 },
}

impl BenchmarkMode {
    pub fn to_run_mode(&self) -> RunMode {
        match self {
            BenchmarkMode::Steady => RunMode::Steady,
            BenchmarkMode::Transient { dt_s, t_end_s } => RunMode::Transient {
                dt_s: *dt_s,
                t_end_s: *t_end_s,
            },
        }
    }
}

/// A single run's timing breakdown.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RunMetrics {
    pub total_time_s: f64,
    pub compile_time_s: f64,
    pub build_time_s: f64,
    pub solve_time_s: f64,
    pub save_time_s: f64,
    // Fine-grained solver timing breakdown (Phase 0 instrumentation)
    pub solve_residual_time_s: Option<f64>,
    pub solve_jacobian_time_s: Option<f64>,
    pub solve_linearch_time_s: Option<f64>,
    pub solve_thermo_time_s: Option<f64>,
    pub solve_residual_eval_count: Option<usize>,
    pub solve_jacobian_eval_count: Option<usize>,
    pub solve_linearch_iter_count: Option<usize>,
    pub rhs_calls: Option<usize>,
    pub rhs_snapshot_time_s: Option<f64>,
    pub rhs_plan_check_time_s: Option<f64>,
    pub rhs_component_rebuild_time_s: Option<f64>,
    pub rhs_snapshot_structure_setup_time_s: Option<f64>,
    pub rhs_boundary_hydration_time_s: Option<f64>,
    pub rhs_direct_solve_setup_time_s: Option<f64>,
    pub rhs_result_unpack_time_s: Option<f64>,
    pub rhs_state_reconstruct_time_s: Option<f64>,
    pub rhs_buffer_init_time_s: Option<f64>,
    pub rhs_flow_routing_time_s: Option<f64>,
    pub rhs_cv_derivative_time_s: Option<f64>,
    pub rhs_lv_derivative_time_s: Option<f64>,
    pub rhs_assembly_time_s: Option<f64>,
    pub rhs_surrogate_time_s: Option<f64>,
    pub rk4_bookkeeping_time_s: Option<f64>,
    pub execution_plan_checks: Option<usize>,
    pub execution_plan_unchanged: Option<usize>,
    pub component_rebuilds: Option<usize>,
    pub component_reuses: Option<usize>,
    pub snapshot_setup_rebuilds: Option<usize>,
    pub snapshot_setup_reuses: Option<usize>,
    pub loaded_from_cache: bool,
    pub initialization_strategy: Option<String>,
    pub steady_iterations: Option<usize>,
    pub steady_residual_norm: Option<f64>,
    pub transient_steps: Option<usize>,
    pub transient_cutback_retries: Option<usize>,
    pub transient_fallback_uses: Option<usize>,
    pub transient_real_fluid_attempts: Option<usize>,
    pub transient_real_fluid_successes: Option<usize>,
    pub transient_surrogate_populations: Option<usize>,
}

/// Aggregated statistics for multiple runs.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AggregateMetrics {
    pub run_count: usize,
    pub total_time_median_s: f64,
    pub total_time_min_s: f64,
    pub total_time_max_s: f64,
    pub solve_time_median_s: f64,
    pub solve_time_min_s: f64,
    pub solve_time_max_s: f64,
    // Fine-grained solver timing aggregate (Phase 0 instrumentation)
    pub solve_residual_time_median_s: Option<f64>,
    pub solve_jacobian_time_median_s: Option<f64>,
    pub solve_linearch_time_median_s: Option<f64>,
    pub solve_thermo_time_median_s: Option<f64>,
    pub rhs_calls_median: Option<usize>,
    pub rhs_snapshot_time_median_s: Option<f64>,
    pub rhs_plan_check_time_median_s: Option<f64>,
    pub rhs_component_rebuild_time_median_s: Option<f64>,
    pub rhs_snapshot_structure_setup_time_median_s: Option<f64>,
    pub rhs_boundary_hydration_time_median_s: Option<f64>,
    pub rhs_direct_solve_setup_time_median_s: Option<f64>,
    pub rhs_result_unpack_time_median_s: Option<f64>,
    pub rhs_state_reconstruct_time_median_s: Option<f64>,
    pub rhs_buffer_init_time_median_s: Option<f64>,
    pub rhs_flow_routing_time_median_s: Option<f64>,
    pub rhs_cv_derivative_time_median_s: Option<f64>,
    pub rhs_lv_derivative_time_median_s: Option<f64>,
    pub rhs_assembly_time_median_s: Option<f64>,
    pub rhs_surrogate_time_median_s: Option<f64>,
    pub rk4_bookkeeping_time_median_s: Option<f64>,
    pub execution_plan_checks_median: Option<usize>,
    pub execution_plan_unchanged_median: Option<usize>,
    pub component_rebuilds_median: Option<usize>,
    pub component_reuses_median: Option<usize>,
    pub snapshot_setup_rebuilds_median: Option<usize>,
    pub snapshot_setup_reuses_median: Option<usize>,
    pub steady_iterations_median: Option<usize>,
    pub transient_steps_median: Option<usize>,
    pub transient_cutback_retries_total: usize,
    pub transient_fallback_uses_total: usize,
    pub transient_real_fluid_success_ratio: Option<f64>,
    pub initialization_strategy: Option<String>,
}

/// Complete benchmark result for a scenario.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BenchmarkResult {
    pub scenario: BenchmarkScenario,
    pub runs: Vec<RunMetrics>,
    pub aggregate: AggregateMetrics,
}

/// Collection of benchmark results.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BenchmarkSuite {
    pub timestamp: String,
    pub results: Vec<BenchmarkResult>,
}

/// Run a single benchmark scenario N times.
pub fn run_scenario(
    scenario: &BenchmarkScenario,
    times: usize,
    repo_root: &Path,
) -> Result<BenchmarkResult, Box<dyn std::error::Error>> {
    let project_path = repo_root.join(&scenario.project_path);

    let mut runs = Vec::new();

    for _run_idx in 0..times {
        let request = RunRequest {
            project_path: &project_path,
            system_id: &scenario.system_id,
            mode: scenario.mode.to_run_mode(),
            options: RunOptions {
                use_cache: false,
                solver_version: "0.1.0".to_string(),
                initialization_strategy: None,
            },
        };

        let _wall_start = Instant::now();
        let response = ensure_run_with_progress(&request, None::<&mut dyn FnMut(_)>)?;
        let _wall_elapsed = _wall_start.elapsed().as_secs_f64();

        let timing = &response.timing;
        let metrics = RunMetrics {
            total_time_s: timing.total_time_s,
            compile_time_s: timing.compile_time_s,
            build_time_s: timing.build_time_s,
            solve_time_s: timing.solve_time_s,
            save_time_s: timing.save_time_s,
            solve_residual_time_s: (timing.solve_residual_time_s > 0.0)
                .then_some(timing.solve_residual_time_s),
            solve_jacobian_time_s: (timing.solve_jacobian_time_s > 0.0)
                .then_some(timing.solve_jacobian_time_s),
            solve_linearch_time_s: (timing.solve_linearch_time_s > 0.0)
                .then_some(timing.solve_linearch_time_s),
            solve_thermo_time_s: (timing.solve_thermo_time_s > 0.0)
                .then_some(timing.solve_thermo_time_s),
            solve_residual_eval_count: (timing.solve_residual_eval_count > 0)
                .then_some(timing.solve_residual_eval_count),
            solve_jacobian_eval_count: (timing.solve_jacobian_eval_count > 0)
                .then_some(timing.solve_jacobian_eval_count),
            solve_linearch_iter_count: (timing.solve_linearch_iter_count > 0)
                .then_some(timing.solve_linearch_iter_count),
            rhs_calls: (timing.rhs_calls > 0).then_some(timing.rhs_calls),
            rhs_snapshot_time_s: (timing.rhs_snapshot_time_s > 0.0)
                .then_some(timing.rhs_snapshot_time_s),
            rhs_plan_check_time_s: (timing.rhs_plan_check_time_s > 0.0)
                .then_some(timing.rhs_plan_check_time_s),
            rhs_component_rebuild_time_s: (timing.rhs_component_rebuild_time_s > 0.0)
                .then_some(timing.rhs_component_rebuild_time_s),
            rhs_snapshot_structure_setup_time_s: (timing.rhs_snapshot_structure_setup_time_s > 0.0)
                .then_some(timing.rhs_snapshot_structure_setup_time_s),
            rhs_boundary_hydration_time_s: (timing.rhs_boundary_hydration_time_s > 0.0)
                .then_some(timing.rhs_boundary_hydration_time_s),
            rhs_direct_solve_setup_time_s: (timing.rhs_direct_solve_setup_time_s > 0.0)
                .then_some(timing.rhs_direct_solve_setup_time_s),
            rhs_result_unpack_time_s: (timing.rhs_result_unpack_time_s > 0.0)
                .then_some(timing.rhs_result_unpack_time_s),
            rhs_state_reconstruct_time_s: (timing.rhs_state_reconstruct_time_s > 0.0)
                .then_some(timing.rhs_state_reconstruct_time_s),
            rhs_buffer_init_time_s: (timing.rhs_buffer_init_time_s > 0.0)
                .then_some(timing.rhs_buffer_init_time_s),
            rhs_flow_routing_time_s: (timing.rhs_flow_routing_time_s > 0.0)
                .then_some(timing.rhs_flow_routing_time_s),
            rhs_cv_derivative_time_s: (timing.rhs_cv_derivative_time_s > 0.0)
                .then_some(timing.rhs_cv_derivative_time_s),
            rhs_lv_derivative_time_s: (timing.rhs_lv_derivative_time_s > 0.0)
                .then_some(timing.rhs_lv_derivative_time_s),
            rhs_assembly_time_s: (timing.rhs_assembly_time_s > 0.0)
                .then_some(timing.rhs_assembly_time_s),
            rhs_surrogate_time_s: (timing.rhs_surrogate_time_s > 0.0)
                .then_some(timing.rhs_surrogate_time_s),
            rk4_bookkeeping_time_s: (timing.rk4_bookkeeping_time_s > 0.0)
                .then_some(timing.rk4_bookkeeping_time_s),
            execution_plan_checks: (timing.execution_plan_checks > 0)
                .then_some(timing.execution_plan_checks),
            execution_plan_unchanged: (timing.execution_plan_unchanged > 0)
                .then_some(timing.execution_plan_unchanged),
            component_rebuilds: (timing.component_rebuilds > 0)
                .then_some(timing.component_rebuilds),
            component_reuses: (timing.component_reuses > 0).then_some(timing.component_reuses),
            snapshot_setup_rebuilds: (timing.snapshot_setup_rebuilds > 0)
                .then_some(timing.snapshot_setup_rebuilds),
            snapshot_setup_reuses: (timing.snapshot_setup_reuses > 0)
                .then_some(timing.snapshot_setup_reuses),
            loaded_from_cache: response.loaded_from_cache,
            initialization_strategy: timing.initialization_strategy.clone(),
            steady_iterations: (timing.steady_iterations > 0).then_some(timing.steady_iterations),
            steady_residual_norm: (timing.steady_residual_norm > 0.0)
                .then_some(timing.steady_residual_norm),
            transient_steps: (timing.transient_steps > 0).then_some(timing.transient_steps),
            transient_cutback_retries: (timing.transient_cutback_retries > 0)
                .then_some(timing.transient_cutback_retries),
            transient_fallback_uses: (timing.transient_fallback_uses > 0)
                .then_some(timing.transient_fallback_uses),
            transient_real_fluid_attempts: (timing.transient_real_fluid_attempts > 0)
                .then_some(timing.transient_real_fluid_attempts),
            transient_real_fluid_successes: (timing.transient_real_fluid_successes > 0)
                .then_some(timing.transient_real_fluid_successes),
            transient_surrogate_populations: (timing.transient_surrogate_populations > 0)
                .then_some(timing.transient_surrogate_populations),
        };

        runs.push(metrics);
    }

    let aggregate = compute_aggregates(&runs, scenario);

    Ok(BenchmarkResult {
        scenario: scenario.clone(),
        runs,
        aggregate,
    })
}

fn compute_aggregates(runs: &[RunMetrics], _scenario: &BenchmarkScenario) -> AggregateMetrics {
    if runs.is_empty() {
        return AggregateMetrics {
            run_count: 0,
            total_time_median_s: 0.0,
            total_time_min_s: 0.0,
            total_time_max_s: 0.0,
            solve_time_median_s: 0.0,
            solve_time_min_s: 0.0,
            solve_time_max_s: 0.0,
            solve_residual_time_median_s: None,
            solve_jacobian_time_median_s: None,
            solve_linearch_time_median_s: None,
            solve_thermo_time_median_s: None,
            rhs_calls_median: None,
            rhs_snapshot_time_median_s: None,
            rhs_plan_check_time_median_s: None,
            rhs_component_rebuild_time_median_s: None,
            rhs_snapshot_structure_setup_time_median_s: None,
            rhs_boundary_hydration_time_median_s: None,
            rhs_direct_solve_setup_time_median_s: None,
            rhs_result_unpack_time_median_s: None,
            rhs_state_reconstruct_time_median_s: None,
            rhs_buffer_init_time_median_s: None,
            rhs_flow_routing_time_median_s: None,
            rhs_cv_derivative_time_median_s: None,
            rhs_lv_derivative_time_median_s: None,
            rhs_assembly_time_median_s: None,
            rhs_surrogate_time_median_s: None,
            rk4_bookkeeping_time_median_s: None,
            execution_plan_checks_median: None,
            execution_plan_unchanged_median: None,
            component_rebuilds_median: None,
            component_reuses_median: None,
            snapshot_setup_rebuilds_median: None,
            snapshot_setup_reuses_median: None,
            steady_iterations_median: None,
            transient_steps_median: None,
            transient_cutback_retries_total: 0,
            transient_fallback_uses_total: 0,
            transient_real_fluid_success_ratio: None,
            initialization_strategy: None,
        };
    }

    let mut total_times: Vec<_> = runs.iter().map(|r| r.total_time_s).collect();
    let mut solve_times: Vec<_> = runs.iter().map(|r| r.solve_time_s).collect();
    total_times.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));
    solve_times.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));

    let total_time_median = total_times[total_times.len() / 2];
    let solve_time_median = solve_times[solve_times.len() / 2];

    // Compute fine-grained timing medians (Phase 0 instrumentation)
    let mut solve_residual_times: Vec<_> = runs
        .iter()
        .filter_map(|r| r.solve_residual_time_s)
        .collect();
    solve_residual_times.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));
    let solve_residual_time_median = solve_residual_times
        .get(solve_residual_times.len() / 2)
        .copied();

    let mut solve_jacobian_times: Vec<_> = runs
        .iter()
        .filter_map(|r| r.solve_jacobian_time_s)
        .collect();
    solve_jacobian_times.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));
    let solve_jacobian_time_median = solve_jacobian_times
        .get(solve_jacobian_times.len() / 2)
        .copied();

    let mut solve_linearch_times: Vec<_> = runs
        .iter()
        .filter_map(|r| r.solve_linearch_time_s)
        .collect();
    solve_linearch_times.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));
    let solve_linearch_time_median = solve_linearch_times
        .get(solve_linearch_times.len() / 2)
        .copied();

    let mut solve_thermo_times: Vec<_> =
        runs.iter().filter_map(|r| r.solve_thermo_time_s).collect();
    solve_thermo_times.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));
    let solve_thermo_time_median = solve_thermo_times
        .get(solve_thermo_times.len() / 2)
        .copied();

    let mut rhs_calls: Vec<_> = runs.iter().filter_map(|r| r.rhs_calls).collect();
    rhs_calls.sort_unstable();
    let rhs_calls_median = rhs_calls.get(rhs_calls.len() / 2).copied();

    let mut rhs_snapshot_times: Vec<_> =
        runs.iter().filter_map(|r| r.rhs_snapshot_time_s).collect();
    rhs_snapshot_times.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));
    let rhs_snapshot_time_median = rhs_snapshot_times
        .get(rhs_snapshot_times.len() / 2)
        .copied();

    let mut rhs_plan_check_times: Vec<_> = runs
        .iter()
        .filter_map(|r| r.rhs_plan_check_time_s)
        .collect();
    rhs_plan_check_times.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));
    let rhs_plan_check_time_median = rhs_plan_check_times
        .get(rhs_plan_check_times.len() / 2)
        .copied();

    let mut rhs_component_rebuild_times: Vec<_> = runs
        .iter()
        .filter_map(|r| r.rhs_component_rebuild_time_s)
        .collect();
    rhs_component_rebuild_times
        .sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));
    let rhs_component_rebuild_time_median = rhs_component_rebuild_times
        .get(rhs_component_rebuild_times.len() / 2)
        .copied();

    let mut rhs_snapshot_structure_setup_times: Vec<_> = runs
        .iter()
        .filter_map(|r| r.rhs_snapshot_structure_setup_time_s)
        .collect();
    rhs_snapshot_structure_setup_times
        .sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));
    let rhs_snapshot_structure_setup_time_median = rhs_snapshot_structure_setup_times
        .get(rhs_snapshot_structure_setup_times.len() / 2)
        .copied();

    let mut rhs_boundary_hydration_times: Vec<_> = runs
        .iter()
        .filter_map(|r| r.rhs_boundary_hydration_time_s)
        .collect();
    rhs_boundary_hydration_times
        .sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));
    let rhs_boundary_hydration_time_median = rhs_boundary_hydration_times
        .get(rhs_boundary_hydration_times.len() / 2)
        .copied();

    let mut rhs_direct_solve_setup_times: Vec<_> = runs
        .iter()
        .filter_map(|r| r.rhs_direct_solve_setup_time_s)
        .collect();
    rhs_direct_solve_setup_times
        .sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));
    let rhs_direct_solve_setup_time_median = rhs_direct_solve_setup_times
        .get(rhs_direct_solve_setup_times.len() / 2)
        .copied();

    let mut rhs_result_unpack_times: Vec<_> = runs
        .iter()
        .filter_map(|r| r.rhs_result_unpack_time_s)
        .collect();
    rhs_result_unpack_times.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));
    let rhs_result_unpack_time_median = rhs_result_unpack_times
        .get(rhs_result_unpack_times.len() / 2)
        .copied();

    let mut rhs_state_reconstruct_times: Vec<_> = runs
        .iter()
        .filter_map(|r| r.rhs_state_reconstruct_time_s)
        .collect();
    rhs_state_reconstruct_times
        .sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));
    let rhs_state_reconstruct_time_median = rhs_state_reconstruct_times
        .get(rhs_state_reconstruct_times.len() / 2)
        .copied();

    let mut rhs_buffer_init_times: Vec<_> = runs
        .iter()
        .filter_map(|r| r.rhs_buffer_init_time_s)
        .collect();
    rhs_buffer_init_times.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));
    let rhs_buffer_init_time_median = rhs_buffer_init_times
        .get(rhs_buffer_init_times.len() / 2)
        .copied();

    let mut rhs_flow_routing_times: Vec<_> = runs
        .iter()
        .filter_map(|r| r.rhs_flow_routing_time_s)
        .collect();
    rhs_flow_routing_times.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));
    let rhs_flow_routing_time_median = rhs_flow_routing_times
        .get(rhs_flow_routing_times.len() / 2)
        .copied();

    let mut rhs_cv_derivative_times: Vec<_> = runs
        .iter()
        .filter_map(|r| r.rhs_cv_derivative_time_s)
        .collect();
    rhs_cv_derivative_times.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));
    let rhs_cv_derivative_time_median = rhs_cv_derivative_times
        .get(rhs_cv_derivative_times.len() / 2)
        .copied();

    let mut rhs_lv_derivative_times: Vec<_> = runs
        .iter()
        .filter_map(|r| r.rhs_lv_derivative_time_s)
        .collect();
    rhs_lv_derivative_times.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));
    let rhs_lv_derivative_time_median = rhs_lv_derivative_times
        .get(rhs_lv_derivative_times.len() / 2)
        .copied();

    let mut rhs_assembly_times: Vec<_> =
        runs.iter().filter_map(|r| r.rhs_assembly_time_s).collect();
    rhs_assembly_times.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));
    let rhs_assembly_time_median = rhs_assembly_times
        .get(rhs_assembly_times.len() / 2)
        .copied();

    let mut rhs_surrogate_times: Vec<_> =
        runs.iter().filter_map(|r| r.rhs_surrogate_time_s).collect();
    rhs_surrogate_times.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));
    let rhs_surrogate_time_median = rhs_surrogate_times
        .get(rhs_surrogate_times.len() / 2)
        .copied();

    let mut rk4_bookkeeping_times: Vec<_> = runs
        .iter()
        .filter_map(|r| r.rk4_bookkeeping_time_s)
        .collect();
    rk4_bookkeeping_times.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));
    let rk4_bookkeeping_time_median = rk4_bookkeeping_times
        .get(rk4_bookkeeping_times.len() / 2)
        .copied();

    let mut execution_plan_checks: Vec<_> = runs
        .iter()
        .filter_map(|r| r.execution_plan_checks)
        .collect();
    execution_plan_checks.sort_unstable();
    let execution_plan_checks_median = execution_plan_checks
        .get(execution_plan_checks.len() / 2)
        .copied();

    let mut execution_plan_unchanged: Vec<_> = runs
        .iter()
        .filter_map(|r| r.execution_plan_unchanged)
        .collect();
    execution_plan_unchanged.sort_unstable();
    let execution_plan_unchanged_median = execution_plan_unchanged
        .get(execution_plan_unchanged.len() / 2)
        .copied();

    let mut component_rebuilds: Vec<_> = runs.iter().filter_map(|r| r.component_rebuilds).collect();
    component_rebuilds.sort_unstable();
    let component_rebuilds_median = component_rebuilds
        .get(component_rebuilds.len() / 2)
        .copied();

    let mut component_reuses: Vec<_> = runs.iter().filter_map(|r| r.component_reuses).collect();
    component_reuses.sort_unstable();
    let component_reuses_median = component_reuses.get(component_reuses.len() / 2).copied();

    let mut snapshot_setup_rebuilds: Vec<_> = runs
        .iter()
        .filter_map(|r| r.snapshot_setup_rebuilds)
        .collect();
    snapshot_setup_rebuilds.sort_unstable();
    let snapshot_setup_rebuilds_median = snapshot_setup_rebuilds
        .get(snapshot_setup_rebuilds.len() / 2)
        .copied();

    let mut snapshot_setup_reuses: Vec<_> = runs
        .iter()
        .filter_map(|r| r.snapshot_setup_reuses)
        .collect();
    snapshot_setup_reuses.sort_unstable();
    let snapshot_setup_reuses_median = snapshot_setup_reuses
        .get(snapshot_setup_reuses.len() / 2)
        .copied();

    let mut steady_iters: Vec<_> = runs.iter().filter_map(|r| r.steady_iterations).collect();
    steady_iters.sort_unstable();

    let mut transient_steps: Vec<_> = runs.iter().filter_map(|r| r.transient_steps).collect();
    transient_steps.sort_unstable();

    let cutback_total: usize = runs
        .iter()
        .filter_map(|r| r.transient_cutback_retries)
        .sum();
    let fallback_total: usize = runs.iter().filter_map(|r| r.transient_fallback_uses).sum();

    let real_fluid_success_ratio = {
        let attempts: usize = runs
            .iter()
            .filter_map(|r| r.transient_real_fluid_attempts)
            .sum();
        let successes: usize = runs
            .iter()
            .filter_map(|r| r.transient_real_fluid_successes)
            .sum();
        if attempts > 0 {
            Some(successes as f64 / attempts as f64)
        } else {
            None
        }
    };

    let init_strategy = runs.first().and_then(|r| r.initialization_strategy.clone());

    AggregateMetrics {
        run_count: runs.len(),
        total_time_median_s: total_time_median,
        total_time_min_s: *total_times.first().unwrap_or(&0.0),
        total_time_max_s: *total_times.last().unwrap_or(&0.0),
        solve_time_median_s: solve_time_median,
        solve_time_min_s: *solve_times.first().unwrap_or(&0.0),
        solve_time_max_s: *solve_times.last().unwrap_or(&0.0),
        solve_residual_time_median_s: solve_residual_time_median,
        solve_jacobian_time_median_s: solve_jacobian_time_median,
        solve_linearch_time_median_s: solve_linearch_time_median,
        solve_thermo_time_median_s: solve_thermo_time_median,
        rhs_calls_median,
        rhs_snapshot_time_median_s: rhs_snapshot_time_median,
        rhs_plan_check_time_median_s: rhs_plan_check_time_median,
        rhs_component_rebuild_time_median_s: rhs_component_rebuild_time_median,
        rhs_snapshot_structure_setup_time_median_s: rhs_snapshot_structure_setup_time_median,
        rhs_boundary_hydration_time_median_s: rhs_boundary_hydration_time_median,
        rhs_direct_solve_setup_time_median_s: rhs_direct_solve_setup_time_median,
        rhs_result_unpack_time_median_s: rhs_result_unpack_time_median,
        rhs_state_reconstruct_time_median_s: rhs_state_reconstruct_time_median,
        rhs_buffer_init_time_median_s: rhs_buffer_init_time_median,
        rhs_flow_routing_time_median_s: rhs_flow_routing_time_median,
        rhs_cv_derivative_time_median_s: rhs_cv_derivative_time_median,
        rhs_lv_derivative_time_median_s: rhs_lv_derivative_time_median,
        rhs_assembly_time_median_s: rhs_assembly_time_median,
        rhs_surrogate_time_median_s: rhs_surrogate_time_median,
        rk4_bookkeeping_time_median_s: rk4_bookkeeping_time_median,
        execution_plan_checks_median,
        execution_plan_unchanged_median,
        component_rebuilds_median,
        component_reuses_median,
        snapshot_setup_rebuilds_median,
        snapshot_setup_reuses_median,
        steady_iterations_median: steady_iters.get(steady_iters.len() / 2).copied(),
        transient_steps_median: transient_steps.get(transient_steps.len() / 2).copied(),
        transient_cutback_retries_total: cutback_total,
        transient_fallback_uses_total: fallback_total,
        transient_real_fluid_success_ratio: real_fluid_success_ratio,
        initialization_strategy: init_strategy,
    }
}

/// Default set of supported benchmark scenarios.
pub fn default_benchmarks() -> Vec<BenchmarkScenario> {
    vec![
        BenchmarkScenario {
            id: "01_steady".to_string(),
            name: "Orifice Steady-State".to_string(),
            project_path: "examples/projects/01_orifice_steady.yaml".to_string(),
            system_id: "s1".to_string(),
            mode: BenchmarkMode::Steady,
            supported: true,
            notes: Some("Simple orifice discharge; single-iteration baseline".to_string()),
        },
        BenchmarkScenario {
            id: "03_transient".to_string(),
            name: "Simple Vent Transient".to_string(),
            project_path: "examples/projects/03_simple_vent_transient.yaml".to_string(),
            system_id: "s1".to_string(),
            mode: BenchmarkMode::Transient {
                dt_s: 0.1,
                t_end_s: 1.0,
            },
            supported: true,
            notes: Some("Single CV venting; 100% real-fluid baseline".to_string()),
        },
        BenchmarkScenario {
            id: "04_transient".to_string(),
            name: "Two-CV Series Vent Transient".to_string(),
            project_path: "examples/projects/04_two_cv_series_vent_transient.yaml".to_string(),
            system_id: "s1".to_string(),
            mode: BenchmarkMode::Transient {
                dt_s: 0.1,
                t_end_s: 1.0,
            },
            supported: true,
            notes: Some("Two CVs in series; fixed topology".to_string()),
        },
        BenchmarkScenario {
            id: "05_transient".to_string(),
            name: "Two-CV Pipe Vent Transient".to_string(),
            project_path: "examples/projects/05_two_cv_pipe_vent_transient.yaml".to_string(),
            system_id: "s1".to_string(),
            mode: BenchmarkMode::Transient {
                dt_s: 0.1,
                t_end_s: 1.0,
            },
            supported: true,
            notes: Some("Tank + buffer with pipe; fixed topology".to_string()),
        },
        BenchmarkScenario {
            id: "07_transient".to_string(),
            name: "LineVolume Simple Vent".to_string(),
            project_path: "examples/projects/07_linevolume_buffered_vent.yaml".to_string(),
            system_id: "s1".to_string(),
            mode: BenchmarkMode::Transient {
                dt_s: 0.1,
                t_end_s: 1.0,
            },
            supported: true,
            notes: Some("Demonstrates LineVolume component".to_string()),
        },
        BenchmarkScenario {
            id: "08_transient".to_string(),
            name: "Two-CV LineVolume System".to_string(),
            project_path: "examples/projects/08_linevolume_two_cv.yaml".to_string(),
            system_id: "s1".to_string(),
            mode: BenchmarkMode::Transient {
                dt_s: 0.1,
                t_end_s: 1.0,
            },
            supported: true,
            notes: Some("Two CVs connected via LineVolume".to_string()),
        },
    ]
}

impl Default for RunMetrics {
    fn default() -> Self {
        RunMetrics {
            total_time_s: 0.0,
            compile_time_s: 0.0,
            build_time_s: 0.0,
            solve_time_s: 0.0,
            save_time_s: 0.0,
            solve_residual_time_s: None,
            solve_jacobian_time_s: None,
            solve_linearch_time_s: None,
            solve_thermo_time_s: None,
            solve_residual_eval_count: None,
            solve_jacobian_eval_count: None,
            solve_linearch_iter_count: None,
            rhs_calls: None,
            rhs_snapshot_time_s: None,
            rhs_plan_check_time_s: None,
            rhs_component_rebuild_time_s: None,
            rhs_snapshot_structure_setup_time_s: None,
            rhs_boundary_hydration_time_s: None,
            rhs_direct_solve_setup_time_s: None,
            rhs_result_unpack_time_s: None,
            rhs_state_reconstruct_time_s: None,
            rhs_buffer_init_time_s: None,
            rhs_flow_routing_time_s: None,
            rhs_cv_derivative_time_s: None,
            rhs_lv_derivative_time_s: None,
            rhs_assembly_time_s: None,
            rhs_surrogate_time_s: None,
            rk4_bookkeeping_time_s: None,
            execution_plan_checks: None,
            execution_plan_unchanged: None,
            component_rebuilds: None,
            component_reuses: None,
            snapshot_setup_rebuilds: None,
            snapshot_setup_reuses: None,
            loaded_from_cache: false,
            initialization_strategy: None,
            steady_iterations: None,
            steady_residual_norm: None,
            transient_steps: None,
            transient_cutback_retries: None,
            transient_fallback_uses: None,
            transient_real_fluid_attempts: None,
            transient_real_fluid_successes: None,
            transient_surrogate_populations: None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_benchmarks_are_defined() {
        let benchmarks = default_benchmarks();
        assert!(!benchmarks.is_empty());
        assert!(benchmarks.iter().all(|b| !b.id.is_empty()));
        assert!(benchmarks.iter().all(|b| !b.name.is_empty()));
    }

    #[test]
    fn all_default_benchmarks_are_supported() {
        let benchmarks = default_benchmarks();
        assert!(benchmarks.iter().all(|b| b.supported));
    }

    #[test]
    fn benchmark_scenario_serializes() {
        let scenario = BenchmarkScenario {
            id: "test".to_string(),
            name: "Test Scenario".to_string(),
            project_path: "examples/projects/test.yaml".to_string(),
            system_id: "s1".to_string(),
            mode: BenchmarkMode::Steady,
            supported: true,
            notes: Some("Test note".to_string()),
        };

        let json = serde_json::to_string(&scenario).expect("should serialize");
        let deserialized: BenchmarkScenario =
            serde_json::from_str(&json).expect("should deserialize");

        assert_eq!(deserialized.id, scenario.id);
        assert_eq!(deserialized.name, scenario.name);
    }

    #[test]
    fn benchmark_result_serializes() {
        let scenario = BenchmarkScenario {
            id: "test".to_string(),
            name: "Test".to_string(),
            project_path: "test.yaml".to_string(),
            system_id: "s1".to_string(),
            mode: BenchmarkMode::Steady,
            supported: true,
            notes: None,
        };

        let metrics = RunMetrics {
            total_time_s: 1.0,
            compile_time_s: 0.1,
            build_time_s: 0.05,
            solve_time_s: 0.8,
            save_time_s: 0.05,
            solve_residual_time_s: None,
            solve_jacobian_time_s: None,
            solve_linearch_time_s: None,
            solve_thermo_time_s: None,
            solve_residual_eval_count: None,
            solve_jacobian_eval_count: None,
            solve_linearch_iter_count: None,
            rhs_calls: None,
            rhs_snapshot_time_s: None,
            rhs_plan_check_time_s: None,
            rhs_component_rebuild_time_s: None,
            rhs_snapshot_structure_setup_time_s: None,
            rhs_boundary_hydration_time_s: None,
            rhs_direct_solve_setup_time_s: None,
            rhs_result_unpack_time_s: None,
            rhs_state_reconstruct_time_s: None,
            rhs_buffer_init_time_s: None,
            rhs_flow_routing_time_s: None,
            rhs_cv_derivative_time_s: None,
            rhs_lv_derivative_time_s: None,
            rhs_assembly_time_s: None,
            rhs_surrogate_time_s: None,
            rk4_bookkeeping_time_s: None,
            execution_plan_checks: None,
            execution_plan_unchanged: None,
            component_rebuilds: None,
            component_reuses: None,
            snapshot_setup_rebuilds: None,
            snapshot_setup_reuses: None,
            loaded_from_cache: false,
            initialization_strategy: Some("Strict".to_string()),
            steady_iterations: Some(5),
            steady_residual_norm: Some(1e-6),
            transient_steps: None,
            transient_cutback_retries: None,
            transient_fallback_uses: None,
            transient_real_fluid_attempts: None,
            transient_real_fluid_successes: None,
            transient_surrogate_populations: None,
        };

        let result = BenchmarkResult {
            scenario,
            runs: vec![metrics.clone()],
            aggregate: compute_aggregates(
                &[metrics],
                &BenchmarkScenario {
                    id: "test".to_string(),
                    name: "Test".to_string(),
                    project_path: "test.yaml".to_string(),
                    system_id: "s1".to_string(),
                    mode: BenchmarkMode::Steady,
                    supported: true,
                    notes: None,
                },
            ),
        };

        let json = serde_json::to_string(&result).expect("should serialize");
        let _deserialized: BenchmarkResult =
            serde_json::from_str(&json).expect("should deserialize");
    }

    #[test]
    fn aggregate_metrics_compute_correctly() {
        let metrics = vec![
            RunMetrics {
                total_time_s: 1.0,
                solve_time_s: 0.8,
                ..Default::default()
            },
            RunMetrics {
                total_time_s: 2.0,
                solve_time_s: 1.6,
                ..Default::default()
            },
            RunMetrics {
                total_time_s: 3.0,
                solve_time_s: 2.4,
                ..Default::default()
            },
        ];

        let scenario = BenchmarkScenario {
            id: "test".to_string(),
            name: "Test".to_string(),
            project_path: "test.yaml".to_string(),
            system_id: "s1".to_string(),
            mode: BenchmarkMode::Steady,
            supported: true,
            notes: None,
        };

        let agg = compute_aggregates(&metrics, &scenario);

        assert_eq!(agg.run_count, 3);
        assert_eq!(agg.total_time_median_s, 2.0); // middle value
        assert_eq!(agg.total_time_min_s, 1.0);
        assert_eq!(agg.total_time_max_s, 3.0);
        assert_eq!(agg.solve_time_median_s, 1.6);
    }

    #[test]
    fn aggregate_handles_empty_runs() {
        let scenario = BenchmarkScenario {
            id: "test".to_string(),
            name: "Test".to_string(),
            project_path: "test.yaml".to_string(),
            system_id: "s1".to_string(),
            mode: BenchmarkMode::Steady,
            supported: true,
            notes: None,
        };

        let agg = compute_aggregates(&[], &scenario);

        assert_eq!(agg.run_count, 0);
        assert_eq!(agg.total_time_median_s, 0.0);
    }

    #[test]
    fn transient_metrics_aggregate_correctly() {
        let metrics = vec![
            RunMetrics {
                total_time_s: 5.0,
                transient_steps: Some(10),
                transient_cutback_retries: Some(2),
                transient_fallback_uses: Some(0),
                transient_real_fluid_attempts: Some(100),
                transient_real_fluid_successes: Some(100),
                ..Default::default()
            },
            RunMetrics {
                total_time_s: 5.1,
                transient_steps: Some(10),
                transient_cutback_retries: Some(1),
                transient_fallback_uses: Some(0),
                transient_real_fluid_attempts: Some(100),
                transient_real_fluid_successes: Some(100),
                ..Default::default()
            },
        ];

        let scenario = BenchmarkScenario {
            id: "test".to_string(),
            name: "Test".to_string(),
            project_path: "test.yaml".to_string(),
            system_id: "s1".to_string(),
            mode: BenchmarkMode::Transient {
                dt_s: 0.1,
                t_end_s: 1.0,
            },
            supported: true,
            notes: None,
        };

        let agg = compute_aggregates(&metrics, &scenario);

        assert_eq!(agg.transient_steps_median, Some(10));
        assert_eq!(agg.transient_cutback_retries_total, 3); // 2 + 1
        assert_eq!(agg.transient_fallback_uses_total, 0);
        assert_eq!(agg.transient_real_fluid_success_ratio, Some(1.0)); // 200/200
    }

    #[test]
    fn benchmark_suite_serializes() {
        let suite = BenchmarkSuite {
            timestamp: "test_timestamp".to_string(),
            results: vec![],
        };

        let json = serde_json::to_string(&suite).expect("should serialize");
        let deserialized: BenchmarkSuite = serde_json::from_str(&json).expect("should deserialize");

        assert_eq!(deserialized.timestamp, suite.timestamp);
        assert_eq!(deserialized.results.len(), 0);
    }
}
