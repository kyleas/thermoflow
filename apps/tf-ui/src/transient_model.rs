//! Legacy transient model retained for editor research only.
//! Canonical transient execution uses `tf_app::run_service::ensure_run_with_progress`
//! and `tf_app::transient_compile::TransientNetworkModel`.
#![allow(dead_code)]

use std::collections::HashMap;

use tf_components::{Orifice, Pipe, Pump, Turbine, TwoPortComponent, Valve, ValveLaw};
use tf_core::units::{Area, DynVisc, Pressure, Temperature, kgps, m, pa};
use tf_core::{CompId, NodeId};
use tf_fluids::{Composition, FluidModel, StateInput};
use tf_project::schema::{
    ActionDef, BoundaryDef, ComponentKind, NodeDef, NodeKind, ScheduleDef, SystemDef, ValveLawDef,
};
use tf_sim::{ControlVolume, ControlVolumeState, SimError, SimResult, TransientModel};
use tf_solver::{SteadyProblem, SteadySolution};
use uom::si::area::square_meter;
use uom::si::dynamic_viscosity::pascal_second;
use uom::si::thermodynamic_temperature::kelvin;

use crate::project_io::{self, SystemRuntime};

#[derive(Clone, Debug)]
pub struct TransientState {
    pub control_volumes: Vec<ControlVolumeState>,
}

impl TransientState {
    fn len(&self) -> usize {
        self.control_volumes.len()
    }
}

#[derive(Default, Clone)]
struct ScheduleData {
    valve_events: HashMap<String, Vec<(f64, f64)>>,
    boundary_pressure_events: HashMap<String, Vec<(f64, f64)>>,
    boundary_temperature_events: HashMap<String, Vec<(f64, f64)>>,
}

pub struct TransientNetworkModel {
    system: SystemDef,
    runtime: SystemRuntime,
    fluid_model: Box<dyn FluidModel>,
    composition: Composition,
    control_volumes: Vec<ControlVolume>,
    cv_node_ids: Vec<NodeId>,
    cv_index_by_node: HashMap<NodeId, usize>,
    initial_state: TransientState,
    schedules: ScheduleData,
    last_steady_solution: Option<SteadySolution>,
    last_cv_pressure: Vec<Option<Pressure>>,
    solution_cache: HashMap<i64, SteadySolution>,
}

struct Snapshot {
    solution: SteadySolution,
    components: HashMap<CompId, Box<dyn TwoPortComponent>>,
}

impl TransientNetworkModel {
    pub fn new(system: &SystemDef, runtime: &SystemRuntime) -> Result<Self, String> {
        let fluid_model = project_io::build_fluid_model(&system.fluid)?;
        let composition = runtime.composition.clone();

        let schedules = build_schedule_data(&system.schedules);

        let (control_volumes, cv_node_ids, cv_index_by_node, initial_state) =
            build_control_volumes(system, runtime, fluid_model.as_ref(), composition.clone())?;

        let last_cv_pressure = vec![None; control_volumes.len()];

        Ok(Self {
            system: system.clone(),
            runtime: SystemRuntime {
                graph: runtime.graph.clone(),
                composition: runtime.composition.clone(),
                node_id_map: runtime.node_id_map.clone(),
                comp_id_map: runtime.comp_id_map.clone(),
            },
            fluid_model,
            composition,
            control_volumes,
            cv_node_ids,
            cv_index_by_node,
            initial_state,
            schedules,
            last_steady_solution: None,
            last_cv_pressure,
            solution_cache: HashMap::new(),
        })
    }

