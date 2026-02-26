//! Transient simulation compilation and runtime setup.
//!
//! This module handles:
//! - Converting a system definition into a transient runtime model
//! - Building control volumes and initial conditions
//! - Parsing and applying scheduled boundary/component changes
//! - Integration with tf-sim for time-stepping

use std::collections::{HashMap, HashSet};

use tf_components::{Orifice, Pipe, Pump, Turbine, TwoPortComponent, Valve, ValveLaw};
use tf_core::units::{kgps, m, pa, Area, DynVisc, Pressure, Temperature};
use tf_core::{CompId, NodeId};
use tf_fluids::{Composition, FluidModel, SpecEnthalpy, StateInput};
use tf_project::schema::{
    ActionDef, BoundaryDef, ComponentDef, ComponentKind, NodeDef, NodeKind, ScheduleDef, SystemDef,
    ValveLawDef,
};
use tf_sim::{ControlVolume, ControlVolumeState, SimError, SimResult, TransientModel};
use tf_solver::{SteadyProblem, SteadySolution};
use uom::si::area::square_meter;
use uom::si::dynamic_viscosity::pascal_second;
use uom::si::thermodynamic_temperature::kelvin;

use crate::runtime_compile::{self, SystemRuntime};
use crate::AppError;

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
    last_active_components: HashSet<CompId>,
}

struct Snapshot {
    solution: SteadySolution,
    components: HashMap<CompId, Box<dyn TwoPortComponent>>,
}

