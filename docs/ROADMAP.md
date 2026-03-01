# Thermoflow Roadmap

## Overview

Thermoflow evolves in seven phases, each building on the previous to add capabilities and workspaces. Each phase is estimated in delivery timeline but prioritized by dependencies and user value.

---

## Phase 1: Core Simulation and Editing Foundation

**Timeline**: Complete (foundational work started 2024)

**Objective**: Establish steady-state simulation backbone and basic P&ID editor.

**Key Deliverables**:
- Steady-state solver (tf-solver) with convergence and robustness
- P&ID editor with basic node/component creation and editing
- Project file format (YAML) and schema
- Run caching and result storage
- CLI proof-of-concept

**Dependencies**: None (foundation)

**Why it matters**:
- Proves the simulation approach works
- Establishes project/run persistence model
- Enables user iteration on system designs

**Status**: Core crates stable; P&ID editor functional but basic

---

## Phase 2: Shared App Services and CLI/GUI Unification

**Timeline**: Complete (2026-02)

**Objective**: Eliminate duplicate business logic; enable both CLI and GUI backends to coexist with identical behavior.

**Key Deliverables**:
- tf-app crate with project, compilation, execution, and query services
- Full CLI with subcommands (validate, systems, run, export-series)
- GUI refactored to use tf-app (remove duplicated run_worker logic)
- Transient simulation unified through tf-app (moved from tf-ui)
- Integration test suite for end-to-end flows
- Architecture documentation
- Explicit Atmosphere node for fixed-reservoir boundaries (improves venting/transient robustness)
- Shared backend progress/timing API consumed by both CLI and GUI

**Dependencies**: Phase 1 (core simulation works)

**Why it matters**:
- CLI becomes the gold standard for reproducibility
- GUI can focus on UX instead of reinventing simulation
- Easier to add future frontends (web, batch tools)
- Clear separation of concerns
- Transient logic no longer duplicated in GUI

**Status**: ✅ Complete (including transient unification)

**Stabilization update (2026-02-27)**:
- Fixed-topology multi-control-volume transient benchmarks are now part of the supported envelope
- Timed valve schedules remain explicitly unsupported

---

## Phase 2A: Controls and Actuation Foundation

**Timeline**: Core integration complete (2026-02-27)

**Objective**: Add control system architecture for closed-loop transient simulations with measured variables, controllers, and actuator dynamics.

**Key Deliverables (Foundation - Complete)**:
- ✅ tf-controls crate with signal graph architecture
- ✅ Separate control/signal domain distinct from fluid network
- ✅ Scalar signal types and block abstractions (sources, processors, sinks)
- ✅ PI and PID controller implementations with anti-windup
- ✅ FirstOrderActuator with rate limiting and position clamping
- ✅ Sampled execution primitives (SampleClock, ZeroOrderHold)
- ✅ Measured variable reference definitions (node pressure/temperature, edge flow)
- ✅ Comprehensive unit tests for all control primitives

**Key Deliverables (Completed in this step)**:
- ✅ Schema extensions for control blocks and control graph wiring in project YAML
- ✅ Validation for references, malformed topology, parameter ranges, and graph cycles
- ✅ Runtime control graph compilation in `tf-app::transient_compile`
- ✅ Sampled controller execution integrated into transient loop
- ✅ Measured-variable extraction from transient runtime state
- ✅ Actuator output wired to runtime valve position overrides
- ✅ Two end-to-end control examples (`09`, `10`)
- ✅ Control history persistence in transient results

**Key Deliverables (Remaining - Future)**:
- GUI control graph editing and signal wiring UX
- Dedicated control plotting/report views in GUI
- Advanced control patterns (feedforward, cascade, gain scheduling)

**Dependencies**: Phase 2 (transient simulation works)

