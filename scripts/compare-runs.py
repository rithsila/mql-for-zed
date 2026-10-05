#!/usr/bin/env python3
"""Compare two backtest runs by providing their run IDs or directory paths."""

import argparse
import json
import os
import sys

def load_run(run_path):
    manifest_path = os.path.join(run_path, "manifest.json")
    result_path = os.path.join(run_path, "result.json")
    if not os.path.isfile(manifest_path) or not os.path.isfile(result_path):
        return None
    with open(manifest_path, "r") as f:
        manifest = json.load(f)
    with open(result_path, "r") as f:
        result = json.load(f)
    
    inputs_path = os.path.join(run_path, "inputs.set")
    inputs = {}
    if os.path.isfile(inputs_path):
        with open(inputs_path, "r", encoding="utf-16-le", errors="ignore") as f:
            for line in f:
                line = line.strip()
                if not line or line.startswith(";"):
                    continue
                parts = line.split("=", 1)
                if len(parts) == 2:
                    k, v = parts[0], parts[1].split("||")[0]
                    inputs[k] = v

    return {"manifest": manifest, "result": result, "inputs": inputs, "path": run_path}

def compare_runs(run1, run2):
    m1, m2 = run1["manifest"], run2["manifest"]
    r1, r2 = run1["result"], run2["result"]
    i1, i2 = run1["inputs"], run2["inputs"]

    print(f"Comparing runs:")
    print(f"  Run 1: {m1.get('run_id')} ({m1.get('timestamp')})")
    print(f"  Run 2: {m2.get('run_id')} ({m2.get('timestamp')})")
    print("-" * 50)

    # Check material conditions
    c1, c2 = m1.get("conditions", {}), m2.get("conditions", {})
    warnings = []
    for k in ["symbol", "period", "model", "from_date", "to_date", "deposit", "currency", "leverage"]:
        if c1.get(k) != c2.get(k):
            warnings.append(f"{k} differs: {c1.get(k)} vs {c2.get(k)}")
    
    if warnings:
        print("WARNING: Material test conditions differ between runs!")
        for w in warnings:
            print(f"  - {w}")
        print("-" * 50)

    # Compare inputs
    print("Inputs comparison:")
    all_keys = sorted(set(i1.keys()) | set(i2.keys()))
    diff_inputs = False
    for k in all_keys:
        v1, v2 = i1.get(k), i2.get(k)
        if v1 != v2:
            print(f"  {k}: {v1} -> {v2}")
            diff_inputs = True
    if not diff_inputs:
        print("  (No input differences)")
    print("-" * 50)

    # Compare metrics
    print("Metrics comparison (Run 1 -> Run 2):")
    if r1.get("status") != "success" or r2.get("status") != "success":
        print(f"  Run 1 status: {r1.get('status')} {r1.get('error', '')}")
        print(f"  Run 2 status: {r2.get('status')} {r2.get('error', '')}")
        return

    metrics = ["net_profit", "equity_dd", "trades", "recovery_factor"]
    m_dict1 = r1.get("metrics", {})
    m_dict2 = r2.get("metrics", {})

    for m in metrics:
        v1 = m_dict1.get(m, {}).get("value")
        v2 = m_dict2.get(m, {}).get("value")
        if v1 is None and v2 is None:
            print(f"  {m:15s} : Not available in both")
        else:
            v1_str = f"{v1:.2f}" if isinstance(v1, float) else str(v1)
            v2_str = f"{v2:.2f}" if isinstance(v2, float) else str(v2)
            print(f"  {m:15s} : {v1_str:>10s} -> {v2_str:>10s}")

def find_run_path(run_arg):
    if os.path.isdir(run_arg):
        return run_arg
    
    # Try finding in .mql/runs
    possible_dir = os.path.join(".mql", "runs", run_arg)
    if os.path.isdir(possible_dir):
        return possible_dir
    
    return None

if __name__ == "__main__":
    parser = argparse.ArgumentParser(description="Compare two MQL5 tester runs.")
    parser.add_argument("run1", help="First run ID or path")
    parser.add_argument("run2", help="Second run ID or path")
    args = parser.parse_args()

    p1 = find_run_path(args.run1)
    p2 = find_run_path(args.run2)

    if not p1:
        print(f"Error: Could not find run '{args.run1}'")
        sys.exit(1)
    if not p2:
        print(f"Error: Could not find run '{args.run2}'")
        sys.exit(1)

    r1 = load_run(p1)
    r2 = load_run(p2)

    if not r1:
        print(f"Error: Run 1 '{p1}' is missing manifest.json or result.json")
        sys.exit(1)
    if not r2:
        print(f"Error: Run 2 '{p2}' is missing manifest.json or result.json")
        sys.exit(1)

    compare_runs(r1, r2)
