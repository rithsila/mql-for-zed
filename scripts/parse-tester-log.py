#!/usr/bin/env python3
"""Summarise one MT5 Strategy Tester run from the terminal log and tester log (only the bytes written during the run)."""
import argparse
import re
import sys


def segment(path, offset):
    try:
        with open(path, "rb") as f:
            head = f.read(200)
            wide = b"\x00" in head
            f.seek(offset - offset % 2 if wide else offset)
            data = f.read()
    except OSError:
        return []
    return data.decode("utf-16-le" if wide else "utf-8", "replace").splitlines()


def message(line):
    parts = line.split("\t")
    return parts[-1].strip() if len(parts) >= 5 else line.strip()


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("--terminal-log", required=True)
    ap.add_argument("--terminal-offset", type=int, default=0)
    ap.add_argument("--tester-log", required=True)
    ap.add_argument("--tester-offset", type=int, default=0)
    ap.add_argument("--deposit", type=float, default=0)
    ap.add_argument("--currency", default="")
    ap.add_argument("--save-tester-segment")
    a = ap.parse_args()

    term = [message(l) for l in segment(a.terminal_log, a.terminal_offset)]
    raw_test = segment(a.tester_log, a.tester_offset)
    if a.save_tester_segment:
        with open(a.save_tester_segment, "w") as f:
            f.write("\n".join(raw_test) + "\n")
    test = [message(l) for l in raw_test]

    for m in term:
        if re.search(r"tester (not started|didn't start)|tester .*does not exist|account is not specified|shutdown with -", m, re.I):
            print(f"FAIL: {m}")
    failed = [m for m in term if re.search(r"tester (not started|didn't start)", m, re.I)]
    if failed:
        return 1

    final = [re.search(r"final balance\s+([-\d.]+)\s*(\w*)", m) for m in test]
    final = [m for m in final if m]
    finished = [m for m in term if "last test passed" in m or "last test failed" in m]
    if not final:
        print("FAIL: no 'final balance' in tester log" + (f" ({finished[-1]})" if finished else " (tester did not run)"))
        for m in [m for m in test if re.search(r"error|fail|cannot|not found|critical", m, re.I)][-8:]:
            print(f"  {m}")
        return 1

    bal = float(final[-1].group(1))
    cur = final[-1].group(2) or a.currency
    deals = sum(1 for m in test if re.search(r"\bdeal #\d+ (buy|sell) .* done", m))
    ontester = next((m.split("result")[-1].strip() for m in reversed(test) if m.startswith("OnTester result")), None)
    took = next((re.search(r"Test passed in (\S+)", m).group(1) for m in reversed(test) if re.search(r"Test passed in \S+", m)), None)
    mem = next((m for m in reversed(test) if re.search(r"ticks.*bars generated", m)), None)

    print("---- Strategy Tester summary ----")
    if a.deposit:
        net = bal - a.deposit
        print(f"Deposit       : {a.deposit:.2f} {cur}")
        print(f"Final balance : {bal:.2f} {cur}  (net {net:+.2f}, {net / a.deposit * 100:+.2f}%)")
    else:
        print(f"Final balance : {bal:.2f} {cur}")
    print(f"Deals         : {deals}")
    if ontester:
        print(f"OnTester      : {ontester}")
    if took:
        print(f"Test time     : {took}")
    if mem:
        print(f"Data          : {mem.split(': ', 1)[-1].split('. ')[0]}")
    print("Not shown     : drawdown, profit factor (needs the HTML report; see README)")
    return 0


if __name__ == "__main__":
    sys.exit(main())
