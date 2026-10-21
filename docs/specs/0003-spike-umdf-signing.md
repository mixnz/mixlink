# 0003 — Spike S1: UMDF driver signed by a per-machine certificate

**Status:** in-progress
**Date:** 2026-10-22
**Roadmap:** T14
**Plan:** [0003-spike-umdf-signing](../plans/0003-spike-umdf-signing.md) (local only, written by writing-plans skill)
**Crates touched:** none. New: `drivers/vdisplay/` (C++), `drivers/CLAUDE.md`, `tools/spikes/s1-signing/`

## Goal
Prove or disprove ADR-0004's core bet: a UMDF 2 driver whose catalog is signed at install time by a
fresh per-machine certificate (private key deleted right after) installs and loads on Windows 11 with
Secure Boot on and test-signing off. Every driver row (T20–T22) waits on this answer.

## Scope
- In:
  - `drivers/vdisplay/`: Microsoft `IddSampleDriver` (Windows-driver-samples, MIT, notice kept), built
    unchanged except names/IDs (`MixLink Virtual Display`). An IddCx driver is used, not a dummy, so a
    pass is visible (a monitor appears) and S2 (T15) starts from it.
  - Build: MSBuild + WDK from NuGet (`Microsoft.Windows.WDK.x64`, no system WDK install), one script
    producing `vdisplay.inf`, `vdisplay.dll`, unsigned `vdisplay.cat` (Inf2Cat from the same package).
    Version pinned to `10.0.28000.2526` (pulls `Microsoft.Windows.SDK.cpp.x64` 10.0.28000 itself).
    The package brings its own MSBuild driver toolset and IddCx headers; the system SDK is not used.
  - Build prerequisites: VS Build Tools with the C++ workload **and** the components
    "C++ Spectre-mitigated libraries for x64/x86 (Latest MSVC)" (driver projects link Spectre libs) and
    "Windows Driver Kit" (MSBuild toolset `WindowsUserModeDriver10.0`; the NuGet package has no toolset).
  - `tools/spikes/s1-signing/install.ps1` (elevated), steps in Design. `uninstall.ps1` reverses all.
  - Two signing methods tried in order: `Set-AuthenticodeSignature` on the `.cat`; if it cannot sign a
    catalog, `SignerSignEx3` (`mssign32.dll`, present on every Windows) from a tiny test program.
  - Results written into this spec (section Results) and the decision for ADR-0004 (holds / fallback).
- Out:
  - Rust installer, service, IOCTLs, monitor modes, EDID per client (T20, T21).
  - HID driver (S3), frame sharing (S2), anti-cheat (S5).
  - CI build of drivers (T20). Gate is unchanged.

## Design

### Install steps (`install.ps1`, run elevated by the user)
1. Record state: Secure Boot (`Confirm-SecureBootUEFI`), test-signing (`bcdedit`), Memory integrity
   (HVCI), Smart App Control, OS build. Abort if test-signing is on (the result would mean nothing).
2. `New-SelfSignedCertificate -Type CodeSigningCert` in `Cert:\LocalMachine\My`, subject
   `CN=MixLink Driver (this PC)`, key non-exportable, 30-year validity.
3. Sign `vdisplay.cat` (SHA-256 digest; timestamp not used, the cert never expires in practice).
4. Export the public cert only; import into `LocalMachine\Root` and `LocalMachine\TrustedPublisher`.
5. Delete the cert **with its key** from `LocalMachine\My`; verify no private key remains anywhere.
6. `pnputil /add-driver vdisplay.inf /install`; create the root-enumerated device node
   (`devcon install` from the WDK package for the spike; T20 replaces it with SetupAPI calls in Rust).
7. Report: device status (`Get-PnpDevice`), driver signer (`Get-AuthenticodeSignature` on the `.cat`),
   whether the new monitor is listed (`Get-CimInstance Win32_DesktopMonitor` / display settings).

### Test matrix (each run on a clean state after `uninstall.ps1`)
| Case | Expected if ADR-0004 holds |
|---|---|
| A. Normal install | Device OK, a second monitor appears, no Windows "unverified publisher" prompt |
| B. Reboot after A | Driver loads again at boot, monitor present before login |
| C. Cert not trusted (skip step 4) | Install refused (negative control: signature is really checked) |
| D. `vdisplay.dll` changed after signing | Install or load refused (hash in catalog enforced) |
| E. Memory integrity on (if it was off, or the reverse) | Same as A |
| G. SAC state recorded on every case (Evaluation expected) | No SAC block events in Code Integrity log |
| F. `uninstall.ps1` | Device, driver package and both certs gone; `pnputil /enum-drivers` clean |

### Test machine
A freshly installed Win11 PC, Secure Boot on, Smart App Control (SAC) in **Evaluation** mode.
- Never turn SAC off there: it cannot be turned back on without a reset.
- Evaluation does not block, but Windows may switch it to On or Off by itself. `install.ps1` records
  the SAC state on every run; a run where SAC changed state is noted in Results.
- Evaluation is the accepted level for this spike. SAC On is not tested (ADR-0004 already expects it
  to block the unsigned app).
Scripts change machine trust stores, so **the user runs them**; the AI session only builds and reads output.

## Latency impact
None. No hot path is touched.

## Acceptance criteria
- [ ] One script builds `vdisplay.inf/.dll/.cat` from a clean clone using only VS Build Tools + NuGet.
- [ ] Case A: after `install.ps1`, the device is `OK` and Windows shows an extra monitor.
- [ ] Step 5 verified: no private key for the MixLink cert exists on the machine after install.
- [ ] Cases B–F give the expected result, or the spec records exactly what differed.
- [ ] Results section filled: OS build, Secure Boot / HVCI / SAC state (and any SAC state change), signing method that worked,
  each case's outcome, and the verdict: ADR-0004 holds, or switch to its fallback.

## Results
(filled when the spike runs)

## Open questions
- (Answered 2026-10-22) The WDK NuGet package includes Inf2Cat and devcon: checked in the nuget.org
  catalog file list of `10.0.28000.2526` and `10.0.26100.6584`. Paths in the package:
  `c/bin/<ver>/x86/Inf2Cat.exe` (.NET Framework tool, with its `HardwareWorkflow` DLLs),
  `c/tools/<ver>/x64/devcon.exe`, `c/tools/<ver>/x64/infverif.exe`. No system WDK install needed.
