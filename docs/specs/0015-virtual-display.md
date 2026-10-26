# 0015 — Virtual display: monitors on demand, stable EDID per client, topology, monitor before login

**Status:** done
**Date:** 2026-10-25
**Roadmap:** T21
**Plan:** [0015-virtual-display](../plans/0015-virtual-display.md) (local only, written by writing-plans skill)
**Crates touched:** `drivers/vdisplay`, `apps/svc`, `crates/core` (config)

## Goal
The service controls the virtual display (ADR-0006): it adds a monitor with exactly the client's mode and
a fixed identity per client, makes it the main display (virtual only or extend), and puts the user's
previous screen layout back afterwards, also after a crash. With no physical monitor active, the host has
a virtual monitor from boot, so the login screen can be streamed. T9 calls the same code per session;
until then a developer drives it with `mixlink-svc display` commands.

## Scope
- In:
  - `vdisplay`: no monitor at start; IOCTL control channel (add, remove, query); modes from the IOCTL;
    up to 4 monitors; monitor leaves when the handle that added it closes.
  - `apps/svc`: driver client, EDID builder, topology save / switch / restore, crash recovery at start,
    headless monitor at start, `display` dev commands, render adapter in `display status`.
  - `core::config::HostConfig`: `display.topology` setting (`virtualOnly` default, `extend`).
  - `drivers install | uninstall` stop the `MixLink` service first and start it again after.
- Out:
  - HDR (needs IddCx 1.10 DDIs; this build targets 1.4, spec 0005). Kept possible, see Design → HDR later.
  - Per-monitor DPI (no public API, ADR-0006); hardware cursor (T7 / T9); frame path changes (T7).
  - Wiring to a real client session (T9); tray UI for the topology setting (settings row, not added yet).
  - More than one client monitor per session (one stream at a time in phase 1).
  - Reacting to physical monitors plugged / unplugged while idle: checked only at service start and
    session end.
  - Renaming the `IddSample*` identifiers in the driver.

## Design

### Driver control channel (`drivers/vdisplay`)
- Device interface with a new MixLink GUID (`WdfDeviceCreateDeviceInterface`); IOCTLs through
  `IDD_CX_CLIENT_CONFIG::EvtIddCxDeviceIoControl` (IddCx 1.0+, so fine for the 1.4 build).
- INF `Security` on the device: SYSTEM and Administrators only (`D:P(A;;GA;;;SY)(A;;GA;;;BA)`). The
  input still comes from a privileged caller, but the driver checks every size and range anyway.
- Wire structs in `drivers/vdisplay/Control.h`, mirrored in `apps/svc/src/display/wire.rs` with size and
  offset tests (pattern of the S2 `header.rs`). Every request starts with `version` (1) and `flags`.

| IOCTL | In | Out | Driver does |
|---|---|---|---|
| `ADD_MONITOR` | flags, container GUID, EDID (128 B base + N × 128 B ext), mode list (≤ 16 × w, h, refresh mHz), preferred index | monitor id (u32) | `IddCxMonitorCreate` + `IddCxMonitorArrival` |
| `REMOVE_MONITOR` | monitor id | — | `IddCxMonitorDeparture` |
| `QUERY` | — | monitors (id, connector, mode), render adapter LUID, IddCx version | read state |

- Checks: `version == 1`; `flags == 0` (bit 0 = HDR reserved, rejected for now); EDID length a multiple
  of 128, ≤ 4 blocks, checksums right; 1..=16 modes, width 640..=7680, height 480..=4320, refresh
  1..=500 Hz; free connector. Anything else → `STATUS_INVALID_PARAMETER`, no monitor.
- Modes: `ParseMonitorDescription` and `MonitorQueryModes` answer from the mode list stored with the
  monitor (looked up by EDID bytes / monitor object), not by parsing the EDID: a base EDID cannot describe
  1440p at 240 Hz. Refresh goes in as the rational `{refresh_mhz, 1000}`. The list is the client mode
  (preferred) plus the standard modes from today's `MonitorQueryModes` that are not larger than it, so
  Windows has a fallback.
