# MQL Compile Feature — Implementation Plan

## Context

| Item | Value |
|---|---|
| Source of truth | `MQL5/Experts/` inside the MT5 Wine prefix |
| Wine prefix | `~/Library/Application Support/net.metaquotes.wine.metatrader5` |
| MetaEditor | `drive_c/Program Files/MetaTrader 5/MetaEditor64.exe` |
| MQL version | MQL5 only |
| Current compile workflow | Windows VM (manual) |
| Target output | Terminal output — clickable `file:line:col` errors |
| macOS arch | arm64 + Rosetta 2 (installed) |

---

## Architecture

Two pluggable backends behind one shared script interface.
The backend is selected by a single env var `MQL_BACKEND=local|remote`.

```
Zed task (tasks.json)
        │
        ▼
scripts/compile-mql.sh  <file.mq5>
        │
        ├── MQL_BACKEND=local  ──►  mql-compile-helper (Swift binary)
        │                               └── Wine + MetaEditor64.exe /compile
        │
        └── MQL_BACKEND=remote ──►  SSH → Windows VM
                                        └── MetaEditor64.exe /compile
        │
        ▼  (both paths write a UTF-16LE log to a temp location)
        │
        ▼
scripts/parse-mql-log.py  <log file>
        │
        ▼
stdout: POSIX path:line:col: error|warning: message
        (terminal clickable via Zed's path detection)
```

---

## Phase 1 — Local Wine (macOS + Rosetta 2)

### Why it needs a native helper

Wine's macOS display driver (`winemac.drv`) calls Cocoa APIs that require an
`NSApp` / Cocoa event loop to be alive in the host process.  When `wine` is
launched from a plain terminal there is no `NSApp`, so MetaEditor's Win32
message pump never drains and the process hangs indefinitely.
The fix is a tiny native Swift binary that initialises `NSApplication` before
spawning the wine sub-process.

### 1.1 — Helper binary: `scripts/mql-compile-helper.swift`

Already drafted.  Final design:

```
mql-compile-helper <wine_bin> <wineprefix> <metaeditor_win> <mq5_win> <include_win> <log_win>
```

Key details:
- Activation policy `.regular` + `activate(ignoringOtherApps: true)` so macOS
  delivers window-activation events to Wine's windows.
- Wine subprocess inherits `DYLD_FALLBACK_LIBRARY_PATH` pointing to
  `wine/lib/external`, `wine/lib/wine`, `wine/lib` inside the MT5 bundle.
- `WINEDEBUG=-all` to suppress MoltenVK / Vulkan spam.
- Swift helper exits with Wine's exit code; compile script uses that.

Build step (one-time, checked into repo as a pre-built binary or built in CI):
```sh
swiftc -O \
  -o scripts/mql-compile-helper \
  scripts/mql-compile-helper.swift
```

The compiled binary is committed to `scripts/mql-compile-helper` (arm64,
~200 KB) so users don't need to build it.

### 1.2 — Path translation

MetaEditor needs Windows paths.  `winepath -w <posix>` translates them via the
running wineserver.  The compile script calls this for both the `.mq5` source
and the log output path.

POSIX → Windows rules for paths inside the Wine prefix:
```
~/Library/Application Support/net.metaquotes.wine.metatrader5/drive_c/...
→  C:\...

/tmp/...   →  Z:\tmp\...   (via winepath -w)
```

The `include` path is always the fixed Wine-side path:
```
C:\Program Files\MetaTrader 5
```
(the directory that contains `MQL5/Include` — MetaEditor resolves `Include`
relative to this root).

### 1.3 — Compile script (local path): `scripts/compile-mql.sh`

