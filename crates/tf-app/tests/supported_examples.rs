//! Supported/unsupported workflow validation tests.

use std::path::Path;

#[test]
fn supported_examples_validate() {
    let supported = [
        "../../examples/projects/01_orifice_steady.yaml",
        "../../examples/projects/02_tank_blowdown_transient.yaml",
        "../../examples/projects/03_simple_vent_transient.yaml",
        "../../examples/projects/04_two_cv_series_vent_transient.yaml",
        "../../examples/projects/05_two_cv_pipe_vent_transient.yaml",
        "../../examples/projects/09_pressure_controlled_vent.yaml",
        "../../examples/projects/10_flow_controlled_valve.yaml",
    ];

    for path in supported {
        let project = tf_app::load_project(Path::new(path)).expect("supported example should load");
        tf_app::validate_project(&project).expect("supported example should validate");
    }
}

#[test]
fn unsupported_scheduled_valve_example_rejected() {
    let path = Path::new("../../examples/projects/unsupported/02_tank_blowdown_scheduled.yaml");
    let err = tf_app::load_project(path).expect_err("unsupported example must fail to load");
    let msg = err.to_string();
    assert!(
        msg.contains("Timed valve position schedules")
            || msg.contains("Timed valve opening/closing schedules"),
        "unexpected validation error: {}",
        msg
    );
}
