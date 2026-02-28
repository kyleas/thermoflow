use crate::project_io::SystemRuntime;
use crate::run_worker::{RunWorker, WorkerMessage};
use crate::views::{
    ComponentKindChoice, InspectActions, InspectView, ModuleView, NewComponentSpec, NodeKindChoice,
    PidView, PlotView, RunView,
};
use egui_file_dialog::{DialogMode, FileDialog};
use std::path::PathBuf;
use tf_app::RunProgressEvent;
use tf_project::schema::{
    ComponentDef, ComponentKind, CompositionDef, FluidDef, InitialCvDef, LayoutDef, NodeDef,
    NodeKind, OverlaySettingsDef, Project, RunLibraryDef, SystemDef, ValveLawDef,
};
use tf_project::validate_project;
use tf_results::RunStore;

pub struct ThermoflowApp {
    project: Option<Project>,
    project_path: Option<PathBuf>,
    run_store: Option<RunStore>,
    file_dialog: FileDialog,
    file_dialog_action: Option<FileDialogAction>,
    last_directory: Option<PathBuf>,
    selected_system_id: Option<String>,
    selected_node_id: Option<String>,
    selected_component_id: Option<String>,
    selected_control_block_id: Option<String>,
    selected_run_id: Option<String>,
    active_view: ViewTab,
    pid_view: PidView,
    module_view: ModuleView,
    plot_view: PlotView,
    run_view: RunView,
    inspect_view: InspectView,
    run_worker: Option<RunWorker>,
    use_cached: bool,
    last_worker_message: Option<String>,
    latest_progress: Option<RunProgressEvent>,
    system_runtime: Option<SystemRuntime>,
    transient_dt_s: f64,
    transient_t_end_s: f64,
}

#[derive(Debug, Clone, Copy, PartialEq)]
enum ViewTab {
    Pid,
    Modules,
    Plots,
    Runs,
}

#[derive(Debug, Clone, Copy, PartialEq)]
enum FileDialogAction {
    Open,
    Save,
}

impl ThermoflowApp {
    pub fn new(_cc: &eframe::CreationContext<'_>) -> Self {
        let file_dialog = FileDialog::new();

        Self {
            project: None,
            project_path: None,
            run_store: None,
            file_dialog,
            file_dialog_action: None,
            last_directory: None,
            selected_system_id: None,
            selected_node_id: None,
            selected_component_id: None,
            selected_control_block_id: None,
            selected_run_id: None,
            active_view: ViewTab::Pid,
            pid_view: PidView::default(),
            module_view: ModuleView::default(),
            plot_view: PlotView::default(),
            run_view: RunView::default(),
            inspect_view: InspectView::default(),
            run_worker: None,
            use_cached: false,
            last_worker_message: None,
            latest_progress: None,
            system_runtime: None,
            transient_dt_s: 0.01,
            transient_t_end_s: 1.0,
        }
    }

    fn init_run_store(&mut self) {
        // Create run store in project directory if saved, otherwise use temp directory
        let store = if let Some(ref path) = self.project_path {
            RunStore::for_project(path).ok()
        } else {
            RunStore::new(std::env::temp_dir().join("thermoflow-runs")).ok()
        };
        self.run_store = store;
    }

    fn new_project(&mut self) {
        self.project = Some(Project {
            version: tf_project::LATEST_VERSION,
            name: "New Project".to_string(),
            systems: vec![],
            modules: vec![],
            layouts: vec![],
            runs: RunLibraryDef::default(),
        });
        self.project_path = None;
        self.init_run_store(); // Initialize run store even without a path
        self.selected_system_id = None;
        self.selected_node_id = None;
        self.selected_component_id = None;
        self.selected_control_block_id = None;
        self.system_runtime = None;
        self.pid_view.invalidate_layout();
    }

