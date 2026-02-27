# Thermoflow Examples

Open examples using the GUI:
- Open -> select a file under examples/projects/

## Projects

- 01_orifice_steady.yaml
  Simple steady system with two boundaries and a single orifice.

- 02_tank_blowdown_transient.yaml
  Fixed-component blowdown transient (single control volume + outlet junction).

- 03_simple_vent_transient.yaml
  Supported baseline transient: single CV venting to atmosphere.

- 04_two_cv_series_vent_transient.yaml
  Supported benchmark: two control volumes in series to atmosphere (fixed topology).

- 05_two_cv_pipe_vent_transient.yaml
  Supported benchmark: tank + buffer control volume with fixed feed and outlet components.

- 06_two_cv_junction_vent_transient.yaml
  Junction-heavy fixed-topology multi-CV stress case (experimental).

- 03_turbopump_demo.yaml
  Demo system with a pump and turbine in series between supply and exhaust boundaries.
  This mirrors turbopump hardware; explicit shaft coupling is modeled in the simulator layer.

- unsupported/02_tank_blowdown_scheduled.yaml
  Explicitly unsupported timed valve schedule case (validation should reject).
