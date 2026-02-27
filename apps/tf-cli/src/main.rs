use clap::{Parser, Subcommand};
use std::io::{self, Write};
use std::path::{Path, PathBuf};
use std::time::Instant;
use tf_app::{
    AppResult, RunMode, RunOptions, RunProgressEvent, RunRequest, RunStage, project_service, query,
    run_service,
};

#[derive(Parser)]
#[command(name = "tf-cli")]
#[command(about = "ThermoFlow CLI - Thermal-fluid network simulation tool", long_about = None)]
struct Cli {
    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand)]
enum Commands {
    /// Validate project file syntax and structure
    Validate {
        /// Path to the project YAML file
        project_path: PathBuf,
    },
    /// List systems in a project
    Systems {
        /// Path to the project YAML file
        project_path: PathBuf,
    },
    /// Run a simulation
    #[command(subcommand)]
    Run(RunCommands),
    /// List cached runs for a project
    Runs {
        /// Path to the project YAML file
        project_path: PathBuf,
        /// System ID to list runs for
        system_id: String,
    },
    /// Show details of a cached run
    ShowRun {
        /// Path to the project YAML file
        project_path: PathBuf,
        /// Run ID to display
        run_id: String,
    },
    /// Export time series data from a run
    ExportSeries {
        /// Path to the project YAML file
        project_path: PathBuf,
        /// Run ID
        run_id: String,
        /// Entity ID (node or component)
        entity_id: String,
        /// Variable name (e.g., pressure, temperature, mass_flow)
        variable: String,
        /// Output CSV file path (optional, defaults to stdout)
        #[arg(short, long)]
        output: Option<PathBuf>,
    },
}

#[derive(Subcommand)]
enum RunCommands {
    /// Run steady-state simulation
    Steady {
        /// Path to the project YAML file
        project_path: PathBuf,
        /// System ID to simulate
        system_id: String,
        /// Skip cache and force re-run
        #[arg(long)]
        no_cache: bool,
    },
    /// Run transient simulation
    Transient {
        /// Path to the project YAML file
        project_path: PathBuf,
        /// System ID to simulate
        system_id: String,
        /// Time step in seconds
        #[arg(long)]
        dt: f64,
        /// End time in seconds
        #[arg(long)]
        t_end: f64,
        /// Skip cache and force re-run
        #[arg(long)]
        no_cache: bool,
    },
}

fn main() -> AppResult<()> {
    // Initialize tracing
    tracing_subscriber::fmt::init();

    let cli = Cli::parse();

    match cli.command {
        Commands::Validate { project_path } => cmd_validate(&project_path),
        Commands::Systems { project_path } => cmd_systems(&project_path),
        Commands::Run(run_cmd) => match run_cmd {
            RunCommands::Steady {
                project_path,
                system_id,
                no_cache,
            } => cmd_run_steady(&project_path, &system_id, !no_cache),
            RunCommands::Transient {
                project_path,
                system_id,
                dt,
                t_end,
                no_cache,
            } => cmd_run_transient(&project_path, &system_id, dt, t_end, !no_cache),
        },
        Commands::Runs {
            project_path,
            system_id,
        } => cmd_runs(&project_path, &system_id),
        Commands::ShowRun {
            project_path,
            run_id,
        } => cmd_show_run(&project_path, &run_id),
        Commands::ExportSeries {
            project_path,
            run_id,
            entity_id,
            variable,
            output,
        } => cmd_export_series(
            &project_path,
            &run_id,
            &entity_id,
            &variable,
            output.as_deref(),
        ),
    }
}

fn cmd_validate(project_path: &Path) -> AppResult<()> {
    println!("Validating project: {}", project_path.display());
    let project = project_service::load_project(project_path)?;
    project_service::validate_project(&project)?;
    println!("✓ Project is valid");
    Ok(())
}

fn cmd_systems(project_path: &Path) -> AppResult<()> {
    let project = project_service::load_project(project_path)?;
    let systems = project_service::list_systems(&project);

    if systems.is_empty() {
        println!("No systems found in project");
    } else {
        println!("Systems in project:");
        for sys in systems {
            println!(
                "  {} - {} ({} nodes, {} components)",
                sys.id, sys.name, sys.node_count, sys.component_count
            );
        }
    }
    Ok(())
}

