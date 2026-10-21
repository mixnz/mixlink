#!/usr/bin/env bash
# Verifies that links in Markdown files point to existing files.
# - [text](relative/path) is resolved from the Markdown file's folder.
# - `docs/...md` in backticks is resolved from the repo root.
# Links into docs/plans/ are allowed to be missing (plans are local only).
source "$(dirname "$0")/lib.sh"

# Normalizes "a/b/../c" into "a/c".
normalize() {
  local out=() part
  IFS='/' read -ra parts <<<"$1"
  for part in "${parts[@]}"; do
    case "$part" in
      "" | .) ;;
      ..) ((${#out[@]} > 0)) && unset 'out[${#out[@]}-1]' ;;
      *) out+=("$part") ;;
    esac
  done
  local IFS='/'
  echo "${out[*]}"
}

check_target() {
  local src="$1" target="$2" path="$3"
  path="${path%%#*}"
  [[ -z "$path" ]] && return
  # Placeholders in templates and rule docs.
  [[ "$path" == *NNNN* || "$path" == *...* ]] && return
  path="$(normalize "$path")"
  [[ "$path" == docs/plans/* ]] && return
  [[ -e "$path" ]] || err "$src: broken link '$target'"
}

while IFS= read -r md; do
  dir="$(dirname "$md")"
  # Markdown links, skipping external URLs and pure anchors.
  while IFS= read -r target; do
    case "$target" in
      http://* | https://* | mailto:* | \#*) continue ;;
    esac
    check_target "$md" "$target" "$dir/$target"
  done < <(grep -o '\]([^)[:space:]]*)' "$md" | sed 's/^](//; s/)$//')
  # Backtick references to repo docs.
  while IFS= read -r target; do
    check_target "$md" "$target" "$target"
  done < <(grep -o '`docs/[^`*<>]*\.md`' "$md" | tr -d '`')
done < <(repo_files | grep '\.md$')

finish "check-links"
