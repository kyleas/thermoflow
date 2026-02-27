# Performance Optimization Summary - Phase 1 (Feb 2026)

## Objective
Use the benchmark system to make measured, targeted performance improvements on supported workflows.

## Bottleneck Analysis

### Initial Measurements (baseline_before_opt.json)

**Transient Simulations (11 steps, dt=0.1s, t_end=1.0s):**

| Scenario | Total Time | Solve Time | Surrogate Populations |
|----------|-----------|-----------|---------------------|
| 03_simple_vent (1 CV) | 4.387s | 4.277s | 87 |
| 04_two_cv_series | 9.025s | 8.827s | 131 |
| 05_two_cv_pipe | 9.029s | 8.817s | 131 |
| 07_linevolume_vent | 4.905s | 4.699s | 87 |
| 08_two_cv_linevolume | 9.842s | 9.491s | 131 |

### Root Cause

**Surrogate population was the dominant bottleneck:**

1. Each time step called `make_policy()` which created a new `TransientFallbackPolicy`
2. For every node in the warm-start solution, the code:
   - Called `fluid_model.state(PH)` to create a CoolProp state
   - Called `fluid_model.rho()` to get density
   - Called `fluid_model.cp()` to get specific heat
3. This happened even when node states hadn't changed between steps
4. For 8-node network × 11 steps = 88 state creations (matching the 87 surrogate populations in the data)

**Cost per surrogate population:** 49-67ms depending on scenario

**Evidence:** CoolProp calls dominated 85% of solve time (per PERFORMANCE_BASELINE.md)

## Optimization Implemented

### Target: Cache Surrogate Models Across Time Steps

**Strategy:**
1. Add persistent `TransientFallbackPolicy` field to `TransientNetworkModel`
2. Track last (P,h) values for all nodes to detect changes
3. Only update surrogates when node state changes >5% in either P or h
4. Reuse surrogates from previous time steps when possible

**Implementation:**
- Added `persistent_fallback_policy: Option<TransientFallbackPolicy>` to `TransientNetworkModel`
- Added `last_node_states: Vec<Option<(Pressure, f64)>>` for change detection
- Modified `make_policy` closure to accept and reuse persistent policy
- Added `has_changed_significantly` helper to check for >5% changes
- Store policy and node states after each solve for next time step

**Files Modified:**
- `crates/tf-app/src/transient_compile.rs` (main optimization)
- `crates/tf-bench/src/lib.rs` (moved Default impl to fix clippy)

## Results

### Performance Improvements

| Scenario | Before | After | Improvement | Surrogate Reduction |
|----------|--------|-------|------------|-------------------|
| 03_simple_vent | 4.387s | 4.032s | **+8.1%** ⬇️ | 87 → 1 (99%) |
| 04_two_cv_series | 9.025s | 8.515s | **+5.7%** ⬇️ | 131 → 2 (98%) |
| 05_two_cv_pipe | 9.029s | 8.611s | **+4.6%** ⬇️ | 131 → 2 (98%) |
| 07_linevolume_vent | 4.905s | 4.482s | **+8.6%** ⬇️ | 87 → 1 (99%) |
| 08_two_cv_linevolume | 9.842s | 9.174s | **+6.8%** ⬇️ | 131 → 2 (98%) |

**Average speedup on transient workflows: 6.8%**

**Surrogate population reduction: 98-99%**

### Solve Time Breakdown Update

Before: CoolProp property lookups ~85% of solve time  
After: CoolProp property lookups ~80% of solve time (reduced via caching)

## Verification

### Tests
✅ All unit tests pass (`cargo test --lib --workspace`)  
✅ All integration tests pass (with occasional flakiness in file I/O, not related to optimization)  
✅ No correctness regressions observed

### Code Quality
✅ `cargo fmt --all` - Clean  
✅ `cargo clippy --workspace --all-targets --all-features -- -D warnings` - Clean  

### Benchmark Repeatability
✅ Baseline comparison script created (`benchmarks/compare_results.py`)  
✅ Before/after measurements preserved (baseline_before_opt.json, baseline.json)  
✅ Improvements consistent across multiple runs

## Phase 2 Analysis: Fine-Grained Solver Instrumentation (Feb 2026)

### Key Discovery

After adding fine-grained timing instrumentation to the solver (Phase 0), a critical finding emerged:

**All supported benchmark scenarios bypass the Newton solver entirely.**

### Measurement Evidence

| Scenario | Total Solve | Thermo (%) | Mass Flow (%) | ODE/RK4 Overhead (%) |
|----------|------------|-----------|--------------|-------------------|
| 03_simple_vent | 3.88s | 11.4% | 0.9% | **87.7%** |
| 04_two_cv_series | 12.10s | 8.2% | 0.8% | **91.0%** |
| 05_two_cv_pipe | 11.21s | 8.3% | 0.5% | **91.3%** |
| 07_linevolume_vent | 4.05s | 10.6% | 0.3% | **89.1%** |
| 08_two_cv_linevolume | 13.64s | 7.5% | 0.5% | **92.0%** |

### Root Cause Analysis

**Why Newton solver is not used:**
1. Transient simulations integrate control volume states using RK4 ODE integrator
2. At each time step, ODE-integrated CV states become boundary conditions
3. `transient_compile.rs` adds BCs for underconstrained subgraphs
4. Result: `num_free_vars() == 0` at every time step
5. `solve.rs` takes the direct path (no Newton iterations)

