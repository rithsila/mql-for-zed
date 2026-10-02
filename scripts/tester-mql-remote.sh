#!/usr/bin/env bash
# Compile + deploy an EA, run it in the MT5 Strategy Tester on the Windows VM (terminal64 /config:), print a summary.
#
# Config: env vars, ~/.config/zed-mql/env, then <workspace>/.zed/mql-tester.env (later files win)
#   MQL_VM_HOST / MQL_VM_WORK / MQL_VM_MQL5_ROOT   same as compile-mql-remote.sh (MQL_VM_MQL5_ROOT is required here)
#   MQL_VM_TERMINAL      VM terminal64.exe              (default: C:/Program Files/MetaTrader 5/terminal64.exe)
#   MQL_BT_SYMBOL        tester symbol as the broker names it   (default: XAUUSDc)
#   MQL_BT_PERIOD        M1..MN, H1, D1 ...             (default: M5)
#   MQL_BT_MODEL         0 every tick, 1 1-min OHLC, 2 open prices, 4 real ticks   (default: 1)
#   MQL_BT_FROM/TO       YYYY.MM.DD                     (default: last 30 days)
#   MQL_BT_DEPOSIT       initial deposit                (default: 10000)
#   MQL_BT_CURRENCY      must match the account currency (default: USC, the cent account on the VM)
#   MQL_BT_LEVERAGE      e.g. 1:500                     (default: 1:500)
#   MQL_BT_LOGIN/SERVER  account the tester logs in with (default: read from the VM terminal's config/common.ini)
#   MQL_BT_SET           .set file for EA inputs        (default: <EA>.set next to the .mq5, if present)
#   MQL_BT_TIMEOUT       seconds to wait for the run    (default: 1800)
#   MQL_BT_CLOSE_GUI     1 (default) closes a running GUI terminal of the same install first (needed: one instance per data dir); 0 aborts instead
set -uo pipefail

CONFIG="${MQL_CONFIG:-$HOME/.config/zed-mql/env}"
[[ -f "$CONFIG" ]] && source "$CONFIG"

FILE="${1:?usage: tester-mql-remote.sh <file.mq5|file.mqh>}"
SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
die() { echo "error: $*" >&2; exit 2; }
winpath() { printf '%s' "${1//\//\\}"; }

