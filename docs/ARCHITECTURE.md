# Thermoflow Architecture

## 1. Vision

Thermoflow is a **unified engineering workbench** for designing, analyzing, and optimizing thermo-fluid and propulsion systems. It combines:

- **System simulation**: steady-state and transient modeling of fluid networks
- **Fluid property exploration**: thermodynamic and transport properties  
- **Cycle analysis**: propulsion system design and matching
- **Interactive plotting and analysis**: result visualization and parametric studies

Thermoflow targets long-term replacement of:

- **Visio**: for P&ID and system diagrams
- **RefProp**: for fluid property calculations and state point visualization
- **RPA**: for engine design, turbopump selection, and cycle analysis
- **Excel**: for thermo-fluid modeling workflows

## 2. Product Model

Thermoflow is **one unified application**, not a collection of separate tools.

Users operate within a single project containing:
- One system definition (P&ID, nodes, components, boundaries)
- One project-level run cache
- One shared simulation backend and fluid database

Within that project, users switch between **workspaces**—specialized UI views—each serving a distinct purpose:
- All workspaces operate on the same project file
- All workspaces share the same run cache
- All workspaces use identical backend simulation services
- State from one workspace is visible in another (e.g., a result from System workspace appears in Analysis workspace)

## 3. Workspace Model

### 3.1 System Workspace

**Purpose**: Define and simulate fluid networks; visualize system state.

**User activities**:
- Draw P&ID diagrams (add/edit nodes, components, connections)
- Set boundary conditions
- Define component parameters (orifice discharge coefficient, valve setting, etc.)
- Execute steady-state and transient simulations
- View nodal state overlays (pressure, temperature, flow) on the diagram
- Inspect individual run results

**Key capabilities**:
- P&ID editor with drag-and-drop components
- Real-time diagram validation
- State overlays (color-coded pressure/temperature/flow on nodes/components)
- Run execution with progress feedback
- Result inspector (node summaries, component flows)

**Node kinds**:
- **Junction**: algebraic node with no storage
- **ControlVolume**: finite storage with transient mass/energy state
- **Atmosphere**: infinite reservoir boundary with fixed pressure/temperature (no storage, no enthalpy solve)

**Backend services**:
- tf-project (project schema, validation)
- tf-graph (network topology)
- tf-components (component models)
- tf-solver (steady-state simulation)
- tf-sim (transient simulation)
- tf-results (run caching and time-series storage)
- tf-app (shared application services)

**What it should NOT own**:
- Physics of individual components (use tf-components)
- Equation solving (use tf-solver)
- Fluid properties (use tf-fluids)
- Project persistence (use tf-project)

### 3.2 Fluid Workspace

**Purpose**: Compute and inspect single-point equilibrium fluid states using canonical backend services.

**User activities (MVP)**:
- Select a supported species/fluid
- Select input pair (`P-T`, `P-h`, `rho-h`, `P-s`)
- Enter two inputs and compute one equilibrium state
- Inspect full property table (`P`, `T`, `rho`, `h`, `s`, `cp`, `cv`, `gamma`, `a`, phase/quality when available)
- Persist workspace selection and inputs with the project

**Key capabilities (MVP)**:
- RefProp-like single-state calculator UI
- Canonical backend-first state computation in `tf-fluids`
- Explicit supported input-pair model
- Project-level persistence for fluid workspace state
- Extensible foundation for later sweeps/plots

**Backend services**:
- tf-fluids (thermodynamic models, RefProp wrapper)
- tf-core (unit handling)
- tf-results (optionally store state point references)

**What it should NOT own**:
- Thermodynamic calculation engine (use tf-fluids)
- System simulation (use System workspace)
- Cycle design logic (use Cycle workspace)

**Planned post-MVP**:
- Property sweeps and property-vs-property plotting
- Saved state-point libraries/history
- Additional phase/saturation visual tools

### 3.3 Cycle Workspace

**Purpose**: Design and analyze complete propulsion cycles (engines, pumps, turbines); match components.

**User activities**:
- Build cycle schematics (inlet, compressor, combustor, turbine, nozzle stages)
- Set inlet conditions, pressure ratios, component efficiencies
- Perform matching calculations (turbine power = compressor power)
- Run parametric sweeps (vary compression ratio, find optimal design)
- Export cycle summary and component sizing tables

**Key capabilities**:
- Cycle definition builder (series/parallel component chains)
- Rapid cycle calculations with CEA integration
- Scaling and sensitivity tools
- Trade studies and optimization setup

