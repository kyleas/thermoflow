import json
from pathlib import Path

repo = Path(__file__).resolve().parents[1]
new_path = repo / "benchmarks" / "baseline.json"
old_path = repo / "benchmarks" / "baseline_pre_rhs_opt.json"

with open(new_path, "r", encoding="utf-8") as f:
    new_data = json.load(f)
with open(old_path, "r", encoding="utf-8") as f:
    old_data = json.load(f)

old_by_id = {entry["scenario"]["id"]: entry for entry in old_data["results"]}
new_by_id = {entry["scenario"]["id"]: entry for entry in new_data["results"]}

transient_ids = ["03_transient", "04_transient", "05_transient", "07_transient", "08_transient"]

print("=== BEFORE vs AFTER (median) ===")
for sid in transient_ids:
    old = old_by_id[sid]["aggregate"]
    new = new_by_id[sid]["aggregate"]
    old_total = old["total_time_median_s"]
    new_total = new["total_time_median_s"]
    old_solve = old["solve_time_median_s"]
    new_solve = new["solve_time_median_s"]
    total_pct = (old_total - new_total) / old_total * 100.0
    solve_pct = (old_solve - new_solve) / old_solve * 100.0
    print(f"{sid}: total {old_total:.4f}s -> {new_total:.4f}s ({total_pct:+.1f}%), solve {old_solve:.4f}s -> {new_solve:.4f}s ({solve_pct:+.1f}%)")

print("\n=== RHS SUBPHASE BREAKDOWN (AFTER, transient medians) ===")
for sid in transient_ids:
    entry = new_by_id[sid]
    agg = entry["aggregate"]
    solve = agg["solve_time_median_s"]
    name = entry["scenario"]["name"]
    snapshot = agg.get("rhs_snapshot_time_median_s") or 0.0
    state = agg.get("rhs_state_reconstruct_time_median_s") or 0.0
    buffers = agg.get("rhs_buffer_init_time_median_s") or 0.0
    routing = agg.get("rhs_flow_routing_time_median_s") or 0.0
    cv = agg.get("rhs_cv_derivative_time_median_s") or 0.0
    lv = agg.get("rhs_lv_derivative_time_median_s") or 0.0
    assembly = agg.get("rhs_assembly_time_median_s") or 0.0
    surrogate = agg.get("rhs_surrogate_time_median_s") or 0.0
    rk4_bookkeeping = agg.get("rk4_bookkeeping_time_median_s") or 0.0
    calls = agg.get("rhs_calls_median") or 0

    def pct(v):
        return (v / solve * 100.0) if solve > 0 else 0.0

    print(f"\n{name} ({sid}) | solve={solve:.4f}s | rhs_calls={calls}")
    print(f"  snapshot:    {snapshot:.4f}s ({pct(snapshot):.1f}%)")
    print(f"  state:       {state:.4f}s ({pct(state):.1f}%)")
    print(f"  buffers:     {buffers:.4f}s ({pct(buffers):.3f}%)")
    print(f"  routing:     {routing:.4f}s ({pct(routing):.3f}%)")
    print(f"  cv deriv:    {cv:.4f}s ({pct(cv):.3f}%)")
    print(f"  lv deriv:    {lv:.4f}s ({pct(lv):.3f}%)")
    print(f"  assembly:    {assembly:.4f}s ({pct(assembly):.3f}%)")
    print(f"  surrogate*:  {surrogate:.4f}s ({pct(surrogate):.1f}%)")
    print(f"  rk4+other:   {rk4_bookkeeping:.4f}s ({pct(rk4_bookkeeping):.3f}%)")

print("\n*surrogate is a subcomponent measured inside snapshot work.")
