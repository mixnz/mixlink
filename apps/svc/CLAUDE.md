# mixlink-svc

> Module card. Keep <= 50 lines. Update when the public API changes.

**Purpose:** Host Windows service (ADR-0005, spec 0012): LocalSystem, Session 0. Serves the tray pipe,
installs our drivers (spec 0014), drives the virtual display (spec 0015); later discovery, pairing, media, input.
**Depends on:** mixlink-core (`svc_ipc`, `config`), mixlink-proto (`DisplayInfo`).
**Hot path:** none yet.

## Layout
- `main.rs` (mode dispatch, exit codes), `args.rs`, `stop.rs` (`StopSignal`), `error.rs` (`SvcError`),
  `log_setup.rs`, `run.rs` (`run`, `RunOptions`: shared by service and console), `pipe/session.rs` (pure
  per-connection handling), `pipe/server.rs` (listener + one thread per client); each with `<name>_tests.rs`.
- `strings.rs`: Services entry, install / uninstall, driver and status messages (ADR-0003).
- `drivers/` (spec 0014, pure): `package.rs` (`DRIVERS`, `resolve_source`, `stage_dir`), `state.rs`
  (`DriverState`), `flow.rs` (`DriverHost` trait, `install` / `uninstall` step order + rollback, tested with a
  fake host), `status.rs` (`format_status`), `mod.rs` (`run`: the `drivers` commands).
- `display/` (spec 0015), pure: `wire.rs` (IOCTL bytes = `drivers/vdisplay/Control.h`), `edid.rs` + `cvt.rs`,
  `modes.rs`, `topology.rs` (snapshot, `plan_restore`, `plan_extend`), `store.rs` (`topology.json`), `ports.rs`
  (`DisplayDriver`, `DisplayConfig`), `session.rs` (`begin` / `end` / `start_up`; fakes in tests), `agent.rs`
  (helper JSON protocol, `AgentConfig`). Windows: `service.rs` (thread `svc-display`; Session 0 → helper),
  `commands.rs`. `host_config.rs`: `%ProgramData%\MixLink\host.json`.
- `platform/windows/`: `service.rs` (dispatcher, status, `SERVICE_NAME`), `install.rs` (create / update / delete,
  recovery, `is_running`), `pipe.rs`, `security.rs` (pipe ACL), `console.rs` (Ctrl+C); drivers: `cert.rs` +
  `cert_der.rs`, `sign.rs` (`SignerSignEx3`), `trust_store.rs`, `catalog.rs`, `driver_store.rs`, `devices.rs`
  (root nodes, veto), `drivers_reg.rs` (state in HKLM), `dir_security.rs` (SYSTEM + Admins folder ACL),
  `drivers_host.rs` (`WindowsDriverHost`, `is_elevated`, `query_status`); display: `vdisplay.rs` (`VDisplay`),
  `ccd.rs` + `display_config.rs` (`CcdConfig`), `display_agent.rs` + `console_process.rs` (helper as SYSTEM
  in the console session), `gpu_name.rs`. All `unsafe` here.
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
- Service mode logs to `%ProgramData%\MixLink\logs\svc.log`; console mode to stderr (`RUST_LOG`).
- Exit codes: 0 ok, 1 error, 2 usage, 3 a program holds a MixLink device open (`drivers install | uninstall`).

## Notes
- Pipe input comes from any interactive user and we run as SYSTEM: parse strictly, close on surprises.
- Never log pipe message bodies above `debug`. Signing tests: user-scope key, built `vhid` (skipped if absent).
- Service and `--console` hold the `vdisplay` handle for their lifetime; closing it departs our monitors.
- Manual checks (elevated; no automated SCM / driver test): `install`, `sc qc|qfailure|stop|start`, `uninstall`
  twice; drivers: spec 0014; display: spec 0015 → Results.