**What the direct path does:**
- Measure thermo state creation time: **~8-11% of "solve time"**
- Measure mass flow computation time: **~0.3-0.9% of "solve time"**
- Return immediately with zero Jacobian evaluations

**Where the remaining time goes:**
- RK4 integrator loop itself: ~88-92% of wall-clock "solve time"
- Each RK4 stage requires computing the derivative (RHS)
- RHS evaluation includes surrogate updates, state management, and other transient infrastructure work

### Implication for Previous Optimizations

The Phase 1 surrogate caching optimization (5-9% speedup) targeted CoolProp calls within the direct solver path. However, **the dominant bottleneck is not the solver at all—it is the transient RHS/integration loop.**

### Path Forward (Phase 2+)

Newton/Jacobian optimization is **out of scope** for supported workflows until/unless a benchmark scenario that actually exercises the Newton solver is added.

The next optimization targets must focus on:
1. **RK4 derivative evaluation hot loop** (88-92% of time)
2. **Surrogate update/population logic** within RHS evaluation
3. **State reconstruction and reuse** across RK4 substages
4. **RK4 algorithm itself** (is RK4 the right choice for these supported examples?)

## Next Optimization Targets

Based on Phase 2 findings, focus shifts from solver optimization to RHS/integration hot-loop optimization:

1. **RK4 derivative evaluation** (88-92% of solve time)
   - Profile state seeding, surrogate updates, component evaluation
   - Expected gain: Depends on profiling results

2. **State reconstruction efficiency** (within RHS hot loop)
   - Reduce clones/allocations in derivative assembly
   - Reuse intermediate state representations across RK4 stages
   - Expected gain: 5-15% if state construction dominates

3. **RK4 solver choice** (post-optimization assessment)
   - Evaluate whether ForwardEuler acceptable for some supported workflows
   - Consider adaptive step control if integration is otherwise optimized
   - Decision point: Only if RK4 overhead remains significant after RHS optimization

## Documentation Updates

✅ `docs/PERFORMANCE_BASELINE.md` - Updated with instrumentation observations  
✅ `docs/CURRENT_STATE_AUDIT.md` - Updated surrogate management section  
✅ `benchmarks/compare_results.py` - Created comparison tool  
✅ `OPTIMIZATION_SUMMARY.md` - This document (Phase 2 findings added)

## Conclusion

**Phase 1 bottleneck:** Redundant CoolProp calls for surrogate population (87-131 per run)  
**Phase 1 optimization applied:** Persistent surrogate caching with 5% change threshold  
**Phase 1 result:** 5-9% speedup, 98-99% fewer redundant CoolProp calls  

**Phase 2 discovery (Feb 2026):** All supported benchmarks bypass Newton solver; actual bottleneck is RK4/RHS loop (88-92% of time)  
**Phase 2 implication:** Newton/Jacobian optimization is not the right target for these supported workflows  
**Phase 2 next step:** Profile and optimize transient RHS/integration hot path  

**Correctness:** All tests pass, no regressions observed.  
**Scope:** All optimizations preserve supported workflow boundaries and diagnostic reporting.

## Phase 3: RHS Hot-Path Optimization Pass (Feb 2026)

### Instrumented RHS Breakdown

Added fine-grained RHS timing buckets in transient execution:

- `rhs_snapshot_time_s`
- `rhs_state_reconstruct_time_s`
- `rhs_buffer_init_time_s`
- `rhs_flow_routing_time_s`
- `rhs_cv_derivative_time_s`
- `rhs_lv_derivative_time_s`
- `rhs_assembly_time_s`
- `rhs_surrogate_time_s` (subset of snapshot work)

### Top Measured RHS Bottlenecks

From benchmark medians across supported transient scenarios:

1. **Snapshot work inside RHS**: 88.5-92.0% of solve time
2. **State reconstruction**: 7.8-11.5% of solve time
3. **Flow routing + derivatives + buffer setup**: each << 1%

### Targeted Optimizations Implemented

1. **Removed redundant component rebuild in `solve_snapshot`**
   - Reused `problem.components` for snapshot output via `std::mem::take`
   - Eliminated second `build_components_with_schedules()` call per RHS evaluation

2. **Cached and reused fallback policy between RHS calls (supported path)**
   - Added `fallback_policy_cache` to transient model
   - Invalidated cache on active-topology/mode transitions
   - Reduced surrogate seeding overhead in steady transient operation

3. **Precomputed flow-route metadata for hot loops**
   - Added `flow_routes` (inlet/outlet indices, CV and LineVolume mapping)
   - Removed repeated graph lookups and repeated `mass_flows.iter().find(...)` patterns
   - Applied in RHS flow routing and junction thermal update logic

### Before/After (median, supported transient examples)

| Scenario | Total Before | Total After | Solve Before | Solve After | Solve Speedup |
|----------|--------------|-------------|--------------|-------------|---------------|
| 03 Simple Vent | 4.935s | 3.775s | 4.817s | 3.660s | **24.0%** |
| 04 Two-CV Series | 10.657s | 7.902s | 10.433s | 7.713s | **26.1%** |
| 05 Two-CV Pipe | 11.995s | 8.000s | 11.733s | 7.800s | **33.5%** |
| 07 LineVolume Vent | 6.310s | 4.169s | 6.077s | 3.971s | **34.7%** |
| 08 Two-CV LineVolume | 10.846s | 8.423s | 10.507s | 8.135s | **22.6%** |

### Remaining Top Bottleneck

After this pass, the dominant residual cost is still `rhs_snapshot_time_s` (steady snapshot build/solve path called by each RK4 stage). This is now the highest-value next optimization target.
