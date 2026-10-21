# 0005 — Host media pipeline runs in a Windows service

**Status:** accepted · 2026-10-22

## Context
The host PC is often headless and must be usable through reboot, login screen, lock screen,
UAC prompts, logoff and user switch. A process in the user session dies at logoff, cannot see
the secure desktop, and cannot inject into elevated windows (UIPI).
With frames taken from the virtual display driver (ADR-0006) and input sent through virtual
HID devices (ADR-0007), no stage of the pipeline needs to run inside a user session.

## Decision
- **`mixlink-svc`** (new app `apps/svc`): Windows service, LocalSystem, Session 0, auto-start.
  It owns the whole host pipeline:
  - network: discovery, pairing, UDP media/input/control;
  - driver control over IOCTL: add/remove virtual monitors and HID devices;
  - display topology: save, switch, restore (ADR-0006);
  - capture (shared texture from IddCx) -> hardware encode -> net;
  - WASAPI loopback -> Opus -> net;
  - input from net -> virtual HID.
- **`mixlink-host`** (existing tray app) becomes a pure control UI in the user session:
  status, PIN, paired devices, settings. It talks to the service over a local named pipe
  with an ACL limited to the interactive user and admins. It is not needed for streaming.
- **User-session agent** (later, not on the hot path): small process the service starts in the
  active console session (`WTSGetActiveConsoleSessionId` + `CreateProcessAsUser`) for things
  that need a user session: clipboard, launching a game, notifications.
- Media crates stay unaware of the process model; `apps/svc` wires them.

## Consequences
- Streaming survives login, lock, UAC, logoff and user switch with no process hand-off.
- Login screen is streamable after reboot (virtual monitor exists before login, ADR-0006).
- Risk to prove (spike S4): hardware encoders (NVENC/AMF/QSV) and WASAPI loopback work from
  Session 0. Fallback: the pipeline moves into a SYSTEM process started in the console session.
- Module map in `CLAUDE.md` gains `apps/svc`; `apps/host` loses "wires capture->codec->net".
- Service install/uninstall is part of the installer (ADR-0004).
- Debugging a service is harder: `mixlink-svc` must also run as a console process (`--console`)
  for development.
- A bug in the service runs as SYSTEM: network input parsing must be strict and fuzzed.
