# Performance Baseline Report

**Generated**: 2026-02-27  
**Thermoflow Version**: 0.1.0  
**Build Mode**: Release (optimized)  
**Host**: Windows (Intel)

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
| **03 Simple Vent** | 4387 | 4277 | 11 | 100% | Strict |
| **04 Two-CV Series** | 9026 | 8827 | 11 | 100% | Relaxed |
| **05 Two-CV Pipe** | 9030 | 8817 | 11 | 100% | Relaxed |
| **07 LineVolume Vent** | 4905 | 4699 | 11 | 100% | Relaxed |
| **08 Two-CV LineVolume** | 9842 | 9491 | 11 | 100% | Relaxed |

**Notes:**
- All runs show **100% real-fluid success rate** (no surrogate fallback needed)
- Time dominated by solve phase (steady problem solve at each transient step)
- CoolProp property calls are the bottleneck, not algorithmic complexity
- Initialization strategy is Strict for single-CV, Relaxed for multi-CV systems

---

## Hotspot Analysis

### Where Time Goes (Transient Example: 03_simple_vent)

| Phase | Median (ms) | % of Total |
|-------|-----------|-----------|
| Compile | ~0.01 | <0.1% |
| Build problem | ~0.01 | <0.1% |
| **Solve (11 steps × steady solve each)** | 4277 | **97.5%** |
| Save results | 0.5 | <0.1% |
| **Total** | 4387 | 100% |

### Where Solve Time Goes (Steady example within transient step)

Per-step solve time dominated by:
1. **Property lookups via CoolProp** (P, h) → state: ~85% of solve time
2. **Newton Jacobian computation** (finite differences): ~10%
3. **Mass/energy balance residuals**: ~5%

This is expected: thermodynamic property evaluations are externally-linked (librefprop) and optimization of these is outside Thermoflow's scope.

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

1. **CoolProp API usage** (not Thermoflow logic)
   - Cache property lookups within a time step?
   - Use tabulated properties instead of on-demand CoolProp?

2. **Steady solver convergence**
   - Current solves are fast (dominated by property calls)
   - No obvious algorithmic bottleneck

3. **Transient integration**
   - No cutback retries observed in supported examples
   - Surrogate fallback not activated (real-fluid 100% success)
   - Suggests robustness is good; integration is efficient

---

## Comparison to Earlier Phases

| Phase | Timestamp | Notes |
|-------|-----------|-------|
| Phase 0–2 (prior perf/observability work) | 2026-02-27 | Timing summaries improved, logging overhead reduced |
| **Baseline (Phase 3)** | 2026-02-27 | Initial repeatable benchmark | 

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