[[ -f "$FILE" ]] || die "file not found: $FILE"
FILE="$(cd "$(dirname "$FILE")" && pwd)/$(basename "$FILE")"
WS="${MQL_WORKSPACE_ROOT:-$(git -C "$(dirname "$FILE")" rev-parse --show-toplevel 2>/dev/null || dirname "$FILE")}"; WS="${WS%/}"
[[ -f "$WS/.zed/mql-tester.env" ]] && source "$WS/.zed/mql-tester.env"
case "$FILE" in "$WS"/*) ;; *) die "$FILE is not under workspace root $WS" ;; esac

SRC="$FILE"
if [[ "$FILE" == *.mqh ]]; then
  MARK="$(head -n1 "$FILE" | sed -nE 's|^//###<(.+)>[[:space:]]*$|\1|p')"
  [[ -n "$MARK" ]] || die ".mqh needs first line //###<path/to/Main.mq5> (relative to $WS)"
  SRC="$WS/$MARK"; [[ -f "$SRC" ]] || die "marker target not found: $SRC"
fi

HOST="${MQL_VM_HOST:-mqlvm}"
VM_WORK="${MQL_VM_WORK:-C:/Users/MT5/mql-work}"; VM_WORK="${VM_WORK%/}"
VM_MQL5="${MQL_VM_MQL5_ROOT:-}"; VM_MQL5="${VM_MQL5%/}"
[[ -n "$VM_MQL5" ]] || die "MQL_VM_MQL5_ROOT is not set (see ~/.config/zed-mql/env)"
DATA="${VM_MQL5%/MQL5}"
VM_TERM="${MQL_VM_TERMINAL:-C:/Program Files/MetaTrader 5/terminal64.exe}"

REL="${SRC#"$WS"/}"
NAME="$(basename "${SRC%.*}")"
SUB="$(basename "$WS")/$(dirname "$REL")"; SUB="${SUB%/.}"
EXPERT="$(winpath "$SUB/$NAME.ex5")"

SYMBOL="${MQL_BT_SYMBOL:-XAUUSDc}"; PERIOD="${MQL_BT_PERIOD:-M5}"; MODEL="${MQL_BT_MODEL:-1}"
FROM="${MQL_BT_FROM:-$(date -v-30d +%Y.%m.%d 2>/dev/null || date -d '-30 days' +%Y.%m.%d)}"
TO="${MQL_BT_TO:-$(date +%Y.%m.%d)}"
DEPOSIT="${MQL_BT_DEPOSIT:-10000}"; CURRENCY="${MQL_BT_CURRENCY:-USC}"; LEVERAGE="${MQL_BT_LEVERAGE:-1:500}"
TIMEOUT="${MQL_BT_TIMEOUT:-1800}"

SSH_OPTS=(-o ControlMaster=auto -o ControlPath="$HOME/.ssh/mql-%C" -o ControlPersist=10m -o BatchMode=yes -o ConnectTimeout=8)
vm() { ssh "${SSH_OPTS[@]}" "$HOST" "$@"; }

TMP="$(mktemp -d -t mqltester)"; trap 'rm -rf "$TMP"' EXIT
vm_running() { vm "powershell -NoProfile -Command \"@(Get-Process terminal64 -ErrorAction SilentlyContinue | Where-Object { \$_.Path -eq '$(winpath "$VM_TERM")' }).Count\"" 2>/dev/null | tr -d '\r[:space:]'; }

# 1. Compile + deploy
echo "== Compile + deploy: $REL"
MQL_DEPLOY=1 bash "$SCRIPT_DIR/compile-mql-remote.sh" "$FILE" || die "compile failed; not running the tester"

# 2. Account (the tester needs one; use the terminal's saved account unless overridden)
LOGIN="${MQL_BT_LOGIN:-}"; SERVER="${MQL_BT_SERVER:-}"
if [[ -z "$LOGIN" || -z "$SERVER" ]]; then
  COMMON="$(vm "type \"$(winpath "$DATA")\\config\\common.ini\"" 2>/dev/null | tr -d '\r')"
  [[ -z "$LOGIN" ]] && LOGIN="$(sed -nE 's/^Login=([0-9]+).*/\1/p' <<<"$COMMON" | head -n1)"
  [[ -z "$SERVER" ]] && SERVER="$(sed -nE 's/^Server=(.+)$/\1/p' <<<"$COMMON" | head -n1)"
fi
[[ -n "$LOGIN" && -n "$SERVER" ]] || die "no account: set MQL_BT_LOGIN and MQL_BT_SERVER (or log the VM terminal in once)"

# 3. EA inputs (.set)
SET="${MQL_BT_SET:-${SRC%.*}.set}"; SETLINE=""
if [[ -f "$SET" ]]; then
  vm "mkdir \"$(winpath "$VM_MQL5")\\Profiles\\Tester\" 2>nul & exit 0"
  scp -q "${SSH_OPTS[@]}" "$SET" "$HOST:$VM_MQL5/Profiles/Tester/$(basename "$SET")" || die "could not upload $SET"
  SETLINE="ExpertParameters=$(basename "$SET")"
fi

# 4. Tester ini
INI="$TMP/zedmql-$NAME.ini"
{
  echo "[Common]"; echo "Login=$LOGIN"; echo "Server=$SERVER"
  echo "[Tester]"; echo "Expert=$EXPERT"; [[ -n "$SETLINE" ]] && echo "$SETLINE"
  echo "Symbol=$SYMBOL"; echo "Period=$PERIOD"; echo "Model=$MODEL"; echo "Optimization=0"
  echo "FromDate=$FROM"; echo "ToDate=$TO"; echo "Deposit=$DEPOSIT"; echo "Currency=$CURRENCY"; echo "Leverage=$LEVERAGE"
  echo "ExecutionMode=0"; echo "UseLocal=1"; echo "UseRemote=0"; echo "UseCloud=0"
  echo "Report=zedmql_$NAME.htm"; echo "ReplaceReport=1"; echo "ShutdownTerminal=1"; echo "Visual=0"
} > "$INI"
VM_INI="$VM_WORK/zedmql-$NAME.ini"
scp -q "${SSH_OPTS[@]}" "$INI" "$HOST:$VM_INI" || die "could not upload tester ini"

