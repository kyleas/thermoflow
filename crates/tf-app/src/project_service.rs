//! Project loading, saving, validation, and introspection.

use std::path::Path;
use tf_project::schema::{NodeKind, Project, SystemDef};

use crate::error::{AppError, AppResult};

/// Summary of a system for listing.
#[derive(Debug, Clone)]
pub struct SystemSummary {
    pub id: String,
    pub name: String,
    pub node_count: usize,
    pub component_count: usize,
    pub has_boundaries: bool,
}

/// Load project from a YAML file.
pub fn load_project(path: &Path) -> AppResult<Project> {
    tf_project::load_yaml(path).map_err(AppError::from)
}

/// Save project to a YAML file.
pub fn save_project(path: &Path, project: &Project) -> AppResult<()> {
    tf_project::save_yaml(path, project).map_err(AppError::from)
}

/// Validate project structure.
pub fn validate_project(project: &Project) -> AppResult<()> {
    tf_project::validate_project(project).map_err(|e| AppError::Validation(e.to_string()))
}

/// List all systems in the project with summaries.
pub fn list_systems(project: &Project) -> Vec<SystemSummary> {
    project
        .systems
        .iter()
        .map(|system| SystemSummary {
            id: system.id.clone(),
            name: system.name.clone(),
            node_count: system.nodes.len(),
            component_count: system.components.len(),
            has_boundaries: !system.boundaries.is_empty()
                || system
                    .nodes
                    .iter()
                    .any(|node| matches!(node.kind, NodeKind::Atmosphere { .. })),
        })
        .collect()
}

/// Get a specific system by ID.
pub fn get_system<'a>(project: &'a Project, system_id: &str) -> AppResult<&'a SystemDef> {
    project
        .systems
        .iter()
        .find(|s| s.id == system_id)
        .ok_or_else(|| AppError::SystemNotFound(system_id.to_string()))
}
