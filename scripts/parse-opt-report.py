#!/usr/bin/env python3
"""Parse one MT5 optimization report (SpreadsheetML .xml) into a compact result.json.

Exit codes:
  0  report fresh and contains at least one pass
  4  report missing/unreadable or STALE (Created timestamp before this run started)
  5  report fresh but 0 passes (search never produced a result)
"""
import argparse
import json
import os
import sys
import xml.etree.ElementTree as ET
from datetime import datetime, timezone


def local(tag):
    return tag.split("}")[-1]


def find_created(root):
    for el in root.iter():
        if local(el.tag) == "Created" and el.text:
            txt = el.text.strip()
            for fmt in ("%Y-%m-%dT%H:%M:%SZ", "%Y-%m-%dT%H:%M:%S"):
                try:
                    return datetime.strptime(txt, fmt)
                except ValueError:
                    continue
    return None


def parse_rows(root):
    header = None
    rows = []
    for row in root.iter():
        if local(row.tag) != "Row":
            continue
        cells = []
        for cell in row:
            if local(cell.tag) != "Cell":
                continue
            val = None
            for data in cell:
                if local(data.tag) == "Data":
                    val = (data.text or "").strip()
                    break
            cells.append(val)
        if not cells:
            continue
        if header is None:
            header = cells
            continue
        if len(cells) != len(header):
            continue
        rec = dict(zip(header, cells))
        try:
            rec["Pass"] = int(float(rec.get("Pass")))
        except (TypeError, ValueError):
            continue
        rows.append(rec)
    return rows


def num(v):
    try:
        return float(v)
    except (TypeError, ValueError):
        return None


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("--report", required=True, help="path to report.xml in the run dir")
    ap.add_argument("--run-dir", required=True)
    ap.add_argument("--start-epoch", type=int, required=True, help="unix time when the tester was launched")
    ap.add_argument("--max-passes", type=int, default=0, help="server-side cap; fail if exceeded")
    a = ap.parse_args()

    out_path = os.path.join(a.run_dir, "result.json")

    def fail(status, error):
        result = {"kind": "optimization", "status": error, "error": error, "report": os.path.basename(a.report)}
        with open(out_path, "w") as f:
            json.dump(result, f, indent=2)
        print("error: " + error, file=sys.stderr)
        return status

    if not os.path.isfile(a.report):
        return fail(4, "optimization report missing: terminal produced no report file")
    try:
        root = ET.parse(a.report).getroot()
    except ET.ParseError:
        return fail(4, "optimization report is not valid XML (truncated or empty write?)")

    created = find_created(root)
    if created is None:
        return fail(4, "optimization report has no Created timestamp; cannot prove freshness")
    started = datetime.fromtimestamp(a.start_epoch, timezone.utc).replace(tzinfo=None)
    if created < started.replace(microsecond=0) and (started - created).total_seconds() > 300:
        return fail(4, "optimization report is STALE: created %s before run start %s" % (created, started))

    rows = parse_rows(root)
    if not rows:
        return fail(5, "optimization report contains 0 passes (no data in range, EA init failed, or no optimization ran)")
    if a.max_passes and len(rows) > a.max_passes:
        return fail(5, "passes %d exceed declared max_passes %d" % (len(rows), a.max_passes))

    for r in rows:
        r["Result_num"] = num(r.get("Result"))
    best_rows = [r for r in rows if r["Result_num"] is not None]
    if not best_rows:
        return fail(5, "no numeric Result column values in optimization report")
    best = max(best_rows, key=lambda r: r["Result_num"])
    top = sorted(best_rows, key=lambda r: r["Result_num"], reverse=True)[:20]

    def clean(rec):
        return {k: v for k, v in rec.items() if k != "Result_num"}

    result = {
        "kind": "optimization",
        "status": "success",
        "report_created": created.isoformat() + "Z",
        "passes_count": len(rows),
        "criterion": num(best.get("Result")),
        "best_pass": clean(best),
        "top": [clean(r) for r in top],
    }
    with open(out_path, "w") as f:
        json.dump(result, f, indent=2)
    print("Optimization: %d passes; best result %s (pass %s); result.json written" % (
        len(rows), best.get("Result"), best.get("Pass")))
    return 0


if __name__ == "__main__":
    sys.exit(main())
