#!/usr/bin/env python3
"""Post-run evidence check for one MT5 tester run.

Reads only the tester-log bytes written during the run (from --offset), extracts
real history coverage for the tested symbol, compares it with the requested date
range, merges the evidence into <run-dir>/manifest.json, and classifies the run.

Exit codes:
  0  data present for (part of) the requested range
  3  NO DATA: cached bars do not reach the requested range (stale/missing feed)
  4  evidence unreadable (no tester log at all)

This exists because MT5 happily tests a range with zero bars and still reports a
normal finish. A cent symbol whose feed stopped (for example when the VM terminal
login was switched away from Lirunex) must be reported as missing data, never as
a valid result.
"""
import argparse
import json
import os
import re
import sys
from datetime import datetime

BARS_RE = re.compile(
    "(?P<sym>[A-Za-z0-9._-]+(,[A-Za-z0-9]+)?): history cache allocated for [0-9]+ bars "
    "and contains (?P<bars>[0-9]+) bars from (?P<from>[0-9 .:]+) to (?P<to>[0-9 .:]+)"
)
BEGINS_RE = re.compile(
    "(?P<sym>[A-Za-z0-9._-]+(,[A-Za-z0-9]+)?): history begins from (?P<from>[0-9 .:]+)"
)
QUALITY_RE = re.compile("quality of analyzed history is ([0-9]+)%")


def read_segment(path, offset):
    try:
        with open(path, "rb") as f:
            head = f.read(200)
            wide = any(byte == 0 for byte in head)
            f.seek(offset - offset % 2 if wide else offset)
            data = f.read()
    except OSError:
        return []
    # MT5 logs may contain bare CR inside timestamps; split only on LF
    return data.decode("utf-16-le" if wide else "utf-8", "replace").replace(chr(13), "").split(chr(10))


def msg(line):
    parts = line.split(chr(9))
    return parts[-1].strip() if len(parts) >= 5 else line.strip()


def sym_of(name):
    return name.split(",")[0]


def parse_dt(text):
    text = " ".join(text.split())
    for fmt in ("%Y.%m.%d %H:%M:%S", "%Y.%m.%d %H:%M", "%Y.%m.%d"):
        try:
            return datetime.strptime(text, fmt)
        except ValueError:
            continue
    return None


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("--run-dir", required=True)
    ap.add_argument("--tester-log", required=True)
    ap.add_argument("--offset", type=int, default=0)
    ap.add_argument("--symbol", required=True)
    ap.add_argument("--from", dest="dfrom", required=True, help="YYYY.MM.DD")
    ap.add_argument("--to", dest="dto", required=True, help="YYYY.MM.DD")
    a = ap.parse_args()

    if not os.path.isfile(a.tester_log):
        print("error: tester log missing; cannot prove any data was tested", file=sys.stderr)
        return 4

    coverage = {
        "symbol": a.symbol,
        "requested_from": a.dfrom,
        "requested_to": a.dto,
        "cached_bars": 0,
        "bars_from": None,
        "bars_to": None,
        "history_begin": None,
        "quality_pct": None,
    }

    for line in read_segment(a.tester_log, a.offset):
        b = BARS_RE.search(line)
        if b and sym_of(b.group("sym")) == a.symbol:
            coverage["cached_bars"] = max(coverage["cached_bars"], int(b.group("bars")))
            lo, hi = b.group("from").strip(), b.group("to").strip()
            if coverage["bars_from"] is None or lo < coverage["bars_from"]:
                coverage["bars_from"] = lo
            if coverage["bars_to"] is None or hi > coverage["bars_to"]:
                coverage["bars_to"] = hi
            continue
        g = BEGINS_RE.search(line)
        if g and sym_of(g.group("sym")) == a.symbol and coverage["history_begin"] is None:
            coverage["history_begin"] = g.group("from").strip()
            continue
        q = QUALITY_RE.search(line)
        if q:
            coverage["quality_pct"] = int(q.group(1))

    gen_bars = 0
    gen_re = re.compile(r"(?P<sym>[A-Za-z0-9._-]+)(,[A-Za-z0-9]+)?: [0-9]+ ticks, (?P<bars>[1-9][0-9]*) bars generated")
    for line in read_segment(a.tester_log, a.offset):
        m = gen_re.search(line)
        if m and sym_of(m.group("sym")) == a.symbol:
            gen_bars = max(gen_bars, int(m.group("bars")))

    bars_to = parse_dt(coverage["bars_to"] or "")
    req_from = parse_dt(a.dfrom)
    close_enough = bool(bars_to and req_from and (bars_to >= req_from or (req_from - bars_to).total_seconds() <= 86400))
    data_ok = bool((coverage["cached_bars"] > 0 and close_enough) or gen_bars > 0)
    coverage["has_data_for_requested_range"] = data_ok

    manifest_path = os.path.join(a.run_dir, "manifest.json")
    manifest = {}
    if os.path.isfile(manifest_path):
        try:
            with open(manifest_path) as f:
                manifest = json.load(f)
        except (OSError, json.JSONDecodeError):
            manifest = {}
    manifest.setdefault("schema_version", 1)
    manifest["data_coverage"] = coverage
    tmp = manifest_path + ".tmp"
    with open(tmp, "w") as f:
        json.dump(manifest, f, indent=2)
    os.replace(tmp, manifest_path)

    print("Data coverage: %s bars %s from %s to %s (requested %s..%s, quality %s%%)" % (
        a.symbol, coverage["cached_bars"], coverage["bars_from"], coverage["bars_to"],
        a.dfrom, a.dto, coverage["quality_pct"]))
    if not data_ok:
        print("error: NO DATA for the requested range: cached %s bars end at %s but the test starts at %s. "
              "The symbol feed likely stopped (wrong broker login or delisted cent symbol). "
              "This result must not be used." % (a.symbol, coverage["bars_to"], a.dfrom),
              file=sys.stderr)
        return 3
    return 0


if __name__ == "__main__":
    sys.exit(main())