    fn open_project(&mut self, path: PathBuf) {
        match tf_project::load_yaml(&path) {
            Ok(project) => {
                // Remember the directory for next time
                if let Some(parent) = path.parent() {
                    self.last_directory = Some(parent.to_path_buf());
                }
                self.project = Some(project);
                self.project_path = Some(path);
                self.init_run_store();
                self.selected_system_id = self
                    .project
                    .as_ref()
                    .and_then(|p| p.systems.first())
                    .map(|s| s.id.clone());
                self.selected_node_id = None;
                self.selected_component_id = None;
                self.selected_control_block_id = None;
                self.system_runtime = None;
                self.pid_view.invalidate_layout();
                self.recompile_system();
            }
            Err(e) => {
                self.last_worker_message = Some(format!("Failed to load project: {}", e));
            }
        }
    }

    fn recompile_system(&mut self) {
        if let (Some(project), Some(system_id)) =
            (self.project.as_ref(), self.selected_system_id.as_ref())
        {
            if let Some(system) = project.systems.iter().find(|s| s.id == *system_id) {
                match crate::project_io::compile_system(system) {
                    Ok(runtime) => {
                        self.system_runtime = Some(runtime);
                        self.last_worker_message = None;
                    }
                    Err(e) => {
                        self.system_runtime = None;
                        self.last_worker_message = Some(format!("Compilation failed: {}", e));
                    }
                }
            }
        }
    }

    fn save_project(&mut self) {
        if let (Some(project), Some(path)) = (self.project.as_ref(), self.project_path.as_ref()) {
            if let Err(e) = tf_project::save_yaml(path, project) {
                self.last_worker_message = Some(format!("Failed to save project: {}", e));
            }
        }
    }

    fn save_project_as(&mut self, path: PathBuf) {
        if let Some(project) = self.project.as_ref() {
            if let Err(e) = tf_project::save_yaml(&path, project) {
                self.last_worker_message = Some(format!("Failed to save project: {}", e));
            } else {
                self.project_path = Some(path);
                self.init_run_store();
            }
        }
    }

    fn poll_worker(&mut self) {
        let mut completed = false;
        let mut run_id_result = None;
        let mut error_msg = None;

        if let Some(worker) = &self.run_worker {
            while let Ok(msg) = worker.progress_rx.try_recv() {
                match msg {
                    WorkerMessage::Progress(event) => {
                        self.last_worker_message = event.message.clone();
                        self.latest_progress = Some(event);
                    }
                    WorkerMessage::Complete {
                        run_id,
                        loaded_from_cache,
                        timing,
                    } => {
                        self.latest_progress = None;
                        let init = timing
                            .initialization_strategy
                            .clone()
                            .unwrap_or_else(|| "auto".to_string());
                        self.last_worker_message = Some(format!(
                            "Run {}: {} | init={} | total {:.3}s (compile {:.3}s, build {:.3}s, solve {:.3}s, save {:.3}s, cutbacks {}, fallback {})",
                            if loaded_from_cache {
                                "loaded from cache"
                            } else {
                                "completed"
                            },
                            run_id,
                            init,
                            timing.total_time_s,
                            timing.compile_time_s,
                            timing.build_time_s,
                            timing.solve_time_s,
                            timing.save_time_s,
                            timing.transient_cutback_retries,
                            timing.transient_fallback_uses
                        ));
                        run_id_result = Some(run_id);
                        completed = true;
                        break;
                    }
                    WorkerMessage::Error { message } => {
                        self.latest_progress = None;
                        error_msg = Some(message);
                        completed = true;
                        break;
                    }
                }
            }
        }

        if completed {
            self.run_worker = None;
            if let Some(run_id) = run_id_result {
                self.selected_run_id = Some(run_id);
            }
            if let Some(msg) = error_msg {
                self.last_worker_message = Some(msg);
            }
        }
    }

