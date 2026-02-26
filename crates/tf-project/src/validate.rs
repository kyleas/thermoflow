//! Project validation logic.

use crate::schema::{BoundaryDef, ComponentDef, NodeDef, Project, SystemDef};
use std::collections::HashSet;

#[derive(thiserror::Error, Debug)]
pub enum ValidationError {
    #[error("Duplicate ID: {id} in {context}")]
    DuplicateId { id: String, context: String },

    #[error("Missing reference: {id} in {context}")]
    MissingReference { id: String, context: String },

    #[error("Invalid value: {field} = {value} ({reason})")]
    InvalidValue {
        field: String,
        value: String,
        reason: String,
    },

    #[error("Unsupported version: {version}")]
    UnsupportedVersion { version: u32 },
}

pub fn validate_project(project: &Project) -> Result<(), ValidationError> {
    if project.version > crate::migrate::LATEST_VERSION {
        return Err(ValidationError::UnsupportedVersion {
            version: project.version,
        });
    }

    let mut system_ids = HashSet::new();
    for system in &project.systems {
        if !system_ids.insert(&system.id) {
            return Err(ValidationError::DuplicateId {
                id: system.id.clone(),
                context: "systems".to_string(),
            });
        }
        validate_system(system)?;
    }

    let mut module_ids = HashSet::new();
    for module in &project.modules {
        if !module_ids.insert(&module.id) {
            return Err(ValidationError::DuplicateId {
                id: module.id.clone(),
                context: "modules".to_string(),
            });
        }
    }

    for layout in &project.layouts {
        if !system_ids.contains(&layout.system_id) {
            return Err(ValidationError::MissingReference {
                id: layout.system_id.clone(),
                context: "layout system_id".to_string(),
            });
        }
    }

    Ok(())
}

fn validate_system(system: &SystemDef) -> Result<(), ValidationError> {
    let mut node_ids = HashSet::new();
    for node in &system.nodes {
        if !node_ids.insert(&node.id) {
            return Err(ValidationError::DuplicateId {
                id: node.id.clone(),
                context: format!("system '{}' nodes", system.name),
            });
        }
        validate_node(node)?;
    }

    let mut component_ids = HashSet::new();
    for component in &system.components {
        if !component_ids.insert(&component.id) {
            return Err(ValidationError::DuplicateId {
                id: component.id.clone(),
                context: format!("system '{}' components", system.name),
            });
        }
        validate_component(component, &node_ids, &system.name)?;
    }

    for boundary in &system.boundaries {
        validate_boundary(boundary, &node_ids, &system.name)?;
    }

    Ok(())
}

fn validate_node(node: &NodeDef) -> Result<(), ValidationError> {
    use crate::schema::NodeKind;

    match &node.kind {
        NodeKind::Junction => Ok(()),
        NodeKind::ControlVolume { volume_m3, initial } => {
            if !volume_m3.is_finite() || *volume_m3 <= 0.0 {
                return Err(ValidationError::InvalidValue {
                    field: format!("node '{}' volume_m3", node.name),
                    value: volume_m3.to_string(),
                    reason: "must be positive and finite".to_string(),
                });
            }

            if let Some(p) = initial.p_pa
                && (!p.is_finite() || p <= 0.0)
            {
                return Err(ValidationError::InvalidValue {
                    field: format!("node '{}' initial p_pa", node.name),
                    value: p.to_string(),
                    reason: "must be positive and finite".to_string(),
                });
            }

            if let Some(t) = initial.t_k
                && (!t.is_finite() || t <= 0.0)
            {
                return Err(ValidationError::InvalidValue {
                    field: format!("node '{}' initial t_k", node.name),
                    value: t.to_string(),
                    reason: "must be positive and finite".to_string(),
                });
            }

            if let Some(m) = initial.m_kg
                && (!m.is_finite() || m < 0.0)
            {
                return Err(ValidationError::InvalidValue {
                    field: format!("node '{}' initial m_kg", node.name),
                    value: m.to_string(),
                    reason: "must be non-negative and finite".to_string(),
                });
            }

            Ok(())
        }
    }
}

