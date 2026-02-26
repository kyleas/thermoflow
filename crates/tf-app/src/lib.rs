//! Shared application service layer for thermoflow.
//!
//! This crate provides a unified interface for both CLI and GUI frontends,
//! centralizing business logic for project management, runtime compilation,
//! simulation execution, and result querying.

pub mod error;
pub mod project_service;
pub mod query;
pub mod run_service;
pub mod runtime_compile;
pub mod transient_compile;

// Re-export key types for convenience
pub use error::{AppError, AppResult};
pub use project_service::{
    get_system, list_systems, load_project, save_project, validate_project, SystemSummary,
};
pub use query::{
    extract_component_series, extract_node_series, get_run_summary, list_component_ids,
    list_node_ids, RunSummary,
};
pub use run_service::{
    ensure_run, list_runs, load_run, RunMode, RunOptions, RunRequest, RunResponse,
};
pub use runtime_compile::{
    build_components, build_fluid_model, compile_system, parse_boundaries, BoundaryCondition,
    SystemRuntime,
};
