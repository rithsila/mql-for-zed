import argparse
import csv
import json
import math
import os
import random
import sys

def load_trades(deals_csv):
    """
    Groups deals into trades by position_id.
    Returns a list of trades, where each trade has a net PnL and a closing time.
    """
    positions = {}
    
    with open(deals_csv, newline="", encoding="utf-8", errors="replace") as f:
        reader = csv.DictReader(f)
        if "position_id" not in reader.fieldnames:
            print("Error: deals.csv does not contain 'position_id'. Recompile with updated ZedMqlStats.mqh.")
            sys.exit(1)
            
        for row in reader:
            pos_id = row.get("position_id", "0")
            if pos_id == "0":
                continue # not part of a position
                
            net = sum(float(row[k]) for k in ("profit", "swap", "commission", "fee"))
            t = int(row["time"])
            
            if pos_id not in positions:
                positions[pos_id] = {"net": 0.0, "time": t, "deals": 0}
            
            positions[pos_id]["net"] += net
            # Keep the time of the latest deal as the trade's closing time
            if t > positions[pos_id]["time"]:
                positions[pos_id]["time"] = t
            positions[pos_id]["deals"] += 1

    # Return a list of trades sorted by closing time
    trades = list(positions.values())
    trades.sort(key=lambda x: x["time"])
    return trades

def simulate_path(trades, initial_balance):
    """
    Simulates one equity path given a sequence of trades.
    Returns (final_balance, max_drawdown_pct).
    """
    balance = initial_balance
    peak = initial_balance
    max_dd_pct = 0.0
    
    for t in trades:
        balance += t["net"]
        if balance > peak:
            peak = balance
        else:
            if peak > 0:
                dd_pct = (peak - balance) / peak * 100.0
                if dd_pct > max_dd_pct:
                    max_dd_pct = dd_pct
        
        # Ruin condition
        if balance <= 0:
            return (0, 100.0)
            
    return (balance, max_dd_pct)

def main():
    parser = argparse.ArgumentParser(description="Monte Carlo robustness analysis for MT5 backtest runs.")
    parser.add_argument("--run-dir", required=True, help="Path to the saved run directory")
    parser.add_argument("--iterations", type=int, default=1000, help="Number of Monte Carlo iterations")
    parser.add_argument("--json", action="store_true", help="Output JSON instead of text")
    parser.add_argument("--seed", type=int, help="Random seed for reproducibility")
    args = parser.parse_args()

    deals_csv = os.path.join(args.run_dir, "deals.csv")
    manifest_file = os.path.join(args.run_dir, "manifest.json")
    
    if not os.path.isfile(deals_csv):
        print(f"Error: deals.csv not found in {args.run_dir}")
        sys.exit(1)
        
    if args.seed is not None:
        random.seed(args.seed)
    initial_balance = 10000.0
    if os.path.isfile(manifest_file):
        with open(manifest_file, "r", encoding="utf-8") as f:
            manifest = json.load(f)
            initial_balance = float(manifest.get("conditions", {}).get("deposit", 10000.0))

    trades = load_trades(deals_csv)
    
    if len(trades) < 2:
        print("Not enough trades for Monte Carlo analysis.")
        sys.exit(0)

    # Calculate baseline
    base_final, base_dd = simulate_path(trades, initial_balance)

    # Monte Carlo simulation
    results = []
    trade_pool = trades
    pool_size = len(trade_pool)
    
    for _ in range(args.iterations):
        # Bootstrap resampling (with replacement)
        resampled = [random.choice(trade_pool) for _ in range(pool_size)]
        final_bal, dd_pct = simulate_path(resampled, initial_balance)
        results.append({"balance": final_bal, "dd_pct": dd_pct})

    # Sort results to find percentiles
    results.sort(key=lambda x: x["balance"])
    bals = [r["balance"] for r in results]
    
    results.sort(key=lambda x: x["dd_pct"])
    dds = [r["dd_pct"] for r in results]

    def p(arr, pct):
        idx = min(len(arr) - 1, int(len(arr) * pct / 100.0))
        return arr[idx]

    stats = {
        "trades": pool_size,
        "iterations": args.iterations,
        "baseline": {
            "balance": base_final,
            "drawdown_pct": base_dd
        },
        "percentiles": {
            "balance": {
                "5": p(bals, 5),
                "25": p(bals, 25),
                "50": p(bals, 50),
                "75": p(bals, 75),
                "95": p(bals, 95)
            },
            "drawdown_pct": {
                "5": p(dds, 5),
                "25": p(dds, 25),
                "50": p(dds, 50),
                "75": p(dds, 75),
                "95": p(dds, 95)
            }
        }
    }

    if args.json:
        print(json.dumps(stats, indent=2))
    else:
        print(f"--- Monte Carlo Robustness Analysis ---")
        print(f"Trades analyzed : {pool_size}")
        print(f"Iterations      : {args.iterations}")
        print(f"Initial Deposit : {initial_balance:.2f}")
        print()
        print(f"Baseline Final Balance : {base_final:.2f}")
        print(f"Baseline Max Drawdown  : {base_dd:.2f}%")
        print()
        print(f"Final Balance Distribution:")
        print(f"  5th percentile  : {stats['percentiles']['balance']['5']:.2f}")
        print(f"  25th percentile : {stats['percentiles']['balance']['25']:.2f}")
        print(f"  Median (50th)   : {stats['percentiles']['balance']['50']:.2f}")
        print(f"  75th percentile : {stats['percentiles']['balance']['75']:.2f}")
        print(f"  95th percentile : {stats['percentiles']['balance']['95']:.2f}")
        print()
        print(f"Max Drawdown Distribution:")
        print(f"  5th percentile  : {stats['percentiles']['drawdown_pct']['5']:.2f}% (Best case)")
        print(f"  25th percentile : {stats['percentiles']['drawdown_pct']['25']:.2f}%")
        print(f"  Median (50th)   : {stats['percentiles']['drawdown_pct']['50']:.2f}%")
        print(f"  75th percentile : {stats['percentiles']['drawdown_pct']['75']:.2f}%")
        print(f"  95th percentile : {stats['percentiles']['drawdown_pct']['95']:.2f}% (Worst case)")

if __name__ == "__main__":
    main()