    fn start_run(&mut self, run_type: crate::run_worker::RunType) {
        if self.run_worker.is_some() {
            return;
        }

        // Get project path and system ID
        let project_path = match &self.project_path {
            Some(path) => path,
            None => {
                self.last_worker_message = Some("No project file loaded".to_string());
                return;
            }
        };

        let system_id = match &self.selected_system_id {
            Some(id) => id,
            None => {
                self.last_worker_message = Some("No system selected".to_string());
                return;
            }
        };

        let worker = RunWorker::start(run_type, project_path, system_id, self.use_cached);
        self.latest_progress = None;
        self.last_worker_message = Some("Run started".to_string());
        self.run_worker = Some(worker);
    }

    fn add_system(&mut self) -> Option<String> {
        let project = self.project.as_mut()?;
        let system_id = Self::next_id("s", project.systems.iter().map(|s| &s.id));
        let name = format!("System {}", project.systems.len() + 1);

        project.systems.push(SystemDef {
            id: system_id.clone(),
            name,
            fluid: FluidDef {
                composition: CompositionDef::Pure {
                    species: "Nitrogen".to_string(),
                },
            },
            nodes: vec![],
            components: vec![],
            boundaries: vec![],
            schedules: vec![],
            controls: None,
        });

        project.layouts.push(LayoutDef {
            system_id: system_id.clone(),
            nodes: vec![],
            edges: vec![],
            control_blocks: vec![],
            signal_connections: vec![],
            overlay: OverlaySettingsDef::default(),
        });

        Some(system_id)
    }

    fn delete_system(&mut self, system_id: &str) -> bool {
        let project = match self.project.as_mut() {
            Some(project) => project,
            None => return false,
        };

        let before = project.systems.len();
        project.systems.retain(|s| s.id != system_id);
        project.layouts.retain(|l| l.system_id != system_id);
        before != project.systems.len()
    }

    fn add_node(&mut self, system_id: &str, kind: NodeKind) -> Option<String> {
        let project = self.project.as_mut()?;
        let system = project.systems.iter_mut().find(|s| s.id == system_id)?;

        let node_id = Self::next_id("n", system.nodes.iter().map(|n| &n.id));
        let name = format!("Node {}", system.nodes.len() + 1);

        system.nodes.push(NodeDef {
            id: node_id.clone(),
            name,
            kind,
        });

        let layout = Self::layout_for_system(project, system_id);
        let index = layout.nodes.len();
        let (x, y) = Self::default_node_position(index);
        layout.nodes.push(tf_project::schema::NodeLayout {
            node_id: node_id.clone(),
            x,
            y,
            label_offset_x: 0.0,
            label_offset_y: 0.0,
            overlay: None,
        });

        Some(node_id)
    }

    fn delete_node(&mut self, system_id: &str, node_id: &str) -> Vec<String> {
        let mut removed_components = Vec::new();
        let project = match self.project.as_mut() {
            Some(project) => project,
            None => return removed_components,
        };

        let system = match project.systems.iter_mut().find(|s| s.id == system_id) {
            Some(system) => system,
            None => return removed_components,
        };

        system.nodes.retain(|n| n.id != node_id);
        system.boundaries.retain(|b| b.node_id != node_id);

        system.components.retain(|c| {
            let remove = c.from_node_id == node_id || c.to_node_id == node_id;
            if remove {
                removed_components.push(c.id.clone());
            }
            !remove
        });

        if let Some(layout) = project
            .layouts
            .iter_mut()
            .find(|l| l.system_id == system_id)
        {
            layout.nodes.retain(|n| n.node_id != node_id);
            layout
                .edges
                .retain(|e| !removed_components.iter().any(|id| id == &e.component_id));
        }

        removed_components
    }

