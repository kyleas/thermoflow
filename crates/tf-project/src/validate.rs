//! Project validation logic.

use crate::schema::{
    BoundaryDef, ComponentDef, ComponentKind, ControlBlockKindDef, ControlConnectionDef,
    MeasuredVariableDef, NodeDef, Project, SystemDef,
};
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

    validate_controls(system, &node_ids, &component_ids)?;

    Ok(())
}

fn validate_controls(
    system: &SystemDef,
    node_ids: &HashSet<&String>,
    component_ids: &HashSet<&String>,
) -> Result<(), ValidationError> {
    let Some(controls) = &system.controls else {
        return Ok(());
    };

    let mut block_ids = HashSet::new();
    let mut block_kind_by_id: HashMap<&str, &ControlBlockKindDef> = HashMap::new();

    for block in &controls.blocks {
        if !block_ids.insert(block.id.as_str()) {
            return Err(ValidationError::DuplicateId {
                id: block.id.clone(),
                context: format!("system '{}' controls blocks", system.name),
            });
        }

        validate_control_block(
            system,
            block.kind.clone(),
            node_ids,
            component_ids,
            &block.name,
        )?;
        block_kind_by_id.insert(block.id.as_str(), &block.kind);
    }

    let mut incoming_by_input: HashSet<(&str, &str)> = HashSet::new();
    let mut adjacency: HashMap<&str, Vec<&str>> = HashMap::new();
    let mut in_degree: HashMap<&str, usize> = block_ids.iter().copied().map(|id| (id, 0)).collect();

    for connection in &controls.connections {
        validate_control_connection(
            system,
            connection,
            &block_kind_by_id,
            &mut incoming_by_input,
        )?;

        adjacency
            .entry(connection.from_block_id.as_str())
            .or_default()
            .push(connection.to_block_id.as_str());
        if let Some(deg) = in_degree.get_mut(connection.to_block_id.as_str()) {
            *deg += 1;
        }
    }

    let mut queue: Vec<&str> = in_degree
        .iter()
        .filter(|(_, degree)| **degree == 0)
        .map(|(id, _)| *id)
        .collect();
    let mut visited = 0usize;

    while let Some(node) = queue.pop() {
        visited += 1;
        if let Some(children) = adjacency.get(node) {
            for child in children {
                if let Some(deg) = in_degree.get_mut(child) {
                    *deg -= 1;
                    if *deg == 0 {
                        queue.push(child);
                    }
                }
            }
        }
    }

    if visited != block_ids.len() {
        return Err(ValidationError::InvalidValue {
            field: format!("system '{}' controls graph", system.name),
            value: "cycle detected".to_string(),
            reason: "control graph must be acyclic".to_string(),
        });
    }

    Ok(())
}

