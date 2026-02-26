//! CoolProp integration tests.
//!
//! These tests verify that the CoolProp backend works correctly with realistic scenarios.
//! We use broad tolerances to avoid backend version issues, but enforce physical plausibility.

use tf_core::units::{k, pa};
use tf_fluids::{Composition, CoolPropModel, FluidModel, Species, StateInput};

#[test]
fn water_at_1atm_300k() {
    let model = CoolPropModel::new();
    let comp = Composition::pure(Species::H2O);
    let input = StateInput::PT {
        p: pa(101325.0), // 1 atm
        t: k(300.0),     // 27°C
    };

    let state = model.state(input, comp).unwrap();
    let rho = model.rho(&state).unwrap();

    // Water density at this condition should be around 996 kg/m³
    // Use wide tolerance to avoid version/backend issues
    assert!(
        rho.value > 900.0 && rho.value < 1100.0,
        "rho = {} kg/m³",
        rho.value
    );
}

#[test]
fn nitrogen_gas_density_trend() {
    let model = CoolPropModel::new();
    let comp = Composition::pure(Species::N2);
    let t = k(300.0);

    // Density should increase with pressure at constant temperature
    let p1 = pa(100_000.0); // 1 bar
    let p2 = pa(200_000.0); // 2 bar
    let p3 = pa(500_000.0); // 5 bar

    let state1 = model
        .state(StateInput::PT { p: p1, t }, comp.clone())
        .unwrap();
    let state2 = model
        .state(StateInput::PT { p: p2, t }, comp.clone())
        .unwrap();
    let state3 = model.state(StateInput::PT { p: p3, t }, comp).unwrap();

    let rho1 = model.rho(&state1).unwrap();
    let rho2 = model.rho(&state2).unwrap();
    let rho3 = model.rho(&state3).unwrap();

    // Monotonicity check
    assert!(rho1.value < rho2.value, "rho should increase with pressure");
    assert!(rho2.value < rho3.value, "rho should increase with pressure");

    // Rough check: for ideal gas, rho ~ P, so rho2/rho1 ≈ 2
    let ratio = rho2.value / rho1.value;
    assert!(ratio > 1.8 && ratio < 2.2, "density ratio = {}", ratio);
}

#[test]
fn helium_low_density() {
    let model = CoolPropModel::new();
    let p = pa(101325.0);
    let t = k(300.0);

    let he_comp = Composition::pure(Species::He);
    let n2_comp = Composition::pure(Species::N2);

    let he_state = model.state(StateInput::PT { p, t }, he_comp).unwrap();
    let n2_state = model.state(StateInput::PT { p, t }, n2_comp).unwrap();

    let rho_he = model.rho(&he_state).unwrap();
    let rho_n2 = model.rho(&n2_state).unwrap();

    // Helium is much lighter than nitrogen
    // Molecular weight ratio: He (4) vs N2 (28), so rho_he/rho_n2 ≈ 4/28 ≈ 1/7
    assert!(
        rho_he.value < rho_n2.value,
        "He should be less dense than N2"
    );

    let ratio = rho_he.value / rho_n2.value;
    assert!(ratio < 0.2, "He density ratio to N2 = {}", ratio);
}

#[test]
fn ph_round_trip() {
    let model = CoolPropModel::new();
    let comp = Composition::pure(Species::O2);

    // Start with known P, T
    let p = pa(500_000.0); // 5 bar
    let t_initial = k(350.0);

    // Create state and get enthalpy
    let state_pt = model
        .state(StateInput::PT { p, t: t_initial }, comp.clone())
        .unwrap();
    let h = model.h(&state_pt).unwrap();

    // Now create state from P, h
    let state_ph = model.state(StateInput::PH { p, h }, comp).unwrap();
    let t_recovered = state_ph.temperature();

    // Temperature should match within tolerance
    let t_diff = (t_recovered.value - t_initial.value).abs();
    assert!(t_diff < 5.0, "Temperature round-trip error: {} K", t_diff);
}