**Backend services**:
- tf-solver (thermodynamic equations, iteration)
- tf-combustion-cea (combustion equilibrium; future)
- tf-fluids (fluid properties)
- tf-results (store cycle designs and runs)

**What it should NOT own**:
- Combustion thermodynamics (use tf-combustion-cea)
- Detailed P&ID simulation (use System workspace)
- Individual component physics (reference tf-components)

### 3.4 Analysis Workspace

**Purpose**: Visualize and compare simulation results; perform parametric studies and post-processing.

**User activities**:
- Plot result time-series (pressure history, flow dynamics)
- Compare multiple runs side-by-side (sensitivity, optimization)
- Create plots for export (presentation, publication)
- Perform parameter sweeps and result table generation
- Set up optimization/calibration studies (future)

**Key capabilities**:
- Time-series plotting (Cartesian, custom axes, multiple series)
- Overlay and comparison tools
- Export to CSV, PDF, images
- Parameter sweep matrix generation
- Statistics and curve fitting (future)

**Backend services**:
- tf-results (load and query run data)
- tf-app (result retrieval services)
- plotting library (egui-plot or similar)

**What it should NOT own**:
- Simulation execution (use System or Cycle workspace)
- Project editing (use System workspace)
- Advanced optimization (future separate tool or workspace)

## 4. Core Backend Architecture

### 4.1 Current Crates

| Crate | Role | Responsibility |
|-------|------|-----------------|
| **tf-core** | Foundations | Unit system, ID types, error traits |
| **tf-graph** | Topology | Graph structure for fluid networks |
| **tf-project** | Schema | Project file format and validation |
| **tf-fluids** | Properties | Thermodynamic/transport models; RefProp wrapper |
| **tf-components** | Component models | Orifice, pipe, pump, turbine, valve, LineVolume physics |
| **tf-solver** | Steady solution | Linear/nonlinear system solving; steady-state simulator |
| **tf-sim** | Transient | Integration schemes; transient simulator |
| **tf-controls** | Control systems | Signal graph, controllers, actuators, sampled execution |
| **tf-results** | Storage | Run manifests, time-series record storage, caching |
| **tf-app** | Services | Shared application logic (no duplication between CLI/GUI) |

### 4.2 Planned Crates

| Crate | Purpose | Timeline |
|-------|---------|----------|
| **tf-combustion-cea** | Combustion/equilibrium via CEA | Phase 5 |
| **tf-cycle** | Cycle design/matching tools | Phase 6 |
| **tf-optimization** | Parameter studies, sensitivity, optimization | Phase 7 |

### 4.3 Frontends

| Frontend | Role |
|----------|------|
| **tf-cli** | Command-line interface for automation, scripting, debugging |
| **tf-ui** | Desktop application with multiple workspaces |

Both use **tf-app** for all business logic. Neither duplicates simulation, I/O, or caching.

### 4.4 Control System Architecture

Thermoflow includes a **separate signal/control domain** layered on top of the fluid network. Controls and actuation enable closed-loop system modeling where:

- **Measured variables** extract quantities from the fluid system (pressure, temperature, flow)
- **Controllers** process measurements and compute control actions (PI, PID)
- **Actuators** introduce physical dynamics between controller output and system response (valve position lag, rate limiting)
- **Sampled execution** models digital controller timing (discrete sample rates, zero-order hold)

#### 4.4.1 Separation of Concerns

The control graph is **separate from the fluid graph**:

- **Fluid domain** (`tf-graph`, `tf-components`, `tf-solver`, `tf-sim`): Conservation equations, component physics, steady/transient integration
- **Control domain** (`tf-controls`): Signal graph, controller blocks, actuator dynamics, sampled timing

This separation ensures:
- Clean architecture with clear responsibilities
- Independent evolution of fluid and control models
- Explicit measured-variable and actuator-command interfaces
- Extensible signal processing (future: cascade control, feedforward, gain scheduling)

#### 4.4.2 Signal Graph Model

The control graph consists of:

- **Signal blocks**: Sources (constants, measured variables), processors (controllers), sinks (actuator commands)
- **Signal connections**: Directed edges carrying scalar (`f64`) signals
- **Evaluation order**: Topological sort ensures all inputs are evaluated before processing

Signal types are currently scalar (`f64` only). Future extensions may include vector signals or boolean/discrete signals.

#### 4.4.3 Controller Execution

Controllers operate in **sampled/digital mode**:

