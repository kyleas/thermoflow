//! Shared application service layer for thermoflow.
//!
//! This crate provides a unified interface for both CLI and GUI frontends,
//! centralizing business logic for project management, runtime compilation,
//! simulation execution, and result querying.

pub mod error;
pub mod metrics;
pub mod progress;
pub mod project_service;
pub mod query;
pub mod run_service;
pub mod runtime_compile;
pub mod transient_compile;
pub mod transient_fallback_policy;

// Re-export key types for convenience
pub use error::{AppError, AppResult};
pub use metrics::LoopMetrics;
pub use progress::{RunProgressEvent, RunStage, SteadyProgress, TransientProgress};
pub use project_service::{
    get_system, list_systems, load_project, save_project, validate_project, SystemSummary,
};
pub use query::{
    analyze_control_loops, extract_component_series, extract_control_series, extract_node_series,
    get_run_summary, list_component_ids, list_control_ids, list_node_ids, ControlLoopAnalysis,
    RunSummary,
};
pub use run_service::{
    ensure_run, ensure_run_with_progress, list_runs, load_run, RunMode, RunOptions, RunRequest,
    RunResponse, RunTimingSummary,
};
pub use runtime_compile::{
    build_components, build_fluid_model, compile_system, parse_boundaries,
    parse_boundaries_with_atmosphere, BoundaryCondition, SystemRuntime,
};