#[test]
fn oxygen_properties() {
    let model = CoolPropModel::new();
    let comp = Composition::pure(Species::O2);
    let input = StateInput::PT {
        p: pa(101325.0),
        t: k(300.0),
    };

    let state = model.state(input, comp).unwrap();

    // Query all properties
    let rho = model.rho(&state).unwrap();
    let h = model.h(&state).unwrap();
    let cp = model.cp(&state).unwrap();
    let gamma = model.gamma(&state).unwrap();
    let a = model.a(&state).unwrap();

    // Sanity checks
    assert!(
        rho.value > 0.0 && rho.value < 100.0,
        "rho = {} kg/m³",
        rho.value
    );
    assert!(h.is_finite(), "h = {} J/kg", h);
    assert!(cp > 0.0 && cp < 10000.0, "cp = {} J/(kg·K)", cp);
    assert!(gamma > 1.0 && gamma < 2.0, "gamma = {}", gamma);
    assert!(a.value > 0.0 && a.value < 1000.0, "a = {} m/s", a.value);
}

#[test]
fn methane_properties() {
    let model = CoolPropModel::new();
    let comp = Composition::pure(Species::CH4);
    let input = StateInput::PT {
        p: pa(101325.0),
        t: k(300.0),
    };

    let state = model.state(input, comp).unwrap();

    let rho = model.rho(&state).unwrap();
    let gamma = model.gamma(&state).unwrap();

    // CH4 is a light gas at this condition
    assert!(
        rho.value > 0.0 && rho.value < 10.0,
        "rho = {} kg/m³",
        rho.value
    );
    assert!(gamma > 1.0 && gamma < 1.5, "gamma = {}", gamma);
}

#[test]
fn hydrogen_low_molecular_weight() {
    let model = CoolPropModel::new();
    let comp = Composition::pure(Species::H2);
    let p = pa(101325.0);
    let t = k(300.0);

    let state = model.state(StateInput::PT { p, t }, comp).unwrap();
    let rho = model.rho(&state).unwrap();

    // H2 has molecular weight 2, so at same P,T it's much less dense than air
    // Expected: ~0.08 kg/m³
    assert!(
        rho.value > 0.01 && rho.value < 0.2,
        "rho = {} kg/m³",
        rho.value
    );
}

#[test]
fn unsupported_species() {
    let model = CoolPropModel::new();
    let comp = Composition::pure(Species::RP1);
    let input = StateInput::PT {
        p: pa(101325.0),
        t: k(300.0),
    };

    // RP-1 is not supported by CoolProp
    let result = model.state(input, comp);
    assert!(result.is_err());
}

#[test]
fn mixture_not_supported() {
    let model = CoolPropModel::new();
    let comp =
        Composition::new_mole_fractions(vec![(Species::O2, 0.21), (Species::N2, 0.79)]).unwrap();

    // Mixtures not supported yet
    assert!(!model.supports_composition(&comp));
}

#[test]
fn temperature_pressure_validation() {
    let model = CoolPropModel::new();
    let comp = Composition::pure(Species::N2);

    // Negative pressure should fail
    let result = model.state(
        StateInput::PT {
            p: pa(-100.0),
            t: k(300.0),
        },
        comp.clone(),
    );
    assert!(result.is_err());

    // Zero temperature should fail
    let result = model.state(
        StateInput::PT {
            p: pa(101325.0),
            t: k(0.0),
        },
        comp,
    );
    assert!(result.is_err());
}

#[test]
fn carbon_dioxide_properties() {
    let model = CoolPropModel::new();
    let comp = Composition::pure(Species::CO2);
    let input = StateInput::PT {
        p: pa(101325.0),
        t: k(300.0),
    };

    let state = model.state(input, comp).unwrap();
    let rho = model.rho(&state).unwrap();

    // CO2 has molecular weight ~44, heavier than air (~29)
    assert!(
        rho.value > 1.0 && rho.value < 5.0,
        "rho = {} kg/m³",
        rho.value
    );
}

#[test]
fn high_pressure_density() {
    let model = CoolPropModel::new();
    let comp = Composition::pure(Species::N2);
    let p_high = pa(10_000_000.0); // 100 bar
    let t = k(300.0);

    let state = model.state(StateInput::PT { p: p_high, t }, comp).unwrap();
    let rho = model.rho(&state).unwrap();

    // At high pressure, N2 should be quite dense
    assert!(rho.value > 50.0, "rho = {} kg/m³", rho.value);
}
