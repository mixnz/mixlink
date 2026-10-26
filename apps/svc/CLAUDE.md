# mixlink-svc

> Module card. Keep <= 50 lines. Update when the public API changes.

**Purpose:** Host Windows service (ADR-0005, spec 0012): LocalSystem, Session 0. Serves the tray pipe,
installs our drivers (spec 0014), drives the virtual display (0015), holds virtual input (0016); later more.
**Depends on:** mixlink-core (`svc_ipc`, `config`, `device_io`), mixlink-proto, mixlink-input (`hid`, `VHid`).
**Hot path:** none yet.

## Layout
- `main.rs` (mode dispatch, exit codes), `args.rs`, `stop.rs` (`StopSignal`), `error.rs` (`SvcError`),
  `log_setup.rs`, `run.rs` (`run`, `RunOptions`: shared by service and console), `pipe/session.rs` (pure
  per-connection handling), `pipe/server.rs` (listener + one thread per client); each with `<name>_tests.rs`.
- `strings.rs`: Services entry, install / uninstall, driver and status messages (ADR-0003).
- `drivers/` (spec 0014, pure): `package.rs` (`DRIVERS`: hardware IDs per package), `state.rs`, `flow.rs`
  (`DriverHost` trait, install / uninstall order + rollback, fake host in tests), `status.rs`, `mod.rs` (`run`).
- `display/` (spec 0015), pure: `wire.rs` (= `drivers/vdisplay/Control.h`), `edid.rs` + `cvt.rs`, `modes.rs`,
  `topology.rs`, `store.rs` (`topology.json`), `ports.rs` (`DisplayDriver`, `DisplayConfig`), `session.rs`
  (`begin` / `end` / `start_up`), `agent.rs` (helper protocol). Windows: `service.rs` (thread `svc-display`;
  Session 0 → helper), `commands.rs`. `host_config.rs`: `%ProgramData%\MixLink\host.json`.
- `input/` (spec 0016, Windows): `open_feeder` (open `vhid`, feed neutral reports to own it; `run.rs` holds
  it), `status.rs`, `script.rs` (`steps`, `play` for `input try`), `commands.rs`.
- `platform/windows/` (all `unsafe`): `service.rs`, `install.rs` (`is_running`), `pipe.rs`, `security.rs`,
  `console.rs` (Ctrl+C); drivers: `cert.rs` + `cert_der.rs`, `sign.rs`, `trust_store.rs`, `catalog.rs`,
  `driver_store.rs`, `devices.rs` (root nodes, veto), `drivers_reg.rs`, `dir_security.rs`, `drivers_host.rs`
  (`WindowsDriverHost`, `is_elevated`); display: `vdisplay.rs`, `ccd.rs` + `display_config.rs`,
  `display_agent.rs` + `console_process.rs` (helper as SYSTEM in the console session), `gpu_name.rs`.
- `tests/console_mode.rs`: the binary with `--console --pipe <name>` serves the pipe.

## Modes (process contract)
- No args: started by the service manager. `--console [--pipe <name>]`: same work in this session.
- `install` (admin): creates or updates service `MixLink` (LocalSystem, Automatic, this exe, restart after 5 s
  on the first 3 failures), then starts it. `uninstall` (admin): stops and deletes it; not installed = ok.
- `drivers install [--from <dir>]` (admin): removes what is there (as `drivers uninstall`), then signs
  `vdisplay` + `vhid` with a fresh per-machine cert and installs them, no prompt. Packages from
  `<exe dir>\drivers\<name>` or `--from` (`target\drivers`). `drivers uninstall` (admin): devices,
  packages, certs. Both stop the service first and start it again. `drivers status`: no admin.
- `display status`, `display try --client <32 hex> --mode WxH@Hz --topology virtual-only|extend|keep` (admin,
  service stopped). Hidden: `display probe | reject-check`, `display-agent --in <f> --out <f>` (the helper).
- `input status` (admin): driver, wire version, report sizes, dropped, feeder. `input try --script keys|chord|mouse|
  abs|pad|all|hold [--delay <s>]` (admin): scripted input as the feeder; "in use" while the service runs.
- Service mode logs to `%ProgramData%\MixLink\logs\svc.log`; console mode to stderr (`RUST_LOG`).
- Exit codes: 0 ok, 1 error, 2 usage, 3 a program holds a MixLink device open (`drivers install | uninstall`).

## Notes
- Pipe input comes from any interactive user and we run as SYSTEM: parse strictly, close on surprises. Never log
  pipe bodies above `debug`. Signing tests: user-scope key, built `vhid` (skipped if absent).
- Service and `--console` hold the `vdisplay` and `vhid` handles for life; closing them departs our monitors and
  releases every key / button. Manual checks (elevated): `install`, `sc qc|qfailure|stop|start`, `uninstall`
  twice; drivers: spec 0014; display: 0015; input: 0016 → Results.
