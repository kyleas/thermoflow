//! Pipe component with friction using Darcy-Weisbach correlation.

use crate::common::{EPSILON_MDOT, check_finite};
use crate::error::ComponentResult;
use crate::traits::{PortStates, TwoPortComponent};
use tf_core::units::{DynVisc, Length, MassRate, Pressure};
use tf_fluids::{FluidModel, SpecEnthalpy};

/// Pipe with friction using Darcy-Weisbach correlation.
///
/// Computes pressure drop for a given flow rate using friction factor.
/// Since the TwoPortComponent trait requires computing mdot from states,
/// this component inverts the pressure drop relationship using bisection.
#[derive(Debug, Clone)]
pub struct Pipe {
    name: String,
    /// Pipe length
    pub length: Length,
    /// Pipe inner diameter
    pub diameter: Length,
    /// Surface roughness (absolute)
    pub roughness: Length,
    /// Minor loss coefficient (sum of K factors for fittings, bends, etc.)
    pub k_minor: f64,
    /// Dynamic viscosity (constant for now; future: get from FluidModel)
    pub mu_const: DynVisc,
}

impl Pipe {
    /// Create a new pipe.
    pub fn new(
        name: String,
        length: Length,
        diameter: Length,
        roughness: Length,
        k_minor: f64,
        mu_const: DynVisc,
    ) -> Self {
        Self {
            name,
            length,
            diameter,
            roughness,
            k_minor,
            mu_const,
        }
    }

    /// Compute friction factor using Colebrook-White with Swamee-Jain approximation.
    fn friction_factor(&self, reynolds: f64) -> f64 {
        if reynolds < 2300.0 {
            // Laminar
            64.0 / reynolds
        } else {
            // Turbulent: Swamee-Jain
            let e_d = self.roughness.value / self.diameter.value;
            let a = e_d / 3.7;
            let b = 5.74 / reynolds.powf(0.9);
            let f = 0.25 / (a + b).log10().powi(2);
            f.max(0.0001) // Clamp to avoid issues
        }
    }

    /// Compute pressure drop for a given mass flow rate.
    fn pressure_drop_for_mdot(&self, rho: f64, mdot_abs: f64) -> ComponentResult<f64> {
        if mdot_abs < EPSILON_MDOT {
            return Ok(0.0);
        }

        let area = std::f64::consts::PI * self.diameter.value.powi(2) / 4.0;
        let velocity = mdot_abs / (rho * area);
        let reynolds = rho * velocity * self.diameter.value / self.mu_const.value;

        check_finite(reynolds, "Reynolds number")?;

        let f = self.friction_factor(reynolds);

        // ΔP = (f*L/D + K) * 0.5 * rho * v^2
        let dp = (f * self.length.value / self.diameter.value + self.k_minor)
            * 0.5
            * rho
            * velocity.powi(2);

        check_finite(dp, "pressure drop")?;

        Ok(dp)
    }

    /// Solve for mdot that produces the observed pressure drop using bisection.
    fn solve_mdot(&self, rho: f64, dp_target: f64) -> ComponentResult<f64> {
        const MAX_ITER: usize = 50;
        const TOL: f64 = 1.0; // Pa

        // Handle very small pressure drops
        if dp_target.abs() < TOL {
            return Ok(0.0);
        }

        // Estimate bounds for mdot
        // Use a heuristic: mdot_max ~ sqrt(dp_target) scaling
        let mdot_max = 100.0 * dp_target.abs().sqrt();
        let mut mdot_low = 0.0;
        let mut mdot_high = mdot_max;

        for _ in 0..MAX_ITER {
            let mdot_mid = 0.5 * (mdot_low + mdot_high);
            let dp_mid = self.pressure_drop_for_mdot(rho, mdot_mid)?;

            if (dp_mid - dp_target.abs()).abs() < TOL {
                return Ok(mdot_mid);
            }

            if dp_mid < dp_target.abs() {
                mdot_low = mdot_mid;
            } else {
                mdot_high = mdot_mid;
            }
        }

        // Return best estimate
        Ok(0.5 * (mdot_low + mdot_high))
    }
}

impl TwoPortComponent for Pipe {
    fn name(&self) -> &str {
        &self.name
    }

    fn mdot(&self, fluid: &dyn FluidModel, ports: PortStates<'_>) -> ComponentResult<MassRate> {
        let p_in = ports.inlet.pressure().value;
        let p_out = ports.outlet.pressure().value;
        let dp = p_in - p_out;

        // Determine upstream state for density
        let state_up = if dp > 0.0 { ports.inlet } else { ports.outlet };
        let rho = fluid.rho(state_up)?.value;

        check_finite(rho, "density")?;

        let sign = dp.signum();
        let mdot_abs = self.solve_mdot(rho, dp.abs())?;

        Ok(tf_core::units::kgps(sign * mdot_abs))
    }

    fn delta_p(
        &self,
        fluid: &dyn FluidModel,
        ports: PortStates<'_>,
        mdot: MassRate,
    ) -> ComponentResult<Pressure> {
        let state_up = if mdot.value > 0.0 {
            ports.inlet
        } else {
            ports.outlet
        };
        let rho = fluid.rho(state_up)?.value;

        check_finite(rho, "density")?;

        let mdot_abs = mdot.value.abs();
        let dp_abs = self.pressure_drop_for_mdot(rho, mdot_abs)?;
        let dp = mdot.value.signum() * dp_abs;

        use uom::si::pressure::pascal;
        Ok(Pressure::new::<pascal>(dp))
    }