impl TransientNetworkModel {
    pub fn new(system: &SystemDef, runtime: &SystemRuntime) -> Result<Self, AppError> {
        let fluid_model = runtime_compile::build_fluid_model(&system.fluid)?;
        let composition = runtime.composition.clone();

        let schedules = build_schedule_data(&system.schedules);

        let (control_volumes, cv_node_ids, cv_index_by_node, initial_state) =
            build_control_volumes(system, runtime, fluid_model.as_ref(), composition.clone())
                .map_err(|e| AppError::TransientCompile { message: e })?;

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
            last_active_components: HashSet::new(),
        })
    }

    pub fn build_timeseries_record(
        &mut self,
        time_s: f64,
        state: &TransientState,
    ) -> Result<tf_results::TimeseriesRecord, AppError> {
        use tf_results::{
            EdgeValueSnapshot, GlobalValueSnapshot, NodeValueSnapshot, TimeseriesRecord,
        };

        let solution = if let Some(cached) = self.solution_cache.get(&time_key(time_s)) {
            cached.clone()
        } else {
            let snapshot = self
                .solve_snapshot(time_s, state)
                .map_err(|e| AppError::Backend {
                    message: format!("Transient snapshot failed: {}", e),
                })?;
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
        let boundaries =
            runtime_compile::parse_boundaries(&boundary_defs, &self.runtime.node_id_map).map_err(
                |e| SimError::Backend {
                    message: format!("{}", e),
                },
            )?;

        for (node_id, bc) in &boundaries {
            match bc {
                crate::runtime_compile::BoundaryCondition::PT { p, t } => {
                    problem.set_pressure_bc(*node_id, *p)?;
                    problem.set_temperature_bc(*node_id, *t)?;
                }
                crate::runtime_compile::BoundaryCondition::PH { p, h } => {
                    problem.set_pressure_bc(*node_id, *p)?;
                    problem.set_enthalpy_bc(*node_id, *h)?;
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

        // If the network is effectively disconnected (e.g., closed valve),
        // constrain isolated subgraphs to avoid underdetermined Newton solves.
        let ambient_p = pa(101325.0);
        let ambient_state = self
            .fluid_model
            .state(
                StateInput::PT {
                    p: ambient_p,
                    t: Temperature::new::<kelvin>(300.0),
                },
                self.composition.clone(),
            )
            .map_err(|e| SimError::Backend {
                message: format!("Failed to create ambient state: {}", e),
            })?;
        let ambient_h = self.fluid_model.h(&ambient_state).map_err(|e| {
            SimError::Backend {
                message: format!("Failed to compute ambient enthalpy: {}", e),
            }
        })?;

        let active_components = active_component_ids(
            &self.system,
            &self.runtime.comp_id_map,
            &self.schedules,
            time_s,
        );

        problem
            .convert_all_temperature_bcs()
            .map_err(|e| SimError::Backend {
                message: format!("Failed to convert temperature BCs: {}", e),
            })?;

        let inactive_components = apply_blocked_subgraph_bcs(
            &mut problem,
            &self.system,
            &self.runtime.comp_id_map,
            &self.schedules,
            time_s,
            ambient_p,
            ambient_h,
            &self.last_active_components,
        )?;

        let active_components: HashSet<CompId> = active_components
            .difference(&inactive_components)
            .copied()
            .collect();

        let mut transition_guess: Option<SteadySolution> = None;
        
        // Detect large mode changes to enable adaptive solver tolerance
        let is_mode_transition = self.last_active_components != active_components 
            && active_components.len() > self.last_active_components.len();

        if self.last_active_components != active_components {
            // Active graph mode changed; re-seed mass flows from previous states.
            if let Some(prev) = &self.last_steady_solution {
                let mut adjusted = prev.clone();
                let mut node_states = Vec::new();
                for (i, (&p, &h)) in prev.pressures.iter().zip(prev.enthalpies.iter()).enumerate()
                {
                    let state = self
                        .fluid_model
                        .state(StateInput::PH { p, h }, self.composition.clone())
                        .map_err(|e| SimError::Backend {
                            message: format!("Failed to create state for node {}: {}", i, e),
                        })?;
                    node_states.push(state);
                }

                for (comp_id, mdot) in &mut adjusted.mass_flows {
                    if !active_components.contains(comp_id) {
                        *mdot = 0.0;
                        continue;
                    }

                    let inlet_node = match self.runtime.graph.comp_inlet_node(*comp_id) {
                        Some(node) => node,
                        None => continue,
                    };
                    let outlet_node = match self.runtime.graph.comp_outlet_node(*comp_id) {
                        Some(node) => node,
                        None => continue,
                    };
                    let inlet_state = &node_states[inlet_node.index() as usize];
                    let outlet_state = &node_states[outlet_node.index() as usize];

                    if let Some(component) = problem.components.get(comp_id) {
                        let ports = tf_components::PortStates {
                            inlet: inlet_state,
                            outlet: outlet_state,
                        };
                        if let Ok(mdot_est) = component.mdot(self.fluid_model.as_ref(), ports) {
                            *mdot = mdot_est.value;
                        } else {
                            *mdot = 0.0;
                        }
                    }
                }
                transition_guess = Some(adjusted);
            }
            self.solution_cache.clear();
        }

        let warm_start = transition_guess.as_ref().or(self.last_steady_solution.as_ref());

        // Use adaptive solver config for mode transitions: allow more iterations
        let solver_config = if is_mode_transition {
            Some(tf_solver::NewtonConfig {
                max_iterations: 250,    // Increased from default 200
                ..Default::default()    // Keep strict tolerances and default line search
            })
        } else {
            None  // Use default config for normal timesteps
        };

        let solution = tf_solver::solve_with_active(
            &mut problem,
            solver_config,
            warm_start,
            &active_components,
        )
        .map_err(|e| SimError::Backend {
            message: format!("Solver failed at t={}: {}", time_s, e),
        })?;

        self.last_steady_solution = Some(solution.clone());
        self.store_solution_cache(time_s, solution.clone());
        self.last_active_components = active_components.clone();

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

    for b in &system.boundaries {
        boundary_map.insert(b.node_id.clone(), b.clone());
    }

    for (node_id, events) in &schedules.boundary_pressure_events {
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

fn apply_blocked_subgraph_bcs(
    problem: &mut SteadyProblem,
    system: &SystemDef,
    comp_id_map: &HashMap<String, CompId>,
    schedules: &ScheduleData,
    time_s: f64,
    ambient_p: Pressure,
    ambient_h: SpecEnthalpy,
    last_active_components: &HashSet<CompId>,
) -> SimResult<HashSet<CompId>> {
    let node_count = problem.graph.nodes().len();
    let mut adjacency: Vec<Vec<usize>> = vec![Vec::new(); node_count];
    let mut active_edges: Vec<(CompId, usize, usize)> = Vec::new();

    for component in &system.components {
        let comp_id = match comp_id_map.get(&component.id) {
            Some(id) => *id,
            None => continue,
        };

        if !component_is_active(component, schedules, time_s) {
            continue;
        }

        let inlet = match problem.graph.comp_inlet_node(comp_id) {
            Some(node) => node,
            None => continue,
        };
        let outlet = match problem.graph.comp_outlet_node(comp_id) {
            Some(node) => node,
            None => continue,
        };

        let inlet_idx = inlet.index() as usize;
        let outlet_idx = outlet.index() as usize;
        adjacency[inlet_idx].push(outlet_idx);
        adjacency[outlet_idx].push(inlet_idx);
        active_edges.push((comp_id, inlet_idx, outlet_idx));
    }

    let mut inactive_components = HashSet::new();
    let mut visited = vec![false; node_count];
    let mut stack: Vec<usize> = Vec::new();

    for start in 0..node_count {
        if visited[start] {
            continue;
        }

        stack.clear();
        stack.push(start);
        visited[start] = true;

        let mut group: Vec<usize> = Vec::new();
        let mut anchor_nodes: Vec<usize> = Vec::new();

        while let Some(node_idx) = stack.pop() {
            group.push(node_idx);

            if problem.bc_pressure[node_idx].is_some() && problem.bc_enthalpy[node_idx].is_some() {
                anchor_nodes.push(node_idx);
            }

            for &neighbor in &adjacency[node_idx] {
                if !visited[neighbor] {
                    visited[neighbor] = true;
                    stack.push(neighbor);
                }
            }
        }

        //  If a subgraph has fewer than two anchored nodes, treat it as quiescent,
        // UNLESS it contains newly-activated components (inactive -> active transition).
        // Skip this check at t ≈ 0 to allow normal startup behavior.
        let skip_newly_activated_check = last_active_components.is_empty();
        let has_newly_activated = !skip_newly_activated_check && active_edges.iter().any(|(comp_id, inlet_idx, outlet_idx)| {
            group.contains(inlet_idx) && group.contains(outlet_idx) 
                && !last_active_components.contains(comp_id)
        });

        if anchor_nodes.len() < 2 && !has_newly_activated {
            let (anchor_p, anchor_h) = if let Some(&idx) = anchor_nodes.first() {
                (
                    problem.bc_pressure[idx].unwrap_or(ambient_p),
                    problem.bc_enthalpy[idx].unwrap_or(ambient_h),
                )
            } else {
                (ambient_p, ambient_h)
            };

            for &idx in &group {
                if problem.bc_pressure[idx].is_none() {
                    problem.bc_pressure[idx] = Some(anchor_p);
                }
                if problem.bc_enthalpy[idx].is_none() && problem.bc_temperature[idx].is_none() {
                    problem.bc_enthalpy[idx] = Some(anchor_h);
                }
            }

            for (comp_id, inlet_idx, outlet_idx) in &active_edges {
                if group.contains(inlet_idx) && group.contains(outlet_idx) {
                    inactive_components.insert(*comp_id);
                }
            }
        }
    }

    Ok(inactive_components)
}

fn active_component_ids(
    system: &SystemDef,
    comp_id_map: &HashMap<String, CompId>,
    schedules: &ScheduleData,
    time_s: f64,
) -> HashSet<CompId> {
    let mut active = HashSet::new();
    for component in &system.components {
        let comp_id = match comp_id_map.get(&component.id) {
            Some(id) => *id,
            None => continue,
        };
        if component_is_active(component, schedules, time_s) {
            active.insert(comp_id);
        }
    }
    active
}

fn component_is_active(component: &ComponentDef, schedules: &ScheduleData, time_s: f64) -> bool {
    // Activation threshold for graph connectivity only.
    // This is distinct from the microscopic leakage floor used in component physics.
    const HYDRAULIC_ACTIVE_FACTOR: f64 = 1e-3;

    match &component.kind {
        ComponentKind::Valve {
            position,
            law,
            ..
        } => {
            let mut pos = *position;
            if let Some(events) = schedules.valve_events.get(&component.id) {
                if let Some(value) = last_event_value(events, time_s) {
                    pos = value;
                }
            }

            let factor = match law {
                ValveLawDef::Linear => pos,
                ValveLawDef::Quadratic => pos * pos,
                ValveLawDef::QuickOpening => pos,
            };

            factor > HYDRAULIC_ACTIVE_FACTOR
        }
        ComponentKind::Orifice { area_m2, .. } => *area_m2 > 0.0,
        ComponentKind::Pipe { .. } => true,
        ComponentKind::Pump { area_m2, .. } => *area_m2 > 0.0,
        ComponentKind::Turbine { area_m2, .. } => *area_m2 > 0.0,
    }
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

#[cfg(test)]
mod tests {
    use super::*;
    use tf_fluids::{Composition, CoolPropModel, Species};
    use tf_graph::GraphBuilder;
    use tf_project::schema::{ComponentDef, ComponentKind, CompositionDef, FluidDef, NodeDef, NodeKind, SystemDef, ValveLawDef};

    fn empty_schedules() -> ScheduleData {
        ScheduleData {
            valve_events: HashMap::new(),
            boundary_pressure_events: HashMap::new(),
            boundary_temperature_events: HashMap::new(),
        }
    }

    #[test]
    fn valve_activity_threshold() {
        let valve = ComponentDef {
            id: "v1".to_string(),
            name: "Valve".to_string(),
            kind: ComponentKind::Valve {
                cd: 0.8,
                area_max_m2: 1e-4,
                position: 0.0,
                law: ValveLawDef::Linear,
                treat_as_gas: true,
            },
            from_node_id: "n1".to_string(),
            to_node_id: "n2".to_string(),
        };

        let schedules = empty_schedules();
        assert!(!component_is_active(&valve, &schedules, 0.0));

        let mut valve_open = valve.clone();
        if let ComponentKind::Valve { position, .. } = &mut valve_open.kind {
            *position = 0.01;
        }
        assert!(component_is_active(&valve_open, &schedules, 0.0));
    }

    #[test]
    fn blocked_subgraph_anchors_pressure_and_enthalpy() {
        let mut builder = GraphBuilder::new();
        let n1 = builder.add_node("n1");
        let n2 = builder.add_node("n2");
        let comp_id = builder.add_component("v1", n1, n2);
        let graph = builder.build().expect("Failed to build graph");

        let model = CoolPropModel::new();
        let comp = Composition::pure(Species::N2);
        let mut problem = SteadyProblem::new(&graph, &model, comp.clone());

        let system = SystemDef {
            id: "sys".to_string(),
            name: "sys".to_string(),
            fluid: FluidDef {
                composition: CompositionDef::Pure {
                    species: "Nitrogen".to_string(),
                },
            },
            nodes: vec![
                NodeDef {
                    id: "n1".to_string(),
                    name: "n1".to_string(),
                    kind: NodeKind::Junction,
                },
                NodeDef {
                    id: "n2".to_string(),
                    name: "n2".to_string(),
                    kind: NodeKind::Junction,
                },
            ],
            components: vec![ComponentDef {
                id: "v1".to_string(),
                name: "Valve".to_string(),
                kind: ComponentKind::Valve {
                    cd: 0.8,
                    area_max_m2: 1e-4,
                    position: 0.0,
                    law: ValveLawDef::Linear,
                    treat_as_gas: true,
                },
                from_node_id: "n1".to_string(),
                to_node_id: "n2".to_string(),
            }],
            boundaries: Vec::new(),
            schedules: Vec::new(),
        };

        let mut comp_id_map = HashMap::new();
        comp_id_map.insert("v1".to_string(), comp_id);

        let schedules = empty_schedules();
        let ambient_p = pa(101325.0);
        let ambient_state = model
            .state(
                StateInput::PT {
                    p: ambient_p,
                    t: Temperature::new::<kelvin>(300.0),
                },
                comp,
            )
            .expect("Ambient state failure");
        let ambient_h = model.h(&ambient_state).expect("Ambient enthalpy failure");

        let inactive = apply_blocked_subgraph_bcs(
            &mut problem,
            &system,
            &comp_id_map,
            &schedules,
            0.0,
            ambient_p,
            ambient_h,
            &HashSet::new(),  // No previously-active components in test
        )
        .expect("Blocked subgraph anchoring failed");

        assert!(inactive.contains(&comp_id));
        assert!(problem.bc_pressure[n1.index() as usize].is_some());
        assert!(problem.bc_enthalpy[n1.index() as usize].is_some());
        assert!(problem.bc_pressure[n2.index() as usize].is_some());
        assert!(problem.bc_enthalpy[n2.index() as usize].is_some());
    }
}