fn cmd_run_steady(project_path: &Path, system_id: &str, use_cache: bool) -> AppResult<()> {
    println!("Running steady-state simulation for system: {}", system_id);

    let request = RunRequest {
        project_path,
        system_id,
        mode: RunMode::Steady,
        options: RunOptions {
            use_cache,
            solver_version: "0.1.0".to_string(),
            initialization_strategy: None,
        },
    };

    let mut last_emit = Instant::now();
    let mut last_stage = String::new();
    let response = run_service::ensure_run_with_progress(
        &request,
        Some(&mut |event| {
            let stage_key = format!("{:?}", event.stage);
            let emit_now = stage_key != last_stage || last_emit.elapsed().as_millis() >= 100;
            if emit_now {
                render_cli_progress(&event);
                last_stage = stage_key;
                last_emit = Instant::now();
            }
        }),
    )?;
    clear_progress_line();

    if response.loaded_from_cache {
        println!("✓ Loaded from cache: {}", response.run_id);
    } else {
        println!("✓ Simulation completed: {}", response.run_id);
    }

    print_timing_summary(&request.mode, &response.timing);

    // Load results and show brief summary
    let (_manifest, records) = run_service::load_run(project_path, &response.run_id)?;
    let summary = query::get_run_summary(&records)?;
    println!("  Time points: {}", summary.record_count);
    println!("  Nodes: {}", summary.node_count);
    println!("  Components: {}", summary.component_count);

    Ok(())
}

fn cmd_run_transient(
    project_path: &Path,
    system_id: &str,
    dt: f64,
    t_end: f64,
    use_cache: bool,
) -> AppResult<()> {
    println!("Running transient simulation for system: {}", system_id);
    println!("  dt = {:.3} s, t_end = {:.3} s", dt, t_end);

    let request = RunRequest {
        project_path,
        system_id,
        mode: RunMode::Transient {
            dt_s: dt,
            t_end_s: t_end,
        },
        options: RunOptions {
            use_cache,
            solver_version: "0.1.0".to_string(),
            initialization_strategy: None,
        },
    };

    let mut last_emit = Instant::now();
    let mut last_fraction = -1.0f64;
    let response = run_service::ensure_run_with_progress(
        &request,
        Some(&mut |event| {
            let fraction = event
                .transient
                .as_ref()
                .map(|t| t.fraction_complete)
                .unwrap_or(-1.0);
            let emit_now = (fraction >= 0.0 && (fraction - last_fraction).abs() >= 0.005)
                || last_emit.elapsed().as_millis() >= 100;
            if emit_now {
                render_cli_progress(&event);
                if fraction >= 0.0 {
                    last_fraction = fraction;
                }
                last_emit = Instant::now();
            }
        }),
    )?;
    clear_progress_line();

    if response.loaded_from_cache {
        println!("✓ Loaded from cache: {}", response.run_id);
    } else {
        println!("✓ Simulation completed: {}", response.run_id);
    }

    print_timing_summary(&request.mode, &response.timing);

    // Load results and show brief summary
    let (_manifest, records) = run_service::load_run(project_path, &response.run_id)?;
    let summary = query::get_run_summary(&records)?;
    println!("  Time points: {}", summary.record_count);
    println!("  Nodes: {}", summary.node_count);
    println!("  Components: {}", summary.component_count);

    Ok(())
}

fn clear_progress_line() {
    print!("\r{}\r", " ".repeat(180));
    let _ = io::stdout().flush();
}

fn render_cli_progress(event: &RunProgressEvent) {
    match event.stage {
        RunStage::RunningTransient => {
            if let Some(t) = &event.transient {
                let width = 28usize;
                let filled = ((t.fraction_complete * width as f64).round() as usize).min(width);
                let bar = format!(
                    "{}{}",
                    "#".repeat(filled),
                    "-".repeat(width.saturating_sub(filled))
                );
                print!(
                    "\r[{}] {:>6.2}%  phase={}  t={:.3}/{:.3}s  step={}  cutbacks={}  elapsed={:.1}s",
                    bar,
                    t.fraction_complete * 100.0,
                    event.stage.label(),
                    t.sim_time_s,
                    t.t_end_s,
                    t.step,
                    t.cutback_retries,
                    event.elapsed_wall_s
                );
                let _ = io::stdout().flush();
            }
        }
        _ => {
            let spinner = ['|', '/', '-', '\\'];
            let spin_idx = ((event.elapsed_wall_s * 10.0) as usize) % spinner.len();
            let mut line = format!(
                "\r{} {}  elapsed={:.2}s",
                spinner[spin_idx],
                event.stage.label(),
                event.elapsed_wall_s
            );
            if let Some(strategy) = &event.initialization_strategy {
                line.push_str(&format!("  init={}", strategy));
            }
            if let Some(s) = &event.steady {
                if let Some(iter) = s.iteration {
                    line.push_str(&format!("  iter={}", iter));
                }
                if let Some(residual) = s.residual_norm {
                    line.push_str(&format!("  residual={:.3e}", residual));
                }
            }
            if let Some(msg) = &event.message {
                line.push_str(&format!("  {}", msg));
            }
            print!("{}", line);
            let _ = io::stdout().flush();
        }
    }
}