- Each controller has a configurable sample period (e.g., 10 Hz → 0.1s period)
- Controller updates occur only at sample times
- Outputs are held constant between samples (zero-order hold)
- Transient integrator may sub-step between controller updates

This models realistic digital control systems where:
- Sensors have finite sample rates
- Control computers execute at discrete intervals
- Physical actuators introduce continuous dynamics separate from discrete control logic

#### 4.4.4 Actuator Dynamics

Actuators model physical limitations:

- **First-order lag**: Time constant `τ` models mechanical/electrical response time
- **Rate limiting**: Maximum velocity constraint (e.g., valve motor speed)
- **Position clamping**: Output bounded to [0, 1] for valve position

Actuator integration occurs at the transient integrator timestep (continuous dynamics), separate from discrete controller execution.

#### 4.4.5 tf-controls Crate Contents

`tf-controls` provides:

- `Signal`, `SignalId`, `SignalValue`: Signal type primitives
- `ControlGraph`, `SignalEdge`, `BlockId`: Graph structure
- `SignalBlock`, `SignalSource`, `SignalProcessor`, `SignalSink`: Block abstractions
- `MeasuredVariableRef`: References to fluid system quantities
- `PIController`, `PIDController`: Feedback controller implementations
- `FirstOrderActuator`, `ActuatorState`: Valve/actuator dynamics
- `SampleClock`, `SampleConfig`, `ZeroOrderHold`: Sampled execution primitives

All control logic is backend-first and shared between CLI and GUI.

#### 4.4.6 Control Schema and Runtime Wiring

Control definitions live per-system under `system.controls` in project schema:

- `controls.blocks[]`: constants, measured variables, PI/PID controllers, first-order actuators, actuator-command sinks
- `controls.connections[]`: directed signal edges with explicit destination input ports (`setpoint`, `process`, `command`, `position`)

Canonical backend path for transient closed-loop execution:

1. `tf-project` validates control block parameters, references, connection topology, and graph acyclicity
2. `tf-app::transient_compile::build_control_runtime()` compiles schema blocks into executable runtime blocks
3. `TransientNetworkModel::solve_snapshot()` advances sampled controller/actuator state and applies valve position overrides
4. Steady snapshot solve computes fluid response with updated valve positions
5. Measured variables are extracted from solved fluid state and fed back to control graph
6. Transient result records persist control histories in `global_values.control_values`

Supported measured-variable references in this phase:

- Node pressure
- Node temperature
- Edge mass flow
- Pressure drop

Intentionally unsupported in this phase:

- Timed valve schedules (`ActionDef::SetValvePosition` remains validation-rejected)
- GUI control-graph editing UX
- Advanced controller structures (feedforward/cascade/gain scheduling)

## 5. Shared Services Principle

### 5.1 One Project Model

All workspaces and both frontends operate on a single project file:

```
project.yaml
├── systems[]        (P&ID definitions)
├── fluid            (composition, model choice)
├── metadata         (author, version, tags)
```

The project is persisted to disk via `tf-project` and `tf-app::project_service`.

### 5.2 One Run Cache

Simulation results live in a project-local run store:

```
<project-dir>/.thermoflow/runs/<run-id>/
├── manifest.json   (system_id, timestamp, parameters)
├── timeseries.jsonl (state history for transient; single record for steady)
```

Run identity is computed from system definition + parameters, ensuring deterministic caching.

### 5.3 One Simulation Backbone

All simulation is either:

- **Steady-state**: `tf-solver::solve()` → single state snapshot
- **Transient**: `tf-sim::run_sim()` with `tf-app::transient_compile::TransientNetworkModel` → time-series snapshots

No workspace duplicates solving logic. Both CLI and GUI call **tf-app::run_service::ensure_run()**, which handles caching, execution, and result storage uniformly for steady and transient modes.

Current supported transient envelope includes:
- Single-CV fixed-component venting to atmosphere
- Fixed-topology multi-control-volume transients (no timed component schedules)

Timed valve opening/closing schedules remain explicitly unsupported and are rejected in validation.

### 5.3.1 Initialization Strategy

The solver uses configurable **initialization strategies** to control startup behavior and regularization:

- **Strict**: Minimal regularization, tight tolerances; used for steady-state and single-CV transients
- **Relaxed**: Aggressive regularization, relaxed tolerances; used for multi-CV transients and LineVolume components

Initialization strategy is automatically determined based on system characteristics (CV count, LineVolume presence). The selected strategy is visible in diagnostics and timing summaries for troubleshooting startup issues.

