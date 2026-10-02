#!/usr/bin/env python3
"""Parse a MetaEditor compile log and re-emit POSIX-clickable diagnostics.

Usage: parse-mql-log.py <log> [--vm-root <win MQL5 root> --local-root <posix MQL5 root>]
Env fallbacks: MQL_VM_MQL5_ROOT, MQL_LOCAL_MQL5_ROOT.
"""
import argparse, os, pathlib, re, sys

DEFAULT_PREFIX = pathlib.Path.home() / "Library/Application Support/net.metaquotes.wine.metatrader5"

ap = argparse.ArgumentParser()
ap.add_argument("log")
ap.add_argument("--map", action="append", default=[], metavar="VM_DIR=LOCAL_DIR")
ap.add_argument("--vm-root", default=os.environ.get("MQL_VM_MQL5_ROOT"))
ap.add_argument("--local-root", default=os.environ.get("MQL_LOCAL_MQL5_ROOT"))
args = ap.parse_args()
MAPS = [tuple(m.split("=", 1)) for m in args.map if "=" in m]
if args.vm_root and args.local_root:
    MAPS.append((args.vm_root, args.local_root))
MAPS = sorted(((v.replace("\\", "/").rstrip("/"), l.rstrip("/")) for v, l in MAPS), key=lambda m: -len(m[0]))

raw = pathlib.Path(args.log).read_bytes()
if raw[:2] == b"\xff\xfe":
    text = raw[2:].decode("utf-16-le", errors="replace")
elif raw[:2] == b"\xfe\xff":
    text = raw[2:].decode("utf-16-be", errors="replace")
elif raw[:3] == b"\xef\xbb\xbf":
    text = raw[3:].decode("utf-8", errors="replace")
else:
    text = raw.decode("utf-8", errors="replace")
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


DIAG = re.compile(r"^(?P<path>.+?)(?:\((?P<ln>\d+),(?P<col>\d+)\))?\s*:\s*(?P<lvl>error|warning)\s*(?:\d+)?\s*:\s*(?P<msg>.+)$", re.I)
RESULT = re.compile(r"(\d+)\s+errors?,\s*(\d+)\s+warnings?", re.I)

errors = 0
summary_errors = None
for line in lines:
    s = line.strip().lstrip("\ufeff")
    if not s:
        continue
    m = DIAG.match(s)
    if m and re.search(r"[\\/]|\.mq[h45]$", m["path"], re.I):
        lvl = m["lvl"].lower()
        print(f"{win_to_posix(m['path'])}:{m['ln'] or 1}:{m['col'] or 1}: {lvl}: {m['msg']}")
        errors += lvl == "error"
        continue
    r = RESULT.search(s)
    if r:
        summary_errors = int(r.group(1))
        print(f"Result: {r.group(1)} errors, {r.group(2)} warnings")

sys.exit(0 if (summary_errors if summary_errors is not None else errors) == 0 else 1)
