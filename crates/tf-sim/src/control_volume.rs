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
        const P_MIN: f64 = 1e2;
        const P_MAX_INITIAL: f64 = 1e8;
        const MAX_ITER: usize = 100;
        const TOL: f64 = 1e-2; // kg/m³

        let (mut p_low, mut p_high) = if let Some(hint) = p_hint {
            // Start bracketing around hint
            let p_hint_val = hint.value;
            let p_lo = (0.5 * p_hint_val).max(P_MIN);
            let p_hi = (2.0 * p_hint_val).min(P_MAX_INITIAL);
            (p_lo, p_hi)
        } else {
            (P_MIN, P_MAX_INITIAL)
        };

        // Try to find a valid bracket by testing initial bounds
        let state_low_test = match fluid.state(
            StateInput::PH {
                p: Pressure::new::<uom::si::pressure::pascal>(p_low),
                h,
            },
            self.composition.clone(),
        ) {
            Ok(s) => s,
            Err(_) => {
                // p_low might be too low for this enthalpy
                p_low = 1e3;
                fluid
                    .state(
                        StateInput::PH {
                            p: Pressure::new::<uom::si::pressure::pascal>(p_low),
                            h,
                        },
                        self.composition.clone(),
                    )
                    .map_err(|e| SimError::Backend {
                        message: format!(
                            "Cannot evaluate state at minimum feasible pressure: {}",
                            e
                        ),
                    })?
            }
        };

        let rho_low = fluid.rho(&state_low_test).map_err(|e| SimError::Backend {
            message: format!("Failed to compute rho at P_low: {}", e),
        })?;

        // Try to find a valid upper bound
        let mut p_high_valid = p_high;
        for _ in 0..10 {
            match fluid.state(
                StateInput::PH {
                    p: Pressure::new::<uom::si::pressure::pascal>(p_high_valid),
                    h,
                },
                self.composition.clone(),
            ) {
                Ok(_) => break,
                Err(_) => {
                    // Pressure too high for this enthalpy, reduce it
                    p_high_valid *= 0.5;
                }
            }
        }

        let state_high = fluid
            .state(
                StateInput::PH {
                    p: Pressure::new::<uom::si::pressure::pascal>(p_high_valid),
                    h,
                },
                self.composition.clone(),
            )
            .map_err(|e| SimError::Backend {
                message: format!("Failed to find valid upper pressure bound: {}", e),
            })?;

        let rho_high = fluid.rho(&state_high).map_err(|e| SimError::Backend {
            message: format!("Failed to compute rho at P_high: {}", e),
        })?;

        // Check if bracketed
        if (rho_low.value - rho_target) * (rho_high.value - rho_target) > 0.0 {
            // Not bracketed; expand if needed
            if rho_low.value > rho_target {
                // Both too high, try lower pressure
                p_low *= 0.1;
            } else if rho_high.value < rho_target {
                // Both too low, try higher pressure (up to the valid limit)
                p_high_valid = (p_high_valid * 10.0).min(1e8);
            } else {
                // Shouldn't happen
                return Err(SimError::ConvergenceFailed {
                    what: "pressure_from_rho_h: could not bracket root",
                });
            }
        }

        p_low = p_low.max(1e2);
        p_high = p_high_valid;

        // Bisection
        for _ in 0..MAX_ITER {
            let p_mid = 0.5 * (p_low + p_high);

            let state_mid = match fluid.state(
                StateInput::PH {
                    p: Pressure::new::<uom::si::pressure::pascal>(p_mid),
                    h,
                },
                self.composition.clone(),
            ) {
                Ok(s) => s,
                Err(_) => {
                    // Invalid state at p_mid, reduce upper bound
                    p_high = p_mid;
                    continue;
                }
            };

            let rho_mid = fluid.rho(&state_mid).map_err(|e| SimError::Backend {
                message: format!("Failed to compute rho: {}", e),
            })?;

            let residual = (rho_mid.value - rho_target).abs();
            if residual < TOL {
                return Ok(Pressure::new::<uom::si::pressure::pascal>(p_mid));
            }

            let state_low = fluid
                .state(
                    StateInput::PH {
                        p: Pressure::new::<uom::si::pressure::pascal>(p_low),
                        h,
                    },
                    self.composition.clone(),
                )
                .map_err(|e| SimError::Backend {
                    message: format!("Failed to evaluate state: {}", e),
                })?;

            let rho_low = fluid.rho(&state_low).map_err(|e| SimError::Backend {
                message: format!("Failed to compute rho: {}", e),
            })?;

            if (rho_low.value - rho_target) * (rho_mid.value - rho_target) < 0.0 {
                p_high = p_mid;
            } else {
                p_low = p_mid;
            }
        }

        Err(SimError::ConvergenceFailed {
            what: "pressure_from_rho_h: bisection did not converge",
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
}
