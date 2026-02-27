//! Steady-state network solver for thermodynamic systems.
//!
//! This crate provides a Newton-based nonlinear solver for fluid networks where
//! the unknowns are node pressures (P) and specific enthalpies (h). Temperature
//! is not assumed constant; it is computed from the (P,h) state at each node.

pub mod error;
pub mod initialization;
pub mod jacobian;
pub mod newton;
pub mod problem;
pub mod solve;
pub mod steady;
pub mod thermo_policy;

pub use error::{SolverError, SolverResult};
pub use initialization::InitializationStrategy;
pub use newton::{NewtonConfig, NewtonResult};
pub use problem::SteadyProblem;
pub use solve::{
    SolveProgressEvent, solve, solve_with_active, solve_with_active_and_policy, solve_with_policy,
    solve_with_progress, solve_with_strategy, solve_with_strategy_and_progress,
    solve_with_strategy_policy_and_progress,
};
pub use steady::SteadySolution;
pub use thermo_policy::{StateCreationResult, StrictPolicy, ThermoStatePolicy};
