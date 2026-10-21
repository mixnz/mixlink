#!/usr/bin/env bash
# Enforces the crate dependency direction from CLAUDE.md:
#   apps -> {capture, codec, audio, input, render, net} -> proto -> core
# Every internal (mixlink-*) dependency in any section of a member Cargo.toml
# ([dependencies], [dev-dependencies], [build-dependencies], target tables) must be allowed here.
source "$(dirname "$0")/lib.sh"

# Prints the allowed internal deps of a package ("*" = any), or "?" when unknown.
allowed_for() {
  case "$1" in
    mixlink-core) echo "" ;;
    mixlink-proto) echo "mixlink-core" ;;
    mixlink-net) echo "mixlink-proto mixlink-core" ;;
    mixlink-capture | mixlink-codec | mixlink-audio | mixlink-input | mixlink-render)
      echo "mixlink-core"
      ;;
    mixlink-host | mixlink-client) echo "*" ;;
    *) echo "?" ;;
  esac
}

for manifest in crates/*/Cargo.toml apps/*/Cargo.toml; do
  [[ -f "$manifest" ]] || continue
  pkg="$(grep -m1 -E '^name *= *"' "$manifest" | sed -E 's/^name *= *"([^"]+)".*/\1/')"
  allowed="$(allowed_for "$pkg")"
  if [[ "$allowed" == "?" ]]; then
    err "$manifest: unknown package $pkg — add it to the allow-table in scripts/check-deps.sh"
    continue
  fi
  [[ "$allowed" == "*" ]] && continue
  while IFS= read -r dep; do
    [[ " $allowed " == *" $dep "* ]] && continue
    err "$manifest: $pkg must not depend on $dep (allowed: ${allowed:-none})"
  done < <(grep -oE '^mixlink-[a-z]+(\.workspace)? *=' "$manifest" | sed -E 's/(\.workspace)? *=$//')
done

finish "check-deps"
