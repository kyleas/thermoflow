# Thermoflow Benchmark Suite

This directory contains performance baselines and the benchmark runner for Thermoflow's supported workflows.

## Quick Start

### Run Benchmarks

```bash
cargo build -p tf-bench --release
target/release/tf-bench.exe
```

This will:
1. Run each supported example 5 times
2. Collect timing metrics (compile, build, solve, save)
3. Print a human-readable summary to the terminal
4. Save detailed results to `baseline.json`

### View Results

After running, results are saved in machine-readable JSON format:

```bash
cat benchmarks/baseline.json | jq '.results[0].aggregate'
```

To see a readable summary, check the terminal output:
```
Orifice Steady-State
  Mode: Steady
  Total time:  0.0144s (median), min: 0.0134s, max: 0.0156s
  Solve time:  0.0125s (median), min: 0.0121s, max: 0.0145s
  Iterations:  0 (median)
  Init strategy: Strict
```

## Understanding Results

### Metrics Captured

Per-run metrics (per `runs[].` in JSON):
- `total_time_s` — Wall-clock time from start to finish
- `compile_time_s` — Runtime compilation
- `build_time_s` — Building the problem
- `solve_time_s` — Solving the problem
- `save_time_s` — Saving results to cache
- `loaded_from_cache` — Whether cache was hit (should be false for benchmarks)
- `initialization_strategy` — Strict or Relaxed
- `steady_iterations` — Newton iterations (steady-state)
- `transient_steps` — Integration steps (transient)
- `transient_cutback_retries` — How many times step size was reduced
- `transient_fallback_uses` — How many times surrogate was used
- `transient_real_fluid_success_ratio` — % of successful CoolProp calls

Aggregates (per `results[].aggregate`):
- `run_count` — Number of runs
- `{total, solve}_time_{median, min, max}_s` — Distribution statistics
- `transient_*_total` — Summed counts across all runs
- `transient_real_fluid_success_ratio` — Overall success %

### Interpretation

**Total time** is the main comparison metric. Smaller = faster.

**Solve time** is usually the bottleneck (solve phase dominates).

**Real-fluid success ratio** should be 100% for supported examples. If < 100%, surrogate fallback was activated (thermodynamic inversion failed, but simulation continued).

**Cutback retries** = 0 for stable runs. > 0 suggests integration difficulty.

## Benchmark Scenarios

| ID | Name | Mode | Supported |
|----|------|------|-----------|
| 01_steady | Orifice Steady-State | Steady | ✅ |
| 03_transient | Simple Vent Transient | Transient | ✅ |
| 04_transient | Two-CV Series Vent | Transient | ✅ |
| 05_transient | Two-CV Pipe Vent | Transient | ✅ |
| 07_transient | LineVolume Simple Vent | Transient | ✅ |
| 08_transient | Two-CV LineVolume System | Transient | ✅ |

See `docs/CURRENT_STATE_AUDIT.md` for detailed support matrix.

## Comparing Runs

### Manual Comparison

To compare current results to a saved baseline:

```bash
# Get current baseline
jq '.results[] | {id: .scenario.id, median: .aggregate.total_time_median_s}' \
  benchmarks/baseline.json

# Run new benchmark and diff
target/release/tf-bench.exe
# Compare printed output to previous baseline.json
```

### Automated Comparison

Use `benchmarks/compare.json` utility (if implemented) to programmatically diff:

```bash
# Compare new run to baseline
cargo run -p tf-bench -- --compare benchmarks/baseline.json
```

(This feature is planned for Phase 5.)

## Notes

- **Release mode**: Benchmarks compile with `--release` for realistic timing
- **No cache**: All runs use `--no-cache` to force full computation
- **Five runs**: Median reported to be robust to startup noise
- **Same-machine comparison**: Results are most meaningful when compared on the same hardware/OS configuration

## Adding New Benchmarks

To add a new benchmark scenario:

1. Add an example project to `examples/projects/`
2. Update `crates/tf-bench/src/lib.rs::default_benchmarks()`
3. Add entry to the BenchmarkScenario vec
4. Run `target/release/tf-bench.exe` to generate new baseline

Example:
```rust
BenchmarkScenario {
    id: "09_custom".to_string(),
    name: "My Custom Example".to_string(),
    project_path: "examples/projects/09_my_example.yaml".to_string(),
    system_id: "s1".to_string(),
    mode: BenchmarkMode::Transient { dt_s: 0.1, t_end_s: 1.0 },
    supported: true,
    notes: Some("Description of this benchmark".to_string()),
}
```

## Troubleshooting

### Benchmark fails with "Could not find project"
Check the project path in `BenchmarkScenario` matches the actual file:
```bash
ls -la examples/projects/01_orifice_steady.yaml
```

### Results vary wildly between runs
Normal on a busy system. To minimize variance:
- Close other applications
- Run on AC power
- Run multiple times and compare medians

### "Surrogate fallback was activated"
If `transient_fallback_uses > 0`, thermodynamic inversion failed and a backup was used. This is logged but shouldn't crash the simulation. For supported examples, fallback should be 0.

## See Also

- [docs/PERFORMANCE_BASELINE.md](../docs/PERFORMANCE_BASELINE.md) — Baseline results and analysis
- [crates/tf-bench/](../crates/tf-bench/) — Benchmark library code
- [examples/projects/](../examples/projects/) — Benchmark example projects
