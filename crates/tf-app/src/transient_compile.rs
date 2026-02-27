//! Transient simulation compilation and runtime setup.
//!
//! This module handles:
//! - Converting a system definition into a transient runtime model
//! - Building control volumes and initial conditions
//! - Building line volume storage elements
//! - Parsing and applying scheduled boundary/component changes
//! - Integration with tf-sim for time-stepping

use std::collections::{HashMap, HashSet};

use tf_components::{LineVolume, Orifice, Pipe, Pump, Turbine, TwoPortComponent, Valve, ValveLaw};
use tf_core::timing::Timer;
use tf_core::units::{kgps, m, pa, Area, DynVisc, Pressure, Temperature};
use tf_core::{CompId, NodeId};
use tf_fluids::{
    Composition, FluidModel, FrozenPropertySurrogate, SpecEnthalpy, StateInput, ThermoState,
};
use tf_project::schema::{
    ActionDef, BoundaryDef, ComponentDef, ComponentKind, NodeDef, NodeKind, ScheduleDef, SystemDef,
    ValveLawDef,
};
use tf_project::CvInitMode;
use tf_sim::{
    junction_thermal::{JunctionThermalConfig, JunctionThermalState},
    ControlVolume, ControlVolumeState, SimError, SimResult, TransientModel,
};
use tf_solver::{SteadyProblem, SteadySolution};
use uom::si::area::square_meter;
use uom::si::dynamic_viscosity::pascal_second;
use uom::si::thermodynamic_temperature::kelvin;

use crate::runtime_compile::{self, SystemRuntime};
use crate::AppError;

#[derive(Clone, Debug)]
pub struct TransientState {
    pub control_volumes: Vec<ControlVolumeState>,
    pub line_volumes: Vec<ControlVolumeState>, // LineVolume storage (same structure as CV)
}

impl TransientState {
    #[allow(dead_code)]
    fn len(&self) -> usize {
        self.control_volumes.len() + self.line_volumes.len()
    }
}

#[derive(Default, Clone)]
struct ScheduleData {
    valve_events: HashMap<String, Vec<(f64, f64)>>,
    boundary_pressure_events: HashMap<String, Vec<(f64, f64)>>,
    boundary_temperature_events: HashMap<String, Vec<(f64, f64)>>,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum TransientLogMode {
    Summary,
    Verbose,
}

impl TransientLogMode {
    fn from_env() -> Self {
        match std::env::var("THERMOFLOW_TRANSIENT_LOG") {
            Ok(value) if value.eq_ignore_ascii_case("verbose") => Self::Verbose,
            _ => Self::Summary,
        }
    }

    fn is_verbose(self) -> bool {
        matches!(self, Self::Verbose)
    }
}

pub struct TransientNetworkModel {
    system: SystemDef,
    runtime: SystemRuntime,
    fluid_model: Box<dyn FluidModel>,
    composition: Composition,
    control_volumes: Vec<ControlVolume>,
    cv_node_ids: Vec<NodeId>,
    cv_index_by_node: HashMap<NodeId, usize>,
    // LineVolume storage elements (component-based storage, not node-based)
    line_volumes: Vec<ControlVolume>, // Uses CV mechanics for storage
    #[allow(dead_code)]
    lv_comp_ids: Vec<CompId>, // Component IDs of LineVolume components
    lv_index_by_comp: HashMap<CompId, usize>, // Map comp_id -> line_volume index
    initial_state: TransientState,
    schedules: ScheduleData,
    has_dynamic_schedules: bool,
    last_steady_solution: Option<SteadySolution>,
    last_cv_pressure: Vec<Option<Pressure>>,
    last_cv_enthalpy: Vec<Option<f64>>, // CoolProp-compatible h when fallback is active
    #[allow(dead_code)]
    last_lv_pressure: Vec<Option<Pressure>>, // LineVolume internal pressures
    #[allow(dead_code)]
    last_lv_enthalpy: Vec<Option<f64>>, // LineVolume internal enthalpies
    solution_cache: HashMap<i64, SteadySolution>,
    last_active_components: HashSet<CompId>,
    last_valve_positions: HashMap<String, f64>, // Track valve positions for continuation
    last_time: f64,                             // Track last solve time for event detection

    // Thermodynamic fallback surrogates: one per control volume for robustness
    // Built from the last valid real-fluid state; used when CoolProp fails mid-solve
    cv_surrogate_models: Vec<Option<FrozenPropertySurrogate>>,
    #[allow(dead_code)]
    lv_surrogate_models: Vec<Option<FrozenPropertySurrogate>>, // Surrogates for LineVolumes

    // Diagnostic counters for observability
    real_fluid_attempts: usize, // How many times we tried real-fluid state creation
    real_fluid_successes: usize, // How many times real-fluid succeeded
    surrogate_populations: usize, // How many times we populated/updated surrogates
    fallback_uses: usize,       // How many times we actually used fallback during solve

