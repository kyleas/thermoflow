//! Sampled execution primitives for digital controllers.
//!
//! Controllers operate in sampled/digital mode with a configured update frequency.
//! Between samples, controller outputs are held constant (zero-order hold).
//!
//! This module provides the timing infrastructure for sampled control execution.

use serde::{Deserialize, Serialize};

/// Sample configuration for a controller or control block.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct SampleConfig {
    /// Sample period in seconds.
    pub dt: f64,
}

impl SampleConfig {
    /// Create a new sample configuration.
    ///
    /// # Arguments
    ///
    /// * `dt` - Sample period in seconds (must be positive)
    ///
    /// # Panics
    ///
    /// Panics if `dt` is not positive.
    pub fn new(dt: f64) -> Self {
        assert!(dt > 0.0, "Sample period must be positive");
        Self { dt }
    }

    /// Create a sample configuration from frequency in Hz.
    pub fn from_frequency(freq_hz: f64) -> Self {
        assert!(freq_hz > 0.0, "Frequency must be positive");
        Self { dt: 1.0 / freq_hz }
    }

    /// Get the sample frequency in Hz.
    pub fn frequency(&self) -> f64 {
        1.0 / self.dt
    }
}

/// Sample clock tracks when a controller should execute.
///
/// Controllers are only updated at discrete sample times. Between samples,
/// the output is held constant (zero-order hold).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SampleClock {
    /// Sample configuration.
    pub config: SampleConfig,
    /// Time of next scheduled sample.
    pub next_sample_time: f64,
}

impl SampleClock {
    /// Create a new sample clock.
    ///
    /// # Arguments
    ///
    /// * `config` - Sample configuration
    /// * `initial_time` - Initial simulation time
    pub fn new(config: SampleConfig, initial_time: f64) -> Self {
        Self {
            config,
            next_sample_time: initial_time + config.dt,
        }
    }

    /// Check if a sample should occur at the given time.
    ///
    /// Returns `true` if `current_time >= next_sample_time`.
    pub fn should_sample(&self, current_time: f64) -> bool {
        current_time >= self.next_sample_time
    }

    /// Advance to the next sample time.
    ///
    /// Should be called after a sample has been executed.
    pub fn advance(&mut self) {
        self.next_sample_time += self.config.dt;
    }

    /// Reset the clock to a new time.
    pub fn reset(&mut self, current_time: f64) {
        self.next_sample_time = current_time + self.config.dt;
    }

    /// Get the time until the next sample.
    pub fn time_until_sample(&self, current_time: f64) -> f64 {
        (self.next_sample_time - current_time).max(0.0)
    }
}

/// Zero-order hold state for controller outputs.
///
/// Holds the last controller output value between samples.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ZeroOrderHold {
    /// Held value.
    pub value: f64,
    /// Sample clock.
    pub clock: SampleClock,
}

impl ZeroOrderHold {
    /// Create a new zero-order hold.
    pub fn new(config: SampleConfig, initial_time: f64, initial_value: f64) -> Self {
        Self {
            value: initial_value,
            clock: SampleClock::new(config, initial_time),
        }
    }

    /// Get the current held value.
    pub fn get(&self) -> f64 {
        self.value
    }

    /// Update the held value (if a sample should occur).
    ///
    /// Returns `true` if the value was updated.
    pub fn update(&mut self, current_time: f64, new_value: f64) -> bool {
        if self.clock.should_sample(current_time) {
            self.value = new_value;
            self.clock.advance();
            true
        } else {
            false
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sample_config_creation() {
        let config = SampleConfig::new(0.1);
        assert_eq!(config.dt, 0.1);
        assert!((config.frequency() - 10.0).abs() < 1e-10);
    }

    #[test]
    fn sample_config_from_frequency() {
        let config = SampleConfig::from_frequency(10.0);
        assert!((config.dt - 0.1).abs() < 1e-10);
    }

    #[test]
    fn sample_clock_basic() {
        let config = SampleConfig::new(0.1);
        let mut clock = SampleClock::new(config, 0.0);

        // Should not sample at t=0
        assert!(!clock.should_sample(0.0));

        // Should sample at t=0.1
        assert!(clock.should_sample(0.1));

        // Advance and check next sample
        clock.advance();
        assert!(!clock.should_sample(0.1));
        assert!(clock.should_sample(0.2));
    }

    #[test]
    fn zero_order_hold_basic() {
        let config = SampleConfig::new(0.1);
        let mut zoh = ZeroOrderHold::new(config, 0.0, 0.5);

        assert_eq!(zoh.get(), 0.5);

        // Before sample time, value should not update
        let updated = zoh.update(0.05, 1.0);
        assert!(!updated);
        assert_eq!(zoh.get(), 0.5);

        // At sample time, value should update
        let updated = zoh.update(0.1, 1.0);
        assert!(updated);
        assert_eq!(zoh.get(), 1.0);
    }

    #[test]
    fn sample_clock_time_until_sample() {
        let config = SampleConfig::new(0.1);
        let clock = SampleClock::new(config, 0.0);

        assert!((clock.time_until_sample(0.0) - 0.1).abs() < 1e-10);
        assert!((clock.time_until_sample(0.05) - 0.05).abs() < 1e-10);
        assert_eq!(clock.time_until_sample(0.15), 0.0);
    }
}
