# mixlink-svc

> Module card. Keep <= 50 lines. Update when the public API changes.

**Purpose:** Host Windows service (ADR-0005, spec 0012): LocalSystem, Session 0. Today a shell that serves
the tray pipe; later it owns discovery, pairing, drivers, capture -> encode -> net, audio, input.
**Depends on:** mixlink-core (`svc_ipc` pipe contract).
**Hot path:** none yet.

## Layout
- `main.rs` (mode dispatch, exit codes), `args.rs`, `stop.rs` (`StopSignal`), `error.rs` (`SvcError`),
  `log_setup.rs`, `run.rs` (`run`, `RunOptions`: shared by service and console), `pipe/session.rs` (pure
  per-connection handling), `pipe/server.rs` (listener + one thread per client); each with `<name>_tests.rs`.
- `platform/windows/`: `pipe.rs` (instances), `security.rs` (pipe ACL), `console.rs` (Ctrl+C): all `unsafe` here.
- `tests/console_mode.rs`: the binary with `--console --pipe <name>` serves the pipe.

## Modes (process contract)
- No args: started by the service manager. `--console [--pipe <name>]`: same work in this session.
- `install` / `uninstall`: register / remove service `MixLink` (admin).
- Exit codes: 0 ok, 1 error, 2 usage.

## Notes
- The pipe carries input from any interactive user while we run as SYSTEM: parse strictly, close on
  anything unexpected, never log message bodies above `debug`.
