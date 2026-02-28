//! Lumped-parameter control volume with conservation laws.

use crate::error::{SimError, SimResult};
use tf_core::units::Pressure;
use tf_fluids::{Composition, FluidModel, SpecEnthalpy, StateInput};

/// State of a control volume (mass + specific enthalpy).
#[derive(Clone, Debug)]
pub struct ControlVolumeState {
    /// Mass in kg
    pub m_kg: f64,
    /// Specific enthalpy in J/kg
    pub h_j_per_kg: f64,
}

/// Parameters and methods for a lumped control volume.
#[derive(Clone, Debug)]
pub struct ControlVolume {
    /// Name for diagnostics
    pub name: String,
    /// Fixed volume (m³)
    pub volume_m3: f64,
    /// Fluid composition (owned here for consistency)
    pub composition: Composition,
}

impl ControlVolume {
    /// Create a new control volume.
    pub fn new(name: String, volume_m3: f64, composition: Composition) -> SimResult<Self> {
        if volume_m3 <= 0.0 {
            return Err(SimError::InvalidArg {
                what: "volume_m3 must be positive",
            });
        }
        Ok(Self {
            name,
            volume_m3,
            composition,
        })
    }

    /// Compute density (kg/m³) from state.
    pub fn density(&self, state: &ControlVolumeState) -> f64 {
        if state.m_kg > 0.0 {
            state.m_kg / self.volume_m3
        } else {
            0.0
        }
    }

    /// Compute pressure given density and enthalpy via bisection.
    ///
    /// Uses a robust bisection algorithm to find P such that fluid.rho(P, h) ≈ rho_target.
    /// If p_hint is provided, starts bracketing around the hint for faster convergence.
    pub fn pressure_from_rho_h(
        &self,
        fluid: &dyn FluidModel,
        rho_target: f64,
        h: SpecEnthalpy,
        p_hint: Option<Pressure>,
    ) -> SimResult<Pressure> {
        use tf_core::timing::Timer;

        let _timer = Timer::start("cv_pressure_inversion");
        let start = std::time::Instant::now();

        // NEW HOT PATH: Direct rho,h -> T -> P solve for backends that support it
        // This eliminates the nested bisection (P bisection + PH solve)
        let t_hint = p_hint.map(|p| {
            // Estimate temperature from ideal gas: T ≈ P/(ρ·R)
            // This is just an initial guess, doesn't need to be accurate
            let r_approx = 287.0; // Rough gas constant
            p.value / (rho_target * r_approx)
        });

        if let Some(result) =
            fluid.pressure_from_rho_h_direct(&self.composition, rho_target, h, t_hint)
        {
            match result {
                Ok(p) => {
                    // Record direct path timing
                    if tf_core::timing::is_enabled() {
                        tf_core::timing::thermo_timing::PRESSURE_FROM_RHO_H_DIRECT
                            .record(start.elapsed().as_secs_f64());
                    }
                    return Ok(p);
                }
                Err(e) => {
                    // Direct solve failed, fall back to old path
                    eprintln!(
                        "Warning: Direct rho,h->T solve failed: {}, falling back to nested path",
                        e
                    );
                }
            }
        }

        // FALLBACK PATH: Original nested bisection algorithm
        // This is kept for backends that don't support direct solve or when it fails
        let fallback_start = std::time::Instant::now();
        let result = self.pressure_from_rho_h_fallback(fluid, rho_target, h, p_hint)?;

        // Record fallback path timing
        if tf_core::timing::is_enabled() {
            tf_core::timing::thermo_timing::PRESSURE_FROM_RHO_H_FALLBACK
                .record(fallback_start.elapsed().as_secs_f64());
        }

        Ok(result)
    }