    pub fn build_timeseries_record(
        &mut self,
        time_s: f64,
        state: &TransientState,
    ) -> Result<tf_results::TimeseriesRecord, String> {
        use tf_results::{
            EdgeValueSnapshot, GlobalValueSnapshot, NodeValueSnapshot, TimeseriesRecord,
        };

        let solution = if let Some(cached) = self.solution_cache.get(&time_key(time_s)) {
            cached.clone()
        } else {
            let snapshot = self
                .solve_snapshot(time_s, state)
                .map_err(|e| format!("Transient snapshot failed: {}", e))?;
            snapshot.solution
        };

        let mut node_values = Vec::new();
        for (node_id_str, &node_idx) in &self.runtime.node_id_map {
            if let Some(&p_val) = solution.pressures.get(node_idx.index() as usize) {
                let h_val = solution
                    .enthalpies
                    .get(node_idx.index() as usize)
                    .copied()
                    .unwrap_or_default();

                node_values.push(NodeValueSnapshot {
                    node_id: node_id_str.clone(),
                    p_pa: Some(p_val.value),
                    t_k: None,
                    h_j_per_kg: Some(h_val),
                    rho_kg_m3: None,
                });
            }
        }

        let mut edge_values = Vec::new();
        for (comp_id_str, &comp_idx) in &self.runtime.comp_id_map {
            if let Some((_, mdot)) = solution.mass_flows.iter().find(|(id, _)| *id == comp_idx) {
                edge_values.push(EdgeValueSnapshot {
                    component_id: comp_id_str.clone(),
                    mdot_kg_s: Some(*mdot),
                    delta_p_pa: None,
                });
            }
        }

        Ok(TimeseriesRecord {
            time_s,
            node_values,
            edge_values,
            global_values: GlobalValueSnapshot::default(),
        })
    }

    fn solve_snapshot(&mut self, time_s: f64, state: &TransientState) -> SimResult<Snapshot> {
        let mut problem = SteadyProblem::new(
            &self.runtime.graph,
            self.fluid_model.as_ref(),
            self.composition.clone(),
        );

        let mut components_for_problem = build_components_with_schedules(
            &self.system,
            &self.runtime.comp_id_map,
            &self.schedules,
            time_s,
        )
        .map_err(|e| SimError::Backend { message: e })?;

        for (comp_id, component) in components_for_problem.drain() {
            problem.add_component(comp_id, component)?;
        }

        let boundary_defs = apply_boundary_schedules(&self.system, &self.schedules, time_s);
        let boundaries = project_io::parse_boundaries_with_atmosphere(
            &self.system,
            &boundary_defs,
            &self.runtime.node_id_map,
        )
        .map_err(|e| SimError::Backend { message: e })?;

        for (node_id, bc) in boundaries {
            match bc {
                project_io::BoundaryCondition::PT { p, t } => {
                    problem.set_pressure_bc(node_id, p)?;
                    problem.set_temperature_bc(node_id, t)?;
                }
                project_io::BoundaryCondition::PH { p, h } => {
                    problem.set_pressure_bc(node_id, p)?;
                    problem.set_enthalpy_bc(node_id, h)?;
                }
            }
        }

        // Apply control-volume boundaries
        for (idx, &node_id) in self.cv_node_ids.iter().enumerate() {
            let cv = &self.control_volumes[idx];
            let cv_state = &state.control_volumes[idx];
            let p_hint = self.last_cv_pressure[idx];

            let (p, h) = cv.state_ph_boundary(self.fluid_model.as_ref(), cv_state, p_hint)?;
            self.last_cv_pressure[idx] = Some(p);

            problem.set_pressure_bc(node_id, p)?;
            problem.set_enthalpy_bc(node_id, h)?;
        }

        let solution = tf_solver::solve(&mut problem, None, self.last_steady_solution.as_ref())
            .map_err(|e| SimError::Backend {
                message: format!("Solver failed at t={}: {}", time_s, e),
            })?;

        self.last_steady_solution = Some(solution.clone());
        self.store_solution_cache(time_s, solution.clone());

        let components_for_snapshot = build_components_with_schedules(
            &self.system,
            &self.runtime.comp_id_map,
            &self.schedules,
            time_s,
        )
        .map_err(|e| SimError::Backend { message: e })?;

        Ok(Snapshot {
            solution,
            components: components_for_snapshot,
        })
    }