    // Junction thermal regularization for transient mode
    junction_thermal_state: JunctionThermalState,
    junction_thermal_config: JunctionThermalConfig,
    junction_node_ids: Vec<NodeId>, // Track which nodes are junctions (not CVs)
    // Solver timing accumulation (Phase 0 instrumentation)
    solver_residual_time_s: f64,
    solver_jacobian_time_s: f64,
    solver_linearch_time_s: f64,
    solver_thermo_time_s: f64,
    solver_residual_eval_count: usize,
    solver_jacobian_eval_count: usize,
    solver_linearch_iter_count: usize,
    log_mode: TransientLogMode,
}

struct Snapshot {
    solution: SteadySolution,
    components: HashMap<CompId, Box<dyn TwoPortComponent>>,
}

impl TransientNetworkModel {
    pub fn new(
        system: &SystemDef,
        runtime: &SystemRuntime,
        _initialization_strategy: tf_solver::InitializationStrategy,
    ) -> Result<Self, AppError> {
        let fluid_model = runtime_compile::build_fluid_model(&system.fluid)?;
        let composition = runtime.composition.clone();
        let log_mode = TransientLogMode::from_env();

        let schedules = build_schedule_data(&system.schedules);
        let has_dynamic_schedules = !schedules.valve_events.is_empty()
            || !schedules.boundary_pressure_events.is_empty()
            || !schedules.boundary_temperature_events.is_empty();

        let (control_volumes, cv_node_ids, cv_index_by_node, cv_initial_states) =
            build_control_volumes(system, runtime, fluid_model.as_ref(), composition.clone())
                .map_err(|e| AppError::TransientCompile { message: e })?;

        // Build LineVolume storage elements
        let (line_volumes, lv_comp_ids, lv_index_by_comp, lv_initial_states) =
            build_line_volume_storage(system, runtime, fluid_model.as_ref(), composition.clone())
                .map_err(|e| AppError::TransientCompile { message: e })?;

        let initial_state = TransientState {
            control_volumes: cv_initial_states,
            line_volumes: lv_initial_states,
        };

        let mut last_cv_pressure = vec![None; control_volumes.len()];
        let mut last_cv_enthalpy = vec![None; control_volumes.len()];

        for (idx, cv) in control_volumes.iter().enumerate() {
            if let Some(cv_state) = initial_state.control_volumes.get(idx) {
                if let Ok((p_seed, h_seed)) =
                    cv.state_ph_boundary(fluid_model.as_ref(), cv_state, None)
                {
                    last_cv_pressure[idx] = Some(p_seed);
                    last_cv_enthalpy[idx] = Some(h_seed);
                }
            }
        }

        // Initialize LineVolume pressure/enthalpy hints
        let mut last_lv_pressure = vec![None; line_volumes.len()];
        let mut last_lv_enthalpy = vec![None; line_volumes.len()];

        for (idx, lv) in line_volumes.iter().enumerate() {
            if let Some(lv_state) = initial_state.line_volumes.get(idx) {
                if let Ok((p_seed, h_seed)) =
                    lv.state_ph_boundary(fluid_model.as_ref(), lv_state, None)
                {
                    last_lv_pressure[idx] = Some(p_seed);
                    last_lv_enthalpy[idx] = Some(h_seed);
                }
            }
        }

        // Initialize valve positions from component definitions
        let mut last_valve_positions = HashMap::new();
        for component in &system.components {
            if let ComponentKind::Valve { position, .. } = &component.kind {
                last_valve_positions.insert(component.id.clone(), *position);
            }
        }

        // Initialize surrogate models (empty until first valid state)
        let cv_surrogate_models = vec![None; control_volumes.len()];
        let lv_surrogate_models = vec![None; line_volumes.len()];

        // Identify junction nodes (explicit Junction kind only; Atmosphere is fixed)
        let junction_node_ids: Vec<NodeId> = system
            .nodes
            .iter()
            .filter_map(|node| match node.kind {
                NodeKind::Junction => runtime.node_id_map.get(&node.id).copied(),
                _ => None,
            })
            .collect();

        if log_mode.is_verbose() {
            eprintln!(
                "[TRANSIENT] Model initialized: {} CV nodes, {} LineVolume components, {} junction nodes",
                cv_node_ids.len(),
                lv_comp_ids.len(),
                junction_node_ids.len()
            );
        }

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
            line_volumes,
            lv_comp_ids,
            lv_index_by_comp,
            initial_state,
            schedules,
            has_dynamic_schedules,
            last_steady_solution: None,
            last_cv_pressure,
            last_cv_enthalpy,
            last_lv_pressure,
            last_lv_enthalpy,
            solution_cache: HashMap::new(),
            last_active_components: HashSet::new(),
            last_valve_positions,
            last_time: 0.0,
            cv_surrogate_models,
            lv_surrogate_models,
            real_fluid_attempts: 0,
            real_fluid_successes: 0,
            surrogate_populations: 0,
            fallback_uses: 0,
            junction_thermal_state: JunctionThermalState::new(),
            junction_thermal_config: JunctionThermalConfig::default(),
            junction_node_ids,
            solver_residual_time_s: 0.0,
            solver_jacobian_time_s: 0.0,
            solver_linearch_time_s: 0.0,
            solver_thermo_time_s: 0.0,
            solver_residual_eval_count: 0,
            solver_jacobian_eval_count: 0,
            solver_linearch_iter_count: 0,
            log_mode,
        })
    }

    /// Number of times fallback surrogate state creation was used.
    pub fn fallback_uses(&self) -> usize {
        self.fallback_uses
    }

    pub fn real_fluid_attempts(&self) -> usize {
        self.real_fluid_attempts
    }

    pub fn real_fluid_successes(&self) -> usize {
        self.real_fluid_successes
    }

    pub fn surrogate_populations(&self) -> usize {
        self.surrogate_populations
    }

    // Phase 0 instrumentation: Accessor methods for accumulated solver timing
    pub fn solver_residual_time_s(&self) -> f64 {
        self.solver_residual_time_s
    }

    pub fn solver_jacobian_time_s(&self) -> f64 {
        self.solver_jacobian_time_s
    }

    pub fn solver_linearch_time_s(&self) -> f64 {
        self.solver_linearch_time_s
    }

    pub fn solver_thermo_time_s(&self) -> f64 {
        self.solver_thermo_time_s
    }

    pub fn solver_residual_eval_count(&self) -> usize {
        self.solver_residual_eval_count
    }

    pub fn solver_jacobian_eval_count(&self) -> usize {
        self.solver_jacobian_eval_count
    }

    pub fn solver_linearch_iter_count(&self) -> usize {
        self.solver_linearch_iter_count
    }

    /// Print transient simulation diagnostics.
    pub fn print_diagnostics(&self) {
        eprintln!("\n========== TRANSIENT SIMULATION DIAGNOSTICS ==========");
        eprintln!(
            "Real-fluid state creation attempts:  {}",
            self.real_fluid_attempts
        );
        eprintln!(
            "Real-fluid state creation successes: {}",
            self.real_fluid_successes
        );
        if self.real_fluid_attempts > 0 {
            let success_rate =
                (self.real_fluid_successes as f64) / (self.real_fluid_attempts as f64) * 100.0;
            eprintln!("Real-fluid success rate:              {:.1}%", success_rate);
        }
        eprintln!(
            "Surrogate population events:          {}",
            self.surrogate_populations
        );
        eprintln!(
            "Fallback activations (surrogate use): {}",
            self.fallback_uses
        );
        if self.fallback_uses > 0 {
            eprintln!(
                "\n⚠️  FALLBACK WAS USED - Real-fluid path failed {} times",
                self.fallback_uses
            );
            eprintln!(
                "    This indicates the solver encountered states outside CoolProp's valid region."
            );
            eprintln!("    Surrogate approximations were used to continue the simulation.");
        } else if self.real_fluid_attempts > 0 {
            eprintln!("\n✓  ALL STATES USED REAL-FLUID THERMODYNAMICS");
            eprintln!("    Surrogates were populated but never needed.");
        }
        eprintln!("======================================================\n");
    }

    /// Create fluid state with fallback: try real-fluid first, use surrogate approximation on failure.
    ///
    /// When CoolProp rejects (P, h), estimate T from h using surrogate and create a PT state.
    /// This allows the transient solve to continue with approximate thermodynamics.
    fn create_state_with_fallback(
        &mut self,
        p: Pressure,
        h: f64,
        node_idx: usize,
    ) -> SimResult<ThermoState> {
        // Try real-fluid state first
        self.real_fluid_attempts += 1;
        match self
            .fluid_model
            .state(StateInput::PH { p, h }, self.composition.clone())
        {
            Ok(state) => {
                self.real_fluid_successes += 1;
                Ok(state)
            }
            Err(_) => {
                // Real-fluid failed: use surrogate to estimate T from h
                self.fallback_uses += 1;
                // Use any available CV surrogate or create emergency estimate
                let t_est = if let Some(Some(ref surrogate)) =
                    self.cv_surrogate_models.iter().find(|s| s.is_some())
                {
                    let t = surrogate.estimate_temperature_from_h(h);
                    tf_core::units::k(t)
                } else {
                    // No surrogate available: use default T=300K
                    tf_core::units::k(300.0)
                };

                // Create approximate PT state
                ThermoState::from_pt(p, t_est, self.composition.clone()).map_err(|e| {
                    SimError::Backend {
                        message: format!(
                            "Failed to create fallback state for node {}: {}",
                            node_idx, e
                        ),
                    }
                })
            }
        }
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

        let mass_flow_by_comp: HashMap<_, _> = solution.mass_flows.iter().copied().collect();
        let mut edge_values = Vec::with_capacity(self.runtime.comp_id_map.len());
        for (comp_id_str, &comp_idx) in &self.runtime.comp_id_map {
            if let Some(mdot) = mass_flow_by_comp.get(&comp_idx) {
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
        let _timer = Timer::start("transient_snapshot_solve");

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

        let boundaries = if self.has_dynamic_schedules {
            let boundary_defs = apply_boundary_schedules(&self.system, &self.schedules, time_s);
            runtime_compile::parse_boundaries_with_atmosphere(
                &self.system,
                &boundary_defs,
                &self.runtime.node_id_map,
            )
            .map_err(|e| SimError::Backend {
                message: format!("Failed to parse boundaries: {}", e),
            })?
        } else {
            runtime_compile::parse_boundaries_with_atmosphere(
                &self.system,
                &self.system.boundaries,
                &self.runtime.node_id_map,
            )
            .map_err(|e| SimError::Backend {
                message: format!("Failed to parse static boundaries: {}", e),
            })?
        };

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

            if self.log_mode.is_verbose() {
                eprintln!(
                    "[DEBUG] CV '{}' at t={:.4}s: trying state_ph_boundary with rho={:.3}, h={:.1}",
                    cv.name,
                    time_s,
                    cv.density(cv_state),
                    cv_state.h_j_per_kg
                );
            }

            // Try real-fluid boundary computation
            match cv.state_ph_boundary(self.fluid_model.as_ref(), cv_state, p_hint) {
                Ok((p, h)) => {
                    // Real-fluid succeeded: update surrogate from this valid state
                    self.last_cv_pressure[idx] = Some(p);
                    self.last_cv_enthalpy[idx] = Some(h); // Store actual h

                    // Build/update surrogate for future fallback
                    if let Ok(valid_state) = self
                        .fluid_model
                        .state(tf_fluids::StateInput::PH { p, h }, cv.composition.clone())
                    {
                        if let Ok(cp_val) = self.fluid_model.cp(&valid_state) {
                            let t = valid_state.temperature();
                            let rho = cv.density(cv_state);
                            let molar_mass = cv.composition.molar_mass();

                            let surrogate = tf_fluids::surrogate::FrozenPropertySurrogate::new(
                                p.value, t.value, h, rho, cp_val, molar_mass,
                            );

                            self.cv_surrogate_models[idx] = Some(surrogate);
                        }
                    }

                    problem.set_pressure_bc(node_id, p)?;
                    problem.set_enthalpy_bc(node_id, h)?;
                }
                Err(e) => {
                    // Real-fluid failed: use surrogate fallback
                    eprintln!(
                        "[FALLBACK] CV '{}' at t={:.3}s: state_ph_boundary failed: {}",
                        cv.name, time_s, e
                    );
                    self.fallback_uses += 1;

                    let rho = cv.density(cv_state);
                    let h_cv = cv_state.h_j_per_kg;

                    // Use existing surrogate or create a default one
                    if let Some(ref surrogate) = self.cv_surrogate_models[idx] {
                        // Use surrogate to estimate P and T from (ρ, h)
                        let p_fallback = surrogate.estimate_pressure_from_rho_h(rho, h_cv);
                        let t_fallback = surrogate.estimate_temperature_from_h(h_cv);

                        let p = tf_core::units::pa(p_fallback);
                        let t = tf_core::units::k(t_fallback);

                        // Try to create a CoolProp-compatible state from (P, T) and get h
                        match self
                            .fluid_model
                            .state(StateInput::PT { p, t }, cv.composition.clone())
                        {
                            Ok(state) => {
                                // Use CoolProp h for this (P, T) state to ensure compatibility
                                if let Ok(h_compatible) = self.fluid_model.h(&state) {
                                    self.last_cv_pressure[idx] = Some(p);
                                    self.last_cv_enthalpy[idx] = Some(h_compatible);
                                    problem.set_pressure_bc(node_id, p)?;
                                    problem.set_enthalpy_bc(node_id, h_compatible)?;
                                } else {
                                    // Fallback failed - use approximations
                                    self.last_cv_pressure[idx] = Some(p);
                                    self.last_cv_enthalpy[idx] = Some(h_cv);
                                    problem.set_pressure_bc(node_id, p)?;
                                    problem.set_enthalpy_bc(node_id, h_cv)?;
                                }
                            }
                            Err(_) => {
                                // PT state also failed - use approximations
                                self.last_cv_pressure[idx] = Some(p);
                                self.last_cv_enthalpy[idx] = Some(h_cv);
                                problem.set_pressure_bc(node_id, p)?;
                                problem.set_enthalpy_bc(node_id, h_cv)?;
                            }
                        }
                    } else {
                        // No surrogate available: use ideal gas approximation with default T=300K
                        let t_guess = 300.0;
                        let molar_mass = cv.composition.molar_mass();
                        let r_specific = 8314.462618 / molar_mass;

                        // P = ρ * R_specific * T
                        let p_fallback = rho * r_specific * t_guess;
                        let p = tf_core::units::pa(p_fallback);
                        let t = tf_core::units::k(t_guess);

                        // Try to get CoolProp-compatible h
                        match self
                            .fluid_model
                            .state(StateInput::PT { p, t }, cv.composition.clone())
                        {
                            Ok(state) => {
                                if let Ok(h_compatible) = self.fluid_model.h(&state) {
                                    self.last_cv_pressure[idx] = Some(p);
                                    self.last_cv_enthalpy[idx] = Some(h_compatible);
                                    problem.set_pressure_bc(node_id, p)?;
                                    problem.set_enthalpy_bc(node_id, h_compatible)?;
                                } else {
                                    self.last_cv_pressure[idx] = Some(p);
                                    self.last_cv_enthalpy[idx] = Some(h_cv);
                                    problem.set_pressure_bc(node_id, p)?;
                                    problem.set_enthalpy_bc(node_id, h_cv)?;
                                }
                            }
                            Err(_) => {
                                self.last_cv_pressure[idx] = Some(p);
                                self.last_cv_enthalpy[idx] = Some(h_cv);
                                problem.set_pressure_bc(node_id, p)?;
                                problem.set_enthalpy_bc(node_id, h_cv)?;
                            }
                        }
                    }
                }
            }
        }

        // Apply junction node boundaries using lagged enthalpies (transient thermal regularization)
        // This avoids exact algebraic enthalpy closure during difficult transitions, but we
        // intentionally do not anchor junction enthalpy on the very first snapshot solve.
        let apply_junction_anchor = self.last_steady_solution.is_some() || time_s > 1.0e-12;
        if apply_junction_anchor {
            for &junction_node_id in &self.junction_node_ids {
                // Check if this junction is explicitly bounded by external boundaries
                let has_external_bc = boundaries.contains_key(&junction_node_id);

                if !has_external_bc {
                    // Prefer CV-adjacent enthalpy if available to avoid stale junction states.
                    let mut cv_h_sum = 0.0;
                    let mut cv_count = 0usize;
                    for comp_info in self.runtime.graph.components() {
                        let comp_id = comp_info.id;
                        let inlet = self.runtime.graph.comp_inlet_node(comp_id);
                        let outlet = self.runtime.graph.comp_outlet_node(comp_id);

                        let other = if inlet == Some(junction_node_id) {
                            outlet
                        } else if outlet == Some(junction_node_id) {
                            inlet
                        } else {
                            None
                        };

                        if let Some(other_node) = other {
                            if let Some(&cv_idx) = self.cv_index_by_node.get(&other_node) {
                                if let Some(cv_state) = state.control_volumes.get(cv_idx) {
                                    cv_h_sum += cv_state.h_j_per_kg;
                                    cv_count += 1;
                                }
                            }
                        }
                    }

                    let h_cv_avg = if cv_count > 0 {
                        Some(cv_h_sum / (cv_count as f64))
                    } else {
                        None
                    };

                    let h_lagged = self
                        .junction_thermal_state
                        .get_lagged_enthalpy(junction_node_id);
                    let h_use = h_cv_avg.or(h_lagged);

                    if let Some(h_use) = h_use {
                        // Junction pressure will still be solved algebraically, but enthalpy is anchored
                        problem.set_enthalpy_bc(junction_node_id, h_use)?;

                        eprintln!(
                            "[JUNCTION] Node {:?} using lagged h={:.1} J/kg for t={:.4}s",
                            junction_node_id, h_use, time_s
                        );
                    }
                }
            }
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
        let ambient_h = self
            .fluid_model
            .h(&ambient_state)
            .map_err(|e| SimError::Backend {
                message: format!("Failed to compute ambient enthalpy: {}", e),
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
                let mut all_states_valid = true;

                for (&p, &h) in prev.pressures.iter().zip(prev.enthalpies.iter()) {
                    match self
                        .fluid_model
                        .state(StateInput::PH { p, h }, self.composition.clone())
                    {
                        Ok(state) => node_states.push(state),
                        Err(_) => {
                            // Invalid P,h combination (often near saturation or phase boundary).
                            // Skip warm start entirely rather than fail.
                            all_states_valid = false;
                            break;
                        }
                    }
                }

                if all_states_valid {
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

                        let is_newly_activated = !self.last_active_components.contains(comp_id);

                        if let Some(component) = problem.components.get(comp_id) {
                            let ports = tf_components::PortStates {
                                inlet: inlet_state,
                                outlet: outlet_state,
                            };
                            match component.mdot(self.fluid_model.as_ref(), ports) {
                                Ok(mdot_est) => {
                                    *mdot = mdot_est.value;
                                }
                                Err(_) => {
                                    // If flow estimation fails, use a small positive seed for newly-active components
                                    // This helps Newton find the correct region for mode transitions
                                    *mdot = if is_newly_activated { 0.001 } else { 0.0 };
                                }
                            }
                        } else if is_newly_activated {
                            // Component not in problem yet? Seed with small positive flow to guide Newton
                            *mdot = 0.001;
                        } else {
                            *mdot = 0.0;
                        }
                    }
                    transition_guess = Some(adjusted);
                } // end if all_states_valid
            }
            self.solution_cache.clear();
        }

        // Check for large valve position changes that need continuation
        let mut valve_changes: HashMap<String, (f64, f64)> = HashMap::new(); // (prev_pos, target_pos)
        for component in &self.system.components {
            if let ComponentKind::Valve { position, .. } = &component.kind {
                let mut target_pos = *position;
                if let Some(events) = self.schedules.valve_events.get(&component.id) {
                    if let Some(value) = last_event_value(events, time_s) {
                        target_pos = value;
                    }
                }

                if let Some(&prev_pos) = self.last_valve_positions.get(&component.id) {
                    let delta = (target_pos - prev_pos).abs();
                    // Detect significant valve opening (threshold 0.05) that needs continuation
                    if delta > 0.05 && prev_pos < 0.05 && target_pos > prev_pos {
                        valve_changes.insert(component.id.clone(), (prev_pos, target_pos));
                    }
                }
            }
        }

        let needs_continuation = !valve_changes.is_empty() && is_mode_transition;

        // For mode transitions, skip warm start if the previous states were invalid
        // This avoids Newton starting with P,h combinations outside the valid fluid region
        let warm_start = if is_mode_transition {
            transition_guess.as_ref()
        } else {
            transition_guess
                .as_ref()
                .or(self.last_steady_solution.as_ref())
        };

        let is_startup_solve = self.last_steady_solution.is_none() && time_s <= 1.0e-12;

        // Use adaptive solver config for mode transitions and first-step startup.
        let solver_config = if is_mode_transition {
            Some(tf_solver::NewtonConfig {
                max_iterations: 250,
                enthalpy_delta_abs: 3.0e5,
                enthalpy_delta_rel: 0.5,
                enthalpy_total_abs: 8.0e5,
                enthalpy_total_rel: 2.0,
                weak_flow_mdot: 0.5,
                weak_flow_enthalpy_scale: 0.25,
                ..Default::default()
            })
        } else if is_startup_solve {
            Some(tf_solver::NewtonConfig {
                max_iterations: 300,
                line_search_beta: 0.4,
                max_line_search_iters: 40,
                enthalpy_delta_abs: 8.0e5,
                enthalpy_delta_rel: 0.8,
                enthalpy_total_abs: 2.5e6,
                enthalpy_total_rel: 4.0,
                weak_flow_mdot: 0.5,
                weak_flow_enthalpy_scale: 0.4,
                ..Default::default()
            })
        } else {
            None
        };

        // Create fallback policy for continuation and solver recovery
        let num_nodes = self.runtime.graph.nodes().len();
        let make_policy = |warm_start: Option<&SteadySolution>| {
            let mut fallback_policy =
                crate::transient_fallback_policy::TransientFallbackPolicy::new(num_nodes);
            let mut populated_count = 0;

            if let Some(ws) = warm_start {
                for node_idx in 0..num_nodes {
                    if let (Some(&p), Some(&h)) =
                        (ws.pressures.get(node_idx), ws.enthalpies.get(node_idx))
                    {
                        match self
                            .fluid_model
                            .state(StateInput::PH { p, h }, self.composition.clone())
                        {
                            Ok(state) => {
                                let t = state.temperature();
                                let rho = match self.fluid_model.rho(&state) {
                                    Ok(rho_qty) => rho_qty.value,
                                    Err(_) => 1.0,
                                };
                                let cp = self.fluid_model.cp(&state).unwrap_or(1000.0);
                                let molar_mass = self.composition.molar_mass();
                                fallback_policy.update_surrogate(
                                    node_idx,
                                    crate::transient_fallback_policy::SurrogateSample {
                                        p,
                                        t: t.value,
                                        h,
                                        rho,
                                        cp,
                                        molar_mass,
                                    },
                                );
                                populated_count += 1;
                            }
                            Err(_) => {
                                // Skip invalid states - surrogate won't be available for this node
                            }
                        }
                    }
                }
            } else {
                for (&cv_node_id, (&p_opt, &h_opt)) in self.cv_node_ids.iter().zip(
                    self.last_cv_pressure
                        .iter()
                        .zip(self.last_cv_enthalpy.iter()),
                ) {
                    let (Some(p), Some(h)) = (p_opt, h_opt) else {
                        continue;
                    };

                    let node_idx = cv_node_id.index() as usize;
                    if let Ok(state) = self
                        .fluid_model
                        .state(StateInput::PH { p, h }, self.composition.clone())
                    {
                        let t = state.temperature();
                        let rho = self.fluid_model.rho(&state).map(|r| r.value).unwrap_or(1.0);
                        let cp = self.fluid_model.cp(&state).unwrap_or(1000.0);
                        let molar_mass = self.composition.molar_mass();
                        fallback_policy.update_surrogate(
                            node_idx,
                            crate::transient_fallback_policy::SurrogateSample {
                                p,
                                t: t.value,
                                h,
                                rho,
                                cp,
                                molar_mass,
                            },
                        );
                        populated_count += 1;
                    }
                }
            }

            (fallback_policy, populated_count)
        };

        // Apply continuation strategy if needed
        let solution = if needs_continuation {
            const BASE_SUBSTEPS: usize = 20; // Increased from 12 to be more aggressive from start
            const MAX_CONTINUATION_RETRIES: usize = 4;

            let mut substeps = BASE_SUBSTEPS;
            let mut last_error: Option<String> = None;
            let mut continuation_solution: Option<SteadySolution> = None;

            for retry in 0..=MAX_CONTINUATION_RETRIES {
                // Progressively relax enthalpy limits as retries increase.
                // The goal is to give Newton maximum freedom on later retries to find _any_ converged solution.
                let (delta_abs, total_abs, weak_flow_scale) = if retry == 0 {
                    // First attempt: tight limits
                    (2.5e5, 6.0e5, 0.2)
                } else if retry == 1 {
                    // Retry 1: moderate relaxation
                    (6.0e5, 1.5e6, 0.3)
                } else if retry == 2 {
                    // Retry 2: significant relaxation
                    (1.5e6, 3.0e6, 0.5)
                } else {
                    // Retries 3-4: maximum relaxation - essentially unconstraint
                    (f64::INFINITY, f64::INFINITY, 1.0)
                };

                let continuation_config = Some(tf_solver::NewtonConfig {
                    max_iterations: 300,
                    line_search_beta: 0.4,
                    max_line_search_iters: 40,
                    enthalpy_delta_abs: delta_abs,
                    enthalpy_delta_rel: 0.5,
                    enthalpy_total_abs: total_abs,
                    enthalpy_total_rel: 1.5,
                    weak_flow_mdot: 0.5,
                    weak_flow_enthalpy_scale: weak_flow_scale,
                    ..Default::default()
                });

                let (mut fallback_policy, populated_count) = make_policy(warm_start);
                if populated_count > 0 {
                    self.surrogate_populations += populated_count;
                    if self.log_mode.is_verbose() {
                        eprintln!(
                            "[SURROGATE] Populated {} node surrogates from warm-start",
                            populated_count
                        );
                    }
                }
                let mut current_solution = warm_start.cloned();
                let mut retry_failed = false;

                for substep in 1..=substeps {
                    let alpha = (substep as f64) / (substeps as f64);

                    // Build valve position overrides for this substep
                    let mut valve_overrides = HashMap::new();
                    for (comp_id, (prev_pos, target_pos)) in &valve_changes {
                        let effective_prev = prev_pos.max(0.001);
                        let interp_pos = effective_prev + alpha * (target_pos - effective_prev);
                        valve_overrides.insert(comp_id.clone(), interp_pos);
                    }

                    // Rebuild problem with intermediate valve positions
                    let mut substep_problem = SteadyProblem::new(
                        &self.runtime.graph,
                        self.fluid_model.as_ref(),
                        self.composition.clone(),
                    );

                    let mut components_substep = build_components_with_valve_overrides(
                        &self.system,
                        &self.runtime.comp_id_map,
                        &self.schedules,
                        time_s,
                        &valve_overrides,
                    )
                    .map_err(|e| SimError::Backend { message: e })?;

                    for (comp_id, component) in components_substep.drain() {
                        substep_problem.add_component(comp_id, component)?;
                    }

                    // Apply same boundary conditions
                    for (node_id, bc) in &boundaries {
                        match bc {
                            crate::runtime_compile::BoundaryCondition::PT { p, t } => {
                                substep_problem.set_pressure_bc(*node_id, *p)?;
                                substep_problem.set_temperature_bc(*node_id, *t)?;
                            }
                            crate::runtime_compile::BoundaryCondition::PH { p, h } => {
                                substep_problem.set_pressure_bc(*node_id, *p)?;
                                substep_problem.set_enthalpy_bc(*node_id, *h)?;
                            }
                        }
                    }

                    // Apply control-volume boundaries (internal junctions)
                    // For continuation retries, keep enthalpy boundaries fixed but relax solver limits above
                    for (idx, &node_id) in self.cv_node_ids.iter().enumerate() {
                        let p = self.last_cv_pressure[idx].ok_or_else(|| SimError::Backend {
                            message: "CV pressure not set".to_string(),
                        })?;
                        let h = self.last_cv_enthalpy[idx]
                            .unwrap_or(state.control_volumes[idx].h_j_per_kg);

                        substep_problem.set_pressure_bc(node_id, p)?;
                        substep_problem.set_enthalpy_bc(node_id, h)?;
                    }

                    // Do NOT apply junction enthalpy BCs during continuation substeps.
                    // The junction thermal regularization is only for the main timestep solve.
                    // During continuation, we need to let junction enthalpy vary freely to find
                    // a feasible path through the topology change (e.g., valve opening).
                    // Over-constraining the system by fixing junction enthalpy causes the Newton
                    // solver to fail with "line search failed to find valid step" errors.

                    substep_problem.convert_all_temperature_bcs().map_err(|e| {
                        SimError::Backend {
                            message: format!("Failed to convert temperature BCs: {}", e),
                        }
                    })?;

                    let inactive_substep = apply_blocked_subgraph_bcs(
                        &mut substep_problem,
                        &self.system,
                        &self.runtime.comp_id_map,
                        &self.schedules,
                        time_s,
                        ambient_p,
                        ambient_h,
                        &self.last_active_components,
                    )?;

                    let active_substep: HashSet<CompId> = active_components
                        .difference(&inactive_substep)
                        .copied()
                        .collect();

                    let substep_solution = match tf_solver::solve_with_active_and_policy(
                        &mut substep_problem,
                        continuation_config,
                        current_solution.as_ref(),
                        &active_substep,
                        &fallback_policy,
                    ) {
                        Ok(solution) => solution,
                        Err(e) => {
                            last_error = Some(format!(
                                "Continuation substep {}/{} failed at t={}: {}",
                                substep, substeps, time_s, e
                            ));
                            retry_failed = true;
                            break;
                        }
                    };

                    // Update surrogates from successful solution so next substep uses better fallbacks
                    let mut updated_count = 0;
                    for node_idx in 0..num_nodes {
                        if let (Some(&p), Some(&h)) = (
                            substep_solution.pressures.get(node_idx),
                            substep_solution.enthalpies.get(node_idx),
                        ) {
                            match self
                                .fluid_model
                                .state(StateInput::PH { p, h }, self.composition.clone())
                            {
                                Ok(state) => {
                                    let t = state.temperature();
                                    let rho = match self.fluid_model.rho(&state) {
                                        Ok(rho_qty) => rho_qty.value,
                                        Err(_) => 1.0,
                                    };
                                    let cp = self.fluid_model.cp(&state).unwrap_or(1000.0);
                                    let molar_mass = self.composition.molar_mass();
                                    fallback_policy.update_surrogate(
                                        node_idx,
                                        crate::transient_fallback_policy::SurrogateSample {
                                            p,
                                            t: t.value,
                                            h,
                                            rho,
                                            cp,
                                            molar_mass,
                                        },
                                    );
                                    updated_count += 1;
                                }
                                Err(_) => {
                                    // Skip invalid states
                                }
                            }
                        }
                    }
                    if updated_count > 0 {
                        self.surrogate_populations += updated_count;
                    }
                    if updated_count > 0 && substep == substeps && self.log_mode.is_verbose() {
                        eprintln!(
                            "[SURROGATE] Updated {} surrogates from final substep",
                            updated_count
                        );
                    }

                    current_solution = Some(substep_solution);
                }

                if !retry_failed {
                    continuation_solution = current_solution;
                    if continuation_solution.is_some() {
                        break;
                    }
                }

                if retry < MAX_CONTINUATION_RETRIES {
                    let next_substeps = ((substeps as f64) * 1.5).ceil() as usize;
                    if self.log_mode.is_verbose() {
                        eprintln!(
                            "[CUTBACK] Continuation retry {}/{}: substeps {} -> {}",
                            retry + 1,
                            MAX_CONTINUATION_RETRIES,
                            substeps,
                            next_substeps
                        );
                    }
                    substeps = next_substeps;
                }
            }

            continuation_solution.ok_or_else(|| SimError::Retryable {
                message: format!(
                    "Continuation failed after {} retries: {}",
                    MAX_CONTINUATION_RETRIES + 1,
                    last_error.unwrap_or_else(|| "unknown continuation error".to_string())
                ),
            })?
        } else {
            let (fallback_policy, populated_count) = make_policy(warm_start);
            if populated_count > 0 {
                self.surrogate_populations += populated_count;
                if self.log_mode.is_verbose() {
                    eprintln!(
                        "[SURROGATE] Seeded {} node surrogates from CV startup states",
                        populated_count
                    );
                }
            }
            tf_solver::solve_with_active_and_policy(
                &mut problem,
                solver_config,
                warm_start,
                &active_components,
                &fallback_policy,
            )
            .map_err(|e| {
                if is_mode_transition {
                    SimError::Retryable {
                        message: format!("Solver failed at t={}: {}", time_s, e),
                    }
                } else {
                    SimError::Backend {
                        message: format!("Solver failed at t={}: {}", time_s, e),
                    }
                }
            })?
        };

        // Update tracked valve positions
        for component in &self.system.components {
            if let ComponentKind::Valve { position, .. } = &component.kind {
                let mut target_pos = *position;
                if let Some(events) = self.schedules.valve_events.get(&component.id) {
                    if let Some(value) = last_event_value(events, time_s) {
                        target_pos = value;
                    }
                }
                self.last_valve_positions
                    .insert(component.id.clone(), target_pos);
            }
        }

        self.last_steady_solution = Some(solution.clone());
        self.store_solution_cache(time_s, solution.clone());
        self.last_active_components = active_components.clone();
        self.last_time = time_s;

        let components_for_snapshot = build_components_with_schedules(
            &self.system,
            &self.runtime.comp_id_map,
            &self.schedules,
            time_s,
        )
        .map_err(|e| SimError::Backend { message: e })?;

        // Update junction thermal state using relaxed mixing (PHASE 2)
        // After hydraulic solve, relax junction enthalpies toward their mixed values
        self.update_junction_thermal_state(&solution, time_s, &components_for_snapshot)?;

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

    /// Update junction thermal state using relaxed mixing.
    ///
    /// After the hydraulic solve (which used lagged junction enthalpies), compute the
    /// target mixed enthalpy for each junction from incoming streams and relax toward it.
    fn update_junction_thermal_state(
        &mut self,
        solution: &SteadySolution,
        time_s: f64,
        _components: &HashMap<CompId, Box<dyn TwoPortComponent>>,
    ) -> SimResult<()> {
        // On first call, initialize junction enthalpies from solved state
        if self.junction_thermal_state.update_count == 0 {
            for &junction_node_id in &self.junction_node_ids {
                let node_idx = junction_node_id.index() as usize;
                if let Some(&h) = solution.enthalpies.get(node_idx) {
                    self.junction_thermal_state.set_initial(junction_node_id, h);
                    eprintln!(
                        "[JUNCTION] Node {:?} initialized with h={:.1} J/kg",
                        junction_node_id, h
                    );
                }
            }
            self.junction_thermal_state.update_count += 1;
            return Ok(());
        }

        // For each junction, compute mixed enthalpy from incoming streams
        for &junction_node_id in &self.junction_node_ids {
            let node_idx = junction_node_id.index() as usize;

            // Find all components connected to this junction
            let mut incoming_enthalpy_flux = 0.0; // mdot * h (W)
            let mut total_incoming_mdot = 0.0; // kg/s

            for comp_info in self.runtime.graph.components() {
                let comp_id = comp_info.id;

                // Check if this component flows into the junction
                if let Some(outlet_node) = self.runtime.graph.comp_outlet_node(comp_id) {
                    if outlet_node == junction_node_id {
                        // This component flows into the junction
                        if let Some((_, mdot)) =
                            solution.mass_flows.iter().find(|(id, _)| *id == comp_id)
                        {
                            if *mdot > 0.0 {
                                // Positive flow into junction
                                if let Some(inlet_node) =
                                    self.runtime.graph.comp_inlet_node(comp_id)
                                {
                                    let inlet_idx = inlet_node.index() as usize;
                                    if let Some(&h_inlet) = solution.enthalpies.get(inlet_idx) {
                                        incoming_enthalpy_flux += mdot * h_inlet;
                                        total_incoming_mdot += mdot;
                                    }
                                }
                            } else if *mdot < 0.0 {
                                // Reverse flow: junction flows back into component outlet
                                // Use junction's own enthalpy
                                if let Some(&h_junction) = solution.enthalpies.get(node_idx) {
                                    incoming_enthalpy_flux += mdot.abs() * h_junction;
                                    total_incoming_mdot += mdot.abs();
                                }
                            }
                        }
                    }
                }

                // Check if this component draws from the junction
                if let Some(inlet_node) = self.runtime.graph.comp_inlet_node(comp_id) {
                    if inlet_node == junction_node_id {
                        // This component draws from the junction
                        if let Some((_, mdot)) =
                            solution.mass_flows.iter().find(|(id, _)| *id == comp_id)
                        {
                            if *mdot < 0.0 {
                                // Reverse flow: component outlet flows back into junction
                                if let Some(outlet_node) =
                                    self.runtime.graph.comp_outlet_node(comp_id)
                                {
                                    let outlet_idx = outlet_node.index() as usize;
                                    if let Some(&h_outlet) = solution.enthalpies.get(outlet_idx) {
                                        incoming_enthalpy_flux += mdot.abs() * h_outlet;
                                        total_incoming_mdot += mdot.abs();
                                    }
                                }
                            }
                            // Positive flow out of junction doesn't contribute to incoming
                        }
                    }
                }
            }

            // Compute mixed enthalpy (or keep current if no flow)
            let h_mixed = if total_incoming_mdot > 1e-9 {
                incoming_enthalpy_flux / total_incoming_mdot
            } else {
                // No incoming flow: keep current enthalpy
                self.junction_thermal_state
                    .get_lagged_enthalpy(junction_node_id)
                    .unwrap_or(300_000.0) // Default if missing
            };

            // Compute time step (use default if not tracking)
            let dt = if self.last_time > 0.0 {
                time_s - self.last_time
            } else {
                0.01 // Default first step
            };

            // Update junction enthalpy using relaxed mixing
            let h_new = self.junction_thermal_state.update_relaxed(
                junction_node_id,
                h_mixed,
                dt,
                &self.junction_thermal_config,
            );

            if total_incoming_mdot > 1e-6 {
                eprintln!(
                    "[JUNCTION] Node {:?} at t={:.4}s: h_new={:.1} J/kg, h_mixed={:.1} J/kg, mdot_in={:.4} kg/s",
                    junction_node_id, time_s, h_new, h_mixed, total_incoming_mdot
                );
            }
        }

        Ok(())
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
    // Accumulate solver timing stats
    self.solver_residual_time_s += solution.timing_stats.residual_eval_time_s;
    self.solver_jacobian_time_s += solution.timing_stats.jacobian_eval_time_s;
    self.solver_linearch_time_s += solution.timing_stats.linearch_time_s;
    self.solver_thermo_time_s += solution.timing_stats.thermo_createstate_time_s;
    self.solver_residual_eval_count += solution.timing_stats.residual_eval_count;
    self.solver_jacobian_eval_count += solution.timing_stats.jacobian_eval_count;
    self.solver_linearch_iter_count += solution.timing_stats.linearch_iter_count;

        let mut node_states = Vec::new();
        for (i, (&p, &h)) in solution
            .pressures
            .iter()
            .zip(solution.enthalpies.iter())
            .enumerate()
        {
            let state = self.create_state_with_fallback(p, h, i)?;
            node_states.push(state);
        }

        // Control Volume storage dynamics
        let mut dm_in = vec![0.0; self.control_volumes.len()];
        let mut dm_out = vec![0.0; self.control_volumes.len()];
        let mut dmh_in = vec![0.0; self.control_volumes.len()];
        let mut dmh_out = vec![0.0; self.control_volumes.len()];

        // LineVolume storage dynamics
        let mut lv_dm_in = vec![0.0; self.line_volumes.len()];
        let mut lv_dm_out = vec![0.0; self.line_volumes.len()];
        let mut lv_dmh_in = vec![0.0; self.line_volumes.len()];
        let mut lv_dmh_out = vec![0.0; self.line_volumes.len()];

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

            // Check if this component is a LineVolume with its own storage
            let is_line_volume = self.lv_index_by_comp.contains_key(comp_id);

            if *mdot >= 0.0 {
                // Flow from inlet to outlet
                if let Some(&cv_idx) = self.cv_index_by_node.get(&inlet_node) {
                    dm_out[cv_idx] += *mdot;
                    dmh_out[cv_idx] += *mdot * x.control_volumes[cv_idx].h_j_per_kg;
                }

                // LineVolume storage: inlet side
                if is_line_volume {
                    if let Some(&lv_idx) = self.lv_index_by_comp.get(comp_id) {
                        lv_dm_in[lv_idx] += *mdot;
                        // Enthalpy entering the LineVolume equals inlet node enthalpy
                        let h_in =
                            self.fluid_model
                                .h(inlet_state)
                                .map_err(|e| SimError::Backend {
                                    message: format!(
                                        "Failed to get inlet enthalpy for LineVolume {:?}: {}",
                                        comp_id, e
                                    ),
                                })?;
                        lv_dmh_in[lv_idx] += *mdot * h_in;
                    }
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

                // LineVolume storage: outlet side
                if is_line_volume {
                    if let Some(&lv_idx) = self.lv_index_by_comp.get(comp_id) {
                        // Mass and enthalpy leaving the LineVolume
                        lv_dm_out[lv_idx] += *mdot;
                        lv_dmh_out[lv_idx] += *mdot * x.line_volumes[lv_idx].h_j_per_kg;
                    }
                }
            } else {
                let mdot_abs = -(*mdot);

                // Flow from outlet to inlet
                if let Some(&cv_idx) = self.cv_index_by_node.get(&outlet_node) {
                    dm_out[cv_idx] += mdot_abs;
                    dmh_out[cv_idx] += mdot_abs * x.control_volumes[cv_idx].h_j_per_kg;
                }

                // LineVolume storage: outlet side (reverse flow)
                if is_line_volume {
                    if let Some(&lv_idx) = self.lv_index_by_comp.get(comp_id) {
                        lv_dm_in[lv_idx] += mdot_abs;
                        // Enthalpy entering the LineVolume from outlet side
                        let h_in =
                            self.fluid_model
                                .h(outlet_state)
                                .map_err(|e| SimError::Backend {
                                    message: format!(
                                        "Failed to get outlet enthalpy for LineVolume {:?}: {}",
                                        comp_id, e
                                    ),
                                })?;
                        lv_dmh_in[lv_idx] += mdot_abs * h_in;
                    }
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

                // LineVolume storage: inlet side (reverse flow)
                if is_line_volume {
                    if let Some(&lv_idx) = self.lv_index_by_comp.get(comp_id) {
                        // Mass and enthalpy leaving the LineVolume from inlet side
                        lv_dm_out[lv_idx] += mdot_abs;
                        lv_dmh_out[lv_idx] += mdot_abs * x.line_volumes[lv_idx].h_j_per_kg;
                    }
                }
            }
        }

        // Compute CV derivatives
        let mut cv_deriv = Vec::new();
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

            cv_deriv.push(ControlVolumeState {
                m_kg: dm,
                h_j_per_kg: h_dot,
            });
        }

        // Compute LineVolume derivatives
        let mut lv_deriv = Vec::new();
        for i in 0..self.line_volumes.len() {
            let m = x.line_volumes[i].m_kg;
            if m <= 0.0 {
                return Err(SimError::NonPhysical {
                    what: "line volume mass must be positive",
                });
            }
            let dm = lv_dm_in[i] - lv_dm_out[i];
            let dmh = lv_dmh_in[i] - lv_dmh_out[i];
            let h_dot = (dmh - x.line_volumes[i].h_j_per_kg * dm) / m;

            lv_deriv.push(ControlVolumeState {
                m_kg: dm,
                h_j_per_kg: h_dot,
            });
        }

        Ok(TransientState {
            control_volumes: cv_deriv,
            line_volumes: lv_deriv,
        })
    }

    fn add(&self, a: &Self::State, b: &Self::State) -> Self::State {
        let mut cv_out = Vec::with_capacity(a.control_volumes.len());
        for i in 0..a.control_volumes.len() {
            cv_out.push(ControlVolumeState {
                m_kg: a.control_volumes[i].m_kg + b.control_volumes[i].m_kg,
                h_j_per_kg: a.control_volumes[i].h_j_per_kg + b.control_volumes[i].h_j_per_kg,
            });
        }

        let mut lv_out = Vec::with_capacity(a.line_volumes.len());
        for i in 0..a.line_volumes.len() {
            lv_out.push(ControlVolumeState {
                m_kg: a.line_volumes[i].m_kg + b.line_volumes[i].m_kg,
                h_j_per_kg: a.line_volumes[i].h_j_per_kg + b.line_volumes[i].h_j_per_kg,
            });
        }

        TransientState {
            control_volumes: cv_out,
            line_volumes: lv_out,
        }
    }

    fn scale(&self, a: &Self::State, scale: f64) -> Self::State {
        let mut cv_out = Vec::with_capacity(a.control_volumes.len());
        for cv in &a.control_volumes {
            cv_out.push(ControlVolumeState {
                m_kg: cv.m_kg * scale,
                h_j_per_kg: cv.h_j_per_kg * scale,
            });
        }

        let mut lv_out = Vec::with_capacity(a.line_volumes.len());
        for lv in &a.line_volumes {
            lv_out.push(ControlVolumeState {
                m_kg: lv.m_kg * scale,
                h_j_per_kg: lv.h_j_per_kg * scale,
            });
        }

        TransientState {
            control_volumes: cv_out,
            line_volumes: lv_out,
        }
    }
}

type BuildControlVolumesResult = Result<
    (
        Vec<ControlVolume>,
        Vec<NodeId>,
        HashMap<NodeId, usize>,
        Vec<ControlVolumeState>,
    ),
    String,
>;

type BuildLineVolumeStorageResult = Result<
    (
        Vec<ControlVolume>,
        Vec<CompId>,
        HashMap<CompId, usize>,
        Vec<ControlVolumeState>,
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
        initial_states,
    ))
}