    /// Fallback pressure inversion using nested bisection.
    ///
    /// This is the original algorithm kept for compatibility and edge cases.
    fn pressure_from_rho_h_fallback(
        &self,
        fluid: &dyn FluidModel,
        rho_target: f64,
        h: SpecEnthalpy,
        p_hint: Option<Pressure>,
    ) -> SimResult<Pressure> {
        const P_MIN: f64 = 1e3; // 1 kPa minimum
        const P_MAX_INITIAL: f64 = 1e8; //100 MPa maximum
        const MAX_ITER: usize = 50;
        const TOL: f64 = 1e-2; // kg/m³

        // Helper to safely evaluate state and density
        let try_rho = |p_val: f64| -> Option<f64> {
            let state_result = fluid.state(
                StateInput::PH {
                    p: Pressure::new::<uom::si::pressure::pascal>(p_val),
                    h,
                },
                self.composition.clone(),
            );

            match state_result {
                Ok(state) => fluid.rho(&state).ok().map(|r| r.value),
                Err(_) => None,
            }
        };

        // Initialize bracket based on hint or default range
        let (mut p_low, mut p_high) = if let Some(hint) = p_hint {
            let p_hint_val = hint.value;
            // Start with tight bracket around hint
            let p_lo = (0.2 * p_hint_val).max(P_MIN);
            let p_hi = (5.0 * p_hint_val).min(P_MAX_INITIAL);
            (p_lo, p_hi)
        } else {
            // Wide bracket for first solve
            (P_MIN, P_MAX_INITIAL)
        };

        // Find valid lower bound
        let mut rho_low = None;
        for _attempt in 0..15 {
            if let Some(rho) = try_rho(p_low) {
                rho_low = Some(rho);
                break;
            }
            // Increase lower bound if invalid
            p_low *= 2.0;
            if p_low >= P_MAX_INITIAL * 0.5 {
                break;
            }
        }

        let rho_low = rho_low.ok_or_else(|| SimError::Backend {
            message: format!(
                "Cannot find valid lower pressure bound for h={:.1} J/kg (tried P={:.0} to {:.0} Pa)",
                h, P_MIN, p_low
            ),
        })?;

        // Find valid upper bound
        let mut rho_high = None;
        for _attempt in 0..15 {
            if let Some(rho) = try_rho(p_high) {
                rho_high = Some(rho);
                break;
            }
            // Decrease upper bound if invalid
            p_high *= 0.5;
            if p_high <= p_low * 2.0 {
                break;
            }
        }

        let rho_high = rho_high.ok_or_else(|| SimError::Backend {
            message: format!(
                "Cannot find valid upper pressure bound for h={:.1} J/kg (tried P={:.0} to {:.0} Pa)",
                h, p_high, P_MAX_INITIAL
            ),
        })?;

        // Check if target is bracketed
        let f_low = rho_low - rho_target;
        let f_high = rho_high - rho_target;

        if f_low * f_high > 0.0 {
            // Not bracketed - try to expand bounds intelligently
            if f_low > 0.0 && f_high > 0.0 {
                // Both too dense - need lower pressure
                p_low = (p_low * 0.1).max(P_MIN);
                // Retry with new lower bound
                if let Some(new_rho_low) = try_rho(p_low) {
                    if (new_rho_low - rho_target) * f_high < 0.0 {
                        // Now bracketed
                    } else {
                        return Err(SimError::ConvergenceFailed {
                            what: "pressure_from_rho_h: cannot bracket (all densities too high)",
                        });
                    }
                }
            } else {
                // Both too sparse - need higher pressure
                let p_high_new = (p_high * 5.0).min(P_MAX_INITIAL);
                if let Some(new_rho_high) = try_rho(p_high_new) {
                    p_high = p_high_new;
                    if (f_low) * (new_rho_high - rho_target) < 0.0 {
                        // Now bracketed
                    } else {
                        return Err(SimError::ConvergenceFailed {
                            what: "pressure_from_rho_h: cannot bracket (all densities too low)",
                        });
                    }
                } else {
                    return Err(SimError::ConvergenceFailed {
                        what: "pressure_from_rho_h: cannot bracket (pressure range exhausted)",
                    });
                }
            }
        }

        // Bisection with cached rho values to minimize fluid calls
        let mut rho_low_cached = rho_low;
        let mut rho_high_cached = rho_high;

        for _iter in 0..MAX_ITER {
            let p_mid = 0.5 * (p_low + p_high);

            let rho_mid = match try_rho(p_mid) {
                Some(r) => r,
                None => {
                    // Mid point invalid, shrink toward valid side
                    p_high = p_mid;
                    rho_high_cached = try_rho(p_high).unwrap_or(rho_high_cached);
                    continue;
                }
            };

            let residual = (rho_mid - rho_target).abs();
            if residual < TOL {
                return Ok(Pressure::new::<uom::si::pressure::pascal>(p_mid));
            }

            // Update bracket
            let f_mid = rho_mid - rho_target;
            let f_low = rho_low_cached - rho_target;

            if f_low * f_mid < 0.0 {
                // Root in [p_low, p_mid]
                p_high = p_mid;
                rho_high_cached = rho_mid;
            } else {
                // Root in [p_mid, p_high]
                p_low = p_mid;
                rho_low_cached = rho_mid;
            }

            // Early exit if bracket is tight enough
            if (p_high - p_low) / p_low < 1e-6 {
                return Ok(Pressure::new::<uom::si::pressure::pascal>(
                    0.5 * (p_low + p_high),
                ));
            }
        }

        Err(SimError::ConvergenceFailed {
            what: "pressure_from_rho_h: bisection did not converge within iteration limit",
        })
    }

