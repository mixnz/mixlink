---
name: safe-coding
description: Use whenever writing or changing code in MixLink (Rust or TypeScript) — rules that keep AI-written code from failing: verify APIs, small compiled steps, typed errors, no guessing.
---

# Safe coding

**Before writing**
- Read the module card `CLAUDE.md` and the code you call. Never invent an API: confirm crate/Win32/Tauri
  signatures from existing usage, `cargo doc`, or docs.rs for the pinned version in `Cargo.lock`.
- New dependency → check it exists, its latest version, license (MIT/Apache/BSD/LGPL-dynamic only). Ask first.

**While writing**
- Small steps: after each edit `cargo check -p <crate>` (or `npm run typecheck`); fix before the next edit.
- Test first for logic (TDD); a test you never saw fail proves nothing.
- Rust: `Result` + `thiserror`, no `unwrap/expect/panic!` outside tests; exhaustive `match` (no `_` on own enums);
  newtypes for ids/units (`Millis`, `FrameId`); `unsafe` only in `platform/` with `// SAFETY:`.
- TS: no `any`, no `as` casts, no `!` non-null; every Tauri call through typed `ui/src/api/`; handle the error path.
- Hot path: no alloc/lock/blocking (root CLAUDE.md).

**When something fails**
- Read the whole error. Find the cause, then fix it; never silence (`#[allow]`, `@ts-ignore`, deleting a test).
- Same fix failing twice → stop, use superpowers:systematic-debugging.

**Before saying done**
- `bash scripts/gate.sh` passes; show the output. Not run = not done.
