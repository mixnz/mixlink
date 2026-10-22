# drivers (UMDF, C++)

> Module card. Keep <= 50 lines. Update when the public API changes.

**Purpose:** Our own UMDF 2 drivers, signed per machine at install time (ADR-0004). Not in Cargo.
**Depends on:** WDK + SDK from NuGet (`10.0.28000.2526`, pinned in `vdisplay/packages.config`).
**Hot path:** yes: `SwapChainProcessor::RunCore` → `FrameShare::Publish` per frame.

## Layout
- `vdisplay/`: IddCx virtual display. Microsoft `IddSampleDriver` (Windows-driver-samples
  `bb68ed5e770bce04dc5395ec9258dafab034184d`, MIT, `vdisplay/LICENSE`), renamed:
  hardware ID `Root\MixLinkVDisplay`, files `vdisplay.inf/.dll/.cat`. Code still uses `IddSample*` names.
  - S2 (spec 0004): one monitor, preferred mode 2560x1440@120 (`Driver.cpp`).
  - `FrameShare.{h,cpp}`: 4 shared NT-handle textures + shared fence + `Global\MixLinkS2Frames` header
    (SYSTEM + LocalService). Layout mirrored in `tools/spikes/s2-iddcx-frames/src/header.rs`.
  - `RenderAdapter.{h,cpp}`: `PickRenderAdapter` (most VRAM; hybrid = >1 physical GPU by vendor/device/
    subsystem/revision, since DXGI can list one GPU twice). Not yet tested on an iGPU + dGPU machine.
- `Directory.Build.props`: imports the NuGet WDK/SDK props from `drivers/packages/` (gitignored).
- `build.ps1`: restore + build + Inf2Cat → `target/drivers/vdisplay/pkg/` (catalog unsigned).

## Build
- `powershell -NoProfile -ExecutionPolicy Bypass -File drivers/build.ps1 [-Configuration Debug]`
- Needs VS Build Tools with: C++ workload, "C++ Spectre-mitigated libraries for x64/x86 (Latest MSVC)",
  "Windows Driver Kit" (MSBuild toolset `WindowsUserModeDriver10.0`). No system WDK install.

## Notes
- Use the 64-bit MSBuild (`Bin\amd64`): WDK targets choose the NuGet `bin\x64` tools only when
  `PROCESSOR_ARCHITECTURE` is `AMD64`; the 32-bit MSBuild fails with "Failed to locate stampinf.exe".
- `Inf2CatUseLocalTime=true` / `/uselocaltime`: stampinf writes a local-time `DriverVer`; east of UTC
  Inf2Cat otherwise rejects it as "postdated".
- `SignMode=Off`: the WDK build must never create or use a `WDKTestCert`.
- `infverif /w` fails on the sample (DLL in `%12%\UMDF`, not DIRID 13); `/v` passes. Driver isolation
  is for T20.
- Signing and install live in `tools/spikes/s1-signing/` (spike) and later in T20 tooling.
- New `.cpp` files include `Driver.h` first (UMDF_USING_NTSTATUS needs it before WRL).
- S2 verdict (spec 0004): frame sharing works from Session 0; p99 above DDA's → T23 before ADR-0006 changes.
- S1 verdict (spec 0003): per-machine signed catalog installs and loads (Secure Boot on, HVCI on/off).
  UMDF files are checked at install only; install without UI (spec 0003 → Verdict lists T20 constraints).