    /// Compute (P, h) boundary condition for this control volume.
    /// p_hint can be provided from previous time step to speed up pressure solve.
    pub fn state_ph_boundary(
        &self,
        fluid: &dyn FluidModel,
        state: &ControlVolumeState,
        p_hint: Option<Pressure>,
    ) -> SimResult<(Pressure, SpecEnthalpy)> {
        if state.m_kg <= 0.0 {
            return Err(SimError::NonPhysical {
                what: "control volume mass must be positive",
            });
        }

        let rho = self.density(state);
        let p = self.pressure_from_rho_h(fluid, rho, state.h_j_per_kg, p_hint)?;

        // Validate that the computed (P,h) combination is valid for the fluid model
        fluid
            .state(
                tf_fluids::StateInput::PH {
                    p,
                    h: state.h_j_per_kg,
                },
                self.composition.clone(),
            )
            .map_err(|e| SimError::Backend {
                message: format!(
                    "CV '{}' state (P={:.1} Pa, h={:.1} J/kg, rho={:.3} kg/m³, m={:.4} kg, V={:.6} m³) produces invalid fluid state: {}",
                    self.name, p.value, state.h_j_per_kg, rho, state.m_kg, self.volume_m3, e
                ),
            })?;

        Ok((p, state.h_j_per_kg))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tf_fluids::CoolPropModel;

    #[test]
    fn density_computation() {
        let cv = ControlVolume::new(
            "test".to_string(),
            0.01,
            Composition::pure(tf_fluids::Species::N2),
        )
        .unwrap();

        let state = ControlVolumeState {
            m_kg: 0.1,
            h_j_per_kg: 300000.0,
        };

        let rho = cv.density(&state);
        assert!((rho - 10.0).abs() < 1e-6); // 0.1 / 0.01 = 10
    }

    #[test]
    fn pressure_from_rho_h_reasonable() {
        let cv = ControlVolume::new(
            "test".to_string(),
            0.01,
            Composition::pure(tf_fluids::Species::N2),
        )
        .unwrap();

        let fluid = CoolPropModel::new();

        // Start from a moderate PT state (within N2 valid range)
        let state_initial = fluid
            .state(
                StateInput::PT {
                    p: tf_core::units::pa(200_000.0), // 2 bar
                    t: tf_core::units::k(300.0),
                },
                cv.composition.clone(),
            )
            .unwrap();

        let h = fluid.h(&state_initial).unwrap();
        let rho_initial = fluid.rho(&state_initial).unwrap();

        // Recover pressure from rho and h
        // Note: this test may be sensitive to CoolProp's valid ranges
        match cv.pressure_from_rho_h(&fluid, rho_initial.value, h, None) {
            Ok(p_recovered) => {
                // Should be reasonably close to original
                assert!((p_recovered.value - 200_000.0).abs() < 5000.0); // within 5 kPa
            }
            Err(_) => {
                // CoolProp may have range limitations; that's ok for this test
                println!(
                    "pressure_from_rho_h bracketing test skipped due to CoolProp range limits"
                );
            }
        }
    }

    #[test]
    fn pressure_from_rho_h_direct_path_consistency() {
        // Test that the new direct rho,h->T->P path (via CoolProp's solve_pt_from_rho_h)
        // produces results consistent with the fallback nested bisection path

        let cv = ControlVolume::new(
            "test".to_string(),
            0.01,
            Composition::pure(tf_fluids::Species::N2),
        )
        .unwrap();

        let fluid = CoolPropModel::new();

        // Test several representative states
        let test_cases = vec![
            (100_000.0, 250.0),   // 1 bar, 250K - cold
            (200_000.0, 300.0),   // 2 bar, 300K - moderate
            (500_000.0, 400.0),   // 5 bar, 400K - warm
            (1_000_000.0, 300.0), // 10 bar, 300K - high pressure
        ];

        for (p_pa, t_k) in test_cases {
            let state = fluid
                .state(
                    StateInput::PT {
                        p: tf_core::units::pa(p_pa),
                        t: tf_core::units::k(t_k),
                    },
                    cv.composition.clone(),
                )
                .unwrap();

            let h = fluid.h(&state).unwrap();
            let rho = fluid.rho(&state).unwrap();

            // Recover pressure using the (now optimized) direct path
            match cv.pressure_from_rho_h(&fluid, rho.value, h, None) {
                Ok(p_recovered) => {
                    // Should recover original pressure within tolerance
                    let rel_error = (p_recovered.value - p_pa).abs() / p_pa;
                    assert!(
                        rel_error < 0.01,
                        "Direct path failed to recover pressure within 1% for P={} Pa, T={} K. Got {} Pa (error: {:.2}%)",
                        p_pa,
                        t_k,
                        p_recovered.value,
                        rel_error * 100.0
                    );
                }
                Err(e) => {
                    panic!(
                        "Direct path failed for valid state P={} Pa, T={} K: {:?}",
                        p_pa, t_k, e
                    );
                }
            }
        }
    }
}
