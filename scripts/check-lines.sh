#!/usr/bin/env bash
# Enforces file size limits from docs/conventions/file-limits.md.
source "$(dirname "$0")/lib.sh"

# Prints "soft hard" for a path, or nothing when the file is not limited.
limits_for() {
  local f="$1"
  case "$f" in
    CLAUDE.md) echo "100 120" ;;
    */CLAUDE.md) echo "40 50" ;;
    docs/STATUS.md) echo "70 80" ;;
    docs/decisions/*.md) echo "70 80" ;;
    *.md) echo "250 300" ;;
    */lib.rs | */mod.rs) echo "80 150" ;;
    */tests/*.rs | *_tests.rs) echo "300 500" ;;
    *.rs) echo "250 400" ;;
    *.ts | *.tsx | *.css) echo "150 250" ;;
    *.sh | .githooks/*) echo "120 150" ;;
    *.yml | *.yaml | *.toml) echo "150 200" ;;
  esac
}

while IFS= read -r f; do
  read -r soft hard <<<"$(limits_for "$f")"
  [[ -z "$hard" ]] && continue
  if [[ "$f" == *.rs ]] && head -n 1 "$f" | grep -q '// GENERATED'; then
    continue
  fi
  n=$(wc -l <"$f")
  if ((n > hard)); then
    err "$f: $n lines (hard limit $hard) — split it"
  elif ((n > soft)); then
    warn "$f: $n lines (soft limit $soft)"
  fi
done < <(repo_files)

finish "check-lines"