fn print_timing_summary(mode: &RunMode, timing: &tf_app::RunTimingSummary) {
    if let Some(strategy_name) = &timing.initialization_strategy {
        println!("\nInitialization: {}", strategy_name);
    }

    let total = timing.total_time_s.max(1.0e-12);
    let compile_pct = 100.0 * timing.compile_time_s / total;
    let build_pct = 100.0 * timing.build_time_s / total;
    let solve_pct = 100.0 * timing.solve_time_s / total;
    let save_pct = 100.0 * timing.save_time_s / total;

    println!("\nTiming summary:");
    println!(
        "  Compile: {:.3}s ({:.1}%)",
        timing.compile_time_s, compile_pct
    );
    if timing.build_time_s > 0.0 {
        println!("  Build:   {:.3}s ({:.1}%)", timing.build_time_s, build_pct);
    }
    println!("  Solve:   {:.3}s ({:.1}%)", timing.solve_time_s, solve_pct);
    println!("  Save:    {:.3}s ({:.1}%)", timing.save_time_s, save_pct);
    if timing.load_cache_time_s > 0.0 {
        println!("  Cache load: {:.3}s", timing.load_cache_time_s);
    }
    println!("  Total:   {:.3}s", timing.total_time_s);

    match mode {
        RunMode::Steady => {
            println!("  Steady iterations: {}", timing.steady_iterations);
            if timing.steady_residual_norm > 0.0 {
                println!("  Final residual: {:.3e}", timing.steady_residual_norm);
            }
        }
        RunMode::Transient { .. } => {
            println!("  Transient steps: {}", timing.transient_steps);
            println!("  Cutback retries: {}", timing.transient_cutback_retries);
            println!("  Fallback uses:   {}", timing.transient_fallback_uses);
            if timing.transient_real_fluid_attempts > 0 {
                let success_pct = 100.0 * (timing.transient_real_fluid_successes as f64)
                    / (timing.transient_real_fluid_attempts as f64);
                println!(
                    "  Real-fluid:      {}/{} ({:.1}%)",
                    timing.transient_real_fluid_successes,
                    timing.transient_real_fluid_attempts,
                    success_pct
                );
            }
            println!(
                "  Surrogate updates: {}",
                timing.transient_surrogate_populations
            );
        }
    }
}

fn cmd_runs(project_path: &Path, system_id: &str) -> AppResult<()> {
    let runs = run_service::list_runs(project_path, system_id)?;

    if runs.is_empty() {
        println!("No cached runs found for system: {}", system_id);
    } else {
        println!("Cached runs for system '{}':", system_id);
        for manifest in runs {
            println!("  {} ({})", manifest.run_id, manifest.timestamp);
        }
    }
    Ok(())
}

fn cmd_show_run(project_path: &Path, run_id: &str) -> AppResult<()> {
    println!("Loading run: {}", run_id);

    let (_manifest, records) = run_service::load_run(project_path, run_id)?;
    let summary = query::get_run_summary(&records)?;

    println!("\nRun Summary:");
    println!("  Time points: {}", summary.record_count);
    println!(
        "  Time range: {:.3} - {:.3} s",
        summary.time_range.0, summary.time_range.1
    );
    println!("  Nodes: {}", summary.node_count);
    println!("  Components: {}", summary.component_count);

    let node_ids = query::list_node_ids(&records);
    println!("\nNodes:");
    for id in node_ids {
        println!("  {}", id);
    }

    let comp_ids = query::list_component_ids(&records);
    println!("\nComponents:");
    for id in comp_ids {
        println!("  {}", id);
    }

    Ok(())
}

fn cmd_export_series(
    project_path: &Path,
    run_id: &str,
    entity_id: &str,
    variable: &str,
    output: Option<&Path>,
) -> AppResult<()> {
    let (_manifest, records) = run_service::load_run(project_path, run_id)?;

    // Try node variable first
    let series = if let Ok(data) = query::extract_node_series(&records, entity_id, variable) {
        data
    } else {
        // Try component variable
        query::extract_component_series(&records, entity_id, variable)?
    };

    // Build CSV
    let mut csv = String::from("time_s,value\n");
    for (t, val) in &series {
        csv.push_str(&format!("{},{}\n", t, val));
    }

    // Write to file or stdout
    if let Some(path) = output {
        std::fs::write(path, csv)?;
        println!(
            "✓ Exported {} data points to {}",
            series.len(),
            path.display()
        );
    } else {
        print!("{}", csv);
    }

    Ok(())
}
