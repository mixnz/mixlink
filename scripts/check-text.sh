#!/usr/bin/env bash
# Checks text hygiene: LF line endings, final newline, no tabs in Markdown/TOML/YAML.
source "$(dirname "$0")/lib.sh"

mapfile -t files < <(repo_files)
((${#files[@]} == 0)) && finish "check-text"

# -I skips binary files; -U stops Git-for-Windows grep from stripping CR;
# -P '\r' because Git Bash drops a literal $'\r' from scripts.
while IFS= read -r f; do
  err "$f: CRLF line endings (must be LF)"
done < <(grep -lIUP '\r' -- "${files[@]}")

for f in "${files[@]}"; do
  [[ -s "$f" ]] || continue
  grep -qI '' -- "$f" || continue # binary (icons, images): no final-newline rule
  # Last byte must be a newline (command substitution strips it, leaving empty).
  [[ -z "$(tail -c 1 "$f")" ]] || err "$f: missing final newline"
done

mapfile -t tab_scope < <(printf '%s\n' "${files[@]}" | grep -E '\.(md|toml|ya?ml)$')
if ((${#tab_scope[@]} > 0)); then
  while IFS= read -r f; do
    warn "$f: contains tab characters"
  done < <(grep -lP '\t' -- "${tab_scope[@]}")
fi

finish "check-text"
