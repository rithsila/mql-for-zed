#!/usr/bin/env bash
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
PROJECT_ROOT="$(cd "$SCRIPT_DIR/.." && pwd)"
SERVER="$PROJECT_ROOT/mql-mcp/target/release/mql-mcp"

if [[ ! -x "$SERVER" ]]; then
  echo "mql-mcp not built: run cargo build --release --manifest-path $PROJECT_ROOT/mql-mcp/Cargo.toml" >&2
  exit 1
fi

cd "$PROJECT_ROOT"
exec "$SERVER"
