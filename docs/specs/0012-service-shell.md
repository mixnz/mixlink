# 0012 — Service shell: `mixlink-svc`, install / uninstall, named pipe to the tray

**Status:** done
**Date:** 2026-10-24
**Roadmap:** T19
**Plan:** [0012-service-shell](../plans/0012-service-shell.md) (local only, written by writing-plans skill)
**Crates touched:** `apps/svc` (new), `apps/host`, `crates/core`, `scripts/build.sh`, workspace `Cargo.toml`

## Goal
The host gets the process ADR-0005 describes: `mixlink-svc` runs as a LocalSystem Windows service
(or as a console process for development), installs and uninstalls itself, and serves a local named
pipe. The host tray connects to it and shows whether the service is running. Later rows (T5 discovery,
T6 pairing, T9 streaming, T20 drivers) add their work to this shell instead of building their own.

## Scope
- In:
  - `apps/svc` (package `mixlink-svc`, binary `mixlink-svc.exe`) with four modes:
    service (started by the SCM), `--console`, `install`, `uninstall`.
  - SCM glue (`StartServiceCtrlDispatcherW`, `RegisterServiceCtrlHandlerExW`, `SetServiceStatus`) and
    install / uninstall (`OpenSCManagerW`, `CreateServiceW`, `ChangeServiceConfig2W`, `DeleteService`)
    in `apps/svc/src/platform/windows/`, adapted from the S2 / S4 spikes (`tools/spikes/s4-encode-audio/src/service.rs`).
  - Log file for the service: `%ProgramData%\MixLink\logs\svc.log`.
  - Named pipe server `\\.\pipe\mixlink-svc` with an ACL (ADR-0005) and strict line parsing.
  - Pipe contract `mixlink-core::svc_ipc` (messages, version, line limit) and a small blocking pipe
    client helper there (std only, no `unsafe`), shared by the tray and the tests.
  - Host tray: a background thread that keeps a pipe connection, reconnects, and reports one of
    three states to the UI and the tray tooltip.
  - `scripts/build.sh release` copies `mixlink-svc.exe` to `dist/`.
- Out:
  - Driver install and control (T20, T21, T22); discovery (T5); pairing and PIN (T6); streaming (T9).
  - Encode priority levers and `HIGH_PRIORITY_CLASS` (ADR-0005 amendment): T7 measures which are needed.
  - User-session agent (ADR-0005, later); installer and tray autostart at login (phase 2).
  - Log rotation; Windows Event Log.
  - Vietnamese strings (language switch row, not added yet).
  - Verifying that the pipe server really is the service (`GetNamedPipeServerProcessId` + image check):
    see Open questions.

## Design

### Dependencies
None new. `windows` 0.62.2 (workspace) gains features in `apps/svc`: `Win32_Foundation`,
`Win32_Security`, `Win32_Security_Authorization` (SDDL), `Win32_Storage_FileSystem`,
`Win32_System_Pipes`, `Win32_System_Services`, `Win32_System_Threading`, `Win32_System_Console`
(Ctrl+C in `--console`). `log` + `env_logger` (workspace) for logging.

### Modes (`apps/svc/src/main.rs`, plain `std::env::args` parsing)
| Command | Who runs it | Does |
|---|---|---|
| (no args) | SCM | `StartServiceCtrlDispatcherW`; not started by the SCM → prints a hint to use `--console` or `install`, exit code 2 |
| `--console [--pipe <name>]` | developer, tests | same `run` as the service in the current session; Ctrl+C stops it |
| `install` | admin | creates or updates service `MixLink`, then starts it |
| `uninstall` | admin | stops the service (waits ≤ 10 s), deletes it |
- Both service and console mode call one `run(stop: &StopSignal, opts: RunOptions)`; the only
  difference is who sets the stop signal (SCM Stop / Shutdown vs Ctrl+C).
- `install` details: name `MixLink`, display name and description written with `natural-copy`,
  `SERVICE_WIN32_OWN_PROCESS`, LocalSystem, `SERVICE_AUTO_START` (not delayed: a headless host needs it
  early), image path = the running exe's full path, quoted. Recovery: restart after 5 s on the first
  three failures, counter reset after 1 day. Already installed → updates the config and path, restarts.
- `uninstall` when not installed → message, exit code 0. Not elevated → `install` / `uninstall` print
  "run as administrator" and exit 1 (no self-elevation prompt).