    fn add_component(&mut self, system_id: &str, spec: NewComponentSpec) -> Option<String> {
        let project = self.project.as_mut()?;
        let system = project.systems.iter_mut().find(|s| s.id == system_id)?;

        let component_id = Self::next_id("c", system.components.iter().map(|c| &c.id));
        let name = format!("Component {}", system.components.len() + 1);
        let kind = Self::default_component_kind(spec.kind);

        system.components.push(ComponentDef {
            id: component_id.clone(),
            name,
            kind,
            from_node_id: spec.from_node_id.clone(),
            to_node_id: spec.to_node_id.clone(),
        });

        let layout = Self::layout_for_system(project, system_id);
        let mut pid_layout = crate::pid_editor::PidLayout::from_layout_def(layout);
        if let (Some(from), Some(to)) = (
            pid_layout.nodes.get(&spec.from_node_id).map(|n| n.pos),
            pid_layout.nodes.get(&spec.to_node_id).map(|n| n.pos),
        ) {
            let points =
                crate::pid_editor::normalize_orthogonal(&crate::pid_editor::autoroute(from, to));
            let component_pos = crate::pid_editor::routing::polyline_midpoint(&points);
            pid_layout.edges.insert(
                component_id.clone(),
                crate::pid_editor::PidEdgeRoute {
                    component_id: component_id.clone(),
                    points,
                    label_offset: egui::Vec2::ZERO,
                    component_pos: Some(component_pos),
                },
            );
        }
        pid_layout.apply_to_layout_def(layout);

        Some(component_id)
    }

    fn delete_component(&mut self, system_id: &str, component_id: &str) -> bool {
        let project = match self.project.as_mut() {
            Some(project) => project,
            None => return false,
        };

        let system = match project.systems.iter_mut().find(|s| s.id == system_id) {
            Some(system) => system,
            None => return false,
        };

        let before = system.components.len();
        system.components.retain(|c| c.id != component_id);

        if let Some(layout) = project
            .layouts
            .iter_mut()
            .find(|l| l.system_id == system_id)
        {
            layout.edges.retain(|e| e.component_id != component_id);
        }

        before != system.components.len()
    }

    fn validate_project_state(&mut self) {
        if let Some(project) = self.project.as_ref() {
            if let Err(err) = validate_project(project) {
                self.last_worker_message = Some(format!("Validation warning: {}", err));
            } else {
                self.last_worker_message = None;
            }
        }
    }

    fn apply_inspect_actions(&mut self, actions: InspectActions) -> bool {
        let mut changed = false;
        let system_id = match self.selected_system_id.clone() {
            Some(id) => id,
            None => return false,
        };

        if let Some(kind_choice) = actions.add_node {
            let kind = Self::default_node_kind(kind_choice);
            if let Some(node_id) = self.add_node(&system_id, kind) {
                self.selected_node_id = Some(node_id);
                self.selected_component_id = None;
                self.selected_control_block_id = None;
                self.pid_view.clear_control_selection();
                changed = true;
            }
        }

        if let Some(node_id) = actions.delete_node_id {
            let removed_components = self.delete_node(&system_id, &node_id);
            if self.selected_node_id.as_ref() == Some(&node_id) {
                self.selected_node_id = None;
            }
            if let Some(comp_id) = self.selected_component_id.clone() {
                if removed_components.iter().any(|id| id == &comp_id) {
                    self.selected_component_id = None;
                }
            }
            if self.selected_node_id.is_none() {
                if let Some(project) = self.project.as_ref() {
                    if let Some(system) = project.systems.iter().find(|s| s.id == system_id) {
                        self.selected_node_id = system.nodes.first().map(|n| n.id.clone());
                    }
                }
            }
            self.selected_control_block_id = None;
            self.pid_view.clear_control_selection();
            changed = true;
        }

        if let Some(spec) = actions.add_component {
            if let Some(component_id) = self.add_component(&system_id, spec) {
                self.selected_component_id = Some(component_id);
                self.selected_node_id = None;
                self.selected_control_block_id = None;
                self.pid_view.clear_control_selection();
                changed = true;
            }
        }

        if let Some(component_id) = actions.delete_component_id {
            if self.delete_component(&system_id, &component_id) {
                if self.selected_component_id.as_ref() == Some(&component_id) {
                    self.selected_component_id = None;
                }
                if self.selected_component_id.is_none() {
                    if let Some(project) = self.project.as_ref() {
                        if let Some(system) = project.systems.iter().find(|s| s.id == system_id) {
                            self.selected_component_id =
                                system.components.first().map(|c| c.id.clone());
                        }
                    }
                }
                self.selected_control_block_id = None;
                self.pid_view.clear_control_selection();
                changed = true;
            }
        }

        changed
    }

