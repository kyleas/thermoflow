use std::sync::mpsc::{Receiver, channel};
use std::thread::{self, JoinHandle};
use tf_project::schema::SystemDef;
use tf_results::RunStore;

#[derive(Debug, Clone)]
pub enum RunType {
    Steady,
    Transient { dt_s: f64, t_end_s: f64 },
}

pub struct RunWorker {
    pub progress_rx: Receiver<WorkerMessage>,
    _handle: JoinHandle<()>,
}

#[derive(Debug, Clone)]
pub enum WorkerMessage {
    #[allow(dead_code)]
    Progress {
        step: usize,
        total: usize,
    },
    Complete {
        run_id: String,
    },
    Error {
        message: String,
    },
}

impl RunWorker {
    pub fn start(
        run_type: RunType,
        system: SystemDef,
        system_id: String,
        store: RunStore,
        use_cached: bool,
    ) -> Self {
        let (tx, rx) = channel();

        let handle = thread::spawn(move || {
            if let Err(e) =
                Self::run_simulation(run_type, system, system_id, store, use_cached, &tx)
            {
                let _ = tx.send(WorkerMessage::Error {
                    message: format!("Worker error: {}", e),
                });
            }
        });

        Self {
            progress_rx: rx,
            _handle: handle,
        }
    }

    fn run_simulation(
        run_type: RunType,
        system: SystemDef,
        system_id: String,
        store: RunStore,
        use_cached: bool,
        tx: &std::sync::mpsc::Sender<WorkerMessage>,
    ) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        use tf_results::RunType as ResultsRunType;

        // Convert run type for ID computation
        let result_run_type = match &run_type {
            RunType::Steady => ResultsRunType::Steady,
            RunType::Transient { dt_s, t_end_s } => ResultsRunType::Transient {
                dt_s: *dt_s,
                t_end_s: *t_end_s,
                steps: ((*t_end_s / *dt_s).ceil() as usize),
            },
        };

        // Compute run ID based on system and run type
        let run_id = tf_results::compute_run_id(&system, &result_run_type, "tf-ui-v1");

        // Check cache
        if use_cached && store.has_run(&run_id) {
            tx.send(WorkerMessage::Complete { run_id })?;
            return Ok(());
        }

        // Compile system
        let runtime = crate::project_io::compile_system(&system)
            .map_err(|e| format!("Compilation failed: {}", e))?;

        // Execute simulation based on run type
        match run_type {
            RunType::Steady => {
                Self::run_steady(&system, system_id, &runtime, store, tx, run_id)?;
            }
            RunType::Transient { dt_s, t_end_s } => {
                Self::run_transient(
                    &system, system_id, &runtime, store, tx, run_id, dt_s, t_end_s,
                )?;
            }
        }

