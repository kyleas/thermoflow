# Canonical Paths in Thermoflow

**Quick reference for developers**: Which crates and functions handle each responsibility.

---

## Project Management

| Task | Canonical Function | File |
|------|-------------------|------|
| Load project from YAML | `tf-app::project_service::load_project()` | `crates/tf-app/src/project_service.rs` |
| Validate project | `tf-app::project_service::validate_project()` | `crates/tf-app/src/project_service.rs` |
| Save project to YAML | `tf-app::project_service::save_project()` | `crates/tf-app/src/project_service.rs` |
| Get system definition | `tf-app::project_service::get_system()` | `crates/tf-app/src/project_service.rs` |
| List systems in project | `tf-app::project_service::list_systems()` | `crates/tf-app/src/project_service.rs` |

**Schema validation** (called by above):
- `tf-project::validate_project()` — schema rules, unsupported feature checks

---

## Runtime Compilation (Steady-State)

| Step | Canonical Function | File | Inputs | Outputs |
|------|-------------------|------|--------|---------|
| 1. Compile system | `tf-app::runtime_compile::compile_system()` | `crates/tf-app/src/runtime_compile.rs` | `SystemDef` | `SystemRuntime` |
| 2. Build fluid model | `tf-app::runtime_compile::build_fluid_model()` | `crates/tf-app/src/runtime_compile.rs` | `CompositionDef` | `Box<dyn FluidModel>` |
| 3. Parse boundaries | `tf-app::runtime_compile::parse_boundaries_with_atmosphere()` | `crates/tf-app/src/runtime_compile.rs` | `SystemDef`, boundaries | `HashMap<NodeId, BoundaryCondition>` |
| 4. Build components | `tf-app::runtime_compile::build_components()` | `crates/tf-app/src/runtime_compile.rs` | `SystemDef`, comp_id_map | `HashMap<ComponentId, Box<...>>` |
| 5. Create problem | `tf-solver::SteadyProblem::new()` | `crates/tf-solver/src/problem.rs` | `Graph`, `FluidModel`, composition | `SteadyProblem` |
| 6. Add components | `SteadyProblem::add_component()` | `crates/tf-solver/src/problem.rs` | component ID, model | — |
| 7. Set boundary conditions | `SteadyProblem::set_pressure_bc()`, `set_temperature_bc()`, `set_enthalpy_bc()` | `crates/tf-solver/src/problem.rs` | node ID, value | — |

---

## Steady-State Execution

**Main entry point**:
```rust
tf-app::run_service::ensure_run(&request)
→ ensure_run_with_progress(&request, Option<callback>)
```

| Phase | Duration Field | Canonical Call | File |
|-------|---|---|---|
| Compile | `compile_time_s` | `tf-app::runtime_compile::compile_system()` + `build_fluid_model()` + `parse_boundaries_with_atmosphere()` + `build_components()` | `crates/tf-app/src/run_service.rs` lines ~280 |
| Build problem | `build_time_s` | `SteadyProblem::new()` + `add_component()` + set BCs | `crates/tf-app/src/run_service.rs` lines ~300 |
| Solve | `solve_time_s` | `tf-solver::solve_with_progress(&problem, ...)` | `crates/tf-app/src/run_service.rs` lines ~340 |
| Save | `save_time_s` | `tf-results::RunStore::save_run(manifest, &[record])` | `crates/tf-app/src/run_service.rs` lines ~420 |

**Progress events emitted**:
- `RunStage::LoadingProject`
- `RunStage::CheckingCache`
- `RunStage::CompilingRuntime`
- `RunStage::BuildingSteadyProblem`
- `RunStage::SolvingSteady` (with `SteadyProgress` iterations/residual)
- `RunStage::SavingResults`
- `RunStage::Completed`

**Solver details**:
- Unknowns: node pressures P and specific enthalpies h
- Solver: Newton-Raphson on residual vector (conservation of mass/energy)
- Convergence: norm(R) < tolerance
- Iteration loop: `tf-solver::newton::newton_solve_with_validator()`

---

## Transient Execution

**Entry point**: Same as steady (`ensure_run()` with `RunMode::Transient { dt_s, t_end_s }`)

| Phase | Duration Field | Canonical Logic | File |
|-------|---|---|---|
| Compile | `compile_time_s` | Same as steady | `crates/tf-app/src/run_service.rs` lines ~480 |
| Build transient model | (incl. compile_time) | `tf-app::transient_compile::TransientNetworkModel::from_steady()` | `crates/tf-app/src/transient_compile.rs` |
| Integrate | `solve_time_s` | `tf-sim::run_sim_with_progress(&model, opts, &callback)` | `crates/tf-app/src/run_service.rs` lines ~520 |
| Save | `save_time_s` | `store.save_run(manifest, &records)` (N records, one per step) | `crates/tf-app/src/run_service.rs` lines ~580 |