    fn outlet_enthalpy(
        &self,
        fluid: &dyn FluidModel,
        ports: PortStates<'_>,
        _mdot: MassRate,
    ) -> ComponentResult<SpecEnthalpy> {
        // Isenthalpic (no work, adiabatic): h_out = h_in
        Ok(fluid.h(ports.inlet)?)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tf_core::units::{k, pa};
    use tf_fluids::{Composition, CoolPropModel, Species, StateInput};
    use uom::si::{dynamic_viscosity::pascal_second, length::meter};

    #[test]
    fn pipe_zero_flow_equal_pressure() {
        let model = CoolPropModel::new();
        let comp = Composition::pure(Species::N2);

        let state = model
            .state(
                StateInput::PT {
                    p: pa(101325.0),
                    t: k(300.0),
                },
                comp,
            )
            .unwrap();

        let pipe = Pipe::new(
            "test".into(),
            Length::new::<meter>(10.0),
            Length::new::<meter>(0.05),
            Length::new::<meter>(1e-5),
            1.0,
            DynVisc::new::<pascal_second>(1.8e-5),
        );

        let ports = PortStates {
            inlet: &state,
            outlet: &state,
        };

        let mdot = pipe.mdot(&model, ports).unwrap();
        assert!(mdot.value.abs() < 1e-6);
    }

    #[test]
    fn pipe_positive_flow() {
        let model = CoolPropModel::new();
        let comp = Composition::pure(Species::N2);

        let state_in = model
            .state(
                StateInput::PT {
                    p: pa(150_000.0),
                    t: k(300.0),
                },
                comp.clone(),
            )
            .unwrap();

        let state_out = model
            .state(
                StateInput::PT {
                    p: pa(100_000.0),
                    t: k(300.0),
                },
                comp,
            )
            .unwrap();

        let pipe = Pipe::new(
            "test".into(),
            Length::new::<meter>(10.0),
            Length::new::<meter>(0.05),
            Length::new::<meter>(1e-5),
            1.0,
            DynVisc::new::<pascal_second>(1.8e-5),
        );

        let ports = PortStates {
            inlet: &state_in,
            outlet: &state_out,
        };

        let mdot = pipe.mdot(&model, ports).unwrap();
        assert!(mdot.value > 0.0, "Flow should be positive");
        assert!(mdot.value.is_finite(), "Flow should be finite");
    }

    #[test]
    fn pipe_delta_p_round_trip() {
        let model = CoolPropModel::new();
        let comp = Composition::pure(Species::N2);

        let state_in = model
            .state(
                StateInput::PT {
                    p: pa(150_000.0),
                    t: k(300.0),
                },
                comp.clone(),
            )
            .unwrap();

        let state_out = model
            .state(
                StateInput::PT {
                    p: pa(100_000.0),
                    t: k(300.0),
                },
                comp,
            )
            .unwrap();

        let pipe = Pipe::new(
            "test".into(),
            Length::new::<meter>(10.0),
            Length::new::<meter>(0.05),
            Length::new::<meter>(1e-5),
            1.0,
            DynVisc::new::<pascal_second>(1.8e-5),
        );

        let ports = PortStates {
            inlet: &state_in,
            outlet: &state_out,
        };

        // Compute mdot from pressure drop
        let mdot = pipe.mdot(&model, ports).unwrap();

        // Compute pressure drop from mdot
        let dp_computed = pipe.delta_p(&model, ports, mdot).unwrap();
        let dp_actual = state_in.pressure().value - state_out.pressure().value;

        // Should match within bisection tolerance
        let error = (dp_computed.value - dp_actual).abs();
        assert!(error < 10.0, "Pressure drop mismatch: {} Pa", error);
    }

    #[test]
    fn pipe_longer_means_less_flow() {
        let model = CoolPropModel::new();
        let comp = Composition::pure(Species::N2);

        let state_in = model
            .state(
                StateInput::PT {
                    p: pa(150_000.0),
                    t: k(300.0),
                },
                comp.clone(),
            )
            .unwrap();

        let state_out = model
            .state(
                StateInput::PT {
                    p: pa(100_000.0),
                    t: k(300.0),
                },
                comp,
            )
            .unwrap();

        let pipe_short = Pipe::new(
            "short".into(),
            Length::new::<meter>(5.0),
            Length::new::<meter>(0.05),
            Length::new::<meter>(1e-5),
            1.0,
            DynVisc::new::<pascal_second>(1.8e-5),
        );

        let pipe_long = Pipe::new(
            "long".into(),
            Length::new::<meter>(20.0),
            Length::new::<meter>(0.05),
            Length::new::<meter>(1e-5),
            1.0,
            DynVisc::new::<pascal_second>(1.8e-5),
        );

        let ports = PortStates {
            inlet: &state_in,
            outlet: &state_out,
        };

        let mdot_short = pipe_short.mdot(&model, ports).unwrap().value;
        let mdot_long = pipe_long.mdot(&model, ports).unwrap().value;

        assert!(
            mdot_short > mdot_long,
            "Shorter pipe should have higher flow"
        );
    }
}
