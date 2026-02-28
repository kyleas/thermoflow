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

/// Thermodynamic property query timers (Phase 11).
pub mod thermo_timing {
    use super::AccumulatingTimer;

    /// Time spent in cp() property queries
    pub static CP_CALLS: AccumulatingTimer = AccumulatingTimer::new();
    /// Time spent in gamma() property queries
    pub static GAMMA_CALLS: AccumulatingTimer = AccumulatingTimer::new();
    /// Time spent in a() (speed of sound) property queries
    pub static A_CALLS: AccumulatingTimer = AccumulatingTimer::new();
    /// Time spent in property_pack() batch queries (if implemented)
    pub static PROPERTY_PACK_CALLS: AccumulatingTimer = AccumulatingTimer::new();
    /// Time spent in state creation (backend state instantiation)
    pub static STATE_CREATION: AccumulatingTimer = AccumulatingTimer::new();
    /// Time spent in pressure_from_rho_h direct path
    pub static PRESSURE_FROM_RHO_H_DIRECT: AccumulatingTimer = AccumulatingTimer::new();
    /// Time spent in pressure_from_rho_h fallback (nested bisection)
    pub static PRESSURE_FROM_RHO_H_FALLBACK: AccumulatingTimer = AccumulatingTimer::new();

    /// Reset all thermo timers.
    pub fn reset_all() {
        CP_CALLS.reset();
        GAMMA_CALLS.reset();
        A_CALLS.reset();
        PROPERTY_PACK_CALLS.reset();
        STATE_CREATION.reset();
        PRESSURE_FROM_RHO_H_DIRECT.reset();
        PRESSURE_FROM_RHO_H_FALLBACK.reset();
    }

    /// Print thermo timing summary.
    pub fn print_summary() {
        use super::is_enabled;
        if !is_enabled() {
            return;
        }

        println!("\n=== Thermodynamic Query Breakdown ===");

        let cp_count = CP_CALLS.count();
        if cp_count > 0 {
            println!(
                "cp() calls:          {} calls, {:.3}s total, {:.4}ms avg",
                cp_count,
                CP_CALLS.total_seconds(),
                CP_CALLS.average_seconds() * 1000.0
            );
        }

        let gamma_count = GAMMA_CALLS.count();
        if gamma_count > 0 {
            println!(
                "gamma() calls:       {} calls, {:.3}s total, {:.4}ms avg",
                gamma_count,
                GAMMA_CALLS.total_seconds(),
                GAMMA_CALLS.average_seconds() * 1000.0
            );
        }

        let a_count = A_CALLS.count();
        if a_count > 0 {
            println!(
                "a() calls:           {} calls, {:.3}s total, {:.4}ms avg",
                a_count,
                A_CALLS.total_seconds(),
                A_CALLS.average_seconds() * 1000.0
            );
        }

        let pack_count = PROPERTY_PACK_CALLS.count();
        if pack_count > 0 {
            println!(
                "property_pack():     {} calls, {:.3}s total, {:.4}ms avg",
                pack_count,
                PROPERTY_PACK_CALLS.total_seconds(),
                PROPERTY_PACK_CALLS.average_seconds() * 1000.0
            );
        }

        let state_count = STATE_CREATION.count();
        if state_count > 0 {
            println!(
                "state creation:      {} calls, {:.3}s total, {:.4}ms avg",
                state_count,
                STATE_CREATION.total_seconds(),
                STATE_CREATION.average_seconds() * 1000.0
            );
        }

        let p_direct_count = PRESSURE_FROM_RHO_H_DIRECT.count();
        if p_direct_count > 0 {
            println!(
                "pressure_from_rho_h (direct): {} calls, {:.3}s total, {:.4}ms avg",
                p_direct_count,
                PRESSURE_FROM_RHO_H_DIRECT.total_seconds(),
                PRESSURE_FROM_RHO_H_DIRECT.average_seconds() * 1000.0
            );
        }

        let p_fallback_count = PRESSURE_FROM_RHO_H_FALLBACK.count();
        if p_fallback_count > 0 {
            println!(
                "pressure_from_rho_h (fallback): {} calls, {:.3}s total, {:.4}ms avg",
                p_fallback_count,
                PRESSURE_FROM_RHO_H_FALLBACK.total_seconds(),
                PRESSURE_FROM_RHO_H_FALLBACK.average_seconds() * 1000.0
            );
        }

        let total_thermo = CP_CALLS.total_seconds()
            + GAMMA_CALLS.total_seconds()
            + A_CALLS.total_seconds()
            + PROPERTY_PACK_CALLS.total_seconds();
        if total_thermo > 0.0 {
            println!(
                "TOTAL thermo queries: {:.3}s ({:.1}% through cp+gamma+a)",
                total_thermo,
                (total_thermo
                    / (CP_CALLS.total_seconds()
                        + GAMMA_CALLS.total_seconds()
                        + A_CALLS.total_seconds()
                        + PROPERTY_PACK_CALLS.total_seconds())
                    * 100.0)
                    .max(0.0)
            );
        }

        println!("======================================\n");
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

        // Print thermo breakdown
        thermo_timing::print_summary();
    }
}
