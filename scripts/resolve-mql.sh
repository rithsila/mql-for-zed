# Sourced helper: resolve_mql_file <path>
# .mq5/.mqh pass through. For any other file (e.g. _tester.ini, .set, .ex5, .log) use the single .mq5 in the same folder,
# or the one named like the folder. Prints the resolved path, or an error on stderr and returns 2.
resolve_mql_file() {
  local f="$1" dir cands base
  case "$f" in *.mq5|*.mqh) printf '%s' "$f"; return 0 ;; esac
  dir="$(dirname "$f")"
  cands=()
  while IFS= read -r line; do cands+=("$line"); done < <(find "$dir" -maxdepth 1 -type f -name '*.mq5' | sort)
  if [[ ${#cands[@]} -eq 1 ]]; then printf '%s' "${cands[0]}"; return 0; fi
  base="$(basename "$dir")"
  if [[ -f "$dir/$base.mq5" ]]; then printf '%s' "$dir/$base.mq5"; return 0; fi
  if [[ ${#cands[@]} -eq 0 ]]; then
    echo "error: $(basename "$f") is not an .mq5 and $dir has no .mq5 to use; open the EA's .mq5" >&2
  else
    echo "error: $(basename "$f") is not an .mq5 and $dir has ${#cands[@]} of them; open the one you want" >&2
  fi
  return 2
}