**Progress events emitted**:
- `RunStage::LoadingProject`
- `RunStage::CheckingCache`
- `RunStage::CompilingRuntime`
- `RunStage::RunningTransient` (with `TransientProgress` sim_time/fraction/step/cutbacks)
- `RunStage::SavingResults`
- `RunStage::Completed`

**Integrator details**:
- Method: RK4 with adaptive substep retry on convergence failure
- Per-step: Solve steady problem at new CV state, emit progress, store result
- Control volume dynamics: `dM/dt`, `dU/dt` integrated; (P,h) computed from (M,U)
- Fallback: If CoolProp state creation fails, use surrogate models (logged as `fallback_uses`)
- Max retries: Substeps doubled up to 102 on failure

---

## Result Caching

| Task | Canonical Function | File |
|------|-------------------|------|
| Compute run ID | `tf-results::compute_run_id(system, run_type, solver_version)` | `crates/tf-results/src/hash.rs` |
| Check cache | `tf-results::RunStore::has_run(run_id)` | `crates/tf-results/src/store.rs` |
| Load from cache | `RunStore::load_manifest(run_id)` + `load_timeseries(run_id)` | `crates/tf-results/src/store.rs` |
| Save result | `RunStore::save_run(manifest, records)` | `crates/tf-results/src/store.rs` |
| List runs | `RunStore::list_runs()` | `crates/tf-results/src/store.rs` |

**Cache location**: `<project-dir>/.thermoflow/runs/<run-id>/`

**Files**:
- `manifest.json` — metadata (system_id, timestamp, parameters)
- `timeseries.jsonl` — one JSON line per `TimeseriesRecord`

---

## Result Querying

| Task | Canonical Function | File |
|------|-------------------|------|
| Load manifest | `RunStore::load_manifest(run_id)` | `crates/tf-results/src/store.rs` |
| Load time-series | `RunStore::load_timeseries(run_id)` | `crates/tf-results/src/store.rs` |
| Get run summary | `tf-app::query::get_run_summary(records)` | `crates/tf-app/src/query.rs` |
| Extract node series | `tf-app::query::extract_node_series(records, node_id, property)` | `crates/tf-app/src/query.rs` |
| Extract component series | `tf-app::query::extract_component_series(records, comp_id, property)` | `crates/tf-app/src/query.rs` |
| List node IDs | `tf-app::query::list_node_ids(records)` | `crates/tf-app/src/query.rs` |
| List component IDs | `tf-app::query::list_component_ids(records)` | `crates/tf-app/src/query.rs` |

---

## GUI Run Workflow

**Entry point**: User clicks "Run" button in `System` workspace

| Step | Code Location | Function |
|------|---|---|
| 1. User specifies params | `tf-ui/src/views/run_view.rs` | User input collection |
| 2. Create RunRequest | `tf-ui/src/run_worker.rs` | `RunWorker::spawn(request)` |
| 3. Spawn worker thread | `tf-ui/src/run_worker.rs` | `std::thread::spawn` |
| 4. Call backend | `tf-ui/src/run_worker.rs` | `tf-app::run_service::ensure_run_with_progress(...)` |
| 5. Send progress | `tf-ui/src/run_worker.rs` | `progress_tx.send(RunProgressEvent)` (throttled) |
| 6. Poll worker | `tf-ui/src/app.rs::poll_worker()` | `worker.progress_rx.try_recv()` |
| 7. Update UI state | `tf-ui/src/app.rs` | `self.latest_progress = Some(event)` |
| 8. Render progress | `tf-ui/src/app.rs` lines 825–870 | Display stage, elapsed, progress bar |
| 9. On completion | `tf-ui/src/app.rs` | Receive `WorkerMessage::Complete { timing, ... }` |
| 10. Update run list | `tf-ui/src/views/` | Refresh run pane with new run |

**Progress rendering**:
- Stage label and elapsed time in headers
- Transient: `[=====----] 56.2%` progress bar with `t={sim}/{end}s | step={} | cb={}`
- Steady: Spinner with iteration/residual info
- Both: Wall-clock measurements

---

## CLI Run Workflow

**Entry point**: `cargo run -p tf-cli -- run (steady|transient) <project> <system> ...`

| Command | Implementation | File |
|---------|---|---|
| `run steady` | `cmd_run_steady()` | `apps/tf-cli/src/main.rs` |
| `run transient` | `cmd_run_transient()` | `apps/tf-cli/src/main.rs` |
| `validate` | `cmd_validate()` | `apps/tf-cli/src/main.rs` |
| `systems` | `cmd_systems()` | `apps/tf-cli/src/main.rs` |
| `runs` | `cmd_runs()` | `apps/tf-cli/src/main.rs` |
| `export-series` | `cmd_export_series()` | `apps/tf-cli/src/main.rs` |

