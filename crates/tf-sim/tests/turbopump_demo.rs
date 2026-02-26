//! Integration test: Turbopump with coupled shaft dynamics.
//!
//! Network:
//! - Gas source → Turbine → exhaust (low pressure)
//! - Liquid source → Pump → restriction → ambient
//! - Turbine and pump coupled via shared rotating shaft
//!
//! Demonstrates:
//! - Turbine extracts power from high-pressure gas
//! - Shaft accelerates from rest
//! - Pump pressure rise increases with shaft speed
//! - Coupled dynamic response

use tf_components::{Pump, Turbine, TwoPortComponent};
use tf_core::units::{k, m, pa};
use tf_fluids::{Composition, CoolPropModel, FluidModel, Species, StateInput};
use tf_graph::GraphBuilder;
use tf_sim::{IntegratorType, Shaft, ShaftState, SimOptions, SimResult, TransientModel, run_sim};

/// Turbopump demo model with coupled shaft dynamics.
#[allow(dead_code)] // Simplified model doesn't use all fields
struct TurboPumpModel<'a> {
    // Gas turbine circuit
    turbine_graph: tf_graph::Graph,
    turbine_inlet_node: tf_core::NodeId,
    turbine_outlet_node: tf_core::NodeId,
    turbine_comp_id: tf_core::CompId,
    turbine_cd: f64,
    turbine_area: tf_core::units::Area,
    turbine_eta: f64,

    // Pump circuit (separate graph for simplicity)
    pump_graph: tf_graph::Graph,
    pump_inlet_node: tf_core::NodeId,
    pump_outlet_node: tf_core::NodeId,
    pump_comp_id: tf_core::CompId,
    orifice_comp_id: tf_core::CompId,
    pump_cd: f64,
    pump_area: tf_core::units::Area,
    pump_eta: f64,
    orifice_cd: f64,
    orifice_area: tf_core::units::Area,

    // Boundary conditions
    turbine_inlet_p: tf_core::units::Pressure,
    turbine_inlet_t: tf_core::units::Temperature,
    turbine_outlet_p: tf_core::units::Pressure,
    turbine_outlet_t: tf_core::units::Temperature,

    pump_inlet_p: tf_core::units::Pressure,
    pump_inlet_t: tf_core::units::Temperature,
    pump_outlet_p: tf_core::units::Pressure,
    pump_outlet_t: tf_core::units::Temperature,

    // Fluids
    gas_fluid: &'a dyn FluidModel,
    gas_comp: Composition,
    liquid_fluid: &'a dyn FluidModel,
    liquid_comp: Composition,

    // Shaft
    shaft: Shaft,

    // Pump speed map: delta_p = k * omega^2 (simplified)
    pump_speed_coeff: f64,
}

/// Combined state: shaft only (simplified - no control volumes for this demo).
#[derive(Clone, Debug)]
struct TurboPumpState {
    shaft: ShaftState,
}

