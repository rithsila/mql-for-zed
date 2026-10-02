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
[[ "${1:-}" == "--check" ]] && { MODE=check; shift; }
FILE="${1:?usage: compile-mql-remote.sh [--check] <file.mq5|file.mqh>}"
SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"

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

die() { echo "error: $*" >&2; exit 2; }
winpath() { printf '%s' "${1//\//\\}"; }

[[ -f "$FILE" ]] || die "file not found: $FILE"
FILE="$(cd "$(dirname "$FILE")" && pwd)/$(basename "$FILE")"
WS="${MQL_WORKSPACE_ROOT:-$(git -C "$(dirname "$FILE")" rev-parse --show-toplevel 2>/dev/null || dirname "$FILE")}"
WS="${WS%/}"
case "$FILE" in "$WS"/*) ;; *) die "$FILE is not under workspace root $WS" ;; esac

SRC="$FILE"
if [[ "$FILE" == *.mqh ]]; then
  MARK="$(head -n1 "$FILE" | sed -nE 's|^//###<(.+)>[[:space:]]*$|\1|p')"
  [[ -n "$MARK" ]] || die ".mqh needs first line //###<path/to/Main.mq5> (relative to $WS)"
  SRC="$WS/$MARK"
  [[ -f "$SRC" ]] || die "marker target not found: $SRC"
fi

REL="${SRC#"$WS"/}"
VM_WS="$VM_WORK/$(basename "$WS")"
VM_SRC="$VM_WS/$REL"
VM_LOG="${VM_SRC%.*}.log"
VM_EX5="${VM_SRC%.*}.ex5"
LOCAL_EX5="${SRC%.*}.ex5"
LOCAL_LOG="$(mktemp -t mqlcompile).log"
trap 'rm -f "$LOCAL_LOG"' EXIT

SSH_OPTS=(-o ControlMaster=auto -o ControlPath="$HOME/.ssh/mql-%C" -o ControlPersist=10m -o BatchMode=yes -o ConnectTimeout=8)

# 1. Sync sources (only .mq5/.mqh/.mq4-free tree, no VCS or build dirs)
if ! (cd "$WS" && find . \( -name .git -o -name node_modules -o -name .venv -o -name .cache \) -prune -o \
        -type f \( -name '*.mq5' -o -name '*.mqh' \) -print \
      | COPYFILE_DISABLE=1 tar -cf - -T -) \
      | ssh "${SSH_OPTS[@]}" "$HOST" "mkdir \"$(winpath "$VM_WS")\" 2>nul & tar -xf - -C \"$(winpath "$VM_WS")\""; then
  die "failed to sync sources to $HOST (is ssh working? try: ssh $HOST)"
fi

# 2. Compile (start /wait: MetaEditor is a GUI program and cmd would not wait otherwise)
FLAGS="/compile:\"$(winpath "$VM_SRC")\" /log:\"$(winpath "$VM_LOG")\""
[[ "$MODE" == "check" ]] && FLAGS="$FLAGS /s"
[[ -n "$VM_INC" ]] && FLAGS="$FLAGS /inc:\"$(winpath "$VM_INC")\""
ssh "${SSH_OPTS[@]}" "$HOST" "del \"$(winpath "$VM_LOG")\" 2>nul & start \"\" /wait \"$(winpath "$VM_ME")\" $FLAGS" \
  || echo "note: remote MetaEditor exited non-zero (expected when there are compile errors)" >&2

# 3. Fetch log and print diagnostics
scp -q "${SSH_OPTS[@]}" "$HOST:$VM_LOG" "$LOCAL_LOG" || die "no compile log produced at $VM_LOG"
MAPS=(--map "$VM_WS=$WS")
[[ -n "$VM_MQL5" ]] && MAPS+=(--map "$VM_MQL5=$LOCAL_MQL5")
python3 "$SCRIPT_DIR/parse-mql-log.py" "$LOCAL_LOG" "${MAPS[@]}"
STATUS=$?

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