Strategies control Newton solver parameters: weak-flow regularization scale, enthalpy residual tolerances, and initial guess generation.

### 5.4 CLI and GUI Parity

Both frontends (tf-cli, tf-ui) use identical services:

```
tf-cli run steady project.yaml system-id
# executes: tf_app::run_service::ensure_run(request)
# result cached, queryable by both CLI and GUI

cargo run -p tf-ui
# GUI open same project, same run cache
```

This ensures:
- No ghost results (one frontend doesn't see other's runs)
- Reproducible debugging (CLI can replicate GUI session)
- Rapid iteration (`tf-cli` for quick tests, `tf-ui` for interactive exploration)

### 5.5 Shared Progress and Timing Reporting

Run execution progress and timing are emitted from backend shared services:

- `tf_app::run_service::ensure_run_with_progress`
- `tf_app::progress::{RunProgressEvent, RunStage, SteadyProgress, TransientProgress}`
- `tf_app::run_service::RunTimingSummary`

Steady runs report stage + iteration/residual information (no fake percent).
Transient runs report simulated-time progress fraction (`sim_time / t_end`), step count, and cutback retry counts.

`RunTimingSummary` includes:
- Initialization strategy (`Strict`/`Relaxed`)
- Compile/build/solve/save/total timings
- Steady iterations and residual (steady mode)
- Step count, cutbacks, fallback uses (transient mode)
- Real-fluid attempts/successes and surrogate update counts (transient diagnostics)

`tf-cli` and `tf-ui` consume this shared API and must not implement separate execution progress logic.

## 6. UI Architecture Principles

### 6.1 One Shell, Multiple Workspaces

The UI is a single application with a **workspace switcher**:

```
┌─────────────────────────────────────────────────┐
│  Thermoflow  [Project: engine_cycle.yaml]      │
├────────────┬────────────┬─────────┬────────────┤
│  System    │ Fluid      │ Cycle   │  Analysis  │  ← workspace tabs
├─────────────────────────────────────────────────┤
│                                                  │
│           [Current workspace content]            │
│                                                  │
│                                                  │
└─────────────────────────────────────────────────┘
```

Each workspace is:
- Independently rendered
- Independent of other workspaces' state (except shared project/run data)
- Lazy-loaded (only active workspace is drawn each frame)
- Focused on one task

### 6.2 Minimal Clutter

- No workspace-A data visible in workspace-B unless explicitly relevant
- Context-sensitive inspector panel (shows details of selected item)
- Toolbar and menu consistent across workspaces
- Project-level settings in one place (not repeated per workspace)

### 6.3 Context Inspector

A split-pane layout with:

- **Left**: main workspace content (P&ID, property table, cycle diagram, plot)
- **Right**: inspector panel (properties, metadata, history of selection)

The inspector updates as user selects items in the left pane.

### 6.4 Persistent Layout State

User's workspace layout, splits, window size, zoom, scroll position persist across sessions via:

```
~/.thermoflow/ui_state.json
{
  "last_workspace": "System",
  "last_project": "path/to/engine_cycle.yaml",
  "split_ratio": 0.7,
  "window_size": {"width": 1400, "height": 900}
}
```

### 6.5 System Workspace as Physical Source of Truth

The **System workspace P&ID** is the primary definition of:
- Network topology (nodes, components, connections)
- Component parameters
- Boundary conditions

Other workspaces visualize or analyze data *derived* from the System definition. They do not modify it.

### 6.6 State Overlays

The P&ID in System workspace renders component/node properties as visual overlays:

- Color coding: pressure ranges (e.g., blue = low, red = high)
- Annotations: numerical values on nodes/edges
- Hover tooltips: full property details

Overlays come from the latest run result, ensuring the diagram stays visually synchronized with simulation state.

## 7. Data Ownership

| Artifact | Owner | Storage | Mutability |
|----------|-------|---------|-----------|
| Project definition (systems, nodes, component params) | tf-project | project.yaml on disk | User (via System workspace) |
| Layout/view state | UI app | ~/.thermoflow/ui_state.json | User (implicit via GUI) |
| Run manifests + timeseries | tf-results | <project-dir>/.thermoflow/runs/ | Read-only (written by simulator) |
| Cached analysis (e.g., sweep results) | tf-results or tf-app | ~/.thermoflow/analysis/ | Read-only until recomputed |
| Font, theme, keybinding preferences | UI app | ~/.thermoflow/preferences.yaml | User |
| Fluid property database | tf-fluids | Embedded RefProp or online | Read-only (external source) |

