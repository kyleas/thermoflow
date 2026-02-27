#!/usr/bin/env python3
"""Compare before and after benchmark results."""

import json
import sys

def load_baseline(path):
    with open(path, 'r') as f:
        return json.load(f)

def compare_baselines(before_path, after_path):
    before = load_baseline(before_path)
    after = load_baseline(after_path)
    
    print("=" * 80)
    print("BENCHMARK PERFORMANCE COMPARISON")
    print("=" * 80)
    print()
    
    for b_result, a_result in zip(before['results'], after['results']):
        name = b_result['scenario']['name']
        
        b_total = b_result['aggregate']['total_time_median_s']
        a_total = a_result['aggregate']['total_time_median_s']
        b_solve = b_result['aggregate']['solve_time_median_s']
        a_solve = a_result['aggregate']['solve_time_median_s']
        
        # Get surrogate populations from first run
        b_surr = b_result['runs'][0].get('transient_surrogate_populations', 0)
        a_surr = a_result['runs'][0].get('transient_surrogate_populations', 0)
        
        # Handle None values (steady-state doesn't have surrogate populations)
        b_surr = b_surr if b_surr is not None else 0
        a_surr = a_surr if a_surr is not None else 0
        
        total_improvement = ((b_total - a_total) / b_total) * 100
        solve_improvement = ((b_solve - a_solve) / b_solve) * 100
        surr_reduction = ((b_surr - a_surr) / max(b_surr, 1)) * 100
        
        print(f"📊 {name}")
        print(f"   Total time:  {b_total:.3f}s → {a_total:.3f}s  ({total_improvement:+.1f}% {get_arrow(total_improvement)})")
        print(f"   Solve time:  {b_solve:.3f}s → {a_solve:.3f}s  ({solve_improvement:+.1f}% {get_arrow(solve_improvement)})")
        
        if b_surr > 0:
            print(f"   Surrogate populations: {b_surr} → {a_surr}  ({surr_reduction:.0f}% reduction)")
        print()

def get_arrow(improvement):
    if improvement > 0:
        return "⬇️"  # Faster (time decreased)
    elif improvement < 0:
        return "⬆️"  # Slower (time increased)
    else:
        return "→"

if __name__ == "__main__":
    before = "benchmarks/baseline_before_opt.json"
    after = "benchmarks/baseline.json"
    
    if len(sys.argv) > 2:
        before = sys.argv[1]
        after = sys.argv[2]
    
    compare_baselines(before, after)
