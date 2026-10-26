# drivers (UMDF, C++)

> Module card. Keep <= 50 lines. Update when the public API changes.

**Purpose:** Our own UMDF 2 drivers, signed per machine at install time (ADR-0004). Not in Cargo.
**Depends on:** WDK + SDK from NuGet (`10.0.28000.2526`, pinned in `vdisplay/packages.config`).
**Hot path:** yes: `SwapChainProcessor::RunCore` → `FrameShare::Publish` per frame.

## Layout
- `vdisplay/`: IddCx virtual display. Microsoft `IddSampleDriver` (Windows-driver-samples
  `bb68ed5e770bce04dc5395ec9258dafab034184d`, MIT, `vdisplay/LICENSE`), renamed:
  hardware ID `Root\MixLinkVDisplay`, files `vdisplay.inf/.dll/.cat`. Code still uses `IddSample*` names.
  - Spec 0015: no monitor until `ADD_MONITOR`. `Control.{h,cpp}`: interface GUID, IOCTLs (layout =
    `apps/svc/src/display/wire.rs`), strict checks; INF SDDL SYSTEM + Admins. `Monitors.{h,cpp}`: `MonitorRegistry`
    (≤ 4, modes from the request; a closed handle departs its monitors; no lock across IddCx calls).
  - S2 (spec 0004): `SharedHeader` (`Global\MixLinkS2Frames` v2), `FrameShare`: 4 shared textures + fence, or
    (L3) the slot names the surface. Only the first running swapchain publishes (`FrameShareClaim`).
  - S2b levers (spec 0005), `Levers.{h,cpp}`: registry `HKLM\SOFTWARE\MixLink\S2b\Levers` read per swapchain.
    L1 MMCSS "Pro Audio" critical, L2 `SetGPUThreadPriority(7)`, L3 `SurfaceShare.{h,cpp}` (shares the IddCx
    surface itself), L4 `SwapChain12`/`FrameShare12` (D3D12 copy queue, compiled only for IddCx >= 1.11).
  - `RenderAdapter.{h,cpp}`: `PickRenderAdapter` (most VRAM; hybrid = >1 physical GPU by vendor/device/
    subsystem/revision, since DXGI can list one GPU twice). Not yet tested on an iGPU + dGPU machine.
- `vhid/`: UMDF HID minidriver from `vhidmini2` (same samples commit, MS-PL, `vhid/LICENSE`), C, below inbox
  `mshidumdf.sys`, spec 0016. Nodes in one WUDFHost: `Root\MixLinkVHid` (VID/PID `DEED/FEED`; reports 1 kbd
  NKRO, 2 mouse rel, 3 mouse abs, `Descriptor.c`), `Root\MixLinkVHidCtl` (control: `Control.{h,c}`, SYSTEM +
  Admins, one feeder owns all nodes), Xbox pads `SWD\MixLinkPad\000N` (HW ID `Root\VID_045E&PID_0B13&IG_00`,
  real BT descriptor `PadDescriptor.c`, inbox `xinputhid` on top, ADR-0009). HOT PATH `Feed.c`: `FEED`
  completes a pending read or keeps the last report (Get Input Report); feeder close = neutral reports.
- `Directory.Build.props`: imports the NuGet WDK/SDK props from `drivers/packages/` (gitignored).
- `build.ps1 [-Driver vdisplay|vhid|all]`: restore + build + Inf2Cat → `target/drivers/<name>/pkg/` (catalog unsigned).

## Build
- `powershell -NoProfile -ExecutionPolicy Bypass -File drivers/build.ps1 [-Driver vhid] [-Configuration Debug]`
- Needs VS Build Tools with: C++ workload, "C++ Spectre-mitigated libraries for x64/x86 (Latest MSVC)",
  "Windows Driver Kit" (MSBuild toolset `WindowsUserModeDriver10.0`). No system WDK install.

## Notes
- Use the 64-bit MSBuild (`Bin\amd64`): WDK targets choose the NuGet `bin\x64` tools only when
  `PROCESSOR_ARCHITECTURE` is `AMD64`; the 32-bit MSBuild fails with "Failed to locate stampinf.exe".
- `Inf2CatUseLocalTime=true` / `/uselocaltime`: stampinf writes a local-time `DriverVer`; east of UTC
  Inf2Cat otherwise rejects it as "postdated".
- `SignMode=Off`: the WDK build must never create or use a `WDKTestCert`.
- Driver isolation: both INFs install the DLL to DIRID 13 (runs from the driver store); `infverif /w`
  passes for both (`scripts/check-drivers.sh`, full gate and CI `drivers` job).
- Sign + install only with `mixlink-svc drivers install | uninstall | status` (spec 0014), never `s1-signing`.
- New `.cpp` files include `Driver.h` first (UMDF_USING_NTSTATUS needs it before WRL).
- Build for IddCx 1.4 (`IDDCX_VERSION_MINOR` 4, define 6): a 1.11 build fails `IddCxAdapterInitAsync` with
  `STATUS_INVALID_PARAMETER` on 24H2 (IddCx 1.10), so no monitor arrives. Newer DDIs: runtime checks only.
- Verdicts: S1 (spec 0003) per-machine signed catalog installs and loads, UMDF files checked at install only;
  S2b (spec 0005) with L1 + L3 IddCx p99 256 µs vs DDA 422 µs, 0 lost → ADR-0006 holds.
