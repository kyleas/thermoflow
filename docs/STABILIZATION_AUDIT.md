# Stabilization Audit (2026-02)

This document records the current canonical execution paths, superseded paths, unfinished areas, and support matrix for the stabilization/observability sprint.

## 1) Canonical Paths

### Project validation
- Canonical entry: `tf_app::project_service::validate_project`
- Core validation logic: `tf-project` (`validate.rs` and schema-level checks)
- Frontends (`tf-cli`, `tf-ui`) should call the service layer, not custom validators.

### Runtime compilation
- Canonical path: `tf_app::runtime_compile::compile_system`
- Boundary handling (including atmosphere): `parse_boundaries_with_atmosphere`
- Component model construction: `build_components`

### Steady run execution
- Canonical entry: `tf_app::run_service::ensure_run_with_progress`
- Solve path: `tf_solver::solve_with_progress` (new observer-backed progress events)
- Stages: load project → cache check → compile → build problem → solve → save

### Transient run execution
- Canonical entry: `tf_app::run_service::ensure_run_with_progress`
- Model build path: `tf_app::transient_compile::TransientNetworkModel`
- Integrator path: `tf_sim::run_sim_with_progress`
- Progress based on simulated time fraction (`sim_time / t_end`) and step count

### Run persistence
- Canonical path: `tf_results::RunStore`
- Storage shape: project-local `.thermoflow/runs/<run-id>/manifest.json` + `timeseries.jsonl`
- Run identity: `tf_results::compute_run_id`

### Run query/plot loading
- Canonical backend query path: `tf_app::query`
- GUI data loading path: `tf_results::RunStore::load_timeseries` with view-level memoization (`pid_view`, `plot_view`)

### GUI run worker path
- Canonical GUI execution: `apps/tf-ui/src/run_worker.rs`
- Worker now consumes backend progress API (`ensure_run_with_progress`) and forwards snapshots to UI.

## 2) Superseded / Obsolete / Duplicate Paths

### Superseded but retained (explicitly marked)
- `apps/tf-ui/src/transient_model.rs`
  - Marked as legacy/editor-research path.
  - Not canonical for production run execution.
- `apps/tf-ui/src/project_io.rs`
  - Marked as legacy UI helper path.
  - Canonical compile/run path is in `tf-app`.

### Removed/replaced ambiguity
- Previous placeholder UI progress messages (`step/total`) in `run_worker` were replaced with shared backend `RunProgressEvent` payloads.
- Run timing was implicit and scattered (`Timer` labels); now explicit structured timing summary is returned from `RunResponse`.

## 3) Known Unfinished Areas (Intentional)

- Valve schedule transients remain unsupported by validation design.
- Some fixed-valve blowdown configurations are still numerically fragile at larger horizons.
- GUI still contains editor-local compile helpers for non-run interactions; execution remains backend-canonical.
- Progress event persistence is runtime-only (not yet written to run artifacts).

## 4) Current Support Matrix

### Supported
- Steady runs through shared service path (CLI + GUI parity)
- Simple transient venting workflows with fixed components
- Atmosphere/reservoir node boundaries
- Live backend progress + timing summaries in CLI and GUI

### Unsupported (explicitly rejected)
- Timed valve schedules (`SetValvePosition`) in transient projects

### Experimental
- More complex multi-CV / continuation-heavy transients
- Cases that rely on fallback surrogate activation for state recovery

## 5) Progress & Timing API Summary

### Shared API
- `tf_app::progress::{RunProgressEvent, RunStage, SteadyProgress, TransientProgress}`
- `tf_app::run_service::ensure_run_with_progress`
- `tf_app::run_service::RunTimingSummary`

### Steady progress semantics
- Stage-based + solver details (outer iteration, Newton iteration, residual norm)
- No fake percent completion is presented

### Transient progress semantics
- Fraction complete from simulated time (`sim_time / t_end`)
- Step count and cutback retry count included
- Fallback use count included in timing summary

## 6) Cleanup/Optimization Changes in This Sprint

1. Unified backend progress emission for both frontends (removed duplicate ad hoc reporting paths).
2. Added solver/integrator observer hooks instead of frontend-side polling heuristics.
3. Added UI-side throttling for progress event rendering to avoid unnecessary redraw pressure.
4. Added structured timing summary in run response for concise CLI/GUI reporting.