- Exit codes: 0 ok, 1 error (message on stderr), 2 usage.
- Not Windows: `main` prints "Windows only" and exits 1, so `cargo check --workspace` still works elsewhere.

### Service lifecycle (`platform/windows/service.rs`)
- `SERVICE_START_PENDING` → `SERVICE_RUNNING` once the pipe listener is up. A startup error (log dir,
  pipe creation) → logged, `SERVICE_STOPPED` with a service-specific exit code, so SCM recovery restarts it.
- Stop / Shutdown → `SERVICE_STOP_PENDING` with a 5 s wait hint, stop signal set, `run` returns,
  `SERVICE_STOPPED`. Target: stopped within 1 s.
- A panic in `run` is caught at the service boundary (`catch_unwind`), logged, and reported as a failure
  exit code (release builds use `panic = "abort"`: then the process dies and SCM recovery restarts it).

### Logging
- Service mode: `env_logger` with `Target::Pipe` to `%ProgramData%\MixLink\logs\svc.log` (append; the
  folder is created at start). Default level `info`, `RUST_LOG` honoured. Console mode logs to stderr.
- Log lines never contain pipe message bodies above `debug` (later rows carry PINs and keys).

### Named pipe server (`pipe/`, `platform/windows/pipe.rs`)
- Name `\\.\pipe\mixlink-svc` (`svc_ipc::PIPE_NAME`); `--pipe <name>` overrides it in console mode (tests
  use a unique name per run).
- `CreateNamedPipeW`: duplex, byte mode, `PIPE_REJECT_REMOTE_CLIENTS`, `FILE_FLAG_FIRST_PIPE_INSTANCE` on the
  first instance (start fails if another process already owns the name), max 4 instances, 4 KiB buffers.
- Security descriptor from SDDL `D:P(A;;GA;;;SY)(A;;GA;;;BA)(A;;GA;;;OW)(A;;0x12008b;;;IU)`: SYSTEM, Administrators
  and the pipe owner (the creating account: SYSTEM, or the user in `--console`) full; interactive users
  `FILE_GENERIC_READ | FILE_WRITE_DATA` only (no `FILE_CREATE_PIPE_INSTANCE`, so they cannot add a server instance),
  nobody else (no network logons, no services under other accounts).
- Threads (control plane, blocking I/O is fine; no tokio):
  - `svc-pipe-listen`: creates an instance, `ConnectNamedPipe`, hands the handle (as `std::fs::File`) to
    a new `svc-pipe-conn` thread, repeats. A fifth client gets "pipe busy" (seen as not running) and the tray retries.
  - `svc-pipe-conn`: reads JSON lines, answers each, exits on EOF, error or stop.
  - Stop: the stop signal is set, then the listener is woken by a connect to its own pipe; connection
    threads are not joined (the process ends right after `SERVICE_STOPPED`).
- Parsing is strict (the service is SYSTEM, ADR-0005): a line over `svc_ipc::MAX_LINE` (64 KiB) or one
  that does not decode → one `Error` reply, then the connection is closed. A bad client never stops the
  service or other connections.

### Contract (`crates/core/src/svc_ipc.rs`)
- JSON lines, reusing `ipc::encode_line` / `ipc::decode_line` / `IpcError` (no second codec).
- `PIPE_VERSION: u32 = 1`, `PIPE_NAME`, `MAX_LINE`.
- `TrayToSvc` (tag `type`): `Hello { pipe_version }`, `GetStatus`.
- `SvcToTray` (tag `type`): `Hello { pipe_version, svc_version }` (`svc_version` = crate version string),
  `Status { state: SvcState, uptime_s: u64 }`, `Error { message }`.
- `SvcState`: `Idle` only for now; T5 / T6 / T9 add variants (bumping `PIPE_VERSION` when breaking).
- Session: the tray sends `Hello` first. Different `pipe_version` → the service still answers `Hello`
  (so the tray can show both versions), then closes. Any other first message → `Error`, close.
- `SvcPipe` client helper (std `File` opened on the pipe path, `BufReader` for lines; no `unsafe`):
  `connect(name) -> Result<SvcPipe, SvcPipeError>` (does the `Hello` exchange), `status()`,
  `request(&TrayToSvc) -> SvcToTray`; a busy pipe is retried for 250 ms (the server may be between instances).
  `SvcPipeError`: `NotRunning` (pipe missing / still busy), `Io`, `Ipc`,
  `VersionMismatch { ours, theirs, svc_version }`.