# 5. One instance per data dir: close a running GUI terminal (gracefully, in the RDP session) or abort
if [[ "$(vm_running)" != "0" && -n "$(vm_running)" ]]; then
  [[ "${MQL_BT_CLOSE_GUI:-1}" == "1" ]] || die "terminal is already running on the VM (MQL_BT_CLOSE_GUI=0); close it first"
  echo "== Closing the running VM terminal (same install) ..."
  cat > "$TMP/close.ps1" <<EOF
Get-Process terminal64 -ErrorAction SilentlyContinue | Where-Object { \$_.Path -eq '$(winpath "$VM_TERM")' } | ForEach-Object { taskkill /pid \$_.Id | Out-Null; [void]\$_.WaitForExit(30000) }
EOF
  scp -q "${SSH_OPTS[@]}" "$TMP/close.ps1" "$HOST:$VM_WORK/zedmql-close.ps1"
  vm "schtasks /create /tn zedmql_close /sc once /st 00:00 /it /f /tr \"powershell -NoProfile -ExecutionPolicy Bypass -File $(winpath "$VM_WORK")\\zedmql-close.ps1\" >nul & schtasks /run /tn zedmql_close >nul" >/dev/null
  for _ in $(seq 1 20); do sleep 2; [[ "$(vm_running)" == "0" ]] && break; done
  vm "schtasks /delete /tn zedmql_close /f >nul 2>&1 & exit 0" >/dev/null
  [[ "$(vm_running)" == "0" ]] || die "could not close the VM terminal; close it by hand and re-run"
fi

# 6. Run and wait (bytes already in the logs are skipped when parsing)
DAY="$(vm 'powershell -NoProfile -Command "Get-Date -Format yyyyMMdd"' | tr -d '\r[:space:]')"
TLOG="$DATA/Tester/logs/$DAY.log"; MLOG="$DATA/logs/$DAY.log"
size() { vm "for %F in (\"$(winpath "$1")\") do @echo %~zF" 2>/dev/null | tr -d '\r[:space:]' | grep -E '^[0-9]+$' || echo 0; }
TOFF="$(size "$TLOG")"; MOFF="$(size "$MLOG")"
echo "== Backtest: $NAME $SYMBOL $PERIOD model=$MODEL $FROM -> $TO deposit=$DEPOSIT $CURRENCY (account $LOGIN @ $SERVER)"
vm "del \"$(winpath "$DATA")\\zedmql_$NAME.htm\" 2>nul & start \"\" /wait \"$(winpath "$VM_TERM")\" /config:\"$(winpath "$VM_INI")\"" &
PID=$!
START=$SECONDS
while kill -0 "$PID" 2>/dev/null; do
  if (( SECONDS - START > TIMEOUT )); then
    kill "$PID" 2>/dev/null
    die "tester still running after ${TIMEOUT}s; it was left running on the VM (stop it there or raise MQL_BT_TIMEOUT)"
  fi
  sleep 2
done
echo "== Tester finished in $((SECONDS - START))s"

# 7. Results: HTML report if the terminal wrote one, else the tester log summary
OUT="$WS/.mql-tester"; mkdir -p "$OUT"
if scp -q "${SSH_OPTS[@]}" "$HOST:$DATA/zedmql_$NAME.htm" "$OUT/$NAME.htm" 2>/dev/null; then
  echo "Report: $OUT/$NAME.htm"
fi
scp -q "${SSH_OPTS[@]}" "$HOST:$TLOG" "$TMP/tester.log" 2>/dev/null || : > "$TMP/tester.log"
scp -q "${SSH_OPTS[@]}" "$HOST:$MLOG" "$TMP/terminal.log" 2>/dev/null || : > "$TMP/terminal.log"
python3 "$SCRIPT_DIR/parse-tester-log.py" --tester-log "$TMP/tester.log" --tester-offset "$TOFF" \
  --terminal-log "$TMP/terminal.log" --terminal-offset "$MOFF" --deposit "$DEPOSIT" --currency "$CURRENCY" \
  --save-tester-segment "$OUT/$NAME.tester.log"
STATUS=$?
echo "Tester log: $OUT/$NAME.tester.log"
exit $STATUS
