//! Orifice flow element with compressible and incompressible flow models.

use crate::common::{EPSILON_PRESSURE, check_finite};
use crate::error::ComponentResult;
use crate::traits::{PortStates, TwoPortComponent};
use tf_core::units::{Area, MassRate};
use tf_fluids::{FluidModel, SpecEnthalpy};

/// Orifice flow element with support for compressible and incompressible flow.
///
/// For compressible (gas) flow, implements standard orifice equation with choking.
/// For incompressible (liquid) flow, uses Bernoulli equation.
#[derive(Debug, Clone)]
pub struct Orifice {
    name: String,
    /// Discharge coefficient (dimensionless, typically 0.6-0.9)
    pub cd: f64,
    /// Orifice throat area
    pub area: Area,
    /// Force compressible treatment even for liquids
    pub treat_as_gas: bool,
}

impl Orifice {
    /// Create a new orifice.
    pub fn new(name: String, cd: f64, area: Area) -> Self {
        Self {
            name,
            cd,
            area,
            treat_as_gas: false,
        }
    }

    /// Create an orifice that treats flow as compressible.
    pub fn new_compressible(name: String, cd: f64, area: Area) -> Self {
        Self {
            name,
            cd,
            area,
            treat_as_gas: true,
        }
    }

    /// Compute mass flow for compressible flow with choking.
    fn mdot_compressible(
        &self,
        fluid: &dyn FluidModel,
        ports: PortStates<'_>,
    ) -> ComponentResult<MassRate> {
        let p_in = ports.inlet.pressure().value;
        let p_out = ports.outlet.pressure().value;

        // Small pressure difference => zero flow
        if (p_in - p_out).abs() < EPSILON_PRESSURE {
            return Ok(tf_core::units::kgps(0.0));
        }

        // Determine upstream/downstream based on pressure
        let (p_up, state_up, sign) = if p_in > p_out {
            (p_in, ports.inlet, 1.0)
        } else {
            (p_out, ports.outlet, -1.0)
        };

        let p_down = if p_in > p_out { p_out } else { p_in };

        // Get upstream properties
        let rho_up = fluid.rho(state_up)?.value;
        let gamma = fluid.gamma(state_up)?;
        let a_up = fluid.a(state_up)?.value; // speed of sound

        check_finite(rho_up, "upstream density")?;
        check_finite(gamma, "gamma")?;
        check_finite(a_up, "speed of sound")?;

        // Pressure ratio
        let pr = p_down / p_up;

        // Critical pressure ratio for choking
        let pr_crit = (2.0 / (gamma + 1.0)).powf(gamma / (gamma - 1.0));

        let mdot_abs = if pr <= pr_crit {
            // Choked flow
            // mdot = Cd * A * rho_up * a_up * sqrt(gamma) * (2/(gamma+1))^((gamma+1)/(2*(gamma-1)))
            let choke_factor =
                gamma.sqrt() * (2.0 / (gamma + 1.0)).powf((gamma + 1.0) / (2.0 * (gamma - 1.0)));
            self.cd * self.area.value * rho_up * a_up * choke_factor
        } else {
            // Non-choked compressible flow
            // Use simplified form: mdot ≈ Cd * A * sqrt(2 * rho_up * (p_up - p_down))
            // This is an approximation; exact compressible flow would need integration
            let dp = p_up - p_down;
            self.cd * self.area.value * (2.0 * rho_up * dp).sqrt()
        };

        check_finite(mdot_abs, "mass flow rate")?;

        Ok(tf_core::units::kgps(sign * mdot_abs))
    }

    /// Compute mass flow for incompressible flow.
    fn mdot_incompressible(
        &self,
        fluid: &dyn FluidModel,
        ports: PortStates<'_>,
    ) -> ComponentResult<MassRate> {
        let p_in = ports.inlet.pressure().value;
        let p_out = ports.outlet.pressure().value;
        let dp = p_in - p_out;

        // Small pressure difference => zero flow
        if dp.abs() < EPSILON_PRESSURE {
            return Ok(tf_core::units::kgps(0.0));
        }

        // Use upstream density
        let state_up = if dp > 0.0 { ports.inlet } else { ports.outlet };
        let rho = fluid.rho(state_up)?.value;

        check_finite(rho, "density")?;

        // Bernoulli: mdot = sign(dp) * Cd * A * sqrt(2 * rho * |dp|)
        let sign = dp.signum();
        let mdot = sign * self.cd * self.area.value * (2.0 * rho * dp.abs()).sqrt();

        check_finite(mdot, "mass flow rate")?;

        Ok(tf_core::units::kgps(mdot))
    }
}

impl TwoPortComponent for Orifice {
    fn name(&self) -> &str {
        &self.name
    }

    fn mdot(&self, fluid: &dyn FluidModel, ports: PortStates<'_>) -> ComponentResult<MassRate> {
        if self.treat_as_gas {
            self.mdot_compressible(fluid, ports)
        } else {
            // Default to incompressible
            // Could add heuristic to auto-detect (e.g., check gamma, density changes)
            self.mdot_incompressible(fluid, ports)
        }
    }

    fn outlet_enthalpy(
        &self,
        fluid: &dyn FluidModel,
        ports: PortStates<'_>,
        _mdot: MassRate,
    ) -> ComponentResult<SpecEnthalpy> {
        // Isenthalpic (throttling) process: h_out = h_in
        Ok(fluid.h(ports.inlet)?)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tf_core::units::{k, pa};
    use tf_fluids::{Composition, CoolPropModel, Species, StateInput};
    use uom::si::area::square_meter;

    #[test]
    fn orifice_zero_flow_equal_pressure() {
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

        let orifice = Orifice::new("test".into(), 0.7, Area::new::<square_meter>(0.001));

        let ports = PortStates {
            inlet: &state,
            outlet: &state,
        };

        let mdot = orifice.mdot(&model, ports).unwrap();
        assert!(mdot.value.abs() < 1e-6);
    }

    #[test]
    fn orifice_positive_flow() {
        let model = CoolPropModel::new();
        let comp = Composition::pure(Species::N2);

        let state_in = model
            .state(
                StateInput::PT {
                    p: pa(200_000.0),
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

        let orifice =
            Orifice::new_compressible("test".into(), 0.7, Area::new::<square_meter>(0.001));

        let ports = PortStates {
            inlet: &state_in,
            outlet: &state_out,
        };

        let mdot = orifice.mdot(&model, ports).unwrap();
        assert!(mdot.value > 0.0, "Flow should be positive");
        assert!(mdot.value.is_finite(), "Flow should be finite");
    }

    #[test]
    fn orifice_reverse_flow() {
        let model = CoolPropModel::new();
        let comp = Composition::pure(Species::N2);

        let state_in = model
            .state(
                StateInput::PT {
                    p: pa(100_000.0),
                    t: k(300.0),
                },
                comp.clone(),
            )
            .unwrap();

        let state_out = model
            .state(
                StateInput::PT {
                    p: pa(200_000.0),
                    t: k(300.0),
                },
                comp,
            )
            .unwrap();

        let orifice = Orifice::new("test".into(), 0.7, Area::new::<square_meter>(0.001));

        let ports = PortStates {
            inlet: &state_in,
            outlet: &state_out,
        };

        let mdot = orifice.mdot(&model, ports).unwrap();
        assert!(mdot.value < 0.0, "Flow should be negative (reverse)");
    }
}