### Host tray (`apps/host/src/service_link.rs`)
- Thread `host-svc-link`: `SvcPipe::connect`; on success asks `GetStatus` every 1 s (also the liveness
  check); on any error drops the connection and retries every 2 s. Pure state machine (`LinkState`
  transitions) kept separate from I/O so it is unit-tested.
- States sent to the UI as the Tauri event `svc-status` and kept for a `get_svc_status` command
  (the window may open after the first event): `Connected { svc_version, uptime_s }`, `NotRunning`,
  `VersionMismatch { svc_version }`. Only changes are emitted.
- Tray tooltip follows the state; the UI shows the state where it now says "Host ready".
- Strings: new keys in `apps/host/src/strings.rs` (tooltip) and `ui/src/i18n/en.ts` (window), text
  written with `natural-copy`. No Tauri command besides `get_svc_status`.

### Files (each ≤ 400 lines)
- `apps/svc/src/`: `main.rs` (args, mode dispatch), `run.rs` (shared run loop), `stop.rs` (`StopSignal`),
  `log_setup.rs`, `pipe/server.rs`, `pipe/session.rs` (per-connection message handling, pure, tested),
  `platform/windows/{service.rs, install.rs, pipe.rs, security.rs}`; `apps/svc/CLAUDE.md` module card.
- `crates/core/src/svc_ipc.rs` + `svc_ipc_tests.rs`.

## Latency impact
None: control plane only, not on any per-frame / per-packet / per-input path. The service process
will host those threads later (T7, T9, T10, T11); this spec adds no priority changes.

## Acceptance criteria
- [ ] `mixlink-svc --console` starts, serves the pipe, and stops cleanly on Ctrl+C within 1 s.
- [x] Integration test (no admin): `mixlink-svc --console --pipe <unique>` spawned; `SvcPipe::connect`
      gets `Hello` with `pipe_version` 1 and the crate version; `GetStatus` returns `Idle` with a rising uptime.
- [x] Bad input: a line over 64 KiB, invalid JSON, or `GetStatus` before `Hello` each get one `Error` and a
      closed connection; a second client connected at the same time keeps working.
- [x] Wrong `pipe_version` in `Hello` → service answers `Hello`, closes; `SvcPipe::connect` returns `VersionMismatch`. (service side tested; the client mapping has no test of its own)
- [x] Starting a second `--console` on the same pipe name fails with a clear error (first-instance flag).
- [x] `svc_ipc` round-trips every message through `encode_line` / `decode_line`; unknown `type` is an error.
- [x] Tray `LinkState`: unit tests cover connect, lost connection → `NotRunning` → reconnect, version
      mismatch, and "emit only on change".
- [ ] Manual (user, elevated, main PC): `mixlink-svc install` → service `MixLink` is Running, LocalSystem,
      Automatic, recovery set (`sc qc MixLink`, `sc qfailure MixLink`); log file written in ProgramData.
- [ ] Manual: host tray shows connected with the service version; `sc stop MixLink` → tray shows not
      running within 3 s; `sc start MixLink` → connected again within 3 s, no tray restart.
- [ ] Manual: `install` run twice updates without error; `uninstall` removes it, run again exits 0;
      `install` without admin prints the admin hint and exits 1.
- [ ] Manual: pipe ACL from `Get-Acl \\.\pipe\mixlink-svc` (or accesschk) shows only SYSTEM, Administrators, Owner Rights,
      Interactive; the tray running as a standard user connects.
- [x] `dist/` from `build.sh release` contains `mixlink-svc.exe`.
- [x] `apps/svc/CLAUDE.md`, `apps/host/CLAUDE.md`, `crates/core/CLAUDE.md` describe the new API; root
      `CLAUDE.md` module map already lists `apps/svc`.

## Open questions
- Pipe server identity: a non-admin process that creates `\\.\pipe\mixlink-svc` before the service starts
  could impersonate it to the tray. Harmless while the pipe only carries status; must be closed before
  T6 puts the PIN on the pipe (check `GetNamedPipeServerProcessId` → the service's process, or a
  SYSTEM-owned server). Proposed: T6 handles it.
- Calls made in this spec (review them): no `windows-service` crate (spike code reused); service name
  `MixLink`, Automatic (not delayed) start; `install` starts the service; tray polls every 1 s and retries
  every 2 s; max 4 pipe clients; stdout / stderr log via `env_logger` file target, no rotation.
