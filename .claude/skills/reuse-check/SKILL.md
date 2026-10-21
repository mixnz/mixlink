---
name: reuse-check
description: Use before writing a UI component or Rust helper, and before committing — decide if it will be reused and extract it into a reusable component/module instead of inlining.
---

# Reuse check

**Extract** when any is true: it already exists once elsewhere; it is a UI primitive (Button, Input,
Modal, DeviceCard…); both apps need it; a roadmap task will need it. Otherwise keep it inline. No speculative props.

**Search first:** `Grep` similar names/markup in `apps/*/ui/src/`, `packages/ui/src/`, crate `src/`. Extend before creating.

**UI layout** — one folder per component:
```
components/Button/
  Button.tsx          # component + exported ButtonProps
  Button.module.css
  index.ts            # export { Button }; export type { ButtonProps }
```
- One app → `apps/<app>/ui/src/components/`; both apps → `packages/ui/src/components/`.
- Import via the folder (`@/components/Button`), variants via props, never copies.

**Rust:** one crate → private module; several crates → lowest shared crate allowed by CLAUDE.md dependency direction.

**Before commit:** no duplicated markup / >5-line logic (`Grep`); new reusable units listed under
**Public API** in the module card `CLAUDE.md`.