    fn layout_for_system<'a>(project: &'a mut Project, system_id: &str) -> &'a mut LayoutDef {
        if let Some(idx) = project
            .layouts
            .iter()
            .position(|l| l.system_id == system_id)
        {
            return &mut project.layouts[idx];
        }

        project.layouts.push(LayoutDef {
            system_id: system_id.to_string(),
            nodes: vec![],
            edges: vec![],
            control_blocks: vec![],
            signal_connections: vec![],
            overlay: OverlaySettingsDef::default(),
        });
        let last = project.layouts.len() - 1;
        &mut project.layouts[last]
    }

    fn default_node_position(index: usize) -> (f32, f32) {
        let col = (index % 4) as f32;
        let row = (index / 4) as f32;
        (100.0 + col * 140.0, 100.0 + row * 120.0)
    }

    fn default_component_kind(choice: ComponentKindChoice) -> ComponentKind {
        match choice {
            ComponentKindChoice::Orifice => ComponentKind::Orifice {
                cd: 0.8,
                area_m2: 0.0001,
                treat_as_gas: false,
            },
            ComponentKindChoice::Valve => ComponentKind::Valve {
                cd: 0.8,
                area_max_m2: 0.0002,
                position: 1.0,
                law: ValveLawDef::Linear,
                treat_as_gas: false,
            },
            ComponentKindChoice::Pipe => ComponentKind::Pipe {
                length_m: 1.0,
                diameter_m: 0.05,
                roughness_m: 1e-5,
                k_minor: 0.0,
                mu_pa_s: 1e-5,
            },
            ComponentKindChoice::Pump => ComponentKind::Pump {
                cd: 0.8,
                area_m2: 0.0002,
                delta_p_pa: 200000.0,
                eta: 0.7,
                treat_as_liquid: true,
            },
            ComponentKindChoice::Turbine => ComponentKind::Turbine {
                cd: 0.8,
                area_m2: 0.0002,
                eta: 0.7,
                treat_as_gas: true,
            },
        }
    }

    fn default_node_kind(choice: NodeKindChoice) -> NodeKind {
        match choice {
            NodeKindChoice::Junction => NodeKind::Junction,
            NodeKindChoice::ControlVolume => NodeKind::ControlVolume {
                volume_m3: 0.05,
                initial: InitialCvDef::default(),
            },
            NodeKindChoice::Atmosphere => NodeKind::Atmosphere {
                pressure_pa: 101_325.0,
                temperature_k: 300.0,
            },
        }
    }

    fn next_id<'a, I>(prefix: &str, ids: I) -> String
    where
        I: Iterator<Item = &'a String>,
    {
        let mut max = 0u32;
        for id in ids {
            if let Some(num) = id.strip_prefix(prefix) {
                if let Ok(value) = num.parse::<u32>() {
                    if value > max {
                        max = value;
                    }
                }
            }
        }
        format!("{}{}", prefix, max + 1)
    }
}

impl eframe::App for ThermoflowApp {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        self.poll_worker();

