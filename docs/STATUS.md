# Status

> Replace content each update; keep ≤ 80 lines. History lives in git, not here.

**Updated:** 2026-10-21
**Phase:** 0 — Project scaffolding

## Done
- Repo structure, AI working rules, architecture drafts, ADR-0001/0002 (proposed).
- Check scripts + `scripts/gate.sh`, `scripts/build.sh`, git hooks, CI workflow (gate on windows-latest).

## In progress
- Nothing.

## Next
1. Spec `0001-workspace-bootstrap`: Cargo workspace, empty crates, CI lint/test.
2. Spec `0002-lan-discovery`: mDNS + UDP broadcast, device list on client.
3. Spec `0003-video-pipeline-mvp`: capture → NVENC → UDP → decode → present.

## Open questions
- Virtual gamepad driver choice (see `architecture/overview.md` → Risks).
