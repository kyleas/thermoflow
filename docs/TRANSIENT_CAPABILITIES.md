# Transient Simulation Capabilities

## Overview

ThermoFlow's transient simulation support is under active development. This document clearly defines what IS and IS NOT currently supported to help users understand the limitations and set appropriate expectations.

## ✅ **SUPPORTED** Features

### Transient Simulation Types
- **Simple CV venting to atmosphere**: CV with fixed orifice/valve to atmosphere node
- **Fixed-topology multi-CV transients**: multiple control volumes with fixed components and no timed events
- **Fixed component positions**: Valves with constant position throughout simulation
- **Fixed boundary conditions**: Atmosphere nodes with constant P and T

### Numerical Methods
- **RK4 integration** with adaptive timestep
- **Steady Newton solver** at each timestep
- **Junction thermal regularization** for lagged enthalpy
- **Real-fluid thermodynamics** via CoolProp
- **Surrogate fallback system** (backup for CoolProp failures)

### Initialization Strategies

ThermoFlow uses **automatic initialization strategy selection** to configure Newton solver behavior at startup:

| Strategy | Used For | Characteristics |
|----------|----------|-----------------|
| **Strict** | Steady-state, single-CV transients | Minimal regularization (`weak_flow_mdot=5e-4`), tight enthalpy tolerance (`enthalpy_total_abs=5e5`) |
| **Relaxed** | Multi-CV transients, LineVolume presence | Aggressive regularization (`weak_flow_mdot=1e-3`), relaxed enthalpy tolerance (`enthalpy_total_abs=3e5`) |

**Auto-selection logic**:
- Steady-state systems → `Strict`
- Single ControlVolume transients → `Strict`
- Multiple ControlVolume or LineVolume transients → `Relaxed`

The selected strategy is visible in timing summaries:
```
Initialization: Relaxed
```

**Why this matters**:
- Multi-CV systems often have initialization challenges (pressure balance, flow direction, enthalpy distribution)
- Relaxed strategy applies more aggressive regularization to help Newton solver converge from poor initial guesses
- Single-CV and steady problems converge reliably with minimal regularization (Strict)
- Strategy visibility aids troubleshooting startup failures

### Diagnostic Observability
- **Real-fluid vs fallback tracking**: Clear reporting of whether simulation used real CoolProp thermodynamics or surrogate approximations
- **Success rate statistics**: Number of state creation attempts vs successes
- **Fallback activation count**: How many times surrogate models were actually used
- **Surrogate update count**: Number of surrogate population/update events during startup and continuation
- **Verdict statements**: Clear indication of whether results are trustworthy

By default, transient runs use **summary logging** (phase progress + final diagnostics) to avoid hot-loop console overhead.
Set `THERMOFLOW_TRANSIENT_LOG=verbose` to enable per-step debug/surrogate/cutback trace output for deep troubleshooting.

### Example: Simple Vent Transient (`03_simple_vent_transient.yaml`)
This example demonstrates the **supported workflow**:
- Control volume with nitrogen at elevated pressure
- Fixed orifice to atmosphere
- No topology changes during simulation
- **Result**: 100% real-fluid thermodynamics, stable convergence

```
========== TRANSIENT SIMULATION DIAGNOSTICS ==========
Real-fluid state creation attempts:  808
Real-fluid state creation successes: 808
Real-fluid success rate:              100.0%
Surrogate population events:          0
Fallback activations (surrogate use): 0
✓ ALL STATES USED REAL-FLUID THERMODYNAMICS
======================================================
```

### Supported Multi-CV Benchmarks

- `04_two_cv_series_vent_transient.yaml` — two control volumes in series with fixed valve + fixed outlet orifice
- `05_two_cv_pipe_vent_transient.yaml` — tank + buffer control volume with fixed feed orifice + fixed outlet pipe

### LineVolume Component (Finite-Volume Storage)

**NEW**: The `LineVolume` component provides physically meaningful storage for lines/manifolds between system components.

**Purpose**: Many real systems have pipes or manifolds that cannot be modeled as zero-storage components (orifices/valves) or as full control-volume nodes. LineVolume bridges this gap by providing:
- Finite internal volume with mass/energy storage
- Optional flow resistance (lossless or with cd/area)
- Isenthalpic flow (no heat transfer or work)
- Improved transient robustness by damping pressure oscillations

**Usage Patterns**:
```yaml
components:
  - id: buffering_line
    name: "Buffered Connection"
    kind:
      type: LineVolume
      volume_m3: 0.001
      # Lossless mode (cd = 0, area_m2 = 0)
      
  - id: resistive_line
    name: "Line with Resistance"
    kind:
      type: LineVolume
      volume_m3: 0.002
      cd: 0.75
      area_m2: 0.0003
```

**When to use**:
- Manifolds or lines with non-negligible volume
- Systems requiring distributed storage (not just at tanks/CVs)
- Transients where line storage affects pressure dynamics
- Improving numerical robustness in multi-CV systems

**Examples**:
- `07_linevolume_buffered_vent.yaml` — Tank → LineVolume → Atmosphere
- `08_linevolume_two_cv.yaml` — Two CVs connected via LineVolume

**Transient Behavior**: LineVolume storage integrates mass and energy similar to ControlVolume nodes, but as component-based (not node-based) storage. Derivatives are computed at each timestep based on inlet/outlet flows.

These benchmark cases validate and run through the canonical `tf-app::run_service` path and are covered by regression tests.

