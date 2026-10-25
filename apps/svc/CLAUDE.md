# mixlink-svc

> Module card. Keep <= 50 lines. Update when the public API changes.

**Purpose:** Host Windows service (ADR-0005, spec 0012): LocalSystem, Session 0. Today a shell that serves
the tray pipe and installs our drivers (spec 0014); later it owns discovery, pairing, capture -> encode -> net,
audio, input.
**Depends on:** mixlink-core (`svc_ipc` pipe contract).
**Hot path:** none yet.

## Layout
- `main.rs` (mode dispatch, exit codes), `args.rs`, `stop.rs` (`StopSignal`), `error.rs` (`SvcError`),
  `log_setup.rs`, `run.rs` (`run`, `RunOptions`: shared by service and console), `pipe/session.rs` (pure
  per-connection handling), `pipe/server.rs` (listener + one thread per client); each with `<name>_tests.rs`.
- `strings.rs`: Services entry, install / uninstall, driver and status messages (ADR-0003).
- `drivers/` (spec 0014, pure): `package.rs` (`DRIVERS`, `resolve_source`, `stage_dir`), `state.rs`
  (`DriverState`), `flow.rs` (`DriverHost` trait, `install` / `uninstall` step order + rollback, tested with a
  fake host), `status.rs` (`format_status`), `mod.rs` (`run`: the `drivers` commands).
- `platform/windows/`: `service.rs` (dispatcher, status, `SERVICE_NAME`), `install.rs` (create / update / delete,
  recovery), `pipe.rs` (instances), `security.rs` (pipe ACL), `console.rs` (Ctrl+C); drivers: `cert.rs` +
  `cert_der.rs` (key + self-signed cert, key deletion check), `sign.rs` (`SignerSignEx3`), `trust_store.rs`
  (Root + TrustedPublisher), `catalog.rs` (members + signature), `driver_store.rs`, `devices.rs` (root nodes,
  veto), `drivers_reg.rs` (state in HKLM), `dir_security.rs` (staging ACL), `drivers_host.rs`
  (`WindowsDriverHost`, `is_elevated`, `query_status`). All `unsafe` here.
- `tests/console_mode.rs`: the binary with `--console --pipe <name>` serves the pipe.

## Modes (process contract)
- No args: started by the service manager. `--console [--pipe <name>]`: same work in this session.
- `install` (admin): creates or updates service `MixLink` (LocalSystem, Automatic, this exe, restart after 5 s
  on the first 3 failures), then starts it. `uninstall` (admin): stops and deletes it; not installed = ok.
- `drivers install [--from <dir>]` (admin): removes what is there (as `drivers uninstall`), then signs
  `vdisplay` + `vhid` with a fresh per-machine cert and installs them, no prompt. Packages from
  `<exe dir>\drivers\<name>` or `--from` (`target\drivers`). `drivers uninstall` (admin): devices,
  packages, certs. `drivers status`: no admin.
- Service mode logs to `%ProgramData%\MixLink\logs\svc.log`; console mode to stderr (`RUST_LOG`).
- Exit codes: 0 ok, 1 error, 2 usage, 3 a program holds a MixLink device open (`drivers install | uninstall`).

## Notes
- The pipe carries input from any interactive user while we run as SYSTEM: parse strictly, close on
  anything unexpected, never log message bodies above `debug`.
- Driver signing tests use a user-scope key and the built `vhid` package (skip when not built).
- Code that later opens the drivers must release them before `drivers uninstall`.

## Manual checks (elevated terminal; no automated SCM / driver install test)
- `mixlink-svc install`, then `sc qc MixLink`, `sc qfailure MixLink`; run `install` again (update path).
- `sc stop MixLink` / `sc start MixLink`; `uninstall` twice (second says not installed, exit 0).
- Drivers: spec 0014 → Acceptance criteria / Results.
