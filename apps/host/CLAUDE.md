# mixlink-host

> Module card. Keep <= 50 lines. Update when the public API changes.

**Purpose:** Host tray app (Tauri + React UI in `ui/`, ADR-0001): control UI only — status, PIN, paired devices, settings.
Talks to `mixlink-svc` over a named pipe (ADR-0005, spec 0012); the pipeline lives in the service.
**Depends on:** mixlink-core (`svc_ipc` pipe contract and client).
**Hot path:** none.

## Layout
- `src/main.rs` (Tauri setup, close = hide), `src/tray.rs` (menu, `set_tooltip`), `src/strings.rs`,
  `src/commands.rs`, `src/service_link.rs` (+ `_tests.rs`: `LinkState`, `LinkTracker`, link thread)
- `ui/` (React + Vite): `src/api/svc.ts` (typed command / event), `src/svcStatusText.ts`, `src/i18n/en.ts`

## Public API
- Tauri command `get_svc_status() -> LinkState`; event `svc-status` (payload `LinkState`, only on change:
  kind or service version; uptime alone does not emit).
- `LinkState` (serde tag `kind`, camelCase): `connected { svcVersion, uptimeS }`, `notRunning`,
  `versionMismatch { svcVersion }`.
- Tray menu ids: `open`, `quit`; tray id `main` (tooltip follows `LinkState`).

## Notes
- Thread `host-svc-link`: connects to `svc_ipc::PIPE_NAME`, asks `GetStatus` every 1 s, retries every 2 s
  after any error. It is never joined; app exit ends it.
- Starts hidden (window `main`, `visible: false`); tray left click or Open shows it; X hides; Quit exits.
- Debug builds load `devUrl` (`npm run dev:host`, port 5174); feature `custom-protocol` embeds `ui/dist`.