        Ok(())
    }

    #[allow(clippy::too_many_arguments)]
    fn run_steady(
        system: &SystemDef,
        system_id: String,
        runtime: &crate::project_io::SystemRuntime,
        store: RunStore,
        tx: &std::sync::mpsc::Sender<WorkerMessage>,
        run_id: String,
    ) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        use tf_results::{
            EdgeValueSnapshot, GlobalValueSnapshot, NodeValueSnapshot, RunManifest,
            RunType as ResultsRunType, TimeseriesRecord,
        };
        use tf_solver::SteadyProblem;

        let fluid_model = crate::project_io::build_fluid_model(&system.fluid)
            .map_err(|e| format!("Fluid model failed: {}", e))?;

        let boundaries =
            crate::project_io::parse_boundaries(&system.boundaries, &runtime.node_id_map)
                .map_err(|e| format!("Boundary parse failed: {}", e))?;

        // Build components
        let components = crate::project_io::build_components(system, &runtime.comp_id_map)
            .map_err(|e| format!("Component build failed: {}", e))?;

        // Build problem
        let mut problem = SteadyProblem::new(
            &runtime.graph,
            fluid_model.as_ref(),
            runtime.composition.clone(),
        );

        // Add components
        for (comp_id, component) in components {
            problem
                .add_component(comp_id, component)
                .map_err(|e| format!("Add component failed: {}", e))?;
        }

        // Set boundary conditions
        for (node_id, bc) in boundaries {
            match bc {
                crate::project_io::BoundaryCondition::PT { p, t } => {
                    problem.set_pressure_bc(node_id, p)?;
                    problem.set_temperature_bc(node_id, t)?;
                }
                crate::project_io::BoundaryCondition::PH { p, h } => {
                    problem.set_pressure_bc(node_id, p)?;
                    problem.set_enthalpy_bc(node_id, h)?;
                }
            }
        }

        // Solve
        let solution = tf_solver::solve(&mut problem, None, None)
            .map_err(|e| format!("Solver failed: {}", e))?;

        // Build manifest
        let manifest = RunManifest {
            run_id: run_id.clone(),
            system_id,
            timestamp: chrono::Utc::now().to_rfc3339(),
            run_type: ResultsRunType::Steady,
            solver_version: "0.1.0".to_string(),
        };

        // Convert solution to timeseries record
        let mut node_values = Vec::new();
        for (node_id_str, &node_idx) in &runtime.node_id_map {
            if let Some(&p_val) = solution.pressures.get(node_idx.index() as usize) {
                let h_val = solution
                    .enthalpies
                    .get(node_idx.index() as usize)
                    .copied()
                    .unwrap_or_default();

                node_values.push(NodeValueSnapshot {
                    node_id: node_id_str.clone(),
                    p_pa: Some(p_val.value),
                    t_k: None, // TODO: compute from P,h
                    h_j_per_kg: Some(h_val),
                    rho_kg_m3: None, // TODO: compute from P,h
                });
            }
        }

        let mut edge_values = Vec::new();
        for (comp_id_str, &comp_idx) in &runtime.comp_id_map {
            if let Some((_, mdot)) = solution.mass_flows.iter().find(|(id, _)| *id == comp_idx) {
                edge_values.push(EdgeValueSnapshot {
                    component_id: comp_id_str.clone(),
                    mdot_kg_s: Some(*mdot),
                    delta_p_pa: None,
                });
            }
        }

        let record = TimeseriesRecord {
            time_s: 0.0,
            node_values,
            edge_values,
            global_values: GlobalValueSnapshot::default(),
        };

        // Save to cache
        store.save_run(&manifest, &[record])?;

        tx.send(WorkerMessage::Complete { run_id })?;
        Ok(())
    }

    #[allow(clippy::too_many_arguments)]
    fn run_transient(
        system: &SystemDef,
        system_id: String,
        runtime: &crate::project_io::SystemRuntime,
        store: RunStore,
        tx: &std::sync::mpsc::Sender<WorkerMessage>,
        run_id: String,
        dt_s: f64,
        t_end_s: f64,
    ) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        use tf_results::{RunManifest, RunType as ResultsRunType};
        use tf_sim::{run_sim, IntegratorType, SimOptions};

        let mut model = crate::transient_model::TransientNetworkModel::new(system, runtime)
            .map_err(|e| format!("Transient model build failed: {}", e))?;

        let max_steps = ((t_end_s / dt_s).ceil() as usize).max(1);
        let record_every = (max_steps / 200).max(1);

        let opts = SimOptions {
            dt: dt_s,
            t_end: t_end_s,
            max_steps: max_steps + 1,
            record_every,
            integrator: IntegratorType::ForwardEuler,
        };

        let sim_record = run_sim(&mut model, &opts)
            .map_err(|e| format!("Transient simulation failed: {}", e))?;

        let mut records = Vec::new();
        for (idx, time_s) in sim_record.t.iter().enumerate() {
            let state = sim_record
                .x
                .get(idx)
                .ok_or_else(|| "Transient record missing state".to_string())?;
            let record = model
                .build_timeseries_record(*time_s, state)
                .map_err(|e| format!("Snapshot failed at t={}: {}", time_s, e))?;
            records.push(record);
        }

        let manifest = RunManifest {
            run_id: run_id.clone(),
            system_id,
            timestamp: chrono::Utc::now().to_rfc3339(),
            run_type: ResultsRunType::Transient {
                dt_s: opts.dt,
                t_end_s,
                steps: records.len(),
            },
            solver_version: "0.1.0".to_string(),
        };

        store.save_run(&manifest, &records)?;

        tx.send(WorkerMessage::Complete { run_id })?;
        Ok(())
    }
}