impl<'a> TurboPumpModel<'a> {
    #[allow(clippy::too_many_arguments)]
    fn new(
        gas_fluid: &'a dyn FluidModel,
        liquid_fluid: &'a dyn FluidModel,
        shaft_inertia: f64,
        shaft_loss: f64,
    ) -> SimResult<Self> {
        // Build turbine graph: source -> turbine -> exhaust
        let mut turb_builder = GraphBuilder::new();
        let turb_in = turb_builder.add_node("turbine_inlet");
        let turb_out = turb_builder.add_node("turbine_outlet");
        let turb_comp = turb_builder.add_component("turbine", turb_in, turb_out);
        let turbine_graph = turb_builder
            .build()
            .map_err(|e| tf_sim::SimError::Backend {
                message: e.to_string(),
            })?;

        // Build pump graph: source -> pump -> orifice -> exhaust
        let mut pump_builder = GraphBuilder::new();
        let pump_in = pump_builder.add_node("pump_inlet");
        let pump_mid = pump_builder.add_node("pump_outlet");
        let pump_out = pump_builder.add_node("ambient");
        let pump_comp = pump_builder.add_component("pump", pump_in, pump_mid);
        let orifice_comp = pump_builder.add_component("orifice", pump_mid, pump_out);
        let pump_graph = pump_builder
            .build()
            .map_err(|e| tf_sim::SimError::Backend {
                message: e.to_string(),
            })?;

        let shaft = Shaft::new(shaft_inertia, shaft_loss)?;

        Ok(Self {
            turbine_graph,
            turbine_inlet_node: turb_in,
            turbine_outlet_node: turb_out,
            turbine_comp_id: turb_comp,
            turbine_cd: 0.85,
            turbine_area: m(0.015) * m(0.015) * std::f64::consts::PI / 4.0,
            turbine_eta: 0.75,

            pump_graph,
            pump_inlet_node: pump_in,
            pump_outlet_node: pump_out,
            pump_comp_id: pump_comp,
            orifice_comp_id: orifice_comp,
            pump_cd: 0.8,
            pump_area: m(0.01) * m(0.01) * std::f64::consts::PI / 4.0,
            pump_eta: 0.7,
            orifice_cd: 0.7,
            orifice_area: m(0.008) * m(0.008) * std::f64::consts::PI / 4.0,

            // Turbine: high pressure gas in, low pressure out
            turbine_inlet_p: pa(800_000.0),
            turbine_inlet_t: k(450.0),
            turbine_outlet_p: pa(100_000.0),
            turbine_outlet_t: k(300.0),

            // Pump: liquid at moderate pressure
            pump_inlet_p: pa(200_000.0),
            pump_inlet_t: k(300.0),
            pump_outlet_p: pa(101_325.0),
            pump_outlet_t: k(300.0),

            gas_fluid,
            gas_comp: Composition::pure(Species::N2),
            liquid_fluid,
            liquid_comp: Composition::pure(Species::H2O),

            shaft,
            pump_speed_coeff: 50.0, // Pa/(rad/s)^2
        })
    }

    /// Compute turbine power at current shaft speed.
    fn turbine_power(&self, _omega: f64) -> SimResult<f64> {
        // Create boundary states for turbine
        let state_in = self
            .gas_fluid
            .state(
                StateInput::PT {
                    p: self.turbine_inlet_p,
                    t: self.turbine_inlet_t,
                },
                self.gas_comp.clone(),
            )
            .map_err(|e| tf_sim::SimError::Backend {
                message: e.to_string(),
            })?;

        let state_out = self
            .gas_fluid
            .state(
                StateInput::PT {
                    p: self.turbine_outlet_p,
                    t: self.turbine_outlet_t,
                },
                self.gas_comp.clone(),
            )
            .map_err(|e| tf_sim::SimError::Backend {
                message: e.to_string(),
            })?;

        // Create turbine component
        let turb_comp = Turbine::new(
            "turbine".to_string(),
            self.turbine_cd,
            self.turbine_area,
            self.turbine_eta,
        )
        .map_err(|e| tf_sim::SimError::Backend {
            message: e.to_string(),
        })?;

        let ports = tf_components::PortStates {
            inlet: &state_in,
            outlet: &state_out,
        };

        // Compute mass flow and power
        let mdot =
            turb_comp
                .mdot(self.gas_fluid, ports)
                .map_err(|e| tf_sim::SimError::Backend {
                    message: e.to_string(),
                })?;

        let power = turb_comp
            .shaft_power(self.gas_fluid, ports, mdot)
            .map_err(|e| tf_sim::SimError::Backend {
                message: e.to_string(),
            })?;

        Ok(power.value)
    }

    /// Compute pump power at current shaft speed.
    fn pump_power(&self, omega: f64) -> SimResult<f64> {
        // Compute pump pressure rise from speed (simplified characteristic curve)
        let delta_p_val = (self.pump_speed_coeff * omega.max(0.0).powi(2)).min(500_000.0); // Cap at 500 kPa
        let delta_p = pa(delta_p_val);

        // Create boundary states for pump
        let state_in = self
            .liquid_fluid
            .state(
                StateInput::PT {
                    p: self.pump_inlet_p,
                    t: self.pump_inlet_t,
                },
                self.liquid_comp.clone(),
            )
            .map_err(|e| tf_sim::SimError::Backend {
                message: e.to_string(),
            })?;

        // Outlet state: approximate as inlet + delta_p (incompressible flow)
        let p_out_approx = self.pump_inlet_p.value + delta_p_val;
        let state_out = self
            .liquid_fluid
            .state(
                StateInput::PT {
                    p: pa(p_out_approx),
                    t: self.pump_inlet_t, // Assume minimal temperature rise
                },
                self.liquid_comp.clone(),
            )
            .map_err(|e| tf_sim::SimError::Backend {
                message: e.to_string(),
            })?;

        // Create pump component
        let pump_comp = Pump::new(
            "pump".to_string(),
            delta_p,
            self.pump_eta,
            self.pump_cd,
            self.pump_area,
        )
        .map_err(|e| tf_sim::SimError::Backend {
            message: e.to_string(),
        })?;

        let ports = tf_components::PortStates {
            inlet: &state_in,
            outlet: &state_out,
        };

        // Compute mass flow and power
        let mdot =
            pump_comp
                .mdot(self.liquid_fluid, ports)
                .map_err(|e| tf_sim::SimError::Backend {
                    message: e.to_string(),
                })?;

        let power = pump_comp
            .shaft_power(self.liquid_fluid, ports, mdot)
            .map_err(|e| tf_sim::SimError::Backend {
                message: e.to_string(),
            })?;

        Ok(power.value)
    }
}