    fn store_solution_cache(&mut self, time_s: f64, solution: SteadySolution) {
        let key = time_key(time_s);
        self.solution_cache.insert(key, solution);
        if self.solution_cache.len() > 500 {
            self.solution_cache.clear();
        }
    }
}

impl TransientModel for TransientNetworkModel {
    type State = TransientState;

    fn initial_state(&self) -> Self::State {
        self.initial_state.clone()
    }

    fn rhs(&mut self, t: f64, x: &Self::State) -> SimResult<Self::State> {
        let snapshot = self.solve_snapshot(t, x)?;
        let solution = &snapshot.solution;

        let mut node_states = Vec::new();
        for (i, (&p, &h)) in solution
            .pressures
            .iter()
            .zip(solution.enthalpies.iter())
            .enumerate()
        {
            let state = self
                .fluid_model
                .state(StateInput::PH { p, h }, self.composition.clone())
                .map_err(|e| SimError::Backend {
                    message: format!("Failed to create state for node {}: {}", i, e),
                })?;
            node_states.push(state);
        }

        let mut dm_in = vec![0.0; self.control_volumes.len()];
        let mut dm_out = vec![0.0; self.control_volumes.len()];
        let mut dmh_in = vec![0.0; self.control_volumes.len()];
        let mut dmh_out = vec![0.0; self.control_volumes.len()];

        for (comp_id, mdot) in &solution.mass_flows {
            let inlet_node = self
                .runtime
                .graph
                .comp_inlet_node(*comp_id)
                .ok_or_else(|| SimError::Backend {
                    message: format!("Component {:?} has no inlet", comp_id),
                })?;
            let outlet_node = self
                .runtime
                .graph
                .comp_outlet_node(*comp_id)
                .ok_or_else(|| SimError::Backend {
                    message: format!("Component {:?} has no outlet", comp_id),
                })?;

            let inlet_idx = inlet_node.index() as usize;
            let outlet_idx = outlet_node.index() as usize;

            let inlet_state = &node_states[inlet_idx];
            let outlet_state = &node_states[outlet_idx];

            let component_model =
                snapshot
                    .components
                    .get(comp_id)
                    .ok_or_else(|| SimError::Backend {
                        message: format!("Component model not found for {:?}", comp_id),
                    })?;

            if *mdot >= 0.0 {
                // Flow from inlet to outlet
                if let Some(&cv_idx) = self.cv_index_by_node.get(&inlet_node) {
                    dm_out[cv_idx] += *mdot;
                    dmh_out[cv_idx] += *mdot * x.control_volumes[cv_idx].h_j_per_kg;
                }

                if let Some(&cv_idx) = self.cv_index_by_node.get(&outlet_node) {
                    let ports = tf_components::PortStates {
                        inlet: inlet_state,
                        outlet: outlet_state,
                    };
                    let h_out = component_model
                        .outlet_enthalpy(self.fluid_model.as_ref(), ports, kgps(*mdot))
                        .map_err(|e| SimError::Backend {
                            message: format!("Component {:?} enthalpy failed: {}", comp_id, e),
                        })?;

                    dm_in[cv_idx] += *mdot;
                    dmh_in[cv_idx] += *mdot * h_out;
                }
            } else {
                let mdot_abs = -(*mdot);

                // Flow from outlet to inlet
                if let Some(&cv_idx) = self.cv_index_by_node.get(&outlet_node) {
                    dm_out[cv_idx] += mdot_abs;
                    dmh_out[cv_idx] += mdot_abs * x.control_volumes[cv_idx].h_j_per_kg;
                }

                if let Some(&cv_idx) = self.cv_index_by_node.get(&inlet_node) {
                    let ports = tf_components::PortStates {
                        inlet: outlet_state,
                        outlet: inlet_state,
                    };
                    let h_out = component_model
                        .outlet_enthalpy(self.fluid_model.as_ref(), ports, kgps(mdot_abs))
                        .map_err(|e| SimError::Backend {
                            message: format!("Component {:?} enthalpy failed: {}", comp_id, e),
                        })?;

                    dm_in[cv_idx] += mdot_abs;
                    dmh_in[cv_idx] += mdot_abs * h_out;
                }
            }
        }

        let mut deriv = Vec::new();
        for i in 0..self.control_volumes.len() {
            let m = x.control_volumes[i].m_kg;
            if m <= 0.0 {
                return Err(SimError::NonPhysical {
                    what: "control volume mass must be positive",
                });
            }
            let dm = dm_in[i] - dm_out[i];
            let dmh = dmh_in[i] - dmh_out[i];
            let h_dot = (dmh - x.control_volumes[i].h_j_per_kg * dm) / m;

            deriv.push(ControlVolumeState {
                m_kg: dm,
                h_j_per_kg: h_dot,
            });
        }

        Ok(TransientState {
            control_volumes: deriv,
        })
    }