## 8. Roadmap Dependencies

### 8.1 Phase 1 (Foundation)

- Core simulation and P&ID editor

**Enables**: Basic steady-state workflow.

### 8.2 Phase 2 (Service Layer)

- Unified tf-app services
- CLI/GUI parity

**Enables**: Reproducible debugging, automation, parallel development.

### 8.3 Phase 3 (P&ID Editor)

- Full P&ID editing (drag-drop, constraints, alignment, grouping)
- State overlays on diagram

**Enables**: Visual feedback, faster design iteration.

### 8.4 Phase 4 (Fluid Workspace)

- Fluid property explorer
- Property plots
- State point history

**Depends on**: tf-fluids maturity, plotting library integration.

**Enables**: Direct RefProp replacement, standalone fluid investigation.

### 8.5 Phase 5 (Combustion)

- CEA integration (tf-combustion-cea)
- Equilibrium calculations

**Depends on**: CEA library binding, composition handling in tf-project.

**Enables**: Propellant/oxidizer selection, chamber condition prediction.

### 8.6 Phase 6 (Cycle Workspace)

- Cycle design tools
- Component matching
- Turbopump sizing

**Depends on**: Phase 5 (combustion), tf-solver maturity.

**Enables**: RPA-equivalent cycle analysis.

### 8.7 Phase 7 (Analysis)

- Advanced result comparison
- Optimization/sensitivity framework
- Calibration tools

**Depends on**: Phases 4–6, external optimization libs.

**Enables**: Design space exploration, validation against test data.

## 9. Design Principles

### 9.1 Backend First, UI Thin

- All business logic lives in crates (tf-solver, tf-fluids, tf-app, etc.)
- UI concerns only: rendering, layout, user input dispatch
- Any algorithm worth implementing is worth testing → it belongs in a library crate

### 9.2 Deterministic CLI Path

- CLI (tf-cli) is the gold standard for reproducibility
- `cargo run -p tf-cli -- run steady project.yaml system-id` must always produce identical results
- GUI mirrors CLI behavior; if they diverge, CLI is correct

### 9.3 Schema Versioning

- Project file format evolves with clear backward-compatibility strategy
- Run manifests include schema version and solver version
- Migration utilities in tf-project for upgrading old projects

### 9.4 No Duplicated Business Logic

- Feature rule: if two frontends (CLI, GUI) share logic, it goes in tf-app or lower
- Corollary: GUI never contains if-let simulation branching; it calls tf-app

### 9.5 Clear Sign Conventions

- Flow direction: positive = design direction (left-to-right or inlet-to-outlet)
- Pressure drop: negative Δp in flow direction
- Rotation speed: positive = nominal direction (turbine spin-down is negative)
- Document all sign conventions in component models (tf-components)

### 9.6 Robust Save/Load and Validation

- Project can always be loaded, even if partially corrupted
- Validation is eager (at load time) and provides clear error messages
- Run cache is append-only; old runs never disappear
- Result queries are forgiving (missing field → sensible default or error, not panic)

### 9.7 Testing Over Documentation

- Core crates (tf-solver, tf-fluids, tf-components) have >80% unit test coverage
- Integration tests verify end-to-end flows (CLI run, result export, etc.)
- Manual tests covered by automated regression suite

## 10. Near-Term Priorities

1. **Strengthen Shared Services** (Phase 2 continuation)
   - Fix remaining transient simulator issues
   - Expand tf-app to handle more query patterns
   - Add robust error messages

2. **Improve P&ID Editor** (Phase 3)
   - Drag-drop components from palette
   - Real-time diagram validation feedback
   - State vector overlays (pressure/temp on nodes)

3. **Build Fluid Workspace** (Phase 4)
   - Property browser and plots
   - State point table and history
   - Export property data to CSV

4. **Integrate CEA** (Phase 5 start)
   - Bind CEA equilibrium library
   - Add tf-combustion-cea crate
   - Pressure/mixture ratio sweep calculations

5. **Enable Cycle Workspace** (Phase 6 start)
   - Design builder UI
   - Component matching solver
   - Turbopump selection tools

6. **Infrastructure**
   - Set up continuous integration (tests on PRs)
   - Establish code review process
   - Create style guide for future contributions

---

**Document version**: 1.0  
**Last updated**: 2026-02-26  
**Status**: Active reference for development
