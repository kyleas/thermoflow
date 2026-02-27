//! Legacy UI project/runtime helpers retained for editor-local workflows.
//! Canonical run execution and runtime compilation live in `tf-app`.
#![allow(dead_code)]

use std::collections::HashMap;
use tf_components::{Orifice, Pipe, Pump, Turbine, TwoPortComponent, Valve, ValveLaw};
use tf_core::units::{Area, DynVisc, Pressure, Temperature, m, pa};
use tf_fluids::{Composition, CoolPropModel, FluidModel, Species};
use tf_graph::GraphBuilder;
use tf_project::schema::{
    BoundaryDef, ComponentKind, CompositionDef, FluidDef, NodeKind, SystemDef, ValveLawDef,
};
use uom::si::area::square_meter;
use uom::si::dynamic_viscosity::pascal_second;
use uom::si::pressure::pascal;

use uom::si::thermodynamic_temperature::kelvin;

pub type CompileResult<T> = Result<T, String>;

pub struct SystemRuntime {
    pub graph: tf_graph::Graph,
    pub composition: Composition,
    pub node_id_map: HashMap<String, tf_core::NodeId>,
    pub comp_id_map: HashMap<String, tf_core::CompId>,
}

/// Build components map from system definition.
pub fn build_components(
    system: &SystemDef,
    comp_id_map: &HashMap<String, tf_core::CompId>,
) -> CompileResult<HashMap<tf_core::CompId, Box<dyn TwoPortComponent>>> {
    let mut components: HashMap<tf_core::CompId, Box<dyn TwoPortComponent>> = HashMap::new();

    for component in &system.components {
        let comp_id = *comp_id_map
            .get(&component.id)
            .ok_or_else(|| format!("Component ID not found: {}", component.id))?;
        let boxed: Box<dyn TwoPortComponent> = match &component.kind {
            ComponentKind::Orifice {
                cd,
                area_m2,
                treat_as_gas,
            } => {
                if *treat_as_gas {
                    Box::new(Orifice::new_compressible(
                        component.name.clone(),
                        *cd,
                        area(*area_m2),
                    ))
                } else {
                    Box::new(Orifice::new(component.name.clone(), *cd, area(*area_m2)))
                }
            }
            ComponentKind::Valve {
                cd,
                area_max_m2,
                position,
                law,
                treat_as_gas,
                ..
            } => {
                let valve_law = match law {
                    ValveLawDef::Linear => ValveLaw::Linear,
                    ValveLawDef::Quadratic => ValveLaw::Quadratic,
                    ValveLawDef::QuickOpening => {
                        // Quick opening not in current API, use Linear
                        ValveLaw::Linear
                    }
                };
                let mut valve =
                    Valve::new(component.name.clone(), *cd, area(*area_max_m2), *position);
                valve = valve.with_law(valve_law);
                if *treat_as_gas {
                    valve = valve.with_compressible();
                }
                Box::new(valve)
            }
            ComponentKind::Pipe {
                length_m,
                diameter_m,
                roughness_m,
                k_minor,
                mu_pa_s,
            } => Box::new(Pipe::new(
                component.name.clone(),
                m(*length_m),
                m(*diameter_m),
                m(*roughness_m),
                *k_minor,
                dyn_visc(*mu_pa_s),
            )),
            ComponentKind::Pump {
                cd,
                area_m2,
                delta_p_pa,
                eta,
                ..
            } => Box::new(
                Pump::new(
                    component.name.clone(),
                    pa(*delta_p_pa),
                    *eta,
                    *cd,
                    area(*area_m2),
                )
                .map_err(|e| format!("Pump creation error: {}", e))?,
            ),
            ComponentKind::Turbine {
                cd, area_m2, eta, ..
            } => Box::new(
                Turbine::new(component.name.clone(), *cd, area(*area_m2), *eta)
                    .map_err(|e| format!("Turbine creation error: {}", e))?,
            ),
        };
        components.insert(comp_id, boxed);
    }

    Ok(components)
}

