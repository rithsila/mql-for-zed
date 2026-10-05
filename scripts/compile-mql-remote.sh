#!/usr/bin/env bash
# Compile an .mq5 on a Windows machine over SSH (MetaEditor /compile), then print clickable diagnostics.
# The workspace's .mq5/.mqh files are synced to the VM first (tar over ssh), so relative #includes resolve.
#
# Config: env vars or ~/.config/zed-mql/env (sourced if present)
#   MQL_VM_HOST          ssh host or alias                      (default: mqlvm)
#   MQL_VM_WORK          VM dir that holds synced workspaces    (default: C:/Users/MT5/mql-work)
#   MQL_VM_MQL5_ROOT     VM terminal data folder's MQL5 dir     (maps include errors back to the Mac)
#   MQL_VM_METAEDITOR    path to MetaEditor64.exe               (default: C:/Program Files/MetaTrader 5/MetaEditor64.exe)
#   MQL_VM_INCLUDE       optional /inc: directory
#   MQL_LOCAL_MQL5_ROOT  local MQL5 dir used for include navigation
#   MQL_WORKSPACE_ROOT   local workspace root (default: git toplevel of the file, else its directory)
#   MQL_DEPLOY           1 (default) copies the compiled .ex5 into MQL5/Experts/<workspace>/... for backtesting; 0 disables
#   MQL_LOCAL_MT5_MQL5   local MetaTrader MQL5 dir to deploy to too (default: the Wine install's MQL5 dir if it exists)
set -uo pipefail

CONFIG="${MQL_CONFIG:-$HOME/.config/zed-mql/env}"
[[ -f "$CONFIG" ]] && source "$CONFIG"

