# mixlink-host

> Module card. Keep <= 50 lines. Update when the public API changes.

**Purpose:** Host tray app (Tauri + React UI in `ui/`, ADR-0001): control UI only — status, PIN, paired devices, settings.
Talks to `mixlink-svc` over a named pipe (ADR-0005); the pipeline lives in the service.
**Depends on:** mixlink-core (pipe contract).
**Hot path:** none.

## Planned layout
- Now: `src/main.rs` (Tauri setup, close = hide), `src/tray.rs`, `src/strings.rs`, `ui/` (React + Vite, `ui/src/i18n/en.ts`)
- Later: `src/commands.rs`, `src/session.rs`, `src/threads.rs`

## Public API
- No Tauri commands yet. Tray menu ids: `open`, `quit`.

## Notes
- Starts hidden (window `main`, `visible: false`); tray left click or Open shows it; X hides; Quit exits.
- Debug builds load `devUrl` (`npm run dev:host`, port 5174); feature `custom-protocol` embeds `ui/dist`.