**Why it matters**:
- Enables realistic control system modeling (valves don't just snap open/closed)
- Models digital controller timing (sample rates, zero-order hold)
- Physical actuator dynamics (lag, rate limiting)
- Supports closed-loop pressure regulation, flow control, temperature control
- Foundation for advanced control strategies (cascade, feedforward, model-based)

**Status**: ✅ Backend closed-loop control path complete (GUI editing/reporting still pending)

---

## Phase 3: P&ID Editor Maturity

**Timeline**: ~2 months (2026-04 target)

**Objective**: Make the P&ID editor production-ready for complex systems.

**Key Deliverables**:
- Drag-and-drop component palettes (Orifice, Valve, Pump, Turbine, Pipe, etc.)
- Connection routing and constraint checking
- Auto-layout and manual alignment tools
- Real-time validation feedback (missing boundaries, isolated nodes, etc.)
- State vector overlays on diagram (pressure, temperature, flow color-coded)
- Zoom, pan, and multi-select editing
- Copy/paste and grouping (future: hierarchical blocks)

**Dependencies**: Phase 2 (architecture stable)

**Why it matters**:
- Replaces Visio for P&ID design and documentation
- Visual feedback accelerates iteration
- Complex systems become manageable

---

## Phase 4: Fluid Workspace (RefProp Replacement)

**Timeline**: ~3 months (2026-07 target)

**Objective**: Standalone fluid property exploration; replace RefProp GUI for quick calculations.

**Status update (2026-02-28)**:
- ✅ MVP delivered: single-state calculator workspace with project persistence
- 🔄 Remaining for full phase: sweeps, property plotting, state-point library/history

**MVP delivered**:
- Dedicated Fluid workspace in GUI
- Species selection using supported backend species
- Input-pair selection (`P-T`, `P-h`, `rho-h`, `P-s`)
- Single equilibrium state compute
- Full property table (`P`, `T`, `rho`, `h`, `s`, `cp`, `cv`, `gamma`, `a`, phase/quality when available)
- Workspace state persisted in project file (`fluid_workspace`)

**Key Deliverables**:
- Fluid property browser (searchable data table for P, T, ρ, h, s, μ, k, etc.)
- Property-vs-property 2D plots (T-s, P-h, Mollier, etc.)
- State point calculator (solve for state given pairs like P+h, T+s)
- Phase diagram visualization
- Saved state point library and history
- Multi-fluid support (pure and mixtures)
- Export property data to CSV

**Dependencies**: Phase 1 (tf-fluids works); optionally Phase 5 for mixtures

**Why it matters**:
- Users don't need to buy RefProp just for property lookups
- Tight integration with system design (state points from cycle flow into fluid explorer)
- Property understanding directly supports cycle design

---

## Phase 5: Combustion Support and CEA Integration

**Timeline**: ~2 months (2026-09 target)

**Objective**: Enable combustion equilibrium calculations; support propellant selection.

**Key Deliverables**:
- tf-combustion-cea crate (CEA library binding)
- Extended tf-project schema for combustor definitions (propellant, oxidizer, mixture ratio)
- Equilibrium calculation service (state out of combustor given propellant, O/F ratio, chamber pressure)
- Composition handling in tf-fluids (multi-component fluid properties)
- Combustor node type in System workspace
- Pressure/mixture ratio sweep tool in System workspace

**Dependencies**: Phase 1 (core), Phase 3 (editor), optionally Phase 4 (fluid workspace)

**Why it matters**:
- Opens up propulsion cycle design (currently blocked without combustion)
- Enables turbopump matching with realistic chamber conditions
- Users can size preburner/main combustor combinations

---

## Phase 6: Cycle Workspace and Component Matching (RPA Replacement)

**Timeline**: ~3–4 months (2026-12 target)

**Objective**: Dedicated workspace for cycle design, turbopump selection, and sizing.

**Key Deliverables**:
- Cycle design builder UI (series/parallel architecture canvas)
- Rapid cycle calculation solver (simplified equations for speed)
- Turbopump matching tool (compressor head/flow vs. turbine power)
- Inlet design tool (ram pressure recovery, altitude effects)
- Nozzle expansion ratio calculator
- Parametric sweep and sensitivity matrix
- Component sizing tables (pump displacement, turbine tooth count, etc.)
- Export cycle summary and BOM

**Dependencies**: Phase 5 (combustion), Phase 1 (core solver maturity)

**Why it matters**:
- Replaces RPA for conceptual and preliminary engine design
- Enables rapid what-if analysis (e.g., "what if we increase pressure ratio?")
- Produces sizing inputs for detailed component design

---

## Phase 7: Advanced Analysis and Optimization

**Timeline**: ~4 months (2027-04 target)

**Objective**: Enable parametric studies, result comparison, and design optimization.

**Key Deliverables**:
- Multi-run result comparison (side-by-side plots, difference tables)
- Parameter sweep framework (vary one or more parameters, generate matrix)
- Optimization setup (define objective, constraints, algorithm)
- Sensitivity analysis (local derivatives via finite difference)
- Calibration tools (fit model parameters to test data)
- Export tools (PDF reports, high-quality plots, Excel exports)
- Archived results and version control integration

**Dependencies**: Phases 1–6 (everything working in unison)

**Why it matters**:
- Harnesses simulation fidelity for design trade studies
- Replaces Excel + manual iteration for parametric work
- Enables data-driven design (test data validation, TFEA)

---

## Cross-Cutting Work

### Testing and Quality (All Phases)

- Maintain >80% unit test coverage for core crates
- Add integration tests for new features
- Performance profiling and optimization
- Regression test suite

### Documentation (All Phases)

- Update ARCHITECTURE.md as workspaces are added
- User manual and tutorial videos
- Developer guide for extending components/solvers
- API documentation (rustdoc)

### Infrastructure (Phases 2–3, then continuous)

- CI/CD pipeline (GitHub Actions or similar)
- Code review process
- Style guide (Rust conventions, commit messages)
- Release/versioning process

### User Experience (All Phases)

- User testing with domain experts (aero/prop engineers)
- Keyboard shortcuts and customization
- Drag-and-drop, undo/redo, persistent undo history
- Theming (dark mode, high contrast)
- Accessibility (keyboard-only operation, screen reader support where feasible)

---

## Dependency Graph

```
Phase 1 (Core)
  ↓
Phase 2 (Services + CLI/GUI)
  ↓
Phases 3, 4, 5 (Editors & Features) — can run mostly in parallel
  ├→ Phase 3 (P&ID)
  ├→ Phase 4 (Fluid)
  └→ Phase 5 (Combustion)
      ↓
Phase 6 (Cycle) — depends on Phase 5
  ↓
Phase 7 (Advanced Analysis) — depends on all previous
```

---

## Success Metrics

- **Phase 1**: Reproducible steady-state results; documented command-line interface
- **Phase 2**: Zero test failures; identical results from CLI and GUI
- **Phase 3**: Engineers can design 20+ node systems in <2 hours; visual feedback sufficient to catch errors
- **Phase 4**: Property lookups faster than RefProp; export data to external tools
- **Phase 5**: Predict chamber conditions within 5% of NASA CEA; support LOX/RP1, LOX/H2, etc.
- **Phase 6**: Cycle designs match published reference cycles (RL-10, SSME)
- **Phase 7**: Users report faster iteration than Excel; optimization converges in reasonable time

---

## Review and Adjustment

This roadmap is reviewed quarterly. If dependencies shift, phases may be reordered. User feedback may introduce new phases or defer lower-priority work.

**Last review**: 2026-02-26  
**Next review**: 2026-05-26
