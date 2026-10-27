# mixlink-client

> Module card. Keep <= 50 lines. Update when the public API changes.

**Purpose:** Client Tauri launcher (React UI in `ui/`, ADR-0001): device list (discovery), pairing PIN entry, settings; spawns `mixlink-stream` on Connect.
**Depends on:** all crates.
**Hot path:** only via crates; this app wires threads and queues, no media logic here.

## Layout
- `src/main.rs` (Tauri setup), `src/commands.rs` (Tauri commands), `src/window.rs` (launcher window),
  `src/stream_proc.rs` (spawn + IPC), `src/stream_result.rs`, `src/hosts.rs` (LAN discovery, spec 0017),
  `src/pairing.rs` (thread `client-pair`, spec 0018), `src/strings.rs`; tests in `<name>_tests.rs`.
- `ui/` (React + Vite): `src/api/` (typed commands / events), `src/hooks.ts`, `src/components/{HostList,HostActions,
  PairDialog}/`, `src/i18n/en.ts`.

## Public API
- Commands: `start_stream`, `last_stream_result`, `list_hosts() -> HostRow[]`, pairing below.
- Events: `stream-result` (`StreamResult`), `hosts-changed` (`HostRow[]`, on every list change).
- `HostRow` (camelCase): `id` (32 hex), `name`, `address`, `port`, `compatible` (`ver == WIRE_VERSION`), `paired`.
- Pairing (spec 0018): commands `pair_request(id)`, `pair_submit(pin)` (6 digits), `pair_cancel`, `forget_host(id)`;
  event `pair-state` (`PairState`, tag `kind`: `idle`, `requesting`, `awaitingPin { expiresS, wrongPin }`,
  `checking`, `paired`, `failed { reason: closed | busy | noAnswer | version | failed }`; all but `idle` carry `host`).

## Notes
- The discovery `Browser` runs while the launcher window exists: `window.rs` starts it on open and stops it on
  `Destroyed` (the window is destroyed while streaming and rebuilt afterwards); the list clears on stop.
- Broadcast probes use the default discovery port (47800); the launcher does not read a client config yet.
- Paired hosts live in `%LOCALAPPDATA%\MixLink\known_hosts.json` (`net::client_store`); Forget is local only.
  The demo Connect does not use the secure session yet (streaming wires it).
