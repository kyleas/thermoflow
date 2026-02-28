# Phase 11: Remaining Thermodynamic Hot Path Analysis

**Date**: Phase 11 Start
**Goal**: Identify and eliminate remaining thermodynamic query redundancy after Phase 10's 3.07x speedup

## 1. Phase 10 Baseline Context

After implementing direct density-temperature pressure inversion (Phase 10):
- **Overall speedup**: 2.35-3.67x (average 3.07x, 67% time reduction)
- **CV pressure inversion speedup**: 5.5-7.4x (average 6.6x, 85% reduction)
- **Remaining bottleneck**: CV pressure now only 29-35% of solve time (was 72-81% before)

**Phase 10 Reduced Bottleneck**: 
- Eliminated nested bisection (50 P iterations × 100 T iterations = ~5,000 state creations)
- Replaced with single T bisection at fixed density (~50 iterations = ~50 state creations)
- Used rfluids `FluidInput::density() + temperature()` for efficient backend updates

**Current Solver Time Distribution (Phase 10 Baseline)**:
- CV pressure inversion: 29-35% of solve
- Other thermodynamic evaluations: 15-20% (estimated)
- Non-thermo work: ~45-50%

## 2. Identified Property Query Patterns

### 2.1 Component-Level Queries: Repeated Multi-Property Access

**Pattern 1: Orifice Component** (`crates/tf-components/src/orifice.rs:55-120`)
```rust
// Lines 70-71: TWO separate backend queries on SAME state
let rho_up = fluid.rho(state_up)?;  // Query 1: density
let gamma = fluid.gamma(state_up)?; // Query 2: heat capacity ratio
let a_up = fluid.a(state_up)?;      // Query 3: speed of sound
```

**Analysis**:
- Same `state_up` queried 3 times independently
- Each query creates a new rfluids Fluid instance and calls backend (see coolprop.rs:355-440)
- **Opportunity**: Compute all three properties once per state, reuse result

**Frequency**: Called once per orifice per residual evaluation

---

**Pattern 2: Turbine Component** (`crates/tf-components/src/turbine.rs:95-125`)
```rust
// Lines 107-108: TWO separate backend queries on SAME state
let gamma = fluid.gamma(ports.inlet)?;  // Query 1: heat capacity ratio  
let cp = fluid.cp(ports.inlet)?;        // Query 2: specific heat capacity
```

**Analysis**:
- Same `ports.inlet` state queried twice independently
- **Opportunity**: Compute both once per state, reuse

**Frequency**: Called once per turbine per residual evaluation

---

### 2.2 Control Volume Boundary State Queries

**Pattern 3: State Boundary Computation** (`crates/tf-sim/src/control_volume.rs:270-310`)
```rust
pub fn state_ph_boundary(
    &self,
    fluid: &dyn FluidModel,
    state: &ControlVolumeState,
    p_hint: Option<Pressure>,
) -> SimResult<(Pressure, SpecEnthalpy)> {
    let rho = self.density(state);
    // Line 284: Query pressure via pressure_from_rho_h()
    let p = self.pressure_from_rho_h(fluid, rho, state.h_j_per_kg, p_hint)?;
    
    // Line 287: ANOTHER state creation at computed (P,h) for validation
    fluid.state(
        StateInput::PH { p, h: state.h_j_per_kg },
        self.composition.clone(),
    )?;
    
    Ok((p, state.h_j_per_kg))
}
```

**Analysis**:
- Phase 10's `pressure_from_rho_h()` creates state at (ρ,h) via T bisection
- Then immediately validates by creating ANOTHER state at (P,h)
- The computed (P,h) should be the same as what we just computed from (ρ,h)
- **Potential waste**: Re-validating state we just computed

**Frequency**: Called once per control volume per residual evaluation (but multiple CVs per system)

---

### 2.3 Backend Property Query Cost

**In CoolPropModel** (`crates/tf-fluids/src/coolprop.rs:340-440`):

Each property query creates NEW rfluids Fluid instance and makes independent backend calls:

- `cp()` (line 347): Creates `fluid_at_pt()` → calls `specific_heat()`
- `gamma()` (line 375): Creates `fluid_at_pt()` → calls `specific_heat()` AND `density()` (2 backend calls!)
- `a()` (line 406): Creates `fluid_at_pt()` → calls `sound_speed()`
- `rho()` (line 315): Creates `fluid_at_pt()` → calls `density()`
- `h()` (line 327): Creates `fluid_at_pt()` → calls `enthalpy()`

**Observation**: `gamma()` is particularly costly - it computes `cp`, then `density`, then calculates cv = cp - R_specific, then gamma = cp/cv.

When both `gamma()` and `cp()` are called on same state (Pattern 2), we compute `cp` twice!

---

## 3. Optimization Opportunity: Property-Pack Pattern

### 3.1 Overview

Instead of making 3 separate backend calls on same state:
```rust
// TODAY: 3 independent backend queries
let rho = fluid.rho(state)?.value;      // Backend call 1
let gamma = fluid.gamma(state)?;        // Backend calls 2-3 (includes cp + rho again!)
let a = fluid.a(state)?.value;          // Backend call 4
```

Use **property-pack pattern** - compute all needed properties ONCE per state:
```rust
// PHASE 11: Single batch computation
let pack = fluid.property_pack(state)?; // All properties once
let rho = pack.rho;
let gamma = pack.gamma;
let a = pack.a;
```

### 3.2 Implementation Strategy

**Stage 1**: Create `ThermoPropertyPack` struct in tf-fluids/src/model.rs
```rust
#[derive(Clone, Debug)]
pub struct ThermoPropertyPack {
    pub p: Pressure,
    pub t: Temperature,
    pub rho: Density,        // kg/m³
    pub h: SpecEnthalpy,     // J/kg
    pub cp: SpecHeatCapacity, // J/(kg·K)
    pub gamma: f64,          // cp/cv (dimensionless)
    pub a: Velocity,         // m/s (speed of sound)
}
```

**Stage 2**: Implement efficient computation in CoolPropModel
- Create ONE rfluids Fluid instance at (P,T)
- Extract all needed properties from that single instance
- Return packed struct (no cloning, all values computed once)

**Stage 3**: Add trait method `FluidModel::property_pack(state) -> FluidResult<ThermoPropertyPack>`
- Default implementation: Calls individual property methods (fallback)
- CoolPropModel override: Batch computation as above

**Stage 4**: Use property-pack in hot paths
- Orifice component: Get pack once, use rho/gamma/a from pack
- Turbine component: Get pack once, use cp/gamma from pack
- Control volume: Return pack from pressure_from_rho_h direct paths

### 3.3 Expected Benefit

**Current overhead** (Pattern 1 in Orifice):
- 3 property queries × 2-4 backend operations each ≈ 6-12 backend operations per state
- With ~50 pressure-inversion states per control volume per transient step
- × ~5,000 transient steps in benchmark = millions of redundant backend operations

**With property-pack**:
- 1 batch query = 2-3 backend operations per state
- Same scale, but **60-80% reduction** in redundant calls

---

## 4. Measurement Plan (Phase 1)

To precisely quantify remaining bottleneck, add instrumentation:

### 4.1 Fine-Grained Thermo Timing Buckets

```rust
// In tf-core/src/timing.rs
pub mod thermo_timing {
    pub static CP_CALLS: AccumulatingTimer = AccumulatingTimer::new();
    pub static GAMMA_CALLS: AccumulatingTimer = AccumulatingTimer::new();
    pub static A_CALLS: AccumulatingTimer = AccumulatingTimer::new();
    pub static PROPERTY_PACK_CALLS: AccumulatingTimer = AccumulatingTimer::new();
    pub static STATE_CREATION: AccumulatingTimer = AccumulatingTimer::new();
}
```

### 4.2 Instrumentation Points