## ❌ **UNSUPPORTED** (Explicitly Rejected) Features

### Timed Valve/Component Schedules
**Status**: Validation error, clear error message

**What doesn't work**:
- Schedules with `SetValvePosition` actions
- Timed opening/closing of valves
- Dynamic valve position changes during simulation

**Why**: The continuation solver is not robust enough to handle the rapid topology changes caused by valve transients. Even with extensive numerical strategies (continuation substeps, trust-region constraints, line search), convergence failures persist.

**Error message**:
```
Validation error: Unsupported feature: Timed valve position schedules 
(schedule 'Valve Schedule', component 'v1') - Timed valve opening/closing 
schedules are not yet supported. The continuation solver is not robust 
enough for valve transients. Use fixed valve positions for now.
```

**Workaround**: Define valve with fixed `position` field, omit schedules.

## ⚠️ **EXPERIMENTAL** (May or May Not Work)

### Complex Multi-CV Systems
Most fixed-topology multi-CV cases are now supported, but some junction-heavy setups may still be numerically unstable depending on:
- Initial conditions
- Time step size
- Component flow rates
- Pressure ratios

**Example**: `06_two_cv_junction_vent_transient.yaml` can still experience startup convergence failures at t=0 for some parameter choices. This is tracked as an experimental stress case.

**Recommendation**: Use the supported benchmark templates first (`04`, `05`) and add junction complexity gradually.

## Attempted Numerical Strategies

The following strategies have been attempted to improve transient robustness:

### 1. Continuation Method with Adaptive Substeps
- Start with 20 substeps, increase to 30→45→68→102 on retry
- Interpolate valve positions between initial and final states
- Update surrogate models from intermediate solutions
- **Result**: Helps some cases, not sufficient for valve transients

### 2. Trust-Region Constraints on Enthalpy
- Limit `|Δh|` and total `|h|` to physically reasonable ranges
- Progressive relaxation on retry (tight → moderate → unconstraint)
- Prevent unphysical states that cause CoolProp failures
- **Result**: Reduces but doesn't eliminate failures

### 3. Surrogate Fallback System
- Pre-populate surrogate models from warm-start solution
- Update surrogates during continuation substeps
- Use surrogates to estimate T from h when CoolProp fails
- **Result**: Prevents crashes, but doesn't fix convergence issues

### 4. Line Search with Backtracking
- Up to 40 line search iterations
- Beta = 0.4 for aggressive step size reduction
- **Result**: Still fails at iteration 0 for some valve transients

### 5. Junction Thermal Regularization
- Use lagged enthalpy for junction nodes (0 mass)
- Prevents singular thermal equations
- **Result**: Essential for junction nodes, doesn't help valve convergence

## Diagnostic Output Guide

### What the diagnostics tell you:

**"✓ ALL STATES USED REAL-FLUID THERMODYNAMICS"**
- Simulation used real CoolProp throughout
- Results are trustworthy and accurate
- No fallback approximations were needed

**"⚠ FALLBACK WAS ACTIVATED N times"**
- Some state creations failed, surrogates were used
- Results may be less accurate
- Check which phases/conditions triggered fallback
- Consider adjusting initial conditions or timestep

**"Surrogate population events: N"**
- Surrogates were prepared from valid states
- Does NOT mean surrogates were used
- This is normal warm-start behavior

**Real-fluid success rate:**
- 100%: Perfect, no issues
- 95-99%: Occasional fallback, check results carefully
- <95%: Significant surrogate use, results questionable

## Roadmap

Future improvements planned:
1. **More robust continuation**: Better initial guess strategies
2. **Adaptive constraint tuning**: Per-component trust-region limits
3. **Pseudo-transient method**: Steady-state solver with time-stepping to target
4. **Alternative thermodynamic paths**: Try PT, TH, PS inputs when PH fails
5. **Validation test suite**: Automated testing of support matrix

## Support Matrix Summary

| Feature | Status | Example | Diagnostics |
|---------|--------|---------|-------------|
| Fixed valve CV vent | ✅ Supported | `03_simple_vent_transient.yaml` | 100% real-fluid |
| Fixed-topology multi-CV (series/pipe) | ✅ Supported | `04_two_cv_series_vent_transient.yaml`, `05_two_cv_pipe_vent_transient.yaml` | 100% real-fluid in benchmark runs |
| Timed valve schedules | ❌ Unsupported | `unsupported/02_tank_blowdown_scheduled.yaml` | Validation error |
| Junction-heavy multi-CV startup | ⚠️ Experimental | `06_two_cv_junction_vent_transient.yaml` | Case-dependent startup convergence |
| Fixed blowdown | ⚠️ Experimental | `02_tank_blowdown_transient.yaml` | Convergence sensitivity near startup |

## Getting Help

If you encounter issues:
1. Check validation errors - they tell you if a feature is explicitly unsupported
2. Review diagnostic output - did simulation use real-fluid or fallback?
3. Try simpler configuration first (single CV, atmosphere, fixed components)
4. Reduce timestep - sometimes helps convergence
5. Report issues with full diagnostic output

## Development Conventions

When adding transient features:
1. **Prefer explicit unsupported errors** over mysterious failures
2. **Add diagnostic counters** for any new solver paths
3. **Document attempted strategies** even if they don't work
4. **Create test cases** for supported and unsupported scenarios
5. **Update this document** with findings

---

*Last updated: Phase 3 of transient stabilization work*
*Status: Core diagnostics in place, valve schedules explicitly unsupported*