        egui::TopBottomPanel::top("toolbar").show(ctx, |ui| {
            ui.horizontal(|ui| {
                if ui.button("New").clicked() {
                    self.new_project();
                }

                if ui.button("Open").clicked() {
                    self.file_dialog_action = Some(FileDialogAction::Open);
                    let initial_dir = self.last_directory.as_ref().and_then(|p| p.to_str());
                    let _ = self
                        .file_dialog
                        .open(DialogMode::SelectFile, true, initial_dir);
                }

                if ui.button("Save").clicked() {
                    if self.project_path.is_some() {
                        self.save_project();
                    } else {
                        self.file_dialog_action = Some(FileDialogAction::Save);
                        self.file_dialog.save_file();
                    }
                }

                ui.separator();

                ui.add_enabled_ui(self.project.is_some() && self.run_worker.is_none(), |ui| {
                    if ui.button("Run Steady").clicked() {
                        self.start_run(crate::run_worker::RunType::Steady);
                    }
                    if ui.button("Run Transient").clicked() {
                        self.start_run(crate::run_worker::RunType::Transient {
                            dt_s: self.transient_dt_s,
                            t_end_s: self.transient_t_end_s,
                        });
                    }
                    ui.separator();
                    ui.label("Transient:");
                    ui.add(
                        egui::DragValue::new(&mut self.transient_dt_s)
                            .speed(0.001)
                            .range(1e-5..=10.0)
                            .prefix("dt "),
                    );
                    ui.add(
                        egui::DragValue::new(&mut self.transient_t_end_s)
                            .speed(0.1)
                            .range(0.0..=10_000.0)
                            .prefix("t_end "),
                    );
                });

                if self.run_worker.is_some() && ui.button("Cancel").clicked() {
                    self.run_worker = None;
                }

                ui.separator();
                ui.checkbox(&mut self.use_cached, "Use cached results");
            });
        });

        self.file_dialog.update(ctx);
        if let Some(path) = self.file_dialog.take_selected() {
            match self.file_dialog_action.take() {
                Some(FileDialogAction::Open) => self.open_project(path.to_path_buf()),
                Some(FileDialogAction::Save) => self.save_project_as(path.to_path_buf()),
                None => {}
            }
        }

        let mut add_system = false;
        let mut delete_system = false;

        egui::SidePanel::left("project_tree")
            .default_width(220.0)
            .show(ctx, |ui| {
                ui.heading("Project");
                let mut recompile_needed = false;
                let mut new_selection = None;

                if let Some(project) = self.project.as_ref() {
                    ui.label(format!("Name: {}", project.name));
                    ui.separator();
                    ui.heading("Systems");
                    ui.horizontal(|ui| {
                        if ui.button("+ System").clicked() {
                            add_system = true;
                        }
                        if ui
                            .add_enabled(
                                self.selected_system_id.is_some(),
                                egui::Button::new("Delete System"),
                            )
                            .clicked()
                        {
                            delete_system = true;
                        }
                    });
                    for system in &project.systems {
                        let is_selected = self.selected_system_id.as_ref() == Some(&system.id);
                        if ui.selectable_label(is_selected, &system.name).clicked() {
                            new_selection = Some(system.id.clone());
                            recompile_needed = true;
                        }
                    }

                    ui.separator();
                    ui.heading("Modules");
                    for module in &project.modules {
                        ui.label(&module.name);
                    }
                } else {
                    ui.label("No project loaded");
                }

                if let Some(id) = new_selection {
                    self.selected_system_id = Some(id);
                    self.selected_node_id = None;
                    self.selected_component_id = None;
                    self.selected_control_block_id = None;
                }
                if recompile_needed {
                    self.recompile_system();
                }
            });

        if add_system {
            if let Some(system_id) = self.add_system() {
                self.selected_system_id = Some(system_id);
                self.selected_node_id = None;
                self.selected_component_id = None;
                self.selected_control_block_id = None;
                self.pid_view.invalidate_layout();
                self.recompile_system();
                self.validate_project_state();
            }
        }

        if delete_system {
            if let Some(system_id) = self.selected_system_id.clone() {
                if self.delete_system(&system_id) {
                    let next_system = self
                        .project
                        .as_ref()
                        .and_then(|p| p.systems.first())
                        .map(|s| s.id.clone());
                    self.selected_system_id = next_system;
                    self.selected_node_id = None;
                    self.selected_component_id = None;
                    self.selected_control_block_id = None;
                    self.pid_view.invalidate_layout();
                    self.recompile_system();
                    self.validate_project_state();
                }
            }
        }

