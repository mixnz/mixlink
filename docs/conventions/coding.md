# Coding Conventions

## Language & toolchain
- Rust stable, edition 2024, Cargo workspace. `cargo fmt` + `clippy -D warnings` must pass.
- Windows APIs via the `windows` crate. No C++ unless an SDK has no C ABI.

## Structure inside a crate
```
src/
  lib.rs          # public API re-exports + crate docs
  <feature>.rs    # cross-platform logic
  platform/
    mod.rs
    windows/      # #[cfg(windows)] implementations
```
- Public API = traits + plain data types exported from `lib.rs`. Platform types never leak out.
- One trait per backend family (e.g. `VideoEncoder`), backend selected at runtime.

## Errors & logging
- Library crates: `thiserror` enums. Apps: `anyhow`.
- No `unwrap()` / `expect()` outside tests and startup code.
- `tracing` for logs; hot path uses `trace!` only, or counters in `core::stats`.

## Performance
- Hot-path code marked `// HOT PATH`; latency rules in root `CLAUDE.md`.
- Buffers come from pools allocated at session start.
- Any perf claim needs a benchmark (`criterion`) or a `latency-probe` measurement.

## Testing
- Pure logic (proto, FEC, pacing, bitrate control): unit tests required (TDD).
- Hardware-dependent code: integration tests behind `--features hw-tests`, skipped in CI.
- Network logic tested with `tools/packet-sim` (loss, jitter, reorder).

## Naming
- Crates `mixlink-<name>`, folders `crates/<name>`.
- Types `PascalCase`, functions `snake_case`, constants `SCREAMING_SNAKE_CASE`.
