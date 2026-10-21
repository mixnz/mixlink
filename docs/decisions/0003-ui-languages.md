# 0003 — UI languages

**Status:** accepted · 2026-10-21

## Context
MixLink targets English and Vietnamese speakers. Retrofitting translation after strings are spread
across components and Rust code is expensive.

## Decision
- The apps support **English and Vietnamese**; **English is the default**.
- Every user-visible string has a key. React UIs keep strings in `apps/<app>/ui/src/i18n/<lang>.ts`;
  Rust-side user text (tray menu, window titles) lives in one `src/strings.rs` per app.
- Rust sends typed results/events to the UI, never sentences; the UI turns them into text.
- Copy is written with the `natural-copy` skill; each language written natively, not word by word.
- Language switching (setting, detection, `vi` files) lands in its own roadmap task. Until then
  only `en` exists, but every string already goes through the keyed layer.

## Consequences
- No hard-coded user text in components or Rust logic; reviews reject it.
- Placeholders are named so sentence order can differ per language.
- A string change touches every language file once `vi` exists.