pub fn compile_system(system: &SystemDef) -> CompileResult<SystemRuntime> {
    let mut builder = GraphBuilder::new();
    let mut node_map = HashMap::new();

    for node in &system.nodes {
        let node_id = builder.add_node(&node.name);
        node_map.insert(node.id.clone(), node_id);
    }

    let mut comp_id_map = HashMap::new();
    for component in &system.components {
        let from_node = *node_map
            .get(&component.from_node_id)
            .ok_or_else(|| format!("Node not found: {}", component.from_node_id))?;
        let to_node = *node_map
            .get(&component.to_node_id)
            .ok_or_else(|| format!("Node not found: {}", component.to_node_id))?;
        let comp_id = builder.add_component(&component.name, from_node, to_node);
        comp_id_map.insert(component.id.clone(), comp_id);
    }

    let graph = builder
        .build()
        .map_err(|e| format!("Graph build error: {}", e))?;

    let composition = match &system.fluid.composition {
        CompositionDef::Pure { species } => Composition::pure(parse_species(species)?),
        CompositionDef::Mixture { .. } => {
            return Err("Mixtures not yet supported in UI".to_string());
        }
    };

    Ok(SystemRuntime {
        graph,
        composition,
        node_id_map: node_map,
        comp_id_map,
    })
}

#[allow(dead_code)]
pub fn get_fluid_model() -> CoolPropModel {
    CoolPropModel::new()
}

pub fn build_fluid_model(_fluid_def: &FluidDef) -> CompileResult<Box<dyn FluidModel>> {
    // For now, just use CoolPropModel
    Ok(Box::new(CoolPropModel::new()))
}

pub enum BoundaryCondition {
    PT { p: Pressure, t: Temperature },
    PH { p: Pressure, h: f64 },
}

pub fn parse_boundaries(
    boundary_defs: &[BoundaryDef],
    node_id_map: &HashMap<String, tf_core::NodeId>,
) -> CompileResult<HashMap<tf_core::NodeId, BoundaryCondition>> {
    let mut boundaries = HashMap::new();

    for bnd in boundary_defs {
        let node_id = *node_id_map
            .get(&bnd.node_id)
            .ok_or_else(|| format!("Boundary node not found: {}", bnd.node_id))?;

        let bc = match (bnd.pressure_pa, bnd.temperature_k, bnd.enthalpy_j_per_kg) {
            (Some(p), Some(t), _) => BoundaryCondition::PT {
                p: Pressure::new::<pascal>(p),
                t: Temperature::new::<kelvin>(t),
            },
            (Some(p), None, Some(h)) => BoundaryCondition::PH {
                p: Pressure::new::<pascal>(p),
                h,
            },
            _ => {
                return Err(format!(
                    "Boundary node '{}' must specify either (pressure, temperature) or (pressure, enthalpy)",
                    bnd.node_id
                ));
            }
        };

        boundaries.insert(node_id, bc);
    }

    Ok(boundaries)
}

pub fn parse_boundaries_with_atmosphere(
    system: &SystemDef,
    boundary_defs: &[BoundaryDef],
    node_id_map: &HashMap<String, tf_core::NodeId>,
) -> CompileResult<HashMap<tf_core::NodeId, BoundaryCondition>> {
    let mut boundaries = parse_boundaries(boundary_defs, node_id_map)?;

    for node in &system.nodes {
        if let NodeKind::Atmosphere {
            pressure_pa,
            temperature_k,
        } = node.kind
        {
            let node_id = *node_id_map
                .get(&node.id)
                .ok_or_else(|| format!("Atmosphere node not found: {}", node.id))?;

            if boundaries.contains_key(&node_id) {
                return Err(format!(
                    "Atmosphere node '{}' must not also have a boundary",
                    node.id
                ));
            }

            boundaries.insert(
                node_id,
                BoundaryCondition::PT {
                    p: Pressure::new::<pascal>(pressure_pa),
                    t: Temperature::new::<kelvin>(temperature_k),
                },
            );
        }
    }

    Ok(boundaries)
}

fn parse_species(name: &str) -> CompileResult<Species> {
    match name.to_uppercase().as_str() {
        "N2" | "NITROGEN" => Ok(Species::N2),
        "O2" | "OXYGEN" => Ok(Species::O2),
        "H2" | "HYDROGEN" => Ok(Species::H2),
        "HE" | "HELIUM" => Ok(Species::He),
        "AR" | "ARGON" => Ok(Species::Ar),
        "CH4" | "METHANE" => Ok(Species::CH4),
        "CO2" => Ok(Species::CO2),
        "CO" => Ok(Species::CO),
        "H2O" | "WATER" => Ok(Species::H2O),
        _ => Err(format!("Unknown species: {}", name)),
    }
}

fn area(value: f64) -> Area {
    Area::new::<square_meter>(value)
}

fn dyn_visc(value: f64) -> DynVisc {
    DynVisc::new::<pascal_second>(value)
}