- `MaxMonitorsSupported = 4`. `IddSampleAdapterInitFinished` keeps `PickRenderAdapter` and adds no monitor.
- Owner: each monitor records the file object that added it; on that file's cleanup the driver departs
  it. A crashed service therefore leaves no monitor. If IddCx does not let the driver see file cleanup
  (checked in the first plan task), fallback: `QUERY` + `REMOVE` of every monitor when the service opens
  the driver, and the service restarts within 5 s after a crash (spec 0012).
- Code: `Driver.cpp` (880 lines today) keeps DDI glue only; new `Monitors.{h,cpp}` (create, depart,
  mode list, owner) and `Control.{h,cpp}` (IOCTL parse and dispatch). New `.cpp` include `Driver.h` first.

### EDID builder (`apps/svc/src/display/edid.rs`, pure)
- EDID 1.4 base block, checksum right, 0 extensions now; the API returns blocks so a CTA-861 block can
  be added later without changing callers.
- Identity from the client's `DeviceId` (16 B): manufacturer `MXL`, product code `0x4D58`, 32-bit serial
  = first 4 bytes, serial string descriptor = hex of the first 6 bytes, name descriptor `MixLink`. Windows
  keys its per-monitor settings (scale, position, HDR later) on manufacturer + product + serial, so each
  client keeps its own. Container GUID = the `DeviceId` bytes with version 4 / variant bits set.
- Headless monitor: fixed id (all bytes `0x00` except the last, `0x01`), product code `0x4D59`, name
  `MixLink`, 1920×1080@60. The two product codes let the topology code tell the client monitor from the
  headless one (one client monitor at a time).
- Physical size in cm from `DisplayInfo.dpi` (width / dpi × 2.54), so Windows' default scale is close to
  the client's. dpi 0 → size 0 (undefined).
- Preferred detailed timing = client mode when its pixel clock fits in an EDID DTD (≤ 655.35 MHz, CVT-RB
  timing); otherwise 1920×1080@60 there and the real mode comes only from the IOCTL mode list.
- Unit tests: checksum, identity fields, size from dpi, two different ids give different serials, the
  same id gives the same bytes.

### Driver client (`apps/svc/src/platform/windows/vdisplay.rs`)
`VDisplay::open()` finds the interface (`CM_Get_Device_Interface_ListW`), opens it, and wraps the three
IOCTLs (`DeviceIoControl`). Not found → typed error "driver not installed". Behind a `DisplayDriver` trait
so the session logic is tested with a fake.

### Topology (`apps/svc/src/display/topology.rs` pure + `platform/windows/topology.rs`)
- APIs: `QueryDisplayConfig(QDC_ALL_PATHS)` / `SetDisplayConfig(SDC_APPLY | SDC_USE_SUPPLIED_DISPLAY_CONFIG |
  SDC_SAVE_TO_DATABASE | SDC_ALLOW_CHANGES)` (spec 0008: not `ChangeDisplaySettingsEx`).
- **Save:** before the first switch, the active paths with source / target modes, written to
  `%ProgramData%\MixLink\display\topology.json` (that folder gets the drivers staging DACL). Each target is keyed
  by its monitor device path (`DisplayConfigGetDeviceInfo` target name), never by LUID (changes per boot).
  An existing file is never overwritten: it already holds the user's real layout.
- **Switch:** find the new virtual target by monitor device path, give it the client mode at desktop
  origin (0, 0), so it is the main display. `virtualOnly`: every other path inactive. `extend`: other paths
  stay active, moved right of the virtual monitor.
  - A MixLink target counts as found only once it has a monitor device path (right after `ADD_MONITOR` it
    reports its EDID ids with an empty path).
  - `extend` places the displays from the snapshot taken before `ADD_MONITOR`: when a known client monitor
    arrives, Windows first applies the layout it remembers for that monitor set (e.g. `virtualOnly`).
  - A path taken from an inactive display reports scan-line ordering "unspecified"; with a refresh rate
    set and no target mode, SetDisplayConfig refuses that (`ERROR_INVALID_PARAMETER`), so it becomes
    progressive.
