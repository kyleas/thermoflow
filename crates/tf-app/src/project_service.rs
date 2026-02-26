//! Project loading, saving, validation, and introspection.

use std::path::Path;
use tf_project::schema::{Project, SystemDef};

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
    let content = std::fs::read_to_string(path).map_err(|e| AppError::ProjectFileRead {
        path: path.to_path_buf(),
        source: e,
    })?;

    let project: Project = serde_yaml::from_str(&content)
        .map_err(|e| AppError::Project(format!("Failed to parse project YAML: {}", e)))?;

    Ok(project)
}

/// Save project to a YAML file.
pub fn save_project(path: &Path, project: &Project) -> AppResult<()> {
    let content = serde_yaml::to_string(project)
        .map_err(|e| AppError::Project(format!("Failed to serialize project: {}", e)))?;

    std::fs::write(path, content).map_err(|e| AppError::ProjectFileWrite {
        path: path.to_path_buf(),
        source: e,
    })?;

    Ok(())
}

/// Validate project structure.
pub fn validate_project(project: &Project) -> AppResult<()> {
    // Basic validation: ensure at least one system exists
    if project.systems.is_empty() {
        return Err(AppError::Validation(
            "Project must have at least one system".to_string(),
        ));
    }

    // Validate each system has nodes and valid topology
    for system in &project.systems {
        if system.nodes.is_empty() {
            return Err(AppError::Validation(format!(
                "System '{}' must have at least one node",
                system.id
            )));
        }
    }

    Ok(())
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
            has_boundaries: !system.boundaries.is_empty(),
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