        let inspect_actions = egui::SidePanel::right("inspector")
            .default_width(280.0)
            .show(ctx, |ui| {
                self.inspect_view.show(
                    ui,
                    &mut self.project,
                    &self.selected_system_id,
                    &self.selected_node_id,
                    &self.selected_component_id,
                    &self.selected_control_block_id,
                    &mut self.pid_view,
                )
            })
            .inner;

        let needs_recompile = inspect_actions.needs_recompile;
        let needs_update = self.apply_inspect_actions(inspect_actions);
        if needs_update || needs_recompile {
            if needs_update {
                self.pid_view.invalidate_layout();
                self.validate_project_state();
            } else if needs_recompile {
                self.validate_project_state();
            }
            self.recompile_system();
        }

        egui::CentralPanel::default().show(ctx, |ui| {
            ui.horizontal(|ui| {
                ui.selectable_value(&mut self.active_view, ViewTab::Pid, "P&ID");
                ui.selectable_value(&mut self.active_view, ViewTab::Modules, "Modules");
                ui.selectable_value(&mut self.active_view, ViewTab::Plots, "Plots");
                ui.selectable_value(&mut self.active_view, ViewTab::Runs, "Runs");
            });

            ui.separator();

            match self.active_view {
                ViewTab::Pid => {
                    self.pid_view.show(
                        ui,
                        &mut self.project,
                        &self.selected_system_id,
                        &self.selected_run_id,
                        &self.run_store,
                        self.inspect_view.overlay_settings(),
                    );
                    // Sync selection from PidView to App for inspector panel
                    self.selected_node_id = self.pid_view.selected_node();
                    self.selected_component_id = self.pid_view.selected_component();
                    self.selected_control_block_id = self.pid_view.selected_control_block_id();
                }
                ViewTab::Modules => {
                    self.module_view.show(ui, &mut self.project);
                }
                ViewTab::Plots => {
                    self.plot_view
                        .show(ui, &self.run_store, &self.selected_run_id);
                }
                ViewTab::Runs => {
                    self.run_view.show(
                        ui,
                        &self.run_store,
                        &self.selected_system_id,
                        &mut self.selected_run_id,
                    );
                }
            }

            if self.run_worker.is_some() || self.last_worker_message.is_some() {
                ui.separator();
                ui.group(|ui| {
                    ui.heading("Run Status");

                    if let Some(progress) = &self.latest_progress {
                        ui.label(format!("Stage: {}", progress.stage.label()));
                        ui.label(format!("Elapsed: {:.2}s", progress.elapsed_wall_s));
                        if let Some(strategy) = &progress.initialization_strategy {
                            ui.label(format!("Initialization: {}", strategy));
                        }

                        if let Some(t) = &progress.transient {
                            ui.add(
                                egui::ProgressBar::new(t.fraction_complete as f32)
                                    .show_percentage()
                                    .text(format!(
                                        "t={:.3}/{:.3}s | step {} | cutbacks {}",
                                        t.sim_time_s, t.t_end_s, t.step, t.cutback_retries
                                    )),
                            );
                        }

                        if let Some(s) = &progress.steady {
                            let mut details = Vec::new();
                            if let Some(outer) = s.outer_iteration {
                                details.push(format!("outer {}", outer));
                            }
                            if let Some(iter) = s.iteration {
                                details.push(format!("iter {}", iter));
                            }
                            if let Some(res) = s.residual_norm {
                                details.push(format!("residual {:.3e}", res));
                            }
                            if !details.is_empty() {
                                ui.label(details.join(" | "));
                            }
                            ui.add(
                                egui::ProgressBar::new(0.0)
                                    .animate(true)
                                    .text("Solving steady system"),
                            );
                        }
                    }

                    if let Some(message) = &self.last_worker_message {
                        ui.label(message);
                    }
                });
            }
        });
    }
}
