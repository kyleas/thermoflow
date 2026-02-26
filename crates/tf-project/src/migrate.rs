//! Schema migration framework.

use crate::ProjectError;
use crate::schema::Project;

pub const LATEST_VERSION: u32 = 1;

pub fn migrate_to_latest(mut project: Project) -> Result<Project, ProjectError> {
    while project.version < LATEST_VERSION {
        project = migrate_one_version(project)?;
    }
    Ok(project)
}

fn migrate_one_version(project: Project) -> Result<Project, ProjectError> {
    match project.version {
        0 => migrate_v0_to_v1(project),
        v => Err(ProjectError::Migration {
            what: format!("No migration path from version {}", v),
        }),
    }
}

fn migrate_v0_to_v1(mut project: Project) -> Result<Project, ProjectError> {
    project.version = 1;
    Ok(project)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::schema::RunLibraryDef;

    #[test]
    fn migrate_latest_is_noop() {
        let project = Project {
            version: LATEST_VERSION,
            name: "test".to_string(),
            systems: vec![],
            modules: vec![],
            layouts: vec![],
            runs: RunLibraryDef::default(),
        };

        let migrated = migrate_to_latest(project.clone()).unwrap();
        assert_eq!(migrated, project);
    }
}
