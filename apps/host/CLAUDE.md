# mixlink-host

> Module card. Keep <= 50 lines. Update when the public API changes.

**Purpose:** Host tray app (Tauri + React UI in `ui/`, ADR-0001): control UI only — status, PIN, paired devices, settings.
Talks to `mixlink-svc` over a named pipe (ADR-0005, spec 0012); the pipeline lives in the service.
**Depends on:** mixlink-core (`svc_ipc` pipe contract and client); `windows` (pipe server check).
**Hot path:** none.

## Layout
- `src/main.rs` (Tauri setup, close = hide), `src/tray.rs` (menu, `set_tooltip`, `show_main_window`), `src/strings.rs`,
  `src/commands.rs`, `src/service_link.rs` (+ `_tests.rs`: `LinkState`, `LinkTracker`, link thread),
  `src/pairing.rs` (+ `_tests.rs`: prompt, paired rows, one-shot pipe requests; spec 0018),
  `src/platform/windows/pipe_owner.rs` (`server_is_service`, the only `unsafe`).
- `ui/` (React + Vite): `src/api/svc.ts`, `src/api/pairing.ts`, `src/components/{PairPrompt,PairedList}/`,
  `src/svcStatusText.ts`, `src/i18n/en.ts`.

## Public API
- Tauri commands `get_svc_status() -> LinkState`, `cancel_pairing()`, `list_paired() -> PairedRow[]`,
  `remove_paired(device)` (errors are text). Event `svc-status` (payload `LinkState`, only on change: kind,
  service version, `networkPublic`, or prompt pin / device; uptime and countdown alone do not emit).
- `LinkState` (serde tag `kind`, camelCase): `connected { svcVersion, uptimeS, networkPublic, pairing }`
  (`pairing`: `{ deviceName, pin, expiresS }` or null), `notRunning`, `versionMismatch { svcVersion }`, `untrusted`.
- `PairedRow`: `{ device (32 hex), name, pairedAt (Unix s) }`, never a key.
- Tray menu ids: `open`, `quit`; tray id `main` (tooltip follows `LinkState`).

## Notes
- Thread `host-svc-link`: connects to `svc_ipc::PIPE_NAME`, checks the pipe server's PID is the `MixLink` service before `Hello`
  (`untrusted` otherwise, also for `--console`), asks `GetStatus` every 1 s, retries every 2 s after any error.
  Never joined; app exit ends it.
- A new PIN prompt shows and focuses the window. Pairing commands open their own short pipe connection (same check).
- Starts hidden (window `main`, `visible: false`); tray left click or Open shows it; X hides; Quit exits.
- `networkPublic`: the window shows a hint that clients cannot find a host on a Public network (spec 0017).
- Debug builds load `devUrl` (`npm run dev:host`, port 5174); feature `custom-protocol` embeds `ui/dist`.