MODE=compile
JSON=0
JOB_ID=""
SNAPSHOT=""
FILE=""
ARG_ERROR=""
while [[ $# -gt 0 ]]; do
  case "$1" in
    --check) MODE=check ;;
    --json) JSON=1 ;;
    --job-id|--snapshot)
      KEY="$1"; shift
      if [[ $# -eq 0 ]]; then ARG_ERROR="missing value for $KEY"; break; fi
      if [[ "$KEY" == --job-id ]]; then JOB_ID="$1"; else SNAPSHOT="$1"; fi ;;
    *) FILE="$1" ;;
  esac
  shift
done
SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
emit_failure() {
  python3 -c 'import json,sys; print(json.dumps({"schema_version":1,"job_id":sys.argv[1],"source_snapshot":sys.argv[2],"status":sys.argv[3],"diagnostics":[],"message":sys.argv[4]}))' "$JOB_ID" "$SNAPSHOT" "$1" "$2"
}
die() {
  if [[ "$JSON" == 1 ]]; then emit_failure "$1" "$2"; else echo "error: $2" >&2; fi
  exit 2
}
[[ -z "$ARG_ERROR" ]] || die launch_failure "$ARG_ERROR"
[[ -n "$FILE" ]] || die launch_failure "usage: compile-mql-remote.sh [--check] [--json --job-id ID --snapshot ID] <file.mq5|file.mqh>"
[[ "$JSON" == 0 || "$MODE" == check ]] || die launch_failure "--json requires --check"

HOST="${MQL_VM_HOST:-mqlvm}"
VM_WORK="${MQL_VM_WORK:-C:/Users/MT5/mql-work}"; VM_WORK="${VM_WORK%/}"
VM_ME="${MQL_VM_METAEDITOR:-C:/Program Files/MetaTrader 5/MetaEditor64.exe}"
VM_INC="${MQL_VM_INCLUDE:-}"
VM_MQL5="${MQL_VM_MQL5_ROOT:-}"
LOCAL_MQL5="${MQL_LOCAL_MQL5_ROOT:-}"
[[ -z "$LOCAL_MQL5" && -d "$HOME/.config/zed-mql/MQL5" ]] && LOCAL_MQL5="$HOME/.config/zed-mql/MQL5"
[[ -z "$LOCAL_MQL5" ]] && LOCAL_MQL5="$HOME/Library/Application Support/net.metaquotes.wine.metatrader5/drive_c/Program Files/MetaTrader 5/MQL5"

LOCAL_MT5_MQL5="${MQL_LOCAL_MT5_MQL5:-$HOME/Library/Application Support/net.metaquotes.wine.metatrader5/drive_c/Program Files/MetaTrader 5/MQL5}"
DEPLOY="${MQL_DEPLOY:-1}"

winpath() { printf '%s' "${1//\//\\}"; }
run_quiet() {
  if [[ "$JSON" == 1 ]]; then "$@" >/dev/null; else "$@"; fi
}

[[ -f "$FILE" ]] || die launch_failure "file not found: $FILE"
FILE="$(cd "$(dirname "$FILE")" && pwd)/$(basename "$FILE")"
WS="${MQL_WORKSPACE_ROOT:-$(git -C "$(dirname "$FILE")" rev-parse --show-toplevel 2>/dev/null || dirname "$FILE")}"
WS="${WS%/}"
case "$FILE" in "$WS"/*) ;; *) die launch_failure "$FILE is not under workspace root $WS" ;; esac

SRC="$FILE"
if [[ "$FILE" == *.mqh ]]; then
  MARK="$(head -n1 "$FILE" | sed -nE 's|^//###<(.+)>[[:space:]]*$|\1|p')"
  [[ -n "$MARK" ]] || die launch_failure ".mqh needs first line //###<path/to/Main.mq5> (relative to $WS)"
  SRC="$WS/$MARK"
  [[ -f "$SRC" ]] || die launch_failure "marker target not found: $SRC"
fi

REL="${SRC#"$WS"/}"
VM_WS="$VM_WORK/$(basename "$WS")"
VM_SRC="$VM_WS/$REL"
VM_LOG="${VM_SRC%.*}.log"
VM_EX5="${VM_SRC%.*}.ex5"
LOCAL_EX5="${SRC%.*}.ex5"
LOCAL_LOG="$(mktemp -t mqlcompile).log" || die log_failure "cannot create temporary compile log"
trap 'rm -f "$LOCAL_LOG"' EXIT

SSH_OPTS=(-o ControlMaster=auto -o ControlPath="$HOME/.ssh/mql-%C" -o ControlPersist=10m -o BatchMode=yes -o ConnectTimeout=8)

# 1. Sync sources (only .mq5/.mqh/.mq4-free tree, no VCS or build dirs)
if ! (cd "$WS" && find . \( -name .git -o -name node_modules -o -name .venv -o -name .cache \) -prune -o \
        -type f \( -name '*.mq5' -o -name '*.mqh' \) -print \
      | COPYFILE_DISABLE=1 tar -cf - -T -) \
      | run_quiet ssh "${SSH_OPTS[@]}" "$HOST" "mkdir \"$(winpath "$VM_WS")\" 2>nul & tar -xf - -C \"$(winpath "$VM_WS")\""; then
  die ssh_failure "failed to sync sources to $HOST (is ssh working? try: ssh $HOST)"
fi

# 1b. Helper include for the backtest summary (harmless if unused)
if [[ "$MODE" != check && -n "$VM_MQL5" && -f "$SCRIPT_DIR/../mql/ZedMqlStats.mqh" ]]; then
  scp -q "${SSH_OPTS[@]}" "$SCRIPT_DIR/../mql/ZedMqlStats.mqh" "$HOST:$VM_MQL5/Include/ZedMqlStats.mqh" 2>/dev/null \
    || echo "note: could not copy ZedMqlStats.mqh to the VM Include folder" >&2
fi

# 2. Compile (start /wait: MetaEditor is a GUI program and cmd would not wait otherwise)
FLAGS="/compile:\"$(winpath "$VM_SRC")\" /log:\"$(winpath "$VM_LOG")\""
[[ "$MODE" == "check" ]] && FLAGS="$FLAGS /s"
[[ -n "$VM_INC" ]] && FLAGS="$FLAGS /inc:\"$(winpath "$VM_INC")\""
LAUNCH_STATUS=0
run_quiet ssh "${SSH_OPTS[@]}" "$HOST" "del \"$(winpath "$VM_LOG")\" 2>nul & start \"\" /wait \"$(winpath "$VM_ME")\" $FLAGS" || LAUNCH_STATUS=$?
if [[ $LAUNCH_STATUS -eq 255 ]]; then
  die ssh_failure "ssh failed while launching MetaEditor on $HOST"
elif [[ $LAUNCH_STATUS -ne 0 && "$JSON" == 0 ]]; then
  echo "note: remote MetaEditor exited non-zero (expected when there are compile errors)" >&2
fi

# 3. Fetch log and print diagnostics
SCP_STATUS=0
scp -q "${SSH_OPTS[@]}" "$HOST:$VM_LOG" "$LOCAL_LOG" || SCP_STATUS=$?
if [[ $SCP_STATUS -ne 0 ]]; then
  [[ $SCP_STATUS -eq 255 ]] && die ssh_failure "ssh failed fetching compile log from $HOST"
  [[ $LAUNCH_STATUS -ne 0 ]] && die launch_failure "MetaEditor failed to produce a compile log at $VM_LOG"
  die log_failure "no compile log produced at $VM_LOG"
fi
MAPS=(--map "$VM_WS=$WS")
[[ -n "$VM_MQL5" ]] && MAPS+=(--map "$VM_MQL5=$LOCAL_MQL5")
PARSER_OPTS=()
[[ "$JSON" == 1 ]] && PARSER_OPTS=(--json --job-id "$JOB_ID" --snapshot "$SNAPSHOT")
if [[ "$JSON" == 1 ]]; then
  PARSED="$(python3 "$SCRIPT_DIR/parse-mql-log.py" "${PARSER_OPTS[@]}" "$LOCAL_LOG" "${MAPS[@]}")"
  STATUS=$?
  if [[ $LAUNCH_STATUS -ne 0 && $STATUS -eq 0 ]]; then
    emit_failure launch_failure "MetaEditor exited non-zero despite a clean compile log"
    exit 2
  fi
  printf '%s\n' "$PARSED"
else
  python3 "$SCRIPT_DIR/parse-mql-log.py" "$LOCAL_LOG" "${MAPS[@]}"
  STATUS=$?
fi

# 4. Bring the .ex5 back on success
if [[ $STATUS -eq 0 && "$MODE" == "compile" ]]; then
  scp -q "${SSH_OPTS[@]}" "$HOST:$VM_EX5" "$LOCAL_EX5" && echo "OK: $(basename "$LOCAL_EX5") updated"

  # 5. Deploy to MetaTrader's Experts folder (VM and local) so the Strategy Tester can see it
  if [[ "$DEPLOY" != "0" ]]; then
    SUB="$(basename "$WS")/$(dirname "$REL")"; SUB="${SUB%/.}"
    if [[ -n "$VM_MQL5" ]]; then
      DST="$VM_MQL5/Experts/$SUB"
      if ssh "${SSH_OPTS[@]}" "$HOST" "mkdir \"$(winpath "$DST")\" 2>nul & copy /y \"$(winpath "$VM_EX5")\" \"$(winpath "$DST")\\\" >nul" ; then
        echo "Deployed to VM: $DST/$(basename "$VM_EX5")"
      else
        echo "warning: could not copy .ex5 into VM Experts (file in use by MT5?)" >&2
      fi
    fi
    if [[ -d "$LOCAL_MT5_MQL5/Experts" ]]; then
      DST="$LOCAL_MT5_MQL5/Experts/$SUB"
      mkdir -p "$DST" && cp -f "$LOCAL_EX5" "$DST/" && echo "Deployed locally: $DST/$(basename "$LOCAL_EX5")" \
        || echo "warning: could not copy .ex5 into local MT5 Experts" >&2
    fi
  fi
fi
exit $STATUS
