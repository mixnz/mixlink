# 0001 — Workspace bootstrap

**Status:** in-progress
**Date:** 2026-10-21
**Plan:** [0001-workspace-bootstrap](../plans/0001-workspace-bootstrap.md) (local only, written by writing-plans skill)
**Crates touched:** all (`crates/*`, `apps/*`), repo root config, `scripts/`, CI

## Goal
An empty but compiling Cargo workspace where every lint, limit and dependency rule from
`CLAUDE.md` is enforced by tooling, so later specs only add code — never setup.

## Scope
- In:
  - Root `Cargo.toml` workspace, toolchain pin, rustfmt/clippy config, build profiles, dual license files.
  - 8 library crates + 2 app binaries, empty, with dependency edges already declared.
  - `scripts/check-deps.sh` enforcing the dependency direction; wired into `gate.sh`.
  - CI uses the pinned toolchain.
- Out:
  - Any streaming functionality, third-party dependencies, `tools/` crates, installer/packaging.

## Design

### Root files
| File | Content |
|---|---|
| `Cargo.toml` | `[workspace]` resolver 3, members `crates/*`, `apps/*`; `[workspace.package]` version 0.1.0, edition 2024, rust-version 1.99, license `MIT OR Apache-2.0`; `[workspace.lints]`; profiles |
| `rust-toolchain.toml` | channel `1.99.1` (exact pin; bump deliberately in its own `chore` commit), components `rustfmt`, `clippy` |
| `rustfmt.toml` | `edition = "2024"`, `max_width = 100` |
| `clippy.toml` | `too-many-lines-threshold = 80`, `allow-unwrap-in-tests = true`, `allow-expect-in-tests = true` |
| `LICENSE-MIT`, `LICENSE-APACHE` | standard texts, copyright "The MixLink Authors"; every crate inherits `license.workspace = true` |

### Workspace lints (all fail the gate via `-D warnings`)
- rust: `unsafe_op_in_unsafe_fn`, `missing_debug_implementations`.
- clippy: `unwrap_used`, `expect_used`, `undocumented_unsafe_blocks` (enforces `// SAFETY:`),
  `too_many_lines` (function hard limit 80), `dbg_macro`, `print_stdout` (libs only; apps allow).
- Every crate sets `[lints] workspace = true`.

### Build profiles
| Profile | Settings | Why |
|---|---|---|
| `dev` | deps `opt-level = 2`, own code `opt-level = 0` | debug builds still fast enough to stream |
| `release` | `lto = "thin"`, `codegen-units = 1`, `panic = "abort"`, `debug = "line-tables-only"` | speed + usable stack traces |
| `profiling` | inherits `release`, `debug = true` | for profilers / `latency-probe` |

### Crates
- Packages `mixlink-<name>` in `crates/<name>`: `Cargo.toml` + `src/lib.rs` with only a crate
  doc comment (purpose copied from the module card). No modules yet.
- Apps `mixlink-host`, `mixlink-client`: `src/main.rs` that prints name + version and exits.
- Path dependencies declared now, exactly as `CLAUDE.md` dependency direction:
  `proto → core`, `net → proto, core`, `capture/codec/audio/input/render → core`, apps → all.

### `scripts/check-deps.sh`
- Reads `[dependencies]` path entries (`mixlink-*`) from each member `Cargo.toml` (grep, no jq).
- Compares against an allow-table in the script; any other internal edge = ERROR.
- Added to `gate.sh` (fast and full). Documented in `scripts/README.md`.

### CI
- `lint`/`test` jobs run `rustup toolchain install`, which installs what `rust-toolchain.toml` names
  (`rustup show` no longer installs since rustup 1.28).

## Latency impact
None at runtime. Profiles set here define how later latency measurements are built
(`profiling` profile is the reference for numbers in specs).

## Acceptance criteria
- [ ] `rustup show` in the repo reports `1.99.1` (auto-installed from `rust-toolchain.toml`).
- [ ] `bash scripts/gate.sh` passes with cargo steps running (fmt, clippy, test).
- [ ] `bash scripts/build.sh release` puts `mixlink-host.exe` and `mixlink-client.exe` in `dist/`.
- [ ] Adding `mixlink-net` as a dependency of `mixlink-codec` makes `check-deps.sh` fail.
- [ ] A 81-line function makes clippy fail; an `unwrap()` in a lib makes clippy fail.
- [ ] Each module card `CLAUDE.md` still ≤ 50 lines; `docs/STATUS.md` updated.
- [ ] CI run on GitHub is green.
- [ ] `README.md` has a License section: dual-licensed under MIT OR Apache-2.0.

## Decisions
- License: `MIT OR Apache-2.0` (Rust ecosystem convention).
- ADR-0001/0002 flip to `accepted` in the first commit of this spec once it is approved.
