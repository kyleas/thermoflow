//! Integration tests for tf-components with real fluid models.

use tf_components::{Orifice, Pipe, PortStates, TwoPortComponent, Valve, ValveLaw};
use tf_core::units::{Area, DynVisc, Length, k, pa};
use tf_fluids::{Composition, CoolPropModel, FluidModel, Species, StateInput};
use uom::si::{area::square_meter, dynamic_viscosity::pascal_second, length::meter};

#[test]
fn orifice_nitrogen_high_pressure() {
    let model = CoolPropModel::new();
    let comp = Composition::pure(Species::N2);

    // 5 bar upstream, 1 bar downstream
    let state_in = model
        .state(
            StateInput::PT {
                p: pa(500_000.0),
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

    let orifice = Orifice::new_compressible(
        "N2_orifice".into(),
        0.7,
        Area::new::<square_meter>(0.0001), // 1 cm²
    );

    let ports = PortStates {
        inlet: &state_in,
        outlet: &state_out,
    };

    let mdot = orifice.mdot(&model, ports).unwrap();

    // Sanity checks
    assert!(mdot.value > 0.0, "Flow should be positive (inlet → outlet)");
    assert!(
        mdot.value < 10.0,
        "Flow should be reasonable for small orifice (~kg/s range)"
    );
    assert!(mdot.value.is_finite(), "Flow must be finite");
}

#[test]
fn valve_position_sweep() {
    let model = CoolPropModel::new();
    let comp = Composition::pure(Species::O2);

    let state_in = model
        .state(
            StateInput::PT {
                p: pa(300_000.0),
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

    let positions = [0.0, 0.2, 0.4, 0.6, 0.8, 1.0];
    let mut prev_mdot = -1.0;

    for &pos in &positions {
        let valve = Valve::new(
            "O2_valve".into(),
            0.7,
            Area::new::<square_meter>(0.0002),
            pos,
        );

        let ports = PortStates {
            inlet: &state_in,
            outlet: &state_out,
        };

        let mdot = valve.mdot(&model, ports).unwrap().value;

        // Flow should be monotonic with position
        assert!(mdot >= prev_mdot, "Flow should not decrease with position");
        prev_mdot = mdot;

        // Finite check
        assert!(mdot.is_finite(), "Flow must be finite");
    }

    // Closed valve should give ~zero flow
    let valve_closed = Valve::new(
        "O2_valve_closed".into(),
        0.7,
        Area::new::<square_meter>(0.0002),
        0.0,
    );

    let ports = PortStates {
        inlet: &state_in,
        outlet: &state_out,
    };

    let mdot_closed = valve_closed.mdot(&model, ports).unwrap().value;
    assert!(
        mdot_closed.abs() < 1e-9,
        "Closed valve should have ~zero flow"
    );
}

#[test]
fn valve_quadratic_law() {
    let model = CoolPropModel::new();
    let comp = Composition::pure(Species::CH4);

    let state_in = model
        .state(
            StateInput::PT {
                p: pa(250_000.0),
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

    let valve_linear = Valve::new("linear".into(), 0.7, Area::new::<square_meter>(0.0001), 0.5)
        .with_law(ValveLaw::Linear);

    let valve_quad = Valve::new(
        "quadratic".into(),
        0.7,
        Area::new::<square_meter>(0.0001),
        0.5,
    )
    .with_law(ValveLaw::Quadratic);

    let ports = PortStates {
        inlet: &state_in,
        outlet: &state_out,
    };

    let mdot_linear = valve_linear.mdot(&model, ports).unwrap().value;
    let mdot_quad = valve_quad.mdot(&model, ports).unwrap().value;

    assert!(
        mdot_quad < mdot_linear,
        "Quadratic law should give less flow at 50% position"
    );
}

#[test]
fn pipe_pressure_drop_inversion() {
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
        "N2_pipe".into(),
        Length::new::<meter>(10.0),
        Length::new::<meter>(0.05),            // 5 cm diameter
        Length::new::<meter>(1e-5),            // roughness
        1.0,                                   // K minor
        DynVisc::new::<pascal_second>(1.8e-5), // N2 viscosity
    );

    let ports = PortStates {
        inlet: &state_in,
        outlet: &state_out,
    };

    // Compute mdot from states
    let mdot = pipe.mdot(&model, ports).unwrap();

    assert!(mdot.value > 0.0, "Flow should be positive");
    assert!(mdot.value.is_finite(), "Flow must be finite");

    // Verify pressure drop round-trip
    let dp_computed = pipe.delta_p(&model, ports, mdot).unwrap().value;
    let dp_actual = state_in.pressure().value - state_out.pressure().value;

    // Allow some tolerance due to bisection
    let error = (dp_computed - dp_actual).abs();
    assert!(
        error < 100.0,
        "Pressure drop error: {} Pa (should be < 100 Pa)",
        error
    );
}

#[test]
fn pipe_diameter_scaling() {
    let model = CoolPropModel::new();
    let comp = Composition::pure(Species::N2);

    let state_in = model
        .state(
            StateInput::PT {
                p: pa(120_000.0),
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

    let pipe_small = Pipe::new(
        "small".into(),
        Length::new::<meter>(10.0),
        Length::new::<meter>(0.025), // 2.5 cm
        Length::new::<meter>(1e-5),
        1.0,
        DynVisc::new::<pascal_second>(1.8e-5),
    );

    let pipe_large = Pipe::new(
        "large".into(),
        Length::new::<meter>(10.0),
        Length::new::<meter>(0.1), // 10 cm
        Length::new::<meter>(1e-5),
        1.0,
        DynVisc::new::<pascal_second>(1.8e-5),
    );

    let ports = PortStates {
        inlet: &state_in,
        outlet: &state_out,
    };

    let mdot_small = pipe_small.mdot(&model, ports).unwrap().value;
    let mdot_large = pipe_large.mdot(&model, ports).unwrap().value;

    assert!(
        mdot_large > mdot_small,
        "Larger diameter should allow higher flow"
    );
}

#[test]
fn helium_high_speed_flow() {
    let model = CoolPropModel::new();
    let comp = Composition::pure(Species::He);

    // High pressure drop with light gas
    let state_in = model
        .state(
            StateInput::PT {
                p: pa(1_000_000.0), // 10 bar
                t: k(300.0),
            },
            comp.clone(),
        )
        .unwrap();

    let state_out = model
        .state(
            StateInput::PT {
                p: pa(100_000.0), // 1 bar
                t: k(300.0),
            },
            comp,
        )
        .unwrap();

    let orifice =
        Orifice::new_compressible("He_orifice".into(), 0.65, Area::new::<square_meter>(0.0001));

    let ports = PortStates {
        inlet: &state_in,
        outlet: &state_out,
    };

    let mdot = orifice.mdot(&model, ports).unwrap();

    // Helium is light, so mass flow should be lower than heavier gases
    assert!(mdot.value > 0.0, "Flow should be positive");
    assert!(
        mdot.value < 5.0,
        "Helium mass flow should be reasonable for small orifice"
    );
    assert!(mdot.value.is_finite(), "Flow must be finite");
}

#[test]
fn water_incompressible_orifice() {
    let model = CoolPropModel::new();
    let comp = Composition::pure(Species::H2O);

    // Liquid water at moderate temperature and pressure
    let state_in = model
        .state(
            StateInput::PT {
                p: pa(200_000.0),
                t: k(320.0),
            },
            comp.clone(),
        )
        .unwrap();

    let state_out = model
        .state(
            StateInput::PT {
                p: pa(100_000.0),
                t: k(320.0),
            },
            comp,
        )
        .unwrap();

    let orifice = Orifice::new("H2O_orifice".into(), 0.7, Area::new::<square_meter>(0.0001));

    let ports = PortStates {
        inlet: &state_in,
        outlet: &state_out,
    };

    let mdot = orifice.mdot(&model, ports).unwrap();

    // Liquid has high density, so expect higher mass flow
    assert!(mdot.value > 0.0, "Flow should be positive");
    assert!(mdot.value.is_finite(), "Flow must be finite");
}