/// Build storage elements for LineVolume components.
///
/// LineVolume components are two-port components with internal finite storage.
/// This function creates ControlVolume-like storage for each LineVolume component
/// and initializes their state based on connected inlet node conditions.
fn build_line_volume_storage(
    system: &SystemDef,
    runtime: &SystemRuntime,
    fluid: &dyn FluidModel,
    composition: Composition,
) -> BuildLineVolumeStorageResult {
    let mut line_volumes = Vec::new();
    let mut lv_comp_ids = Vec::new();
    let mut lv_index_by_comp = HashMap::new();
    let mut initial_states = Vec::new();

    for component in &system.components {
        if let ComponentKind::LineVolume { volume_m3, .. } = &component.kind {
            // Create ControlVolume-like storage for this LineVolume
            let lv = ControlVolume::new(
                format!("{}_storage", component.name),
                *volume_m3,
                composition.clone(),
            )
            .map_err(|e| format!("LineVolume storage error: {}", e))?;

            // Initialize state from inlet node conditions if possible
            let (init_p, init_t) = match find_inlet_node_conditions(system, component) {
                Some((p, t)) => (p, t),
                None => {
                    // Fallback to atmospheric conditions
                    eprintln!(
                        "[LINEVOLUME] Warning: {} has no valid inlet CV, initializing with atmospheric conditions",
                        component.name
                    );
                    (101325.0, 300.0)
                }
            };

            let state_default = fluid
                .state(
                    StateInput::PT {
                        p: pa(init_p),
                        t: Temperature::new::<kelvin>(init_t),
                    },
                    composition.clone(),
                )
                .map_err(|e| format!("LineVolume initial state creation failed: {}", e))?;

            let rho = fluid
                .rho(&state_default)
                .map_err(|e| format!("LineVolume density computation failed: {}", e))?;

            let h = fluid
                .h(&state_default)
                .map_err(|e| format!("LineVolume enthalpy computation failed: {}", e))?;

            let m_kg = rho.value * volume_m3;
            let h_j_per_kg = h;

            let comp_id = *runtime
                .comp_id_map
                .get(&component.id)
                .ok_or_else(|| format!("Component not found: {}", component.id))?;

            lv_index_by_comp.insert(comp_id, line_volumes.len());
            line_volumes.push(lv);
            lv_comp_ids.push(comp_id);
            initial_states.push(ControlVolumeState { m_kg, h_j_per_kg });
        }
    }

    Ok((line_volumes, lv_comp_ids, lv_index_by_comp, initial_states))
}

