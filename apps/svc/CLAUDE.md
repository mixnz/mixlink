# mixlink-svc

> Module card. Keep <= 50 lines. Update when the public API changes.

**Purpose:** Host Windows service (ADR-0005, spec 0012), LocalSystem, Session 0: tray pipe, drivers (0014), virtual
display (0015), virtual input (0016), LAN discovery (0017), pairing + secure sessions on the session port (0018).
**Depends on:** mixlink-core (`svc_ipc`, `config`, `device_io`), mixlink-proto, mixlink-input, mixlink-net.
**Hot path:** none yet (the session thread is control plane).

## Layout
- `main.rs` (mode dispatch, exit codes, `host_run_options`), `args.rs`, `stop.rs`, `error.rs` (`SvcError`),
  `log_setup.rs`, `run.rs` (`run`, `RunOptions`), `json_file.rs` (`write_atomic`), `pipe/session.rs` (pure, per
  connection; `PairingPort`), `pipe/server.rs` (listener + one thread per client); each with `<name>_tests.rs`.
- `drivers/` (spec 0014, pure): `package.rs` (`DRIVERS`: hardware IDs per package), `state.rs`, `flow.rs`
  (`DriverHost` trait, install / uninstall order + rollback, fake host in tests), `status.rs`, `mod.rs` (`run`).
- `display/` (spec 0015), pure: `wire.rs` (= `drivers/vdisplay/Control.h`), `edid.rs` + `cvt.rs`, `modes.rs`,
  `topology.rs`, `store.rs` (`topology.json`), `ports.rs` (`DisplayDriver`, `DisplayConfig`), `session.rs`
  (`begin` / `end` / `start_up`), `agent.rs` (helper protocol). Windows: `service.rs` (thread `svc-display`;
  Session 0 → helper), `commands.rs`. `host_config.rs`: `%ProgramData%\MixLink\host.json`.
- `input/` (spec 0016, ADR-0009): `feeder.rs` (`open_feeder`; `run.rs` holds it), `pads.rs` (`PadHost`, `add`,
  `remove`, `remove_all`), `pad_ids.rs`, `padmap.rs`, `script.rs`, `probe*.rs`, `status.rs`, `commands.rs`.
- Discovery (0017): `identity.rs`, `discovery.rs`, `net_check.rs`. Pairing (0018): `secrets.rs` (`secrets` folder:
  `host_key.json`, `paired.json`), `session_listen.rs` (thread `svc-session`, `PairingShared` for the pipe).
- `platform/windows/` (all `unsafe`): `service.rs`, `install.rs` (`is_running`), `pipe.rs`, `security.rs`,
  `console.rs` (Ctrl+C); drivers: `cert.rs` + `cert_der.rs`, `sign.rs`, `trust_store.rs`, `catalog.rs`,
  `driver_store.rs`, `devices.rs` (root nodes, veto), `drivers_reg.rs`, `dir_security.rs`, `drivers_host.rs`
  (`WindowsDriverHost`, `is_elevated`); display: `vdisplay.rs`, `ccd.rs` + `display_config.rs`,
  `display_agent.rs` + `console_process.rs` (helper as SYSTEM in the console session), `gpu_name.rs`;
  input: `pad_nodes.rs` (`WindowsPadHost`), `xinput.rs`, `raw_input.rs`; discovery: `random.rs`, `com.rs`
  (`ComGuard`), `firewall.rs`, `network_category.rs`. `strings.rs` (user text), `input_args.rs`; `tests/console_mode.rs`.

## Modes (process contract)
- No args: started by the service manager. `--console [--pipe <name>]`: same work in this session.
- `install` (admin): creates or updates service `MixLink` (LocalSystem, Automatic, this exe, restart after 5 s
  on the first 3 failures), starts it, adds firewall rule `MixLink` (UDP discovery + session port from `host.json`,
  Private + Domain; port change → `install` again). `uninstall` (admin): stops, deletes, removes the rule.
- `drivers install [--from <dir>]` (admin): removes what is there, then signs `vdisplay` + `vhid` with a fresh
  per-machine cert and installs them, no prompt (packages: `<exe dir>\drivers\<name>` or `--from`). `drivers
  uninstall` (admin): devices, packages, certs. Both restart the service. `drivers status`: no admin.
- `display status`, `display try --client <32 hex> --mode WxH@Hz --topology virtual-only|extend|keep` (admin,
  service stopped). Hidden: `display probe | reject-check`, `display-agent --in <f> --out <f>` (the helper).
- Input (admin; "in use" while the service runs): `input status`, `try --script <name> [--delay <s>]`, `pad
  [--slot n] [--delay <s>]` (type `a`, `rt 500`), `pad-add | pad-remove --slot n`, hidden `probe`.
- Logs: service `%ProgramData%\MixLink\logs\svc.log`, console stderr (`RUST_LOG`). Exit codes: 0 ok, 1 error,
  2 usage, 3 a program holds a MixLink device open (`drivers install | uninstall`).

## Notes
- Pipe input comes from any user and we run as SYSTEM: parse strictly, close on surprises, never log bodies above
  `debug`; the pipe never carries keys. `secrets\` is secured before the key is written, else no pairing (logged).
  Service and `--console` hold `vdisplay` + `vhid` for life. Manual checks: specs 0014-0018 Results.
