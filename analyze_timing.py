import json

with open('benchmarks/baseline.json') as f:
    data = json.load(f)

print('=== TRANSIENT TIMING BREAKDOWN ===\n')
for result in data['results']:
    scenario = result['scenario']
    if 'Transient' not in str(scenario.get('mode', {})): 
        continue
    
    runs = result['runs']
    solve_times = [r['solve_time_s'] for r in runs]
    thermo_times = [r.get('solve_thermo_time_s', 0) or 0 for r in runs]
    residual_times = [r.get('solve_residual_time_s', 0) or 0 for r in runs]
    
    median_solve = sorted(solve_times)[len(solve_times)//2]
    median_thermo = sorted(thermo_times)[len(thermo_times)//2]
    median_residual = sorted(residual_times)[len(residual_times)//2]
    
    instrumented = median_thermo + median_residual
    unaccounted = median_solve - instrumented
    
    print(f"{scenario['id']}: {scenario['name']}")
    print(f"  Total solve:      {median_solve:6.2f}s  (100%)")
    print(f"  Thermo:           {median_thermo:6.3f}s  ({100*median_thermo/median_solve:5.1f}%)")
    print(f"  Mass flow:        {median_residual:6.3f}s  ({100*median_residual/median_solve:5.1f}%)")
    print(f"  ODE + overhead:   {unaccounted:6.2f}s  ({100*unaccounted/median_solve:5.1f}%)")
    print()
