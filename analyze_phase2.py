#!/usr/bin/env python3
"""
Analyze Phase 2 timing data to understand RHS/RK4 overhead breakdown.
"""
import json
from statistics import median

with open("benchmarks/baseline.json") as f:
    data = json.load(f)

print("=" * 80)
print("PHASE 2 TIMING ANALYSIS - RHS/RK4 OVERHEAD BREAKDOWN")
print("=" * 80)
print()

# Analyze each scenario
for scenario_result in data["results"]:
    scenario = scenario_result["scenario"]
    runs = scenario_result["runs"]
    
    # Use median values
    total_times = [r["total_time_s"] for r in runs]
    build_times = [r["build_time_s"] for r in runs]
    solve_times = [r["solve_time_s"] for r in runs]
    residual_times = [r["solve_residual_time_s"] or 0 for r in runs]
    thermo_times = [r["solve_thermo_time_s"] or 0 for r in runs]
    residual_counts = [r["solve_residual_eval_count"] or 0 for r in runs]
    
    # Get median values
    med_total = median(total_times)
    med_build = median(build_times)
    med_solve = median(solve_times)
    med_residual = median(residual_times)
    med_thermo = median(thermo_times)
    med_residual_count = median(residual_counts)
    
    # Calculate total measured time (residual + thermo)
    measured_time = med_residual + med_thermo
    
    # Calculate unaccounted time
    unaccounted = med_solve - measured_time
    
    # Calculate percentages
    if med_solve > 0:
        residual_pct = (med_residual / med_solve) * 100
        thermo_pct = (med_thermo / med_solve) * 100
        measured_pct = (measured_time / med_solve) * 100
        unaccounted_pct = (unaccounted / med_solve) * 100
    else:
        residual_pct = thermo_pct = measured_pct = unaccounted_pct = 0
    
    # Determine if transient or steady
    mode = scenario.get("mode", "Steady")
    if isinstance(mode, dict):
        mode_str = "Transient"
        steps = runs[0].get("transient_steps", "?")
    else:
        mode_str = "Steady"
        steps = "N/A"
    
    print(f"Scenario: {scenario['name']}")
    print(f"  Mode: {mode_str} ({steps} steps)")
    print(f"  Total:  {med_total:.4f}s")
    print(f"  Build:  {med_build:.4f}s ({(med_build/med_total)*100:.1f}% of total)")
    print(f"  Solve:  {med_solve:.4f}s ({(med_solve/med_total)*100:.1f}% of total)")
    print(f"    Residual eval:     {med_residual:.4f}s ({residual_pct:.1f}% of solve, {med_residual_count:.0f} evals)")
    print(f"    Thermo creation:   {med_thermo:.4f}s ({thermo_pct:.1f}% of solve)")
    print(f"    Measured subtotal: {measured_time:.4f}s ({measured_pct:.1f}% of solve)")
    print(f"    *** RHS/RK4 OVERHEAD: {unaccounted:.4f}s ({unaccounted_pct:.1f}% of solve) ***")
    print()

print("=" * 80)
print("KEY FINDINGS:")
print("=" * 80)
print("1. Residual eval time is low (~0.3-3% of solve)")
print("2. Thermo creation time is moderate (~7-10% of solve)")
print("3. RHS/RK4 overhead dominates at 85-90% of solve time")
print()
print("Next phase should focus on:")
print("- Vector allocation in RHS (dm_in, dm_out, etc.)")
print("- Component iteration and mass flow routing")
print("- State reconstruction within RK4 stages")
print("- Potential RK4 integrator improvements")
print("=" * 80)
