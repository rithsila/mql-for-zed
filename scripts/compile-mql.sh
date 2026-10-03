#!/usr/bin/env bash
# Dispatcher: MQL_BACKEND=remote (default) | local (not implemented yet)
set -uo pipefail
SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
source "$SCRIPT_DIR/resolve-mql.sh"
if [[ $# -gt 0 ]]; then
  LAST="${!#}"
  if [[ -f "$LAST" ]]; then
    RESOLVED="$(resolve_mql_file "$LAST")" || exit 2
    set -- "${@:1:$#-1}" "$RESOLVED"
  fi
fi
case "${MQL_BACKEND:-remote}" in
  remote) exec bash "$SCRIPT_DIR/compile-mql-remote.sh" "$@" ;;
  local) echo "error: local Wine backend is not implemented yet; use MQL_BACKEND=remote" >&2; exit 2 ;;
  *) echo "error: unknown MQL_BACKEND '${MQL_BACKEND}'" >&2; exit 2 ;;
esac
