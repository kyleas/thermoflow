# Thermoflow

A unified **thermo-fluid and propulsion engineering workbench** for designing, analyzing, and optimizing thermal systems.

Thermoflow combines:

- **System Simulation**: Steady-state and transient modeling of fluid networks (pumps, turbines, pipes, orifices, valves)
- **Fluid Properties**: RefProp-compatible thermodynamic and transport property calculations
- **Cycle Analysis**: Engine design, turbopump matching, and parametric studies
- **Interactive Plotting**: Time-series visualization, result comparison, parameter sweeps

## Quick Start

### Build

```bash
cargo build
```

### Run the GUI

```bash
cargo run -p tf-ui
```

Open or create a project file (YAML format). Design a P&ID, execute a simulation, and explore results.

### Run the CLI

```bash
# Validate a project
cargo run -p tf-cli -- validate examples/projects/01_orifice_steady.yaml

# List systems
cargo run -p tf-cli -- systems examples/projects/01_orifice_steady.yaml

# Run a steady-state simulation
cargo run -p tf-cli -- run steady examples/projects/01_orifice_steady.yaml s1

# Run a transient simulation
cargo run -p tf-cli -- run transient --dt 0.1 --t-end 1.0 examples/projects/03_simple_vent_transient.yaml s1

# List cached runs
cargo run -p tf-cli -- runs examples/projects/01_orifice_steady.yaml s1

# Export a result as CSV
cargo run -p tf-cli -- export-series examples/projects/01_orifice_steady.yaml <run-id> n1 pressure -o result.csv
```

### Run Observability

CLI and GUI runs report:
- current phase/stage (compile, build, solve, save)
- initialization strategy (`Strict` or `Relaxed`)
- steady iteration/residual or transient simulated-time progress
- cutbacks/retries and fallback usage (transient)
- final timing breakdown with phase percentages
- real-fluid attempts/successes and surrogate update counts for transient diagnostics

For transient debugging, set `THERMOFLOW_TRANSIENT_LOG=verbose` to enable detailed per-step traces.

### Benchmarking

Run the performance baseline suite to measure and compare workflow execution times:

```bash
cargo build -p tf-bench --release
target/release/tf-bench.exe
```

This will:
1. Execute each supported example 5 times with fresh computation (no cache)
2. Collect wall-clock timing metrics for compile, build, solve, and save phases
3. Print a human-readable summary to the terminal
4. Save detailed metrics to `benchmarks/baseline.json` for programmatic analysis

Results include:
- **Median, min, max** timings for robust comparison
- **Phase breakdown** (time spent in each stage)
- **Solver diagnostics** (iterations, steps, real-fluid success %, fallback usage)
- **Aggregates** for trend detection and optimization validation

See [benchmarks/README.md](benchmarks/README.md) for detailed interpretation and [docs/PERFORMANCE_BASELINE.md](docs/PERFORMANCE_BASELINE.md) for initial baseline results and hotspot analysis.

## Project Structure

```
thermoflow/
├── crates/
│   ├── tf-core/               # Unit system, ID types, shared traits
│   ├── tf-graph/              # Network topology representation
│   ├── tf-project/            # Project schema and validation
│   ├── tf-fluids/             # Thermodynamic properties (RefProp)
│   ├── tf-components/         # Component models (orifice, pump, etc.)
│   ├── tf-solver/             # Steady-state nonlinear solver
│   ├── tf-sim/                # Transient integration solver
│   ├── tf-results/            # Run storage and caching
│   └── tf-app/                # Shared application services (for CLI and GUI)
├── apps/
│   ├── tf-cli/                # Command-line interface
│   └── tf-ui/                 # Desktop GUI application
├── docs/
│   ├── ARCHITECTURE.md        # Detailed architecture and design
│   └── ROADMAP.md             # Development phases and timeline
└── examples/
    └── projects/              # Example project files (YAML)
```

## Documentation

- **[ARCHITECTURE.md](docs/ARCHITECTURE.md)** — Core architecture, workspace model, design principles, and crate responsibilities
- **[ROADMAP.md](docs/ROADMAP.md)** — Development phases, feature timeline, and dependencies
- **[TRANSIENT_CAPABILITIES.md](docs/TRANSIENT_CAPABILITIES.md)** — Supported, unsupported, and experimental transient workflows
- **[STABILIZATION_AUDIT.md](docs/STABILIZATION_AUDIT.md)** — Canonical paths, superseded paths, and current support matrix

## Workspace Model

Thermoflow is one unified application with multiple **workspaces**:

### System Workspace

Define and simulate fluid networks. Draw P&ID diagrams, set boundary conditions, execute simulations, and inspect results. Deploy state overlays on the diagram to visualize pressure, temperature, and flow.

### Fluid Workspace (Phase 4)

Standalone fluid property explorer. Browse thermodynamic data, create property plots, and manage state point libraries—no external RefProp needed.

### Cycle Workspace (Phase 6)

Design propulsion cycles. Perform turbopump matching, parametric sweeps, and component sizing. Includes CEA integration for combustion equilibrium (Phase 5).

### Analysis Workspace (Phase 7)

Compare results across multiple runs. Create parameter sweeps, sensitivity matrices, and optimization studies. Export plots and data.

## Features

### Current (✅)

- Steady-state fluid network simulation
- Transient simulation for fixed-topology systems (single-CV and supported multi-CV benchmarks)
- P&ID editor (basic node/component creation and editing)
- Project file format (YAML)
- Run caching and time-series storage
- CLI with full command set
- GUI with System workspace
- RefProp-compatible fluid properties
- Component models: orifice, pipe, pump, turbine, valve
- Timed valve schedules remain explicitly unsupported (validation rejects them)

### Planned

- Phase 3: Drag-and-drop P&ID editor, state overlays
- Phase 4: Fluid workspace
- Phase 5: CEA integration, combustion support
- Phase 6: Cycle workspace, turbopump matching
- Phase 7: Advanced analysis, optimization framework

## Development

### Testing

```bash
# Run all tests
cargo test --workspace

# Run tests for a specific crate
cargo test -p tf-solver

# Run integration tests
cargo test -p tf-app --test integration_steady
```

### Code Quality

```bash
# Format code
cargo fmt --all

# Check for lint issues
cargo clippy --workspace --all-targets -- -D warnings
```

### Architecture Principles

1. **Backend first**: All business logic lives in library crates; UI is thin
2. **CLI gold standard**: Command-line interface is the source of truth for reproducibility
3. **No duplication**: Logic shared between CLI and GUI lives in tf-app
4. **Shared services**: One project, one run cache, one simulation backend

See [ARCHITECTURE.md](docs/ARCHITECTURE.md) for the full design philosophy.

## Targets and Replacements

Thermoflow is designed to eventually replace:

- **Visio**: for P&ID and system diagrams
- **RefProp**: for fluid property calculations
- **RPA**: for engine design and cycle analysis
- **Excel**: for thermo-fluid modeling workflows

## License

TBD

## Contributing

1. Read [ARCHITECTURE.md](docs/ARCHITECTURE.md) and [ROADMAP.md](docs/ROADMAP.md)
2. Follow Rust conventions (cargo fmt, cargo clippy)
3. Add tests for new functionality
4. Update documentation as needed
5. See [DEVELOPMENT_CONVENTIONS.md](docs/DEVELOPMENT_CONVENTIONS.md) for coding standards (if present)

---

**Status**: Active development (Phase 2 complete, Phase 3 in planning)  
**Last updated**: 2026-02-26
