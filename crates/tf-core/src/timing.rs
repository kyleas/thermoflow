//! Lightweight performance timing utilities.
//!
//! This module provides simple timing infrastructure for measuring
//! where runtime is being spent. Can be enabled/disabled via environment
//! variable or programmatically.

use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::time::Instant;

static ENABLED: AtomicBool = AtomicBool::new(false);

/// Enable performance timing globally.
pub fn enable_timing() {
    ENABLED.store(true, Ordering::Relaxed);
}

/// Disable performance timing globally.
pub fn disable_timing() {
    ENABLED.store(false, Ordering::Relaxed);
}

/// Check if timing is enabled.
pub fn is_enabled() -> bool {
    ENABLED.load(Ordering::Relaxed) || std::env::var("TF_TIMING").is_ok()
}

/// A simple timer that measures elapsed time.
pub struct Timer {
    label: &'static str,
    start: Instant,
    enabled: bool,
}

impl Timer {
    /// Create and start a new timer with the given label.
    pub fn start(label: &'static str) -> Self {
        Self {
            label,
            start: Instant::now(),
            enabled: is_enabled(),
        }
    }

    /// Stop the timer and return elapsed time in seconds.
    /// If timing is disabled, returns None.
    pub fn stop(self) -> Option<f64> {
        if self.enabled {
            Some(self.start.elapsed().as_secs_f64())
        } else {
            None
        }
    }

    /// Stop the timer and print the result if enabled.
    pub fn stop_and_print(self) {
        let label = self.label;
        if let Some(elapsed) = self.stop() {
            println!("[TIMING] {}: {:.3}s", label, elapsed);
        }
    }
}

/// Accumulating timer for tracking total time across multiple calls.
pub struct AccumulatingTimer {
    total_ns: AtomicU64,
    count: AtomicU64,
}

impl Default for AccumulatingTimer {
    fn default() -> Self {
        Self::new()
    }
}

impl AccumulatingTimer {
    /// Create a new accumulating timer.
    pub const fn new() -> Self {
        Self {
            total_ns: AtomicU64::new(0),
            count: AtomicU64::new(0),
        }
    }

    /// Record a timing measurement.
    pub fn record(&self, duration_s: f64) {
        let nanos = (duration_s * 1e9) as u64;
        self.total_ns.fetch_add(nanos, Ordering::Relaxed);
        self.count.fetch_add(1, Ordering::Relaxed);
    }

    /// Get total time spent (in seconds).
    pub fn total_seconds(&self) -> f64 {
        self.total_ns.load(Ordering::Relaxed) as f64 / 1e9
    }

    /// Get number of calls.
    pub fn count(&self) -> u64 {
        self.count.load(Ordering::Relaxed)
    }

    /// Get average time per call (in seconds).
    pub fn average_seconds(&self) -> f64 {
        let count = self.count();
        if count > 0 {
            self.total_seconds() / count as f64
        } else {
            0.0
        }
    }

    /// Reset the timer.
    pub fn reset(&self) {
        self.total_ns.store(0, Ordering::Relaxed);
        self.count.store(0, Ordering::Relaxed);
    }
}

/// Performance statistics collector.
#[derive(Default)]
pub struct PerfStats {
    pub compile_time_s: f64,
    pub steady_solve_time_s: f64,
    pub transient_total_time_s: f64,
    pub transient_step_time_s: f64,
    pub transient_steps: usize,
    pub cv_pressure_solve_time_s: f64,
    pub cv_pressure_solve_count: u64,
    pub save_time_s: f64,
    pub load_time_s: f64,
}

impl PerfStats {
    /// Print a formatted summary of the statistics.
    pub fn print_summary(&self) {
        if !is_enabled() {
            return;
        }

        println!("\n=== Performance Summary ===");

        if self.compile_time_s > 0.0 {
            println!("Compile time:        {:.3}s", self.compile_time_s);
        }

        if self.steady_solve_time_s > 0.0 {
            println!("Steady solve time:   {:.3}s", self.steady_solve_time_s);
        }

        if self.transient_total_time_s > 0.0 {
            println!("Transient total:     {:.3}s", self.transient_total_time_s);
            if self.transient_steps > 0 {
                println!("  Steps:             {}", self.transient_steps);
                println!(
                    "  Avg step time:     {:.4}s",
                    self.transient_step_time_s / self.transient_steps as f64
                );
            }
        }

        if self.cv_pressure_solve_count > 0 {
            println!(
                "CV pressure solves:  {} calls, {:.3}s total, {:.4}s avg",
                self.cv_pressure_solve_count,
                self.cv_pressure_solve_time_s,
                self.cv_pressure_solve_time_s / self.cv_pressure_solve_count as f64
            );
        }

        if self.save_time_s > 0.0 {
            println!("Run save time:       {:.3}s", self.save_time_s);
        }

        if self.load_time_s > 0.0 {
            println!("Run load time:       {:.3}s", self.load_time_s);
        }

        println!("==========================\n");
    }
}
