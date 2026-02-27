//! tf-project: canonical project file format and validation.

pub mod cv_init;
pub mod migrate;
pub mod schema;
pub mod validate;

pub use cv_init::CvInitMode;
pub use migrate::{LATEST_VERSION, migrate_to_latest};
pub use schema::*;
pub use validate::{ValidationError, validate_project};

pub type ProjectResult<T> = Result<T, ProjectError>;

#[derive(thiserror::Error, Debug)]
pub enum ProjectError {
    #[error("Validation error: {0}")]
    Validation(#[from] ValidationError),

    #[error("Migration error: {what}")]
    Migration { what: String },

    #[error("Serialization error: {0}")]
    Serialization(String),

    #[error("I/O error: {0}")]
    Io(#[from] std::io::Error),

    #[error("YAML error: {0}")]
    Yaml(#[from] serde_yaml::Error),

    #[error("JSON error: {0}")]
    Json(#[from] serde_json::Error),
}

pub fn load_yaml(path: &std::path::Path) -> ProjectResult<Project> {
    let content = std::fs::read_to_string(path)?;
    let mut project: Project = serde_yaml::from_str(&content)?;
    project = migrate_to_latest(project)?;
    validate_project(&project)?;
    Ok(project)
}

pub fn save_yaml(path: &std::path::Path, project: &Project) -> ProjectResult<()> {
    validate_project(project)?;
    let content = serde_yaml::to_string(project)?;
    std::fs::write(path, content)?;
    Ok(())
}

pub fn load_json(path: &std::path::Path) -> ProjectResult<Project> {
    let content = std::fs::read_to_string(path)?;
    let mut project: Project = serde_json::from_str(&content)?;
    project = migrate_to_latest(project)?;
    validate_project(&project)?;
    Ok(project)
}

pub fn save_json(path: &std::path::Path, project: &Project) -> ProjectResult<()> {
    validate_project(project)?;
    let content = serde_json::to_string_pretty(project)?;
    std::fs::write(path, content)?;
    Ok(())
}
