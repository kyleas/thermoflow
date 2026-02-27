//! Project validation logic.

use crate::schema::{BoundaryDef, ComponentDef, NodeDef, Project, SystemDef};
use std::collections::{HashMap, HashSet};

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

    #[error("Unsupported feature: {feature} - {reason}")]
    Unsupported { feature: String, reason: String },

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

        if let Some(system) = project.systems.iter().find(|s| s.id == layout.system_id) {
            let node_ids: HashSet<&String> = system.nodes.iter().map(|n| &n.id).collect();
            for node in &layout.nodes {
                if !node_ids.contains(&node.node_id) {
                    return Err(ValidationError::MissingReference {
                        id: node.node_id.clone(),
                        context: "layout node_id".to_string(),
                    });
                }
            }

            let component_ids: HashSet<&String> = system.components.iter().map(|c| &c.id).collect();
            for edge in &layout.edges {
                if !component_ids.contains(&edge.component_id) {
                    return Err(ValidationError::MissingReference {
                        id: edge.component_id.clone(),
                        context: "layout component_id".to_string(),
                    });
                }
            }
        }
    }

    Ok(())
}

fn validate_system(system: &SystemDef) -> Result<(), ValidationError> {
    let mut node_ids = HashSet::new();
    let mut node_kind_map = HashMap::new();
    for node in &system.nodes {
        if !node_ids.insert(&node.id) {
            return Err(ValidationError::DuplicateId {
                id: node.id.clone(),
                context: format!("system '{}' nodes", system.name),
            });
        }
        validate_node(node)?;
        node_kind_map.insert(&node.id, &node.kind);
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
        if matches!(
            node_kind_map.get(&boundary.node_id),
            Some(crate::schema::NodeKind::Atmosphere { .. })
        ) {
            return Err(ValidationError::InvalidValue {
                field: format!("boundary node_id '{}'", boundary.node_id),
                value: boundary.node_id.clone(),
                reason: "atmosphere nodes must not have separate boundaries".to_string(),
            });
        }
    }

    for schedule in &system.schedules {
        for event in &schedule.events {
            match &event.action {
                crate::schema::ActionDef::SetValvePosition { component_id, .. } => {
                    // Timed valve schedules are not supported yet
                    return Err(ValidationError::Unsupported {
                        feature: format!(
                            "Timed valve position schedules (schedule '{}', component '{}')",
                            schedule.name, component_id
                        ),
                        reason: "Timed valve opening/closing schedules are not yet supported. \
                                 The continuation solver is not robust enough for valve transients. \
                                 Use fixed valve positions for now.".to_string(),
                    });
                }
                crate::schema::ActionDef::SetBoundaryPressure { node_id, .. }
                | crate::schema::ActionDef::SetBoundaryTemperature { node_id, .. } => {
                    if matches!(
                        node_kind_map.get(node_id),
                        Some(crate::schema::NodeKind::Atmosphere { .. })
                    ) {
                        return Err(ValidationError::InvalidValue {
                            field: format!("schedule '{}' boundary node_id", schedule.name),
                            value: node_id.clone(),
                            reason: "atmosphere nodes have fixed state and cannot be scheduled"
                                .to_string(),
                        });
                    }
                }
            }
        }
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
        NodeKind::Atmosphere {
            pressure_pa,
            temperature_k,
        } => {
            if !pressure_pa.is_finite() || *pressure_pa <= 0.0 {
                return Err(ValidationError::InvalidValue {
                    field: format!("node '{}' pressure_pa", node.name),
                    value: pressure_pa.to_string(),
                    reason: "must be positive and finite".to_string(),
                });
            }
            if !temperature_k.is_finite() || *temperature_k <= 0.0 {
                return Err(ValidationError::InvalidValue {
                    field: format!("node '{}' temperature_k", node.name),
                    value: temperature_k.to_string(),
                    reason: "must be positive and finite".to_string(),
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
        ComponentKind::LineVolume {
            volume_m3,
            cd,
            area_m2,
        } => {
            validate_positive_finite("volume_m3", *volume_m3, &component.name)?;
            // cd and area are optional (for lossless case)
            if *cd > 0.0 {
                validate_positive_finite("cd", *cd, &component.name)?;
                validate_positive_finite("area_m2", *area_m2, &component.name)?;
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