- **Restore:** map saved targets to today's paths by monitor device path, apply, delete the file.
  A saved monitor that is gone is skipped; if none is left, fall back to `SDC_TOPOLOGY_EXTEND`.
- **Session 0:** `QueryDisplayConfig` from the service fails with `ERROR_ACCESS_DENIED` (Results). In
  Session 0 the service therefore uses a helper (`display/agent.rs`, `platform/windows/display_agent.rs`):
  per call it writes the request as JSON into the snapshot folder, runs `mixlink-svc display-agent --in <f>
  --out <f>` as SYSTEM in the active console session (own token, `TokenSessionId` set, `CreateProcessAsUserW`,
  `winsta0\default`, ≤ 10 s, then killed) and reads the JSON reply. The helper attaches to the input desktop
  (`OpenInputDesktop` + `SetThreadDesktop`) and calls the same `CcdConfig`. Outside Session 0 (`display try`)
  `CcdConfig` runs in process.

### Session flow (`apps/svc/src/display/session.rs`, pure over traits)
- `begin(client_id, DisplayInfo, Topology)`: save topology → `ADD_MONITOR` → wait until its path shows up
  (≤ 5 s) → switch → depart the headless monitor if present.
- `end()`: `REMOVE_MONITOR` → restore → add the headless monitor if no physical path is active.
- Any failure in `begin` undoes the steps done so far (remove monitor, restore).

### Service start
1. Open the driver; not there yet → retry every 500 ms up to 30 s (auto-start can beat the adapter init).
2. Leftover `topology.json` → restore (crash or power loss during a session).
3. No active path with a real non-MixLink monitor → add the headless monitor. With nothing plugged in,
   Windows keeps one path active (1024x768) whose target has no monitor device path; it does not count.
   The active displays seen here are logged at `info`.
- The service keeps the handle for its lifetime. `drivers install | uninstall` stop the service first
  (else the open handle vetoes the removal) and start it again after when it was running.

### Dev commands (`mixlink-svc display ...`, admin, for T21 checks until T9)
| Command | Does |
|---|---|
| `display status` | monitors (id, mode), render GPU name + LUID, saved topology present or not |
| `display try --client <32 hex> --mode 2560x1440@120 [--topology virtual-only\|extend\|keep]` | `begin`, then waits for Enter / Ctrl+C, then `end`; `keep` adds the monitor only and leaves the layout alone |
- `display try` refuses while the `MixLink` service runs (two owners of the topology). Killing it mid-way
  is the crash test: monitor leaves with the handle, the next start restores the layout.
- Hidden: `display probe --out <file>` (layout read + re-apply, Session 0 check), `display reject-check`
  (malformed `ADD_MONITOR` requests), `display-agent --in <f> --out <f>` (the helper). `RUST_LOG=debug`
  logs each path handed to SetDisplayConfig.

### Render adapter
The driver keeps choosing (`PickRenderAdapter`: most VRAM, set only on hybrid machines). `QUERY` returns
the LUID, `display status` prints the DXGI adapter name for it. The check on an iGPU + dGPU machine is an
acceptance row. Letting the service override the choice is not needed in phase 1.

### HDR later (not built here)
Kept possible by: `flags` bit 0 reserved in every IOCTL, EDID builder returning extension blocks,
`DisplayInfo.hdr` already in the protocol. The HDR row needs IddCx 1.10 (`...2` DDIs, FP16 / 10-bit
surfaces), a CTA-861 HDR block, 10-bit encode and an HDR swapchain on the client.

### Config
`HostConfig.display.topology: Topology` (`virtualOnly` | `extend`, default `virtualOnly`). The service reads
`%ProgramData%\MixLink\host.json` when present, else defaults; an invalid file is logged and defaults used.

## Latency impact
None per frame: all of this is control plane at session start / end. Mode = client mode at client
refresh removes scaling and rate mismatch, and the virtual monitor as main display gave the lower IddCx
baseline (specs 0004, 0005). Session start cost (add + switch) is measured and reported in Results.

