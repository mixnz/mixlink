#!/usr/bin/env bash
# Points git at the versioned hooks in .githooks/. Run once per clone.
source "$(dirname "$0")/lib.sh"

git config core.hooksPath .githooks
ok "core.hooksPath = .githooks"