    fn add(&self, a: &Self::State, b: &Self::State) -> Self::State {
        let mut out = Vec::with_capacity(a.len());
        for i in 0..a.len() {
            out.push(ControlVolumeState {
                m_kg: a.control_volumes[i].m_kg + b.control_volumes[i].m_kg,
                h_j_per_kg: a.control_volumes[i].h_j_per_kg + b.control_volumes[i].h_j_per_kg,
            });
        }
        TransientState {
            control_volumes: out,
        }
    }

    fn scale(&self, a: &Self::State, scale: f64) -> Self::State {
        let mut out = Vec::with_capacity(a.len());
        for cv in &a.control_volumes {
            out.push(ControlVolumeState {
                m_kg: cv.m_kg * scale,
                h_j_per_kg: cv.h_j_per_kg * scale,
            });
        }
        TransientState {
            control_volumes: out,
        }
    }
}

type BuildControlVolumesResult = Result<
    (
        Vec<ControlVolume>,
        Vec<NodeId>,
        HashMap<NodeId, usize>,
        TransientState,
    ),
    String,
>;

fn build_control_volumes(
    system: &SystemDef,
    runtime: &SystemRuntime,
    fluid: &dyn FluidModel,
    composition: Composition,
) -> BuildControlVolumesResult {
    let mut control_volumes = Vec::new();
    let mut cv_node_ids = Vec::new();
    let mut cv_index_by_node = HashMap::new();
    let mut initial_states = Vec::new();

    for node in &system.nodes {
        if let NodeKind::ControlVolume { volume_m3, initial } = &node.kind {
            let cv = ControlVolume::new(node.name.clone(), *volume_m3, composition.clone())
                .map_err(|e| format!("Control volume error: {}", e))?;
            let state = initial_state_from_def(node, *volume_m3, initial, fluid, &composition)?;

            let node_id = *runtime
                .node_id_map
                .get(&node.id)
                .ok_or_else(|| format!("Node not found: {}", node.id))?;

            cv_index_by_node.insert(node_id, control_volumes.len());
            control_volumes.push(cv);
            cv_node_ids.push(node_id);
            initial_states.push(state);
        }
    }

    Ok((
        control_volumes,
        cv_node_ids,
        cv_index_by_node,
        TransientState {
            control_volumes: initial_states,
        },
    ))
}

