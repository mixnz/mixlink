# Docs Writing Rules

Docs are read by AI every session: optimize for few tokens and fast lookup.

- English only (fewer tokens than Vietnamese). Short sentences, tables, bullet lists.
- One topic per file. Respect limits in `file-limits.md`.
- No code block longer than 10 lines; reference `path/file.rs::Symbol` instead.
- No history or changelog in docs; git keeps history. Docs describe the current truth.
- When code changes behaviour described in a doc, update the doc in the same change.
- New doc -> add a row in `docs/README.md`.

## Specs (`docs/specs/NNNN-kebab-name.md`)
- Copy `docs/specs/_template.md`, number sequentially, add a row in `docs/specs/README.md`.
- Must link its plan `docs/plans/NNNN-kebab-name.md` (local, gitignored, written by the
  writing-plans skill). Plans are never committed; the spec is the durable record.
- Status: `draft` -> `approved` -> `in-progress` -> `done`.
- `**Date:** YYYY-MM-DD` = the day the spec was written; it never changes (git keeps later edits).

## ADRs (`docs/decisions/NNNN-kebab-name.md`)
- Sections: Context, Decision, Consequences. <= 80 lines.
- Status: `proposed` / `accepted` / `superseded by NNNN`.