- Orifice: Before/after gamma + a queries
- Turbine: Before/after cp + gamma queries
- CoolPropModel: Time cp, gamma, a, property_pack implementations
- ControlVolume: Distinguish pressure-inversion time from state-validation time

### 4.3 Measurement Output

After each benchmark run:
- Total cp() calls and time
- Total gamma() calls and time
- Total a() calls and time
- Breakdown: how many are redundant (same state queried multiple times)?

---

## 5. Current Code Locations to Modify

| File | Current Pattern | Phase 11 Change |
|------|-----------------|-----------------|
| `crates/tf-fluids/src/model.rs` | Single-property trait methods | Add `property_pack()` trait method |
| `crates/tf-fluids/src/coolprop.rs` | Separate cp/gamma/a implementations | Add `property_pack()` override with batch computation |
| `crates/tf-components/src/orifice.rs` | 3 separate queries on state_up | Use property-pack |
| `crates/tf-components/src/turbine.rs` | 2 separate queries on ports.inlet | Use property-pack |
| `crates/tf-sim/src/control_volume.rs` | pressure_from_rho_h() + validation state | Potential optimization: return state directly from direct path |
| `crates/tf-core/src/timing.rs` | Existing timing framework | Add thermo-query sub-buckets |

---

## 6. Phase 11 Roadmap

- **Phase 0** ✅: This analysis - identify patterns and opportunities
- **Phase 1**: Add precise instrumentation to measure remaining thermo overhead
- **Phase 2**: Implement property-pack struct and type-safe interface
- **Phase 3**: Refactor hot paths (orifice, turbine) to use property-pack
- **Phase 4**: Re-benchmark against Phase 10 baseline
- **Phase 5-8**: Testing, clippy cleanup, documentation, final verification

---

## 7. Success Criteria for Phase 11

- [ ] Fine-grained thermo instrumentation in place (Phase 1)
- [ ] Property-pack struct designed and implemented (Phase 2)
- [ ] Hot paths refactored to use property-pack (Phase 3)
- [ ] Benchmark shows measurable improvement over Phase 10 baseline
  - Target: Additional 10-20% reduction in thermo time (relative to Phase 10)
  - Minimum: No regression (≥ Phase 10 performance)
- [ ] All tests pass, clippy clean, code formatted
- [ ] Documentation updated with Phase 11 results and methodology

---

## Appendix: Detailed Backend Cost Analysis

### CoolPropModel::gamma() Cost Breakdown

```rust
fn gamma(&self, state: &ThermoState) -> FluidResult<f64> {
    // Line 375-378: Extract composition and get rfluids species
    let species = state.composition().is_pure()
        .ok_or(FluidError::NotSupported { ... })?;
    
    // Line 378-380: Create rfluids instance at (P,T)
    let mut fluid = self.fluid_at_pt(pure, p_pa, t_k)?;  // Backend call 1: create
    
    // Line 385-388: Get cp (FIRST BACKEND CALL)
    let cp = fluid.specific_heat()?;
    
    // Line 389-392: Get density (SECOND BACKEND CALL)
    let rho = fluid.density()?;
    
    // Line 393-394: Calculate cv via thermodynamic relation
    let r_specific = p_pa / (rho * t_k);
    let cv = cp - r_specific;
    
    // Line 396-401: Compute gamma
    let gamma = cp / cv;
    
    Ok(gamma)
}
```

**When both cp() and gamma() called on same state**, we get:
- cp() call: Creates instance, calls specific_heat() → 1 backend call
- gamma() call: Creates ANOTHER instance, calls specific_heat() AGAIN + density() → 2 backend calls
- **Total for cp + gamma**: 3 backend operations

With property-pack:
- Create instance once
- Call specific_heat(), density(), sound_speed() all on same instance
- Return packed struct
- **Total for cp + gamma + a**: Still 3 backend operations, but structured efficiently

This doesn't reduce backend-call count (rfluids probably optimizes internally), but it:
1. Eliminates duplicate Fluid instance creation
2. Makes hot-path intent explicit (better for compiler optimization)
3. Enables component-level caching in future phases if needed
