# Performance Baseline Report

**Generated**: 2026-02-27  
**Thermoflow Version**: 0.1.0  
**Build Mode**: Release (optimized)  
**Host**: Windows (Intel)
**Optimization Status**: Phase 1 complete (surrogate caching implemented)

## Phase 5 Snapshot/Build Split + Direct-Setup Optimization (2026-02-27)

### What was instrumented

The previous coarse snapshot/build cost was split into:

- `rhs_plan_check_time_s`
- `rhs_component_rebuild_time_s`
- `rhs_snapshot_structure_setup_time_s`
- `rhs_boundary_hydration_time_s`
- `rhs_direct_solve_setup_time_s`
- `rhs_result_unpack_time_s`

Additional counters were added:

- `execution_plan_checks`
- `execution_plan_unchanged`
- `component_rebuilds`
- `component_reuses`
- `snapshot_setup_rebuilds`
- `snapshot_setup_reuses`

### Dominant remaining sub-bottleneck

Across supported transient scenarios, the dominant sub-bucket inside snapshot work is now clearly:

- `rhs_direct_solve_setup_time_s`: **~75-83% of solve time**

Other split buckets are small:

- `rhs_plan_check_time_s`: ~microseconds total
- `rhs_component_rebuild_time_s`: ~microseconds total
- `rhs_snapshot_structure_setup_time_s`: ~0.1 ms total
- `rhs_boundary_hydration_time_s`: ~0.02 ms total
- `rhs_result_unpack_time_s`: ~0.2-0.6 ms total

### Measured rebuild/setup frequency

For supported transient runs (median):

- `execution_plan_checks`: 44
- `execution_plan_unchanged`: 43
- `component_rebuilds`: 44
- `component_reuses`: 0
- `snapshot_setup_rebuilds`: 44
- `snapshot_setup_reuses`: 0

This shows structure is repeatedly rebuilt even when execution-plan state is unchanged.

### Targeted optimization applied

Optimized the dominant setup path by throttling CV surrogate refresh work:

- On successful real-fluid CV boundary solves, surrogate refresh now runs only when $(P,h)$ changes by >5% relative to the last surrogate anchor.
- This avoids repeated expensive surrogate refresh setup work when fallback is not being used.

### Before/After medians vs post-Phase-4 baseline

| Scenario | Total Before (s) | Total After (s) | Solve Before (s) | Solve After (s) | Solve Delta |
|----------|------------------|-----------------|------------------|-----------------|------------|
| 03 Simple Vent | 4.469 | 4.119 | 4.363 | 4.006 | **+8.2%** |
| 04 Two-CV Series | 8.556 | 8.450 | 8.345 | 8.226 | **+1.4%** |
| 05 Two-CV Pipe | 8.634 | 10.287 | 8.414 | 10.064 | **-19.6%** |
| 07 LineVolume Vent | 4.472 | 5.861 | 4.272 | 5.602 | **-31.1%** |
| 08 Two-CV LineVolume | 8.978 | 10.791 | 8.654 | 10.217 | **-18.1%** |

Interpretation: instrumentation and optimization now precisely identify where work is concentrated; net benchmark movement is mixed and shows regressions on several multi-CV scenarios in this run.

### Next likely bottleneck

Given the new split, the highest-value next target remains `rhs_direct_solve_setup_time_s`, especially CV boundary hydration/setup logic that executes each RK stage.

## Phase 3 RHS Hot-Path Update (2026-02-27)

Measured on supported transient workflows after adding RHS subphase instrumentation and applying targeted hot-path optimizations:

| Scenario | Total (ms) Before | Total (ms) After | Solve (ms) Before | Solve (ms) After | Solve Speedup |
|----------|-------------------|------------------|-------------------|------------------|---------------|
| 03 Simple Vent | 4935 | 3775 | 4817 | 3660 | **24.0% faster** |
| 04 Two-CV Series | 10657 | 7902 | 10433 | 7713 | **26.1% faster** |
| 05 Two-CV Pipe | 11995 | 8000 | 11733 | 7800 | **33.5% faster** |
| 07 LineVolume Vent | 6310 | 4169 | 6077 | 3971 | **34.7% faster** |
| 08 Two-CV LineVolume | 10846 | 8423 | 10507 | 8135 | **22.6% faster** |

### RHS Subphase Breakdown (post-optimization)

Across transient scenarios, median solve-time composition is now:

- `rhs_snapshot_time_s`: **88.5-92.0%** (dominant)
- `rhs_state_reconstruct_time_s`: **7.8-11.5%**
- `rhs_flow_routing_time_s`: **~0.0-0.27%**
- `rhs_buffer_init_time_s`, `rhs_cv_derivative_time_s`, `rhs_lv_derivative_time_s`, `rhs_assembly_time_s`: negligible
- `rhs_surrogate_time_s` (subset of snapshot): **~0.1-0.2%** after policy-cache reuse

### Bottleneck Shift

The dominant remaining bottleneck is still snapshot work inside each RHS call (steady snapshot build/solve path), not Jacobian/Newton work and not vector allocation.

---

## Overview

This document establishes a performance baseline for Thermoflow's **supported workflows** using a repeatable benchmark suite. The goal is to enable future performance work to be measurement-driven and comparable.

---

## Baseline Metrics

All times are **wall-clock measurements** using the `Instant` timer on a release build with `cargo build --release`.

### Steady-State Simulations

| Scenario | Total (ms) | Solve (ms) | Iterations | Init Strategy |
|----------|-----------|-----------|-----------|---|
| **01 Orifice Steady** | 14.4 | 12.5 | — | Strict |

**Notes:**
- Single orifice between two junctions; minimal computation
- CoolProp calls dominate time (thermodynamic property evaluation)
- No Newton iterations recorded; fully algebraic system

