//! Content-based hashing for run IDs.

use sha2::{Digest, Sha256};
use tf_project::schema::SystemDef;

pub fn compute_run_id(
    system: &SystemDef,
    run_type: &crate::types::RunType,
    solver_version: &str,
) -> String {
    let mut hasher = Sha256::new();

    let system_json = serde_json::to_string(system).unwrap_or_default();
    hasher.update(system_json.as_bytes());

    let run_type_json = serde_json::to_string(run_type).unwrap_or_default();
    hasher.update(run_type_json.as_bytes());

    hasher.update(solver_version.as_bytes());

    let result = hasher.finalize();
    format!("{:x}", result)
}

#[cfg(test)]
mod tests {
    use super::*;
    use tf_project::schema::*;

    #[test]
    fn hash_stability() {
        let system = SystemDef {
            id: "sys1".to_string(),
            name: "Test".to_string(),
            fluid: FluidDef {
                composition: CompositionDef::Pure {
                    species: "N2".to_string(),
                },
            },
            nodes: vec![],
            components: vec![],
            boundaries: vec![],
            schedules: vec![],
            controls: None,
        };

        let run_type = crate::types::RunType::Steady;
        let version = "v1";

        let hash1 = compute_run_id(&system, &run_type, version);
        let hash2 = compute_run_id(&system, &run_type, version);

        assert_eq!(hash1, hash2);
    }

    #[test]
    fn hash_differs_for_different_inputs() {
        let system1 = SystemDef {
            id: "sys1".to_string(),
            name: "Test1".to_string(),
            fluid: FluidDef {
                composition: CompositionDef::Pure {
                    species: "N2".to_string(),
                },
            },
            nodes: vec![],
            components: vec![],
            boundaries: vec![],
            schedules: vec![],
            controls: None,
        };

        let system2 = SystemDef {
            id: "sys2".to_string(),
            name: "Test2".to_string(),
            fluid: FluidDef {
                composition: CompositionDef::Pure {
                    species: "O2".to_string(),
                },
            },
            nodes: vec![],
            components: vec![],
            boundaries: vec![],
            schedules: vec![],
            controls: None,
        };

        let run_type = crate::types::RunType::Steady;
        let version = "v1";

        let hash1 = compute_run_id(&system1, &run_type, version);
        let hash2 = compute_run_id(&system2, &run_type, version);

        assert_ne!(hash1, hash2);
    }
}
