# Development Conventions

This document formalizes coding standards and conventions for Thermoflow development.

## Code Style and Format

### Rust Code

- Follow [Rust API Guidelines](https://rust-lang.github.io/api-guidelines/)
- Use `cargo fmt` for automatic formatting (all code must pass `cargo fmt --all`)
- Use `cargo clippy -- -D warnings` for linting (all code must pass with no warnings)
- Use `cargo test --workspace` for testing (all new features must include tests)

### Documentation

- All public functions and types must have doc comments (`///`)
- Use markdown in doc comments for formatting
- Include examples in doc comments for complex functions
- Document assumptions and sign conventions explicitly (e.g., "flow positive in design direction")

### Commits

- Prefix commits to indicate scope: `feat:`, `fix:`, `refactor:`, `docs:`, `test:`, `chore:`
- Example: `feat: add turbine efficiency sweep to Cycle workspace`
- Keep commits focused; one logical change per commit
- Write clear commit messages: imperative mood, <50 characters on line 1, detailed explanation below

## Architecture and Design

### Business Logic Ownership

**Rule**: If logic is used by more than one module (e.g., both CLI and GUI), it must live in a library crate (not in a frontend).

- **CLI** (tf-cli): input parsing, command dispatch, output formatting
- **GUI** (tf-ui): windowing, rendering, event handling, layout
- **Shared libraries**: project I/O, simulation, caching, result queries (tf-app and below)

**Violation check**: Search the codebase for duplicated logic. If found, extract to a shared crate.

### Error Handling

- Errors must be `thiserror`-based enums in library crates
- Use `AppError` (from tf-app) in shared code; convert from backend errors via `From` impls
- CLI and GUI must never panic on bad input; return error messages to user
- Log errors with `tracing::error!` before returning

### Testing

- Unit tests live in the same file as code (using `#[cfg(test)] mod tests { }`)
- Integration tests live in `tests/` directory and test cross-crate behavior
- Test names describe the scenario, e.g., `test_orifice_reverse_flow_increases_pressure_drop`
- All new public functions must have at least one test
- Target >80% coverage for core crates (tf-solver, tf-components, tf-fluids)

### Naming

- **Functions**: lowercase_with_underscores
- **Types**: CapitalCaseWithNoUnderscores
- **Constants**: UPPERCASE_WITH_UNDERSCORES
- **Generic names**: avoid `x`, `y`, prefer `inlet_pressure`, `outlet_flow`
- **Boolean functions**: prefix with `is_`, `has_`, `can_`, e.g., `is_closed()`, `has_convergence()`

### Units and Dimensions

- Use the `uom` crate for unit-safe values
- Store SI base units internally: Pa, K, kg/s, m³/s
- Document unit assumptions in function signatures
- Example:
  ```rust
  fn orifice_flow(delta_p: Pressure, discharge_area: Area) -> MassFlow {
      // delta_p in Pa, discharge_area in m², returns kg/s
  }
  ```

### Sign Conventions

Establish and document sign conventions for each domain:

- **Flow**: positive = design direction (inlet to outlet, or left to right)
- **Pressure drop**: negative ∆p in flow direction (pump adds pressure, pipe loses)
- **Rotation**: positive = nominal direction; negative = reverse rotation
- **Temperature rise**: positive = heating (pump, combustor), negative = cooling (nozzle expansion)

Document these in the crate's top-level module doc and in component constructors.

## Repository Organization

### Crate Responsibilities

Each crate owns a clear domain:

| Crate | Owns | Does NOT Own |
|-------|------|--------------|
| tf-core | Unit system, ID types, shared traits | Physics models, solving |
| tf-graph | Network topology representation | Component nodes, physics |
| tf-project | Project schema, YAML I/O | Solving, anything beyond serialization |
| tf-fluids | Thermodynamic property models | Fluid production, system design |
| tf-components | Component physics (orifice, pump, etc.) | Integration into system, solving |
| tf-solver | Nonlinear equation solving | Thermodynamics, components |
| tf-sim | Time-stepping, integration schemes | Physics models, component definitions |
| tf-results | Run storage, caching, queries | Execution, solving |
| tf-app | Cross-crate orchestration | Frontier (CLI/GUI) logic |

### Adding a New Crate

1. Create directory: `crates/<name>/`
2. Run: `cargo new --lib <name>`
3. Document its role in ARCHITECTURE.md
4. Update Cargo.toml workspace to include it
5. Establish external dependencies (keep them minimal)

## Testing Strategy

### Unit Tests (in the crate)

Test individual functions in isolation:

```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_orifice_closure_stops_flow() {
        let mut orifice = Orifice::new(/* ... */);
        orifice.set_closure(0.0);
        let flow = orifice.mass_flow(100_000_pa, 80_000_pa);
        assert!(flow.value.abs() < 1e-6);
    }
}
```

### Integration Tests (in tests/ dir)

Test cross-crate workflows:

```rust
// tests/integration_steady.rs
#[test]
fn test_steady_simulation_orifice() {
    let project_path = Path::new("examples/projects/01_orifice_steady.yaml");
    let project = project_service::load_project(project_path).unwrap();
    let response = run_service::ensure_run(&request).unwrap();
    let (_manifest, records) = run_service::load_run(project_path, &response.run_id).unwrap();
    let summary = query::get_run_summary(&records).unwrap();
    assert!(summary.record_count > 0);
}
```

### CI/CD Tests

- All tests must pass on Linux and Windows
- Lint checks must pass (clippy)
- Code must format cleanly (cargo fmt)

## Documentation Standards

### Module-Level Docs

Every public crate module must have a top-level doc comment:

```rust
//! Orifice component model.
//!
//! An orifice is a flow restriction with a discharge coefficient that varies
//! with pressure drop and closure (0.0 = fully closed, 1.0 = fully open).
//!
//! # Sign Convention
//!
//! Flow is positive in the inlet → outlet direction.
```

### Function Docs

Public functions must have doc comments with examples:

```rust
/// Calculate incompressible orifice flow.
///
/// Uses the standard orifice equation: Q = C_d * A * sqrt(2 * ∆P / ρ)
///
/// # Arguments
///
/// * `delta_p` - Pressure drop across orifice (Pa)
/// * `area` - Discharge area (m²)
/// * `rho` - Fluid density (kg/m³)
///
/// # Returns
///
/// Volume flow rate (m³/s)
///
/// # Example
///
/// ```
/// let flow = incompressible_orifice_flow(50_000_pa, 1e-4_m2, 800.0);
/// ```
pub fn incompressible_orifice_flow(delta_p: Pressure, area: Area, rho: f64) -> VolumeFlow {
    // ...
}
```

### API Documentation

Generate and review rustdoc:

```bash
cargo doc --open --no-deps
```

## Transient Simulation Patterns

### Implementing TransientModel for Custom Systems

Transient systems must implement `tf_sim::TransientModel<State=S>`:

```rust
pub struct MyTransientModel { /* ... */ }

impl TransientModel for MyTransientModel {
    type State = MyState;
    
    fn initial_state(&self) -> Self::State { /* ... */ }
    
    fn rhs(&mut self, t: f64, x: &Self::State) -> SimResult<Self::State> {
        // Compute time derivatives
    }
    
    fn add(&self, a: &Self::State, b: &Self::State) -> Self::State { /* vector add */ }
    fn scale(&self, a: &Self::State, scale: f64) -> Self::State { /* scalar mult */ }
}
```

### Using tf-app for Transient Execution

Both CLI and GUI use the identical path—no duplication:

```rust
let request = RunRequest {
    project_path,
    system_id,
    mode: RunMode::Transient { dt_s, t_end_s },
    options: RunOptions { use_cache: true, solver_version: "0.1.0".to_string() },
};
let response = tf_app::run_service::ensure_run(&request)?;
```

**All transient compilation and execution lives in `tf-app`.**
- `tf-app/src/transient_compile.rs` — network-based transient runtime compilation
- `tf-app/src/run_service.rs::execute_transient()` — shared execution logic
- No transient logic in CLI or GUI

### Known Limitations

- **Solver convergence**: Some system configurations (closed valve at t=0, uninitialized junctions) produce singular Jacobian matrices. Future improvements: relaxed tolerances, better initial guesses, quasi-Newton methods.
- **Component scheduling**: Valve position, boundary pressure/temperature changes supported; complex control laws require schema extension.

## Architecture Change Checklist

For major changes (new crates, refactoring shared services, solver/simulation strategy changes, API changes):

- [ ] **Ownership**: Does the change belong in shared services (`tf-app`, `tf-solver`, `tf-sim`) rather than in a frontend (CLI/GUI)?
  - If logic is used by more than one frontend, it must be in a library crate
  - Verify no duplication will be introduced

- [ ] **Parity**: Do both CLI and GUI use identical backend paths?
  - Run the same command through both frontends
  - Verify they produce identical results
  - CLI is the gold standard; GUI must mirror it exactly

- [ ] **Schema Versioning**: Are project/results schema changes backward-compatible or versioned?
  - Update schema version in `tf-project` if the format changes
  - Add migration utilities if needed
  - Validate old projects load/error gracefully

- [ ] **Examples and Tests**: Are example files updated? Are new tests/fixtures added?
  - Update `examples/projects/` if needed
  - Add unit tests in affected crates
  - Add integration tests in `tests/` directories
  - Verify the example works end-to-end

- [ ] **Documentation**: Are ARCHITECTURE.md, ROADMAP.md, and DEVELOPMENT_CONVENTIONS.md updated?
  - `ARCHITECTURE.md`: update crate roles, component ownership, data flow
  - `ROADMAP.md`: mark phase/milestone changes, update dependencies if affected
  - `DEVELOPMENT_CONVENTIONS.md`: document new patterns/conventions if needed
  - Update this checklist if the process itself changes

- [ ] **Verification**: Do all checks pass?
  - `cargo fmt --all` (code formatting)
  - `cargo test --workspace` (all tests pass)
  - `cargo clippy --workspace --all-targets --all-features -- -D warnings` (no warnings)
  - `cargo run -p tf-cli -- --help` (CLI compiles)
  - `cargo build -p tf-ui` (GUI compiles)
  - If applicable, `cargo run -p tf-cli -- validate <exampleproject.yaml>` (example loads)
  - If applicable, `cargo run -p tf-cli -- run steady/transient <exampleproject.yaml> <system_id>` (runs end-to-end)

**Why this matters**: Major changes that skip this checklist tend to introduce regressions, duplicate code, docs drift behind implementation, or broken parity between CLI and GUI. Use this checklist to catch issues early.

## Workflow

### Feature Development

1. Create a branch: `git checkout -b feat/my-feature`
2. Write tests first (or alongside code)
3. Implement feature
4. Run `cargo fmt --all`
5. Run `cargo clippy --workspace -- -D warnings`
6. Run `cargo test --workspace`
7. Update ARCHITECTURE.md or ROADMAP.md if needed
8. Commit: `git commit -m "feat: description of feature"`

### Debugging

- **CLI debugging**: Use `cargo run -p tf-cli -- <args>` to reproduce issues
- **GUI debugging**: Use `cargo run -p tf-ui` and inspect output or add `println!` macros
- **Logging**: Add `tracing::debug!` or `tracing::error!` for runtime diagnostics
- **Determinism**: If behavior differs CLI vs. GUI, run both through the same tf-app call

### Performance Profiling

```bash
# Build with debug symbols but optimizations
cargo build --release

# Profile with perf (Linux) or similar tools
# Example: long-running simulation
time cargo run -p tf-cli -- run steady big_project.yaml system-id
```

## Review Checklist

Before submitting code for review:

- [ ] Code passes `cargo fmt --all`
- [ ] Code passes `cargo clippy -- -D warnings`
- [ ] Code passes `cargo test --workspace`
- [ ] New public types/functions have doc comments
- [ ] New features have corresponding tests
- [ ] If crate responsibilities changed, ARCHITECTURE.md is updated
- [ ] Commit messages are clear and follow conventions
- [ ] No commented-out code or debug prints left behind

## Known Limitations and Workarounds

### Transient Startup at t=0

**Issue**: Transient simulations may fail at t=0 with "Jacobian solve failed" error due to numerical singularity when the solver has no warm-start solution and relies on initial guess.

**Root cause**: At the very first timestep, the system lacks a previous solution to use as warm-start. The initial pressure guess (uniform 101.325 Pa for free nodes) combined with high-pressure control volume boundaries can create an ill-conditioned Jacobian.

**Workaround**: Start transient simulations from a small positive time instead of t=0:

```bash
# Instead of:
cargo run -p tf-cli -- run transient project.yaml system-id --dt 0.01 --t-end 10.0

# Use:
cargo run -p tf-cli -- run transient project.yaml system-id --dt 0.01 --t-start 0.01 --t-end 10.0
```

Once t > 0, the solver has a warm-start solution and becomes well-conditioned.

**Mitigation strategies** (for developers):
- Component models include minimum conductance regularization (e.g., valve effective area floor of 1e-4 * max_area)
- Newton solver uses adaptive line search to maintain feasibility
- Both strategies provide robustness but do not eliminate the t=0 singularity issue

Future work: Instrumenting the t=0 solver to diagnose the Jacobian singularity root cause.

---

**Document version**: 1.0  
**Last updated**: 2026-02-26