---

### Transient Simulations

| Scenario | Total (ms) | Solve (ms) | Steps | Real-fluid % | Init Strategy |
|----------|-----------|-----------|-------|----------|---|
| **03 Simple Vent** | 4032 | 3923 | 11 | 100% | Strict |
| **04 Two-CV Series** | 8515 | 8296 | 11 | 100% | Relaxed |
| **05 Two-CV Pipe** | 8611 | 8395 | 11 | 100% | Relaxed |
| **07 LineVolume Vent** | 4482 | 4285 | 11 | 100% | Relaxed |
| **08 Two-CV LineVolume** | 9174 | 8867 | 11 | 100% | Relaxed |

**Notes:**
- All runs show **100% real-fluid success rate** (no surrogate fallback needed)
- Time dominated by solve phase (steady problem solve at each transient step)
- CoolProp property calls are the bottleneck, not algorithmic complexity
- Initialization strategy is Strict for single-CV, Relaxed for multi-CV systems

**Optimization Impact (Feb 2026):**
- **5-9% speedup** on transient simulations through surrogate caching
- **98-99% reduction** in redundant CoolProp calls for surrogate populations
- Simple scenarios: 87 → 1 surrogate population per run
- Multi-CV scenarios: 131 → 2 surrogate populations per run

---

## Hotspot Analysis

### Where Time Goes (Transient Example: 03_simple_vent)

| Phase | Median (ms) | % of Total |
|-------|-----------|-----------|
| Compile | ~0.01 | <0.1% |
| Build problem | ~0.01 | <0.1% |
| **Solve (11 steps × steady solve each)** | 3923 | **97.3%** |
| Save results | 0.5 | <0.1% |
| **Total** | 4032 | 100% |

### Where Solve Time Goes (Steady example within transient step)

Per-step solve time dominated by:
1. **Property lookups via CoolProp** (P, h) → state: ~80% of solve time (reduced via caching)
2. **Newton Jacobian computation** (finite differences): ~10%
3. **Mass/energy balance residuals**: ~10%

**Optimization applied**: Persistent surrogate caching eliminates redundant property calls when node states haven't changed significantly (>5% threshold).

---

## Supported Examples

The baseline includes only **officially supported** workflows:

✅ **Steady-state** with single CV or junctions  
✅ **Transient** with fixed-position valves and no timed schedules  
✅ **Single or multiple CVs** (series topology tested)  
✅ **Pipes, orifices, valves** (linear components)  
✅ **LineVolume** storage elements  
✅ **Pure nitrogen** (composition is fixed; not a bottleneck)

❌ **NOT included**: Timed valve schedules (unsupported, validation error)  
❌ **NOT included**: Dynamic topology changes (not applicable to fixed CV networks)

---

## Methodology

### Run Count
5 runs per benchmark, median reported (robust to outliers).

### Caching
All benchmark runs use `--no-cache` to force fresh compute and avoid cache artifacts.

### Reproducibility Caveats

Performance numbers are meaningful for **relative comparison** on the **same machine** under similar conditions. Absolute numbers will vary based on:
- CPU speed and thermal state
- OS background processes
- Disk I/O (CoolProp library loading, project file I/O)
- Build flags (we use release mode, but exact `-O` level depends on Rust version)

**To compare to this baseline in the future:**
```bash
cargo build -p tf-bench --release
target/release/tf-bench.exe
# Compare output JSON to benchmarks/baseline.json
```

---

## Baseline Preservation

The `benchmarks/baseline.json` artifact (machine-readable) captures all per-run metrics and aggregates for programmatic comparison.

To add a new baseline:
```bash
target/release/tf-bench --output benchmarks/baseline_new.json
# Then programmatically diff results
```

---

## Future Optimization Targets

Based on this baseline:

1. ✅ **COMPLETED: Surrogate caching** (Phase 1, Feb 2026)
  - Persistent surrogate models cached across time steps
  - Only update when node (P,h) changes >5%
  - Result: 5-9% speedup, 98-99% fewer redundant CoolProp calls

2. **Component construction overhead** (build phase)
  - Currently 100-200ms per transient run
  - Could cache component instances within transient loop
  - Expected gain: 10-20% reduction in build time

3. **CoolProp API usage** (external dependency)
  - Consider tabulated properties for common regions
  - Investigate CoolProp caching strategies
  - Note: This is external to Thermoflow logic

4. **Steady solver convergence**
   - Current solves are fast (dominated by property calls)
   - No obvious algorithmic bottleneck

5. **Transient integration**
   - No cutback retries observed in supported examples
   - Surrogate fallback not activated (real-fluid 100% success)
   - Suggests robustness is good; integration is efficient

---

## Performance History

| Phase | Timestamp | Notes |
|-------|-----------|-------|
| Phase 0 (observability infrastructure) | 2026-02-27 | Timing summaries, logging infrastructure |
| **Baseline (Phase 1)** | 2026-02-27 | Initial repeatable benchmark (pre-optimization) | 
| **Phase 2: Surrogate caching** | 2026-02-27 | +5-9% speedup via persistent surrogates |
| **Phase 3: RHS hot-path pass** | 2026-02-27 | +22-35% transient solve speedup via RHS-targeted changes |

---

## Appendix: Full Baseline JSON

See [benchmarks/baseline.json](../benchmarks/baseline.json) for complete per-run metrics and aggregates.

Example structure:
```json
{
  "timestamp": "timestamp_1772222684",
  "results": [
    {
      "scenario": { "id": "01_steady", "name": "...", "mode": "Steady", ... },
      "runs": [ { "total_time_s": 0.014, "solve_time_s": 0.0125, ... } ],
      "aggregate": { "run_count": 5, "total_time_median_s": 0.014, ... }
    }
  ]
}
```