fn validate_component(
    component: &ComponentDef,
    node_ids: &HashSet<&String>,
    system_name: &str,
) -> Result<(), ValidationError> {
    if !node_ids.contains(&component.from_node_id) {
        return Err(ValidationError::MissingReference {
            id: component.from_node_id.clone(),
            context: format!(
                "system '{}' component '{}' from_node_id",
                system_name, component.name
            ),
        });
    }
    if !node_ids.contains(&component.to_node_id) {
        return Err(ValidationError::MissingReference {
            id: component.to_node_id.clone(),
            context: format!(
                "system '{}' component '{}' to_node_id",
                system_name, component.name
            ),
        });
    }

    use crate::schema::ComponentKind;
    match &component.kind {
        ComponentKind::Orifice { cd, area_m2, .. } => {
            validate_positive_finite("cd", *cd, &component.name)?;
            validate_positive_finite("area_m2", *area_m2, &component.name)?;
        }
        ComponentKind::Valve {
            cd,
            area_max_m2,
            position,
            ..
        } => {
            validate_positive_finite("cd", *cd, &component.name)?;
            validate_positive_finite("area_max_m2", *area_max_m2, &component.name)?;
            if !position.is_finite() || *position < 0.0 || *position > 1.0 {
                return Err(ValidationError::InvalidValue {
                    field: format!("component '{}' position", component.name),
                    value: position.to_string(),
                    reason: "must be in [0, 1]".to_string(),
                });
            }
        }
        ComponentKind::Pipe {
            length_m,
            diameter_m,
            roughness_m,
            mu_pa_s,
            ..
        } => {
            validate_positive_finite("length_m", *length_m, &component.name)?;
            validate_positive_finite("diameter_m", *diameter_m, &component.name)?;
            validate_non_negative_finite("roughness_m", *roughness_m, &component.name)?;
            validate_positive_finite("mu_pa_s", *mu_pa_s, &component.name)?;
        }
        ComponentKind::Pump {
            cd,
            area_m2,
            delta_p_pa,
            eta,
            ..
        } => {
            validate_positive_finite("cd", *cd, &component.name)?;
            validate_positive_finite("area_m2", *area_m2, &component.name)?;
            validate_non_negative_finite("delta_p_pa", *delta_p_pa, &component.name)?;
            if !eta.is_finite() || *eta <= 0.0 || *eta > 1.0 {
                return Err(ValidationError::InvalidValue {
                    field: format!("component '{}' eta", component.name),
                    value: eta.to_string(),
                    reason: "must be in (0, 1]".to_string(),
                });
            }
        }
        ComponentKind::Turbine {
            cd, area_m2, eta, ..
        } => {
            validate_positive_finite("cd", *cd, &component.name)?;
            validate_positive_finite("area_m2", *area_m2, &component.name)?;
            if !eta.is_finite() || *eta <= 0.0 || *eta > 1.0 {
                return Err(ValidationError::InvalidValue {
                    field: format!("component '{}' eta", component.name),
                    value: eta.to_string(),
                    reason: "must be in (0, 1]".to_string(),
                });
            }
        }
    }

    Ok(())
}

fn validate_boundary(
    boundary: &BoundaryDef,
    node_ids: &HashSet<&String>,
    system_name: &str,
) -> Result<(), ValidationError> {
    if !node_ids.contains(&boundary.node_id) {
        return Err(ValidationError::MissingReference {
            id: boundary.node_id.clone(),
            context: format!("system '{}' boundary node_id", system_name),
        });
    }

    if let Some(p) = boundary.pressure_pa
        && (!p.is_finite() || p <= 0.0)
    {
        return Err(ValidationError::InvalidValue {
            field: "boundary pressure_pa".to_string(),
            value: p.to_string(),
            reason: "must be positive and finite".to_string(),
        });
    }

    if let Some(t) = boundary.temperature_k
        && (!t.is_finite() || t <= 0.0)
    {
        return Err(ValidationError::InvalidValue {
            field: "boundary temperature_k".to_string(),
            value: t.to_string(),
            reason: "must be positive and finite".to_string(),
        });
    }

    Ok(())
}

fn validate_positive_finite(
    field: &str,
    value: f64,
    component_name: &str,
) -> Result<(), ValidationError> {
    if !value.is_finite() || value <= 0.0 {
        return Err(ValidationError::InvalidValue {
            field: format!("component '{}' {}", component_name, field),
            value: value.to_string(),
            reason: "must be positive and finite".to_string(),
        });
    }
    Ok(())
}

fn validate_non_negative_finite(
    field: &str,
    value: f64,
    component_name: &str,
) -> Result<(), ValidationError> {
    if !value.is_finite() || value < 0.0 {
        return Err(ValidationError::InvalidValue {
            field: format!("component '{}' {}", component_name, field),
            value: value.to_string(),
            reason: "must be non-negative and finite".to_string(),
        });
    }
    Ok(())
}
