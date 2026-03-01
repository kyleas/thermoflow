//! Schema migration framework.

use crate::ProjectError;
use crate::schema::Project;

pub const LATEST_VERSION: u32 = 2;

pub fn migrate_to_latest(mut project: Project) -> Result<Project, ProjectError> {
    while project.version < LATEST_VERSION {
        project = migrate_one_version(project)?;
    }
    Ok(project)
}

fn migrate_one_version(project: Project) -> Result<Project, ProjectError> {
    match project.version {
        0 => migrate_v0_to_v1(project),
        1 => migrate_v1_to_v2(project),
        v => Err(ProjectError::Migration {
            what: format!("No migration path from version {}", v),
        }),
    }
}

fn migrate_v0_to_v1(mut project: Project) -> Result<Project, ProjectError> {
    project.version = 1;
    Ok(project)
}

fn migrate_v1_to_v2(mut project: Project) -> Result<Project, ProjectError> {
    use crate::schema::NodeKind;

    for system in &mut project.systems {
        let mut boundary_by_node: std::collections::HashMap<String, usize> =
            std::collections::HashMap::new();
        for (idx, boundary) in system.boundaries.iter().enumerate() {
            boundary_by_node.insert(boundary.node_id.clone(), idx);
        }

        let mut boundaries_to_remove = Vec::new();

        for node in &mut system.nodes {
            if !matches!(node.kind, NodeKind::Junction) {
                continue;
            }

            let name = format!("{} {}", node.id, node.name).to_ascii_lowercase();
            let is_ambient = name.contains("ambient") || name.contains("atmosphere");
            if !is_ambient {
                continue;
            }

            let boundary_idx = match boundary_by_node.get(&node.id) {
                Some(idx) => *idx,
                None => continue,
            };

            let boundary = &system.boundaries[boundary_idx];
            if let (Some(p), Some(t), None) = (
                boundary.pressure_pa,
                boundary.temperature_k,
                boundary.enthalpy_j_per_kg,
            ) {
                node.kind = NodeKind::Atmosphere {
                    pressure_pa: p,
                    temperature_k: t,
                };
                boundaries_to_remove.push(boundary_idx);
            }
        }

        boundaries_to_remove.sort_unstable_by(|a, b| b.cmp(a));
        for idx in boundaries_to_remove {
            system.boundaries.remove(idx);
        }
    }

    project.version = 2;
    Ok(project)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::schema::RunLibraryDef;
    use crate::schema::{
        BoundaryDef, ComponentDef, ComponentKind, CompositionDef, FluidDef, NodeDef, NodeKind,
        SystemDef,
    };

    #[test]
    fn migrate_latest_is_noop() {
        let project = Project {
            version: LATEST_VERSION,
            name: "test".to_string(),
            systems: vec![],
            modules: vec![],
            layouts: vec![],
            runs: RunLibraryDef::default(),
            plotting_workspace: None,
        };

        let migrated = migrate_to_latest(project.clone()).unwrap();
        assert_eq!(migrated, project);
    }

    #[test]
    fn migrate_ambient_boundary_to_atmosphere() {
        let system = SystemDef {
            id: "s1".to_string(),
            name: "Ambient System".to_string(),
            fluid: FluidDef {
                composition: CompositionDef::Pure {
                    species: "N2".to_string(),
                },
            },
            nodes: vec![
                NodeDef {
                    id: "n1".to_string(),
                    name: "Inlet".to_string(),
                    kind: NodeKind::Junction,
                },
                NodeDef {
                    id: "n_ambient".to_string(),
                    name: "Ambient".to_string(),
                    kind: NodeKind::Junction,
                },
            ],
            components: vec![ComponentDef {
                id: "c1".to_string(),
                name: "Orifice".to_string(),
                kind: ComponentKind::Orifice {
                    cd: 0.8,
                    area_m2: 1e-4,
                    treat_as_gas: true,
                },
                from_node_id: "n1".to_string(),
                to_node_id: "n_ambient".to_string(),
            }],
            boundaries: vec![BoundaryDef {
                node_id: "n_ambient".to_string(),
                pressure_pa: Some(101_325.0),
                temperature_k: Some(300.0),
                enthalpy_j_per_kg: None,
            }],
            schedules: vec![],
            controls: None,
        };

        let project = Project {
            version: 1,
            name: "Ambient Project".to_string(),
            systems: vec![system],
            modules: vec![],
            layouts: vec![],
            runs: RunLibraryDef::default(),
            plotting_workspace: None,
        };

        let migrated = migrate_to_latest(project).unwrap();
        assert_eq!(migrated.version, LATEST_VERSION);

        let system = &migrated.systems[0];
        let ambient = system
            .nodes
            .iter()
            .find(|n| n.id == "n_ambient")
            .expect("missing ambient node");

        match ambient.kind {
            NodeKind::Atmosphere {
                pressure_pa,
                temperature_k,
            } => {
                assert_eq!(pressure_pa, 101_325.0);
                assert_eq!(temperature_k, 300.0);
            }
            _ => panic!("ambient node not migrated to atmosphere"),
        }

        assert!(
            !system.boundaries.iter().any(|b| b.node_id == "n_ambient"),
            "ambient boundary should be removed after migration"
        );
    }
}
