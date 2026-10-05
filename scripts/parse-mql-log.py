#!/usr/bin/env python3
"""Parse a MetaEditor compile log and re-emit POSIX-clickable diagnostics.

Usage: parse-mql-log.py <log> [--vm-root <win MQL5 root> --local-root <posix MQL5 root>]
Env fallbacks: MQL_VM_MQL5_ROOT, MQL_LOCAL_MQL5_ROOT.
"""
import argparse
import json
import os
import pathlib
import re
import sys

DEFAULT_PREFIX = pathlib.Path.home() / "Library/Application Support/net.metaquotes.wine.metatrader5"

ap = argparse.ArgumentParser()
ap.add_argument("log")
ap.add_argument("--json", action="store_true")
ap.add_argument("--job-id", default="")
ap.add_argument("--snapshot", default="")
ap.add_argument("--map", action="append", default=[], metavar="VM_DIR=LOCAL_DIR")
ap.add_argument("--vm-root", default=os.environ.get("MQL_VM_MQL5_ROOT"))
ap.add_argument("--local-root", default=os.environ.get("MQL_LOCAL_MQL5_ROOT"))
args = ap.parse_args()
MAPS = [tuple(m.split("=", 1)) for m in args.map if "=" in m]
if args.vm_root and args.local_root:
    MAPS.append((args.vm_root, args.local_root))
MAPS = sorted(((v.replace("\\", "/").rstrip("/"), l.rstrip("/")) for v, l in MAPS), key=lambda m: -len(m[0]))

def emit(status: str, diagnostics: list, message=None) -> None:
    if args.json:
        print(json.dumps({"schema_version": 1, "job_id": args.job_id,
                          "source_snapshot": args.snapshot, "status": status,
                          "diagnostics": diagnostics, "message": message}))


try:
    raw = pathlib.Path(args.log).read_bytes()
    if raw[:2] == b"\xff\xfe":
        text = raw[2:].decode("utf-16-le")
    elif raw[:2] == b"\xfe\xff":
        text = raw[2:].decode("utf-16-be")
    elif raw[:3] == b"\xef\xbb\xbf":
        text = raw[3:].decode("utf-8")
    else:
        text = raw.decode("utf-8")
except (OSError, UnicodeError) as exc:
    message = f"cannot read compile log: {exc}"
    emit("log_failure", [], message)
    if not args.json:
        print(f"error: {message}", file=sys.stderr)
    sys.exit(2)
lines = text.replace("\r\n", "\n").replace("\r", "\n").splitlines()


def norm(p: str) -> str:
    return p.replace("\\", "/")


def win_to_posix(wpath: str) -> str:
    p = norm(wpath.strip())
    for vm, local in MAPS:
        if p.lower().startswith(vm.lower() + "/") or p.lower() == vm.lower():
            return local + p[len(vm):]
    if re.match(r"^[cC]:/", p):
        return str(DEFAULT_PREFIX / "drive_c" / p[3:])
    if re.match(r"^[zZ]:/", p):
        return p[2:]
    return p


DIAG = re.compile(r"^(?P<path>.+?)(?:\((?P<ln>\d+),(?P<col>\d+)\))?\s*:\s*(?P<lvl>error|warning)\s*(?P<code>\d+)?\s*:\s*(?P<msg>.+)$", re.IGNORECASE)
RESULT = re.compile(r"(\d+)\s+errors?,\s*(\d+)\s+warnings?", re.IGNORECASE)

errors = 0
summary_errors = None
diagnostics = []
for line in lines:
    s = line.strip().lstrip("\ufeff")
    if not s:
        continue
    m = DIAG.match(s)
    if m and re.search(r"[\\/]|\.mq[h45]$", m["path"], re.IGNORECASE):
        lvl = m["lvl"].lower()
        path = win_to_posix(m["path"])
        ln = max(1, int(m["ln"] or 1))
        col = max(1, int(m["col"] or 1))
        diagnostics.append({"uri": pathlib.Path(path).resolve().as_uri(), "severity": lvl,
                            "message": m["msg"], "line": ln, "col": col,
                            "code": m["code"]})
        if not args.json:
            print(f"{path}:{ln}:{col}: {lvl}: {m['msg']}")
        errors += lvl == "error"
        continue
    r = RESULT.search(s)
    if r:
        summary_errors = int(r.group(1))
        if not args.json:
            print(f"Result: {r.group(1)} errors, {r.group(2)} warnings")

if summary_errors is None:
    message = "compile log has no result summary"
    emit("log_failure", diagnostics, message)
    if not args.json:
        print(f"error: {message}", file=sys.stderr)
    sys.exit(2)
status = "compiler_errors" if summary_errors or errors else "success"
emit(status, diagnostics)
sys.exit(1 if status == "compiler_errors" else 0)