impl<'a> TransientModel for TurboPumpModel<'a> {
    type State = TurboPumpState;

    fn initial_state(&self) -> Self::State {
        TurboPumpState {
            shaft: ShaftState {
                omega_rad_s: 1.0, // Start with small non-zero speed
            },
        }
    }

    fn rhs(&mut self, _t: f64, x: &Self::State) -> SimResult<Self::State> {
        let omega = x.shaft.omega_rad_s.max(0.1); // Regularize

        // Compute component powers
        let p_turbine = self.turbine_power(omega)?;
        let p_pump = self.pump_power(omega)?;

        // Convert powers to torques
        let tau_turbine = self.shaft.power_to_torque(p_turbine, omega);
        let tau_pump = self.shaft.power_to_torque(p_pump, omega);

        // Compute shaft acceleration
        let torques = vec![tau_turbine, tau_pump];
        let domega_dt = self.shaft.angular_acceleration(&torques, omega);

        Ok(TurboPumpState {
            shaft: ShaftState {
                omega_rad_s: domega_dt,
            },
        })
    }

    fn add(&self, a: &Self::State, b: &Self::State) -> Self::State {
        TurboPumpState {
            shaft: ShaftState {
                omega_rad_s: a.shaft.omega_rad_s + b.shaft.omega_rad_s,
            },
        }
    }

    fn scale(&self, a: &Self::State, scale: f64) -> Self::State {
        TurboPumpState {
            shaft: ShaftState {
                omega_rad_s: scale * a.shaft.omega_rad_s,
            },
        }
    }
}

#[test]
fn turbopump_coupled_dynamics() {
    let gas_fluid = CoolPropModel::new();
    let liquid_fluid = CoolPropModel::new();

    let shaft_inertia = 0.5; // kg·m² (increased for realistic dynamics)
    let shaft_loss = 0.1; // N·m·s/rad (increased friction)

    let mut model = TurboPumpModel::new(&gas_fluid, &liquid_fluid, shaft_inertia, shaft_loss)
        .expect("Failed to create model");

    let opts = SimOptions {
        dt: 2e-3,
        t_end: 0.1,
        max_steps: 10_000,
        record_every: 5,
        integrator: IntegratorType::ForwardEuler,
    };

    let record = run_sim(&mut model, &opts).expect("Simulation failed");

    println!("Simulation completed: {} steps recorded", record.t.len());

    // Extract initial and final states
    let omega_initial = record.x.first().unwrap().shaft.omega_rad_s;
    let omega_final = record.x.last().unwrap().shaft.omega_rad_s;

    println!(
        "Shaft speed: initial = {} rad/s, final = {} rad/s",
        omega_initial, omega_final
    );

    // Trend assertions (no exact values)
    assert!(omega_final > omega_initial, "Shaft speed should increase");
    assert!(omega_final > 1.0, "Shaft should spin up from initial speed");
    assert!(omega_final < 1000.0, "Shaft speed should remain reasonable");

    // Verify all states are finite
    for state in &record.x {
        assert!(
            state.shaft.omega_rad_s.is_finite(),
            "All omega values should be finite"
        );
        assert!(
            state.shaft.omega_rad_s >= 0.0,
            "Omega should be non-negative"
        );
    }

    println!("Turbopump demo test PASSED (trends verified)");
}