fn initial_state_from_def(
    node: &NodeDef,
    volume_m3: f64,
    initial: &tf_project::schema::InitialCvDef,
    fluid: &dyn FluidModel,
    composition: &Composition,
) -> Result<ControlVolumeState, String> {
    let mut m_kg = initial.m_kg;
    let mut h_j_per_kg = initial.h_j_per_kg;

    if h_j_per_kg.is_none() {
        if let (Some(p_pa), Some(t_k)) = (initial.p_pa, initial.t_k) {
            let state = fluid
                .state(
                    StateInput::PT {
                        p: pa(p_pa),
                        t: Temperature::new::<kelvin>(t_k),
                    },
                    composition.clone(),
                )
                .map_err(|e| format!("Initial state invalid for {}: {}", node.id, e))?;

            let h = fluid
                .h(&state)
                .map_err(|e| format!("Initial enthalpy invalid for {}: {}", node.id, e))?;
            h_j_per_kg = Some(h);
        }
    }

    if m_kg.is_none() {
        if let (Some(p_pa), Some(t_k)) = (initial.p_pa, initial.t_k) {
            let state = fluid
                .state(
                    StateInput::PT {
                        p: pa(p_pa),
                        t: Temperature::new::<kelvin>(t_k),
                    },
                    composition.clone(),
                )
                .map_err(|e| format!("Initial state invalid for {}: {}", node.id, e))?;
            let rho = fluid
                .rho(&state)
                .map_err(|e| format!("Initial density invalid for {}: {}", node.id, e))?;
            m_kg = Some(rho.value * volume_m3);
        } else if let (Some(p_pa), Some(h)) = (initial.p_pa, h_j_per_kg) {
            let state = fluid
                .state(StateInput::PH { p: pa(p_pa), h }, composition.clone())
                .map_err(|e| format!("Initial state invalid for {}: {}", node.id, e))?;
            let rho = fluid
                .rho(&state)
                .map_err(|e| format!("Initial density invalid for {}: {}", node.id, e))?;
            m_kg = Some(rho.value * volume_m3);
        }
    }

    let m_kg = m_kg.ok_or_else(|| {
        format!(
            "Control volume '{}' requires initial mass or (p,t) to derive mass",
            node.id
        )
    })?;

    let h_j_per_kg = h_j_per_kg.ok_or_else(|| {
        format!(
            "Control volume '{}' requires initial enthalpy or (p,t) to derive enthalpy",
            node.id
        )
    })?;

    Ok(ControlVolumeState { m_kg, h_j_per_kg })
}

fn build_schedule_data(schedules: &[ScheduleDef]) -> ScheduleData {
    let mut data = ScheduleData::default();

    for schedule in schedules {
        for event in &schedule.events {
            match &event.action {
                ActionDef::SetValvePosition {
                    component_id,
                    position,
                } => {
                    data.valve_events
                        .entry(component_id.clone())
                        .or_default()
                        .push((event.time_s, *position));
                }
                ActionDef::SetBoundaryPressure {
                    node_id,
                    pressure_pa,
                } => {
                    data.boundary_pressure_events
                        .entry(node_id.clone())
                        .or_default()
                        .push((event.time_s, *pressure_pa));
                }
                ActionDef::SetBoundaryTemperature {
                    node_id,
                    temperature_k,
                } => {
                    data.boundary_temperature_events
                        .entry(node_id.clone())
                        .or_default()
                        .push((event.time_s, *temperature_k));
                }
            }
        }
    }

    for events in data.valve_events.values_mut() {
        events.sort_by(|a, b| a.0.partial_cmp(&b.0).unwrap_or(std::cmp::Ordering::Equal));
    }
    for events in data.boundary_pressure_events.values_mut() {
        events.sort_by(|a, b| a.0.partial_cmp(&b.0).unwrap_or(std::cmp::Ordering::Equal));
    }
    for events in data.boundary_temperature_events.values_mut() {
        events.sort_by(|a, b| a.0.partial_cmp(&b.0).unwrap_or(std::cmp::Ordering::Equal));
    }

    data
}