/// Find inlet node (P, T) conditions for a LineVolume component.
/// Returns Some((p_pa, t_k)) if inlet is a ControlVolume or Atmosphere with initial conditions.
fn find_inlet_node_conditions(system: &SystemDef, component: &ComponentDef) -> Option<(f64, f64)> {
    let inlet_node = system
        .nodes
        .iter()
        .find(|n| n.id == component.from_node_id)?;

    match &inlet_node.kind {
        NodeKind::ControlVolume { initial, .. } => {
            // Extract P and T from initial conditions
            let mode_str = initial.mode.as_ref()?;
            match mode_str.as_str() {
                "PT" => {
                    let p = initial.p_pa?;
                    let t = initial.t_k?;
                    Some((p, t))
                }
                _ => None,
            }
        }
        NodeKind::Atmosphere {
            pressure_pa,
            temperature_k,
        } => Some((*pressure_pa, *temperature_k)),
        _ => None,
    }
}

fn initial_state_from_def(
    node: &NodeDef,
    volume_m3: f64,
    initial: &tf_project::schema::InitialCvDef,
    fluid: &dyn FluidModel,
    composition: &Composition,
) -> Result<ControlVolumeState, String> {
    // Validate and resolve the initialization mode
    let mode = CvInitMode::from_def(initial, &node.id)?;

    // Compute derived thermodynamic values based on the explicit mode
    let (m_kg, h_j_per_kg) = match mode {
        CvInitMode::PT { p_pa, t_k } => {
            // PT mode: compute density from (P, T), then mass from rho*V, and h from PT
            let state = fluid
                .state(
                    StateInput::PT {
                        p: pa(p_pa),
                        t: Temperature::new::<kelvin>(t_k),
                    },
                    composition.clone(),
                )
                .map_err(|e| format!("Initial state (PT) invalid for '{}': {}", node.id, e))?;

            let rho = fluid.rho(&state).map_err(|e| {
                format!(
                    "Initial density computation failed for '{}': {}",
                    node.id, e
                )
            })?;

            let h = fluid.h(&state).map_err(|e| {
                format!(
                    "Initial enthalpy computation failed for '{}': {}",
                    node.id, e
                )
            })?;

            (rho.value * volume_m3, h)
        }

        CvInitMode::PH { p_pa, h_j_per_kg } => {
            // PH mode: compute density from (P, h), then mass from rho*V
            let state = fluid
                .state(
                    StateInput::PH {
                        p: pa(p_pa),
                        h: h_j_per_kg,
                    },
                    composition.clone(),
                )
                .map_err(|e| format!("Initial state (PH) invalid for '{}': {}", node.id, e))?;

            let rho = fluid.rho(&state).map_err(|e| {
                format!(
                    "Initial density computation failed for '{}': {}",
                    node.id, e
                )
            })?;

            (rho.value * volume_m3, h_j_per_kg)
        }

        CvInitMode::mT {
            m_kg: _specified_mass,
            t_k: _,
        } => {
            // mT mode: compute rho = m/V, then find P via iteration or direct CoolProp lookup
            // For now, return an error noting that this requires more complex thermodynamic inversion
            return Err(format!(
                "Control volume '{}' uses mT mode, which requires iterative pressure inversion. \
                 Please use PT mode instead (specify pressure and temperature directly).",
                node.id
            ));
        }

        CvInitMode::mH {
            m_kg: _specified_mass,
            h_j_per_kg: _,
        } => {
            // mH mode: compute rho = m/V, then find P from (rho, h)
            // This also requires CoolProp's direct (rho, h) inversion, not available via StateInput yet
            return Err(format!(
                "Control volume '{}' uses mH mode, which requires thermodynamic inversion. \
                 Please use PH mode instead (specify pressure and enthalpy directly).",
                node.id
            ));
        }
    };

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

#[allow(clippy::too_many_arguments)]
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
        let has_newly_activated = !skip_newly_activated_check
            && active_edges.iter().any(|(comp_id, inlet_idx, outlet_idx)| {
                group.contains(inlet_idx)
                    && group.contains(outlet_idx)
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
        ComponentKind::Valve { position, law, .. } => {
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
        ComponentKind::LineVolume { .. } => true,
    }
}

/// Build components with optional valve position overrides for continuation
fn build_components_with_valve_overrides(
    system: &SystemDef,
    comp_id_map: &HashMap<String, CompId>,
    schedules: &ScheduleData,
    time_s: f64,
    valve_position_overrides: &HashMap<String, f64>,
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

                // Check for override first, then schedule, then default
                let mut pos = *position;
                if let Some(&override_pos) = valve_position_overrides.get(&component.id) {
                    pos = override_pos;
                } else if let Some(events) = schedules.valve_events.get(&component.id) {
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
            ComponentKind::LineVolume {
                volume_m3,
                cd,
                area_m2,
            } => {
                use tf_core::units::Volume;
                use uom::si::volume::cubic_meter;

                let vol = Volume::new::<cubic_meter>(*volume_m3);
                if *cd > 0.0 {
                    Box::new(LineVolume::new_with_resistance(
                        component.name.clone(),
                        vol,
                        *cd,
                        area_from_m2(*area_m2),
                    ))
                } else {
                    Box::new(LineVolume::new_lossless(component.name.clone(), vol))
                }
            }
        };

        components.insert(comp_id, boxed);
    }

    Ok(components)
}

fn build_components_with_schedules(
    system: &SystemDef,
    comp_id_map: &HashMap<String, CompId>,
    schedules: &ScheduleData,
    time_s: f64,
) -> Result<HashMap<CompId, Box<dyn TwoPortComponent>>, String> {
    // Use the override-capable version with empty overrides
    build_components_with_valve_overrides(system, comp_id_map, schedules, time_s, &HashMap::new())
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
    use tf_project::schema::{
        ComponentDef, ComponentKind, CompositionDef, FluidDef, NodeDef, NodeKind, SystemDef,
        ValveLawDef,
    };

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
                    position: 0.5, // OPEN valve to make it active
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
            &HashSet::new(), // No previously-active components in test
        )
        .expect("Blocked subgraph anchoring failed");

        assert!(inactive.contains(&comp_id));
        assert!(problem.bc_pressure[n1.index() as usize].is_some());
        assert!(problem.bc_enthalpy[n1.index() as usize].is_some());
        assert!(problem.bc_pressure[n2.index() as usize].is_some());
        assert!(problem.bc_enthalpy[n2.index() as usize].is_some());
    }
}
