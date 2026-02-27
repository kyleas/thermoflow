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

impl Default for RunMetrics {
    fn default() -> Self {
        RunMetrics {
            total_time_s: 0.0,
            compile_time_s: 0.0,
            build_time_s: 0.0,
            solve_time_s: 0.0,
            save_time_s: 0.0,
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