fn apply_boundary_schedules(
    system: &SystemDef,
    schedules: &ScheduleData,
    time_s: f64,
) -> Vec<BoundaryDef> {
    let mut boundary_map: HashMap<String, BoundaryDef> = HashMap::new();
    let atmosphere_nodes: std::collections::HashSet<&str> = system
        .nodes
        .iter()
        .filter_map(|node| match node.kind {
            NodeKind::Atmosphere { .. } => Some(node.id.as_str()),
            _ => None,
        })
        .collect();

    for b in &system.boundaries {
        if atmosphere_nodes.contains(b.node_id.as_str()) {
            continue;
        }
        boundary_map.insert(b.node_id.clone(), b.clone());
    }

    for (node_id, events) in &schedules.boundary_pressure_events {
        if atmosphere_nodes.contains(node_id.as_str()) {
            continue;
        }
        if let Some(value) = last_event_value(events, time_s) {
            boundary_map
                .entry(node_id.clone())
                .and_modify(|b| b.pressure_pa = Some(value))
                .or_insert(BoundaryDef {
                    node_id: node_id.clone(),
                    pressure_pa: Some(value),
                    temperature_k: None,
                    enthalpy_j_per_kg: None,
                });
        }
    }

    for (node_id, events) in &schedules.boundary_temperature_events {
        if atmosphere_nodes.contains(node_id.as_str()) {
            continue;
        }
        if let Some(value) = last_event_value(events, time_s) {
            boundary_map
                .entry(node_id.clone())
                .and_modify(|b| b.temperature_k = Some(value))
                .or_insert(BoundaryDef {
                    node_id: node_id.clone(),
                    pressure_pa: None,
                    temperature_k: Some(value),
                    enthalpy_j_per_kg: None,
                });
        }
    }

    boundary_map.into_values().collect()
}

fn build_components_with_schedules(
    system: &SystemDef,
    comp_id_map: &HashMap<String, CompId>,
    schedules: &ScheduleData,
    time_s: f64,
) -> Result<HashMap<CompId, Box<dyn TwoPortComponent>>, String> {
    let mut components: HashMap<CompId, Box<dyn TwoPortComponent>> = HashMap::new();

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
                        area_from_m2(*area_m2),
                    ))
                } else {
                    Box::new(Orifice::new(
                        component.name.clone(),
                        *cd,
                        area_from_m2(*area_m2),
                    ))
                }
            }
            ComponentKind::Valve {
                cd,
                area_max_m2,
                position,
                law,
                treat_as_gas,
            } => {
                let valve_law = match law {
                    ValveLawDef::Linear => ValveLaw::Linear,
                    ValveLawDef::Quadratic => ValveLaw::Quadratic,
                    ValveLawDef::QuickOpening => ValveLaw::Linear,
                };

                let mut pos = *position;
                if let Some(events) = schedules.valve_events.get(&component.id) {
                    if let Some(value) = last_event_value(events, time_s) {
                        pos = value;
                    }
                }

                let mut valve =
                    Valve::new(component.name.clone(), *cd, area_from_m2(*area_max_m2), pos);
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
                dyn_visc_from_pa_s(*mu_pa_s),
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
                    area_from_m2(*area_m2),
                )
                .map_err(|e| format!("Pump creation error: {}", e))?,
            ),
            ComponentKind::Turbine {
                cd, area_m2, eta, ..
            } => Box::new(
                Turbine::new(component.name.clone(), *cd, area_from_m2(*area_m2), *eta)
                    .map_err(|e| format!("Turbine creation error: {}", e))?,
            ),
        };

        components.insert(comp_id, boxed);
    }

    Ok(components)
}

fn last_event_value(events: &[(f64, f64)], time_s: f64) -> Option<f64> {
    let mut value = None;
    for (t, v) in events {
        if *t <= time_s {
            value = Some(*v);
        } else {
            break;
        }
    }
    value
}

fn time_key(time_s: f64) -> i64 {
    (time_s * 1e9).round() as i64
}

fn area_from_m2(value: f64) -> Area {
    Area::new::<square_meter>(value)
}

fn dyn_visc_from_pa_s(value: f64) -> DynVisc {
    DynVisc::new::<pascal_second>(value)
}