fn validate_control_block(
    system: &SystemDef,
    kind: ControlBlockKindDef,
    node_ids: &HashSet<&String>,
    component_ids: &HashSet<&String>,
    block_name: &str,
) -> Result<(), ValidationError> {
    match kind {
        ControlBlockKindDef::Constant { value } => {
            if !value.is_finite() {
                return Err(ValidationError::InvalidValue {
                    field: format!("control block '{}' value", block_name),
                    value: value.to_string(),
                    reason: "must be finite".to_string(),
                });
            }
        }
        ControlBlockKindDef::MeasuredVariable { reference } => {
            validate_measured_reference(system, node_ids, component_ids, &reference)?;
        }
        ControlBlockKindDef::PIController {
            kp,
            ti_s,
            out_min,
            out_max,
            integral_limit,
            sample_period_s,
        } => {
            if !kp.is_finite() {
                return Err(ValidationError::InvalidValue {
                    field: format!("control block '{}' kp", block_name),
                    value: kp.to_string(),
                    reason: "must be finite".to_string(),
                });
            }
            if !ti_s.is_finite() || ti_s <= 0.0 {
                return Err(ValidationError::InvalidValue {
                    field: format!("control block '{}' ti_s", block_name),
                    value: ti_s.to_string(),
                    reason: "must be positive and finite".to_string(),
                });
            }
            if !sample_period_s.is_finite() || sample_period_s <= 0.0 {
                return Err(ValidationError::InvalidValue {
                    field: format!("control block '{}' sample_period_s", block_name),
                    value: sample_period_s.to_string(),
                    reason: "must be positive and finite".to_string(),
                });
            }
            if !out_min.is_finite() || !out_max.is_finite() || out_min >= out_max {
                return Err(ValidationError::InvalidValue {
                    field: format!("control block '{}' output limits", block_name),
                    value: format!("{}, {}", out_min, out_max),
                    reason: "out_min must be finite and less than out_max".to_string(),
                });
            }
            if let Some(limit) = integral_limit
                && (!limit.is_finite() || limit <= 0.0)
            {
                return Err(ValidationError::InvalidValue {
                    field: format!("control block '{}' integral_limit", block_name),
                    value: limit.to_string(),
                    reason: "must be positive and finite".to_string(),
                });
            }
        }
        ControlBlockKindDef::PIDController {
            kp,
            ti_s,
            td_s,
            td_filter_s,
            out_min,
            out_max,
            integral_limit,
            sample_period_s,
        } => {
            if !kp.is_finite() {
                return Err(ValidationError::InvalidValue {
                    field: format!("control block '{}' kp", block_name),
                    value: kp.to_string(),
                    reason: "must be finite".to_string(),
                });
            }
            if !ti_s.is_finite() || ti_s <= 0.0 {
                return Err(ValidationError::InvalidValue {
                    field: format!("control block '{}' ti_s", block_name),
                    value: ti_s.to_string(),
                    reason: "must be positive and finite".to_string(),
                });
            }
            if !td_s.is_finite() || td_s < 0.0 {
                return Err(ValidationError::InvalidValue {
                    field: format!("control block '{}' td_s", block_name),
                    value: td_s.to_string(),
                    reason: "must be non-negative and finite".to_string(),
                });
            }
            if !td_filter_s.is_finite() || td_filter_s <= 0.0 {
                return Err(ValidationError::InvalidValue {
                    field: format!("control block '{}' td_filter_s", block_name),
                    value: td_filter_s.to_string(),
                    reason: "must be positive and finite".to_string(),
                });
            }
            if !sample_period_s.is_finite() || sample_period_s <= 0.0 {
                return Err(ValidationError::InvalidValue {
                    field: format!("control block '{}' sample_period_s", block_name),
                    value: sample_period_s.to_string(),
                    reason: "must be positive and finite".to_string(),
                });
            }
            if !out_min.is_finite() || !out_max.is_finite() || out_min >= out_max {
                return Err(ValidationError::InvalidValue {
                    field: format!("control block '{}' output limits", block_name),
                    value: format!("{}, {}", out_min, out_max),
                    reason: "out_min must be finite and less than out_max".to_string(),
                });
            }
            if let Some(limit) = integral_limit
                && (!limit.is_finite() || limit <= 0.0)
            {
                return Err(ValidationError::InvalidValue {
                    field: format!("control block '{}' integral_limit", block_name),
                    value: limit.to_string(),
                    reason: "must be positive and finite".to_string(),
                });
            }
        }
        ControlBlockKindDef::FirstOrderActuator {
            tau_s,
            rate_limit_per_s,
            initial_position,
        } => {
            if !tau_s.is_finite() || tau_s <= 0.0 {
                return Err(ValidationError::InvalidValue {
                    field: format!("control block '{}' tau_s", block_name),
                    value: tau_s.to_string(),
                    reason: "must be positive and finite".to_string(),
                });
            }
            if !rate_limit_per_s.is_finite() || rate_limit_per_s <= 0.0 {
                return Err(ValidationError::InvalidValue {
                    field: format!("control block '{}' rate_limit_per_s", block_name),
                    value: rate_limit_per_s.to_string(),
                    reason: "must be positive and finite".to_string(),
                });
            }
            if !initial_position.is_finite() || !(0.0..=1.0).contains(&initial_position) {
                return Err(ValidationError::InvalidValue {
                    field: format!("control block '{}' initial_position", block_name),
                    value: initial_position.to_string(),
                    reason: "must be finite and in [0, 1]".to_string(),
                });
            }
        }
        ControlBlockKindDef::ActuatorCommand { component_id } => {
            if !component_ids.contains(&component_id) {
                return Err(ValidationError::MissingReference {
                    id: component_id.clone(),
                    context: format!("system '{}' control actuator target", system.name),
                });
            }

            let maybe_kind = system
                .components
                .iter()
                .find(|c| c.id == component_id)
                .map(|c| &c.kind);
            if !matches!(maybe_kind, Some(ComponentKind::Valve { .. })) {
                return Err(ValidationError::InvalidValue {
                    field: format!("control actuator target '{}'", component_id),
                    value: component_id.clone(),
                    reason: "actuator commands currently support Valve components only".to_string(),
                });
            }
        }
    }
    Ok(())
}