```sh
#!/usr/bin/env bash
set -euo pipefail

MQ5_FILE="$1"          # absolute POSIX path to the .mq5 file
LOG_TMP="$(mktemp).log"

WINE_BIN="/Applications/MetaTrader 5.app/Contents/SharedSupport/wine/bin/wine"
WINEPREFIX="$HOME/Library/Application Support/net.metaquotes.wine.metatrader5"
INCLUDE="C:\Program Files\MetaTrader 5"
METAEDITOR="C:\Program Files\MetaTrader 5\MetaEditor64.exe"

# Warm wineserver if not already running
WINEPREFIX="$WINEPREFIX" WINEDEBUG=-all "$WINE_BIN/../wineserver" -p 2>/dev/null || true

# Translate paths via winepath
MQ5_WIN=$(WINEPREFIX="$WINEPREFIX" WINEDEBUG=-all "$WINE_BIN" winepath -w "$MQ5_FILE" 2>/dev/null)
LOG_WIN=$(WINEPREFIX="$WINEPREFIX" WINEDEBUG=-all "$WINE_BIN" winepath -w "$LOG_TMP" 2>/dev/null)

# Run MetaEditor inside NSApp context
SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
"$SCRIPT_DIR/mql-compile-helper" \
  "$WINE_BIN" "$WINEPREFIX" "$METAEDITOR" \
  "$MQ5_WIN" "$INCLUDE" "$LOG_WIN"

# Parse and print errors
python3 "$SCRIPT_DIR/parse-mql-log.py" "$LOG_TMP"
```

### 1.4 — Known blocker / contingency

During exploration, MetaEditor consistently hangs even after providing NSApp
context.  Investigation status:

- macdrv creates Windows successfully (confirmed via `WINEDEBUG=trace+macdrv`)
- No network activity (MetaEditor doesn't call home)
- Process runs indefinitely without producing the log file
- Root cause: **likely a first-run workspace/dialog or a rendering stall in
  MoltenVK** that blocks the Win32 message loop before the `/compile` flag is
  processed

**Contingency** (if helper still hangs after testing):
Try `WINEDLLOVERRIDES="winemac.drv=d"` + `DISPLAY=:0` with XQuartz installed,
or fall back to Phase 2 (Remote VM) as the primary backend while the local
path is debugged.

---

## Phase 2 — Remote Windows VM

### Why this is the primary workflow for now

The Windows VM is the user's current proven compile path.  Phase 2 delivers
a fast, reliable loop immediately.  Phase 1 becomes the optional "offline" path.

### 2.1 — Prerequisites (user-configured once)

```sh
# ~/.zshrc or Zed environment
export MQL_BACKEND=remote
export MQL_VM_HOST=192.168.x.x        # Windows VM IP or hostname
export MQL_VM_USER=rithsila           # Windows username (for SSH)
export MQL_VM_MQL5_ROOT="C:/Users/rithsila/AppData/Roaming/MetaQuotes/Terminal/.../MQL5"
export MQL_VM_METAEDITOR="C:/Program Files/MetaTrader 5/MetaEditor64.exe"
export MQL_VM_INCLUDE="C:/Program Files/MetaTrader 5"
```

SSH key auth assumed (`ssh-copy-id` to the VM).
The VM must be running OpenSSH (built-in on Windows 10+ or installed via
Windows Optional Features).

### 2.2 — Remote compile flow

```
1. rsync .mq5 file  →  VM at same relative path under MQL5/Experts/
2. ssh: MetaEditor64.exe /compile /include /log   (log on VM tmp)
3. rsync log file   ←  VM
4. parse log locally → print errors
5. rsync .ex5 file  ←  VM (next to .mq5)
```

Step 1 preserves the relative path so `#include` directives that reference
sibling `.mqh` files resolve correctly.

### 2.3 — Compile script (remote path): `scripts/compile-mql-remote.sh`

```sh
#!/usr/bin/env bash
set -euo pipefail

MQ5_FILE="$1"          # absolute POSIX path on this Mac
SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"

HOST="${MQL_VM_HOST:?set MQL_VM_HOST}"
USER="${MQL_VM_USER:?set MQL_VM_USER}"
VM_MQL5="${MQL_VM_MQL5_ROOT:?set MQL_VM_MQL5_ROOT}"
VM_ME="${MQL_VM_METAEDITOR:-C:/Program Files/MetaTrader 5/MetaEditor64.exe}"
VM_INC="${MQL_VM_INCLUDE:-C:/Program Files/MetaTrader 5}"

# Derive relative path from MQL5/ root on this Mac
LOCAL_MQL5="$HOME/Library/Application Support/net.metaquotes.wine.metatrader5/drive_c/Program Files/MetaTrader 5/MQL5"
REL="${MQ5_FILE#$LOCAL_MQL5/}"      # e.g. Experts/FlexGridPro/FlexGridPro.mq5
VM_MQ5="$VM_MQL5/$REL"

VM_LOG="${VM_MQ5%.mq5}.log"
VM_EX5="${VM_MQ5%.mq5}.ex5"

# 1. Push the source file
rsync -az -e ssh "$MQ5_FILE" "$USER@$HOST:$VM_MQ5"

# 2. Compile remotely
ssh "$USER@$HOST" "\"$VM_ME\" /compile:\"$VM_MQ5\" /include:\"$VM_INC\" /log:\"$VM_LOG\""

# 3. Pull log
LOCAL_LOG="${MQ5_FILE%.mq5}.log"
rsync -az -e ssh "$USER@$HOST:$VM_LOG" "$LOCAL_LOG"

# 4. Parse and print
python3 "$SCRIPT_DIR/parse-mql-log.py" "$LOCAL_LOG"

# 5. Pull .ex5 back
rsync -az -e ssh "$USER@$HOST:$VM_EX5" "${MQ5_FILE%.mq5}.ex5" && \
  echo "✓  $(basename ${MQ5_FILE%.mq5}.ex5) updated"
```

---

## Phase 3 — Shared Log Parser: `scripts/parse-mql-log.py`

MetaEditor writes logs as **UTF-16LE + CRLF**.  Raw error lines look like:

```
C:\...\FlexGridPro.mq5(42,18) : error 236: 'TradeResult' - undeclared identifier
C:\...\FlexGridPro.mq5(67,0) : warning 43: possible loss of data
MetaEditor build: 0 error(s), 2 warning(s)      ← last line
```

The parser must:
1. Detect and decode UTF-16LE (BOM check: `FF FE`), falling back to UTF-8.
2. Strip `\r\n` line endings.
3. Re-emit errors in POSIX-clickable format:
   ```
   /abs/path/to/file.mq5:42:18: error: 'TradeResult' - undeclared identifier
   ```
4. Print the final summary line unchanged.
5. Exit code: `0` if `0 error(s)`, else `1`.

Windows paths in the log (`C:\...\`) are converted back to absolute POSIX paths
using a simple prefix substitution based on the WINEPREFIX (local) or the
synced local path (remote).

```python
#!/usr/bin/env python3
"""Parse a MetaEditor compile log and re-emit POSIX-clickable diagnostics."""
import re, sys, pathlib

LOG_FILE = pathlib.Path(sys.argv[1])

# Detect encoding
raw = LOG_FILE.read_bytes()
if raw[:2] == b'\xff\xfe':
    text = raw.decode('utf-16-le')
else:
    text = raw.decode('utf-8', errors='replace')

lines = text.replace('\r\n', '\n').replace('\r', '\n').splitlines()

WINEPREFIX = pathlib.Path.home() / "Library/Application Support/net.metaquotes.wine.metatrader5"
C_DRIVE = WINEPREFIX / "drive_c"

def win_to_posix(wpath: str) -> str:
    p = wpath.replace('\\', '/')
    if p.lower().startswith('c:/'):
        return str(C_DRIVE / p[3:])
    return p

# Pattern: path(line,col) : (error|warning) NNN: message
PAT = re.compile(r'^(.+?)\((\d+),(\d+)\)\s*:\s*(error|warning)\s+\d+:\s*(.+)$')
SUMMARY = re.compile(r'^\d+ error\(s\),')

errors = 0
for line in lines:
    m = PAT.match(line.strip())
    if m:
        fpath, ln, col, level, msg = m.groups()
        posix = win_to_posix(fpath)
        print(f"{posix}:{ln}:{col}: {level}: {msg}")
        if level == 'error':
            errors += 1
    elif SUMMARY.search(line.strip()):
        print(line.strip())

sys.exit(0 if errors == 0 else 1)
```

---

## Phase 4 — Zed Task Integration: `.zed/tasks.json`

No extension API changes needed.  Zed tasks run in its built-in terminal,
which renders POSIX `file:line` paths as clickable links.

```jsonc
// .zed/tasks.json  (lives in the MQL5 workspace root, not in zed-mql/)
[
  {
    "label": "MQL: Compile (local Wine)",
    "command": "bash $ZED_WORKTREE_ROOT/../zed-mql/scripts/compile-mql.sh $ZED_FILE",
    "tags": ["mql5"],
    "allow_concurrent_tasks": false,
    "reveal": "always"
  },
  {
    "label": "MQL: Compile (remote VM)",
    "command": "MQL_BACKEND=remote bash $ZED_WORKTREE_ROOT/../zed-mql/scripts/compile-mql-remote.sh $ZED_FILE",
    "tags": ["mql5"],
    "allow_concurrent_tasks": false,
    "reveal": "always"
  }
]
```

`$ZED_FILE` = absolute path to the currently open file.
Task is triggered with `cmd+shift+b` → select label, or bound to a key.

---

## Phase 5 — Warm Wineserver (local latency)

Cold Wine startup takes ~3–5 s (wineserver init + Rosetta translation cache
warm-up).  A persistent wineserver cuts this to ~0.5 s on subsequent compiles.

Add to `~/.zshrc` or a login item:
```sh
WINEPREFIX="$HOME/Library/Application Support/net.metaquotes.wine.metatrader5" \
  "/Applications/MetaTrader 5.app/Contents/SharedSupport/wine/bin/wineserver" -p &
```

Or have `compile-mql.sh` auto-warm it if not already running (already in the
script draft above).

---

## File Layout After Implementation

```
zed-mql/
├── scripts/
│   ├── mql-compile-helper.swift    ← Swift source (checked in)
│   ├── mql-compile-helper          ← compiled arm64 binary (checked in)
│   ├── compile-mql.sh              ← local Wine backend
│   ├── compile-mql-remote.sh       ← remote VM backend
│   └── parse-mql-log.py            ← shared log parser
├── .zed/
│   └── tasks.json                  ← Zed task definitions (for zed-mql dev)
└── implement_plan.md               ← this file
```

The MQL5 workspace (MT5 data folder) gets its own `.zed/tasks.json`
pointing to the scripts above.

---

## Implementation Order

| # | Task | Effort | Blocks |
|---|---|---|---|
| 1 | `parse-mql-log.py` | 1 hr | everything |
| 2 | `compile-mql-remote.sh` (Phase 2) | 2 hr | immediate value |
| 3 | End-to-end test with real `.mq5` on VM | 1 hr | confirm log format |
| 4 | `.zed/tasks.json` for the MQL5 workspace | 30 min | daily use |
| 5 | `mql-compile-helper` debugging (Phase 1) | 2–4 hr | local compile |
| 6 | `compile-mql.sh` (Phase 1) | 1 hr | after #5 |
| 7 | Warm-wineserver login item | 15 min | after #6 |

Start with **#1 + #2 + #3** — this delivers a working compile loop
via VM today without any Wine debugging.

---

## Open Questions

1. **VM SSH details**: Is the Windows VM on the same LAN (low-latency rsync)
   or accessed via Tailscale/VPN?  This affects whether we use `rsync` or
   `scp` and whether latency warrants keeping a persistent SSH connection
   (`ControlMaster auto`).

2. **VM MQL5 root path**: The script needs the exact path on the VM where
   `MQL5/Experts` lives.  Confirm: is it
   `C:\Users\<user>\AppData\Roaming\MetaQuotes\Terminal\<hash>\MQL5`
   or a portable install at `C:\Program Files\MetaTrader 5\MQL5`?

3. **File already in MQL5 tree?** If the `.mq5` files on this Mac live inside
   the Wine prefix `MQL5/Experts/` tree (confirmed: they do), they may already
   be in sync with the VM if both use the same folder structure.  We may be
   able to skip the rsync push and just trigger the remote compile directly —
   reducing the script to SSH + compile + rsync log + rsync .ex5.

4. **SSH key on VM**: Windows OpenSSH uses `C:\Users\<user>\.ssh\authorized_keys`.
   Confirm key is in place before scripting.

5. **MetaEditor on VM**: Is MetaEditor installed at the default path
   `C:\Program Files\MetaTrader 5\MetaEditor64.exe`?  Or portable?

---

## What Is NOT in This Plan

- MCP / context-server integration (future — enables agent-driven compile loop)
- Problems panel diagnostics (requires a companion LSP — future V3)
- Backtest task runner (separate feature, same script layer)
- MQL4 support (out of scope per brief)