## Acceptance criteria
- [x] Driver installed, service stopped: no MixLink monitor exists (after install; not checked on a cold boot).
- [x] `display try` with 2560x1440@120 and 1920x1080@60: Windows shows a "MixLink" monitor at exactly that
      mode and refresh, as main display; Enter → monitor gone, previous layout back (positions, main, modes).
- [x] `virtual-only` turns physical monitors off during `try`; `extend` keeps them on at the right.
- [x] Two client ids: each keeps its own scale set in Windows Settings across `try` runs.
- [ ] `display try` killed from Task Manager: the monitor goes away; next service start restores the layout
      and deletes `topology.json`. Power cut during `try`: layout right after the next boot.
      (Kill + service-start restore pass; power cut not tried.)
- [ ] Headless (no physical monitor active), service running: a virtual monitor exists from boot, the login
      screen and lock screen render to it (checked with the S2 harness or a DDA grab).
      (Monitor from boot passes and the login screen went to it; no frame grab yet, lock screen not tried.)
- [x] Host with a monitor, service running: no headless monitor is added.
- [x] `drivers uninstall` with the service running: stops it, removes everything, exit 0; `drivers install`
      starts it again. (Checked with `drivers install`, which runs the uninstall steps first.)
- [x] `ADD_MONITOR` with a bad EDID checksum, 0 modes, 17 modes, 8K+ width, `flags = 1`: rejected, no monitor.
- [ ] iGPU + dGPU machine: `display status` names the dGPU; frames still arrive (S2 harness). No such host yet.
- [x] Session start time (`try` → monitor main) reported for both topologies on the main PC.

## Open questions
- (Answered) IddCx lets the driver see file cleanup: killing the owner departs its monitors within ~2 s.
- (Answered) CCD from Session 0: `ERROR_ACCESS_DENIED`; the console-session helper works (Design → Topology).
- iGPU + dGPU host for the render adapter row: none yet; the row stays open like spec 0004's.
- A physical monitor plugged in while the headless monitor is main: the login prompt stays on the
  virtual monitor, so a person at the host signs in blind. Reacting to plug / unplug while idle is out of
  scope here; it needs its own row (move the headless monitor aside or remove it when a monitor arrives).

## Results
Main PC (RTX 5070, MSI 3840x2160@160 on DP, Windows IddCx `0x10c00`), elevated, 2026-10-26.
- `display probe` in session 1: CCD read + re-apply work. `edidManufactureId` is EDID bytes 8..10 read
  little-endian (`DEL` = `0xac10`, `MXL_MFG_ID`); the product code as stored.
- Driver reinstall: no monitor until `ADD_MONITOR`; render GPU RTX 5070. `try --topology keep` 2560x1440@120:
  "MixLink", 120 Hz in Advanced display; Enter removes it. `taskkill` of `try`: monitor gone within ~2 s.
- `reject-check`: all five malformed requests refused, 0 monitors after.
- `extend` 2560x1440@120: main at (0,0), MSI right, restore exact; ready in 269 ms (1356 ms when Windows
  first applied a remembered `virtualOnly` layout). `virtualOnly`: MSI off, restore exact; ready 381–659 ms.
  1920x1080@60 `extend`: 147–365 ms.
- Fixed during the runs: restore of a display that was off (`ERROR_INVALID_PARAMETER`, scan-line ordering);
  switch before the monitor had a device path; `extend` turning the MSI off for a client that had used
  `virtualOnly` (Windows applies its remembered layout on arrival).
- Two client ids at 1920x1080@60: 125 % kept for one, default for the other, across runs.
- Kill during `extend`: layout kept on disk. Service start (Session 0): CCD denied (error 5) → helper added;
  then the service restored the layout through the helper and deleted the file.
- `drivers install` with the service running: service stopped, drivers reinstalled, service started, exit 0.
- Cold boot with the MSI unplugged: Windows keeps a 1024x768 path with no monitor; after excluding it the
  service added the headless monitor (`Some(1)`, 1920x1080@60) 9 s after boot, and the login prompt was on
  it (signed in blind after plugging the MSI back). With the MSI plugged in: no headless monitor.