fn validate_measured_reference(
    system: &SystemDef,
    node_ids: &HashSet<&String>,
    component_ids: &HashSet<&String>,
    reference: &MeasuredVariableDef,
) -> Result<(), ValidationError> {
    match reference {
        MeasuredVariableDef::NodePressure { node_id }
        | MeasuredVariableDef::NodeTemperature { node_id } => {
            if !node_ids.contains(node_id) {
                return Err(ValidationError::MissingReference {
                    id: node_id.clone(),
                    context: format!("system '{}' control measured node", system.name),
                });
            }
        }
        MeasuredVariableDef::EdgeMassFlow { component_id } => {
            if !component_ids.contains(component_id) {
                return Err(ValidationError::MissingReference {
                    id: component_id.clone(),
                    context: format!("system '{}' control measured component", system.name),
                });
            }
        }
        MeasuredVariableDef::PressureDrop {
            from_node_id,
            to_node_id,
        } => {
            if !node_ids.contains(from_node_id) {
                return Err(ValidationError::MissingReference {
                    id: from_node_id.clone(),
                    context: format!("system '{}' control pressure-drop source node", system.name),
                });
            }
            if !node_ids.contains(to_node_id) {
                return Err(ValidationError::MissingReference {
                    id: to_node_id.clone(),
                    context: format!(
                        "system '{}' control pressure-drop destination node",
                        system.name
                    ),
                });
            }
        }
    }

    Ok(())
}

fn validate_control_connection<'a>(
    system: &SystemDef,
    connection: &'a ControlConnectionDef,
    block_kind_by_id: &HashMap<&'a str, &'a ControlBlockKindDef>,
    incoming_by_input: &mut HashSet<(&'a str, &'a str)>,
) -> Result<(), ValidationError> {
    let Some(from_kind) = block_kind_by_id.get(connection.from_block_id.as_str()) else {
        return Err(ValidationError::MissingReference {
            id: connection.from_block_id.clone(),
            context: format!("system '{}' controls connection from_block_id", system.name),
        });
    };
    let Some(to_kind) = block_kind_by_id.get(connection.to_block_id.as_str()) else {
        return Err(ValidationError::MissingReference {
            id: connection.to_block_id.clone(),
            context: format!("system '{}' controls connection to_block_id", system.name),
        });
    };

    if !incoming_by_input.insert((
        connection.to_block_id.as_str(),
        connection.to_input.as_str(),
    )) {
        return Err(ValidationError::InvalidValue {
            field: format!(
                "system '{}' controls connection {}.{}",
                system.name, connection.to_block_id, connection.to_input
            ),
            value: "duplicate incoming connection".to_string(),
            reason: "each block input may have at most one source".to_string(),
        });
    }

    let source_has_output = matches!(
        from_kind,
        ControlBlockKindDef::Constant { .. }
            | ControlBlockKindDef::MeasuredVariable { .. }
            | ControlBlockKindDef::PIController { .. }
            | ControlBlockKindDef::PIDController { .. }
            | ControlBlockKindDef::FirstOrderActuator { .. }
    );
    if !source_has_output {
        return Err(ValidationError::InvalidValue {
            field: format!(
                "system '{}' controls connection from '{}'",
                system.name, connection.from_block_id
            ),
            value: connection.from_block_id.clone(),
            reason: "source block does not produce an output signal".to_string(),
        });
    }

    let valid_input = match to_kind {
        ControlBlockKindDef::PIController { .. } | ControlBlockKindDef::PIDController { .. } => {
            connection.to_input == "setpoint" || connection.to_input == "process"
        }
        ControlBlockKindDef::FirstOrderActuator { .. } => connection.to_input == "command",
        ControlBlockKindDef::ActuatorCommand { .. } => connection.to_input == "position",
        _ => false,
    };
    if !valid_input {
        return Err(ValidationError::InvalidValue {
            field: format!(
                "system '{}' controls connection to '{}.{}'",
                system.name, connection.to_block_id, connection.to_input
            ),
            value: connection.to_input.clone(),
            reason: "invalid destination input port".to_string(),
        });
    }

    if matches!(to_kind, ControlBlockKindDef::ActuatorCommand { .. })
        && !matches!(from_kind, ControlBlockKindDef::FirstOrderActuator { .. })
    {
        return Err(ValidationError::InvalidValue {
            field: format!(
                "system '{}' controls connection to actuator command '{}'",
                system.name, connection.to_block_id
            ),
            value: connection.from_block_id.clone(),
            reason: "actuator command input must come from a FirstOrderActuator block".to_string(),
        });
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
