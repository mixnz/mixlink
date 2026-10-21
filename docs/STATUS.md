# Status

> Replace content each update; keep ≤ 80 lines. History lives in git, not here.

**Updated:** 2026-10-21
**Phase:** 1 — LAN MVP

## Done
- Repo structure, AI working rules, architecture docs; ADR-0001/0002 accepted.
- Quality gate (`scripts/gate.sh`): text, lines, links, docs, crate deps, fmt, clippy, tests.
- CI: `ci.yml` (docs/lint/test) on dispatch via `scripts/ask-ci.sh`; `/execute`, `/land`, `/watch-ci`.
- Cargo workspace (spec 0001): toolchain 1.99.1, 8 empty lib crates + 2 apps, workspace lints,
  dev/release/profiling profiles, MIT OR Apache-2.0.

## In progress
- Nothing.

## Next
1. Spec `0002-lan-discovery`: mDNS + UDP broadcast, device list on client.
2. Spec `0003-video-pipeline-mvp`: capture → NVENC → UDP → decode → present.

## Open questions
- Virtual gamepad driver choice (see `architecture/overview.md` → Risks).
