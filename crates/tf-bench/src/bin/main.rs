//! Standalone benchmark runner for Thermoflow.

use std::fs;
use std::path::PathBuf;
use std::time::SystemTime;
use tf_bench::{default_benchmarks, run_scenario, BenchmarkSuite};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Determine repo root (one level up from crate root).
    let crate_root = env!("CARGO_MANIFEST_DIR");
    let crate_path = PathBuf::from(crate_root);
    let repo_root = crate_path
        .parent()
        .and_then(|p| p.parent())
        .ok_or("Could not determine repo root")?
        .to_path_buf();

    println!("Thermoflow Benchmark Suite");
    println!("===========================\n");
    println!("Repo root: {}", repo_root.display());

    let benchmarks = default_benchmarks();
    println!("Running {} benchmarks, 5 runs each...\n", benchmarks.len());

    let mut results = Vec::new();

    for (idx, scenario) in benchmarks.iter().enumerate() {
        let scenario_name = &scenario.name;
        print!("[{}/{}] {} ... ", idx + 1, benchmarks.len(), scenario_name);
        std::io::Write::flush(&mut std::io::stdout())?;

        match run_scenario(scenario, 5, &repo_root) {
            Ok(result) => {
                let median = result.aggregate.total_time_median_s;
                println!("OK ({:.3}s median)", median);
                results.push(result);
            }
            Err(e) => {
                println!("FAILED");
                eprintln!("  Error: {}", e);
            }
        }
    }

    println!("\n===========================");
    println!("Benchmark Results Summary");
    println!("===========================\n");

    // Print human-readable summary.
    for result in &results {
        let scenario = &result.scenario;
        let agg = &result.aggregate;

        println!("{}", scenario.name);
        println!("  Mode: {:?}", scenario.mode);
        println!(
            "  Total time:  {:.4}s (median), min: {:.4}s, max: {:.4}s",
            agg.total_time_median_s, agg.total_time_min_s, agg.total_time_max_s
        );
        println!(
            "  Solve time:  {:.4}s (median), min: {:.4}s, max: {:.4}s",
            agg.solve_time_median_s, agg.solve_time_min_s, agg.solve_time_max_s
        );

        if let Some(iters) = agg.steady_iterations_median {
            println!("  Iterations:  {} (median)", iters);
        }
        if let Some(steps) = agg.transient_steps_median {
            println!("  Steps:       {} (median)", steps);
        }
        if agg.transient_cutback_retries_total > 0 {
            println!(
                "  Cutbacks:    {} (total)",
                agg.transient_cutback_retries_total
            );
        }
        if agg.transient_fallback_uses_total > 0 {
            println!(
                "  Fallbacks:   {} (total)",
                agg.transient_fallback_uses_total
            );
        }
        if let Some(ratio) = agg.transient_real_fluid_success_ratio {
            println!("  Real-fluid:  {:.1}% success", ratio * 100.0);
        }
        if let Some(init) = &agg.initialization_strategy {
            println!("  Init strategy: {}", init);
        }

        println!();
    }

    // Write JSON baseline.
    let timestamp = SystemTime::now()
        .duration_since(SystemTime::UNIX_EPOCH)?
        .as_secs();
    let baseline_json = serde_json::to_string_pretty(&BenchmarkSuite {
        timestamp: format!("timestamp_{}", timestamp),
        results,
    })?;

    let baseline_path = repo_root.join("benchmarks").join("baseline.json");
    fs::create_dir_all(baseline_path.parent().unwrap())?;
    fs::write(&baseline_path, baseline_json)?;

    println!("Baseline saved to: {}", baseline_path.display());

    Ok(())
}
