#!/usr/bin/env bash
# Dispatcher: MQL_BACKEND=remote (default) | local (not implemented yet)
set -uo pipefail
SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
source "$SCRIPT_DIR/resolve-mql.sh"
JSON=0; JOB_ID=""; SNAPSHOT=""
for ((i=1; i<=$#; i++)); do
  case "${!i}" in
    --json) JSON=1 ;;
    --job-id|--snapshot)
      KEY="${!i}"; ((i++)); VALUE="${!i:-}"
      if [[ "$KEY" == --job-id ]]; then JOB_ID="$VALUE"; else SNAPSHOT="$VALUE"; fi ;;
  esac
done
fail() {
  if [[ "$JSON" == 1 ]]; then
    python3 -c 'import json,sys; print(json.dumps({"schema_version":1,"job_id":sys.argv[1],"source_snapshot":sys.argv[2],"status":"launch_failure","diagnostics":[],"message":sys.argv[3]}))' "$JOB_ID" "$SNAPSHOT" "$1"
  else
    echo "error: $1" >&2
  fi
  exit 2
}
if [[ $# -gt 0 ]]; then
  LAST="${!#}"
  if [[ -f "$LAST" ]]; then
    if [[ "$JSON" == 1 ]]; then
          RESOLVED="$(resolve_mql_file "$LAST" 2>/dev/null)" || fail "could not resolve MQL source for $LAST"
        else
          RESOLVED="$(resolve_mql_file "$LAST")" || exit 2
        fi
    set -- "${@:1:$#-1}" "$RESOLVED"
  fi
fi
case "${MQL_BACKEND:-remote}" in
  remote) exec bash "$SCRIPT_DIR/compile-mql-remote.sh" "$@" ;;
  local) fail "local Wine backend is not implemented yet; use MQL_BACKEND=remote" ;;
  *) fail "unknown MQL_BACKEND '${MQL_BACKEND}'" ;;
esac
