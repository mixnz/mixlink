# File Size Limits

Small files keep each AI read cheap and each change focused.

## Code (Rust)
| Kind | Soft limit | Hard limit |
|---|---|---|
| Source file (`*.rs`) | 250 lines | 400 lines |
| `lib.rs` / `mod.rs` | 80 lines | 150 lines (re-exports + docs only) |
| Test file (`tests/*.rs`, `*_tests.rs`) | 300 lines | 500 lines |
| Function | 40 lines | 80 lines |
| Generated FFI bindings | exempt | exempt, header `// GENERATED` |

- Over soft limit: consider splitting at the next change. Over hard limit: split before adding code.
- Split by responsibility (e.g. `encoder/nvenc.rs`, `encoder/amf.rs`), not by line count.
- Inline `mod tests` longer than ~60 lines moves to a sibling `<name>_tests.rs`.

## Frontend (TypeScript / CSS)
| Kind | Soft limit | Hard limit |
|---|---|---|
| Component / module (`*.ts`, `*.tsx`) | 150 lines | 250 lines |
| Stylesheet (`*.css`) | 150 lines | 250 lines |
| Function / component body | 40 lines | 80 lines (eslint `max-lines-per-function`) |

## Docs (Markdown)
| Kind | Limit |
|---|---|
| Root `CLAUDE.md` | 120 lines |
| Crate/app `CLAUDE.md` | 50 lines |
| `docs/STATUS.md` | 80 lines |
| Spec, ADR, architecture doc | 250 soft / 300 hard (ADR <= 80) |

- A spec over 300 lines is too big: split into multiple specs.
- Docs do not paste code; reference `path/file.rs` + symbol name instead.
