//! Steady-state network solver for thermodynamic systems.
//!
//! This crate provides a Newton-based nonlinear solver for fluid networks where
//! the unknowns are node pressures (P) and specific enthalpies (h). Temperature
//! is not assumed constant; it is computed from the (P,h) state at each node.

pub mod error;
pub mod jacobian;
pub mod newton;
pub mod problem;
pub mod solve;
pub mod steady;

pub use error::{SolverError, SolverResult};
pub use newton::{NewtonConfig, NewtonResult};
pub use problem::SteadyProblem;
pub use solve::{solve, solve_with_active};
pub use steady::SteadySolution;