**Progress rendering** (for both steady and transient):
- Calls `ensure_run_with_progress(request, Some(&mut callback))`
- Callback renders to terminal:
  - Transient: `[#####---] 45.2%` progress bar with details, spinner animation
  - Steady: Spinner with stage + iteration info
- On completion: prints `RunTimingSummary` with breakdown and mode-specific stats

---

## Thermodynamic Properties

| Task | Canonical Interface | Primary Implementation | File |
|------|---|---|---|
| Create state from (P,T) | `FluidModel::state(StateInput::PT { p, t }, comp)` | `CoolPropModel` | `crates/tf-fluids/src/coolprop.rs` |
| Create state from (P,h) | `FluidModel::state(StateInput::PH { p, h }, comp)` | `CoolPropModel` | `crates/tf-fluids/src/coolprop.rs` |
| Fallback on CoolProp error | `TransientFallbackPolicy::state()` | Surrogate fit + estimation | `crates/tf-app/src/transient_fallback_policy.rs` |

**Composition types**:
- Pure: Single species (N₂, O₂, H₂, CH₄, H₂O, CO₂, etc.)
- Mixture: Multiple species with mole/mass fractions

---

## Component Physics

**Interface**: All components implement `TwoPortComponent` trait (or equivalent)

| Component | Canonical Model | File |
|-----------|---|---|
| Orifice | `mdot = Cd·A·sqrt(2·ρ·|ΔP|)` | `crates/tf-components/src/orifice.rs` |
| Valve | Orifice with position-dependent area | `crates/tf-components/src/valve.rs` |
| Pipe | `mdot` via Darcy-Weisbach | `crates/tf-components/src/pipe.rs` |
| Pump | Isentropic + efficiency | `crates/tf-components/src/pump.rs` |
| Turbine | Isentropic + efficiency | `crates/tf-components/src/turbine.rs` |

All components are deterministic functions of (P_in, h_in, P_out, h_out, geometry, control inputs).

---

## Validation

| Rule | Check Location | Error Type |
|------|---|---|
| Timed valve schedules | `tf-project::validate.rs` | `ValidationError::Unsupported` |
| Scheduled atmosphere actions | `tf-project::validate.rs` | `ValidationError::Unsupported` |
| Invalid node kinds | `tf-project::validate.rs` | `ValidationError::InvalidScheme` |
| Missing required fields | `tf-project::validate.rs` | `ValidationError::MissingField` |

---

## Progress Types

**Emitted by**: Backend solvers  
**Consumed by**: tf-app → CLI/GUI via callback

```rust
pub struct RunProgressEvent {
    pub mode: RunMode,
    pub stage: RunStage,
    pub elapsed_wall_s: f64,
    pub message: Option<String>,
    pub steady: Option<SteadyProgress>,
    pub transient: Option<TransientProgress>,
}

pub enum RunStage {
    LoadingProject,
    CheckingCache,
    LoadingCachedResult,
    CompilingRuntime,
    BuildingSteadyProblem,
    SolvingSteady,
    RunningTransient,
    SavingResults,
    Completed,
}

pub struct SteadyProgress {
    pub outer_iteration: Option<usize>,
    pub max_outer_iterations: Option<usize>,
    pub iteration: Option<usize>,          // Newton iteration within outer loop
    pub residual_norm: Option<f64>,
}

pub struct TransientProgress {
    pub sim_time_s: f64,
    pub t_end_s: f64,
    pub fraction_complete: f64,             // sim_time / t_end
    pub step: usize,
    pub cutback_retries: usize,
    pub fallback_uses: Option<usize>,
}
```

---

## Golden Rules for Maintainers

1. **All user-facing runs go through `tf-app::run_service::ensure_run()`**
   - No alternative execution paths
   - Ensures cache consistency, progress reporting, timing

2. **CLI and GUI have feature parity**
   - Both call same backend services
   - If features diverge, refactor to shared code
   - CLI is source of truth for reproducibility

3. **Validation is upfront**
   - Unsupported features rejected at load time with clear error messages
   - No silent feature skipping or degradation

4. **Progress reporting uses shared types**
   - No separate progress structs for CLI vs GUI
   - Frontends render shared `RunProgressEvent` differently, but data is identical

5. **Thermodynamic fallback is transparent**
   - Logged (fallback_uses counter)
   - Never crashes
   - Captured in final diagnostics

6. **Caching is transparent**
   - Run ID computed deterministically
   - Cache hits invisible to user (but reported in timing)
   - Cache misses trigger full recompute

---

## Related Documents

- **CURRENT_STATE_AUDIT.md** — Detailed code audit with examples and architecture deep-dives
- **ARCHITECTURE.md** — Design principles and workspace vision
- **TRANSIENT_CAPABILITIES.md** — Transient support matrix, known limitations, diagnostics
- **ROADMAP.md** — Development timeline and phase dependencies

---

**Last Updated**: 2026-02-27  
**Status**: Verified against codebase; all paths tested and passing

