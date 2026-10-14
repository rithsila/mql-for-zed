#!/usr/bin/env bash
# MQL5 Code Explainer & Knowledge Generator Dispatcher
set -euo pipefail

# Find mql-lsp binary
MQL_LSP=""
if [[ -x "$HOME/.local/bin/mql-lsp" ]]; then
  MQL_LSP="$HOME/.local/bin/mql-lsp"
elif command -v mql-lsp &>/dev/null; then
  MQL_LSP="$(command -v mql-lsp)"
elif [[ -x "$(dirname "$0")/../lsp/target/release/mql-lsp" ]]; then
  MQL_LSP="$(dirname "$0")/../lsp/target/release/mql-lsp"
elif [[ -x "$(dirname "$0")/../lsp/target/debug/mql-lsp" ]]; then
  MQL_LSP="$(dirname "$0")/../lsp/target/debug/mql-lsp"
else
  echo "Error: mql-lsp binary not found. Run 'cargo build --release' in lsp/ directory." >&2
  exit 1
fi

ACTION="${1:-explain}"

case "$ACTION" in
  explain)
    shift || true
    SNIPPET="${*:-}"
    if [[ -z "$SNIPPET" ]]; then
      # If no snippet passed as arguments, read from stdin
      if [ ! -t 0 ]; then
        SNIPPET="$(cat)"
      fi
    fi
    if [[ -z "$SNIPPET" ]]; then
      echo "⚠️ No code selected. In Zed, highlight a snippet of MQL5 code, then run 'MQL: Explain Selection'."
      exit 0
    fi
    exec "$MQL_LSP" explain "$SNIPPET"
    ;;
  generate-knowledge|knowledge)
    shift || true
    TARGET_FILE="${1:-}"
    if [[ -z "$TARGET_FILE" ]]; then
      echo "Error: target file required. Usage: explain-mql.sh generate-knowledge <file.mq5>" >&2
      exit 1
    fi
    exec "$MQL_LSP" generate-knowledge "$TARGET_FILE"
    ;;
  *)
    exec "$MQL_LSP" explain "$@"
    ;;
esac
