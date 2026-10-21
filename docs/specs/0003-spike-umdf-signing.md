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
**Machine:** ASUS desktop, Windows 11 Pro 26H2 build 26300.9711, UEFI. Secure Boot was found **off** on the
first check and enabled in firmware on 2026-10-22 before any run. HVCI (Memory integrity) on, SAC Evaluation,
test-signing off. Code Integrity logs event 3084 at boot: "WHQL driver enforcement for this boot session".
**Build:** WDK/SDK NuGet `10.0.28000.2526`, VS Build Tools 2026 (MSVC 14.51), sample `bb68ed5`.
Logs: `tools/spikes/s1-signing/out/` (local only).

| Case | Expected | Observed | SAC | Log |
|---|---|---|---|---|
| A | Device OK, extra monitor, no prompt | **Pass.** Catalog signed by `Set-AuthenticodeSignature` (no fallback needed); key deleted (CNG container + file checked); `pnputil` 0 (`oem18.inf`), `devcon` 0; `ROOT\DISPLAY\0000` OK, problem 0; driver-store `.cat` Valid, signer `MixLink Driver (this PC)`; setupapi: signer score Authenticode, "signed ... from a trusted publisher"; 3 monitors added (sample's `IDD_SAMPLE_MONITOR_COUNT`), screens 1 → 4, Settings > Display shows displays 1–4 extended; no prompt; no CI events | Eval | `20261022-013724-A-install` |

| B | Loads again at boot | **Pass.** After reboot (boot 01:43): `ROOT\DISPLAY\0000` OK, problem 0; store `.cat` Valid; 4 screens; CI log has only boot policy events (3116/3099) and 3084 "WHQL driver enforcement" — no block event, so that enforcement does not cover this UMDF driver | Eval | `20261022-014504-B-report` |
| F | Device, package, both certs gone | **Pass.** `pnputil /remove-device` 0, `/delete-driver oem18.inf /uninstall` 0, cert removed from Root and TrustedPublisher; `RESULT CLEAN`; screens back to 1; no `vdisplay.inf_*` in FileRepository, no `drivers\UMDF\vdisplay.dll`, no `MixLinkVDisplay` service key | Eval | `20261022-014549-F-uninstall` |
| C | Install refused (cert not trusted) | **Pass (refused).** Catalog signed, key deleted, trust skipped; `pnputil` fails 0x800B0109 "terminated in a root certificate which is not trusted"; setupapi: "catalog file certificate does not belong to Trusted Root Certificates, and Code Integrity is enforced"; `devcon` exit 2; no monitor added; refused outright, no prompt offered | Eval | `20261022-014813-C-install` |
| D | Install or load refused (hash enforced) | **Pass (refused), with a prompt.** Cert trusted, one byte appended to `vdisplay.dll` after signing. setupapi: "Catalog did not contain file hash ... appears to be tampered" (0xE000024B). Windows showed "Windows can't verify the publisher of this driver software" with **"Install this driver software anyway"**, once for `pnputil` and once for `devcon`; user chose "Don't install" both times → `pnputil` fails 0xE000024B, `devcon` exit 2, no monitor | Eval | `20261022-014950-D-install` |
| D2 (extra) | Does load time catch what install let through? | **Not caught at load.** Same as D, user chose "Install anyway" (one prompt, from `pnputil`): setupapi "appears to be tampered, but user wants to install it anyway"; `pnputil` 0, `devcon` 0, device OK, problem 0, 3 monitors, no CI events. Loaded `drivers\UMDF\vdisplay.dll` is the tampered one (50 689 bytes, SHA-256 `7B1A7D3E…` vs signed `408C5CE8…`, file `NotSigned`) | Eval | `20261022-015256-D2-install` |
| E | HVCI off: same as A | **Pass.** Memory integrity turned off + reboot (`Hvci: False`, Secure Boot still on): signed by `Set-AuthenticodeSignature`, key deleted, `pnputil` 0, `devcon` 0, device OK, problem 0, store `.cat` Valid, 4 screens, no CI events. A (HVCI on) and E (HVCI off) behave the same | Eval | `20261022-015845-E-install` |
| G | SAC recorded every run, no SAC blocks | **Pass.** SAC `Evaluation` in all 15 logs (no state change); CI Operational log 01:00–02:03: 60 events, all Information (28× 3116, 28× 3099 policy refresh at boot, 4× 3084), no warning, error or block event | Eval | all |

After the runs the machine is back to its start state (`20261022-020250-final-report`): Secure Boot on,
HVCI on, SAC Evaluation, no MixLink device, driver package or certificate.

Notes on A:
- First attempt (`20261022-013307`) signed fine but aborted before pnputil: the key-deletion check read an
  empty `RSACng.Key.UniqueName`. Fixed (`0bddf91`); an elevated listing of `Crypto\Keys` showed no key file
  from that run, so `Remove-Item -DeleteKey` had removed it.
- C: `devcon install` still creates an empty `ROOT\DISPLAY\0000` node (no driver INF) before failing;
  `uninstall.ps1` removes it. T20 must create the node only after the driver package is accepted.
- D: a tampered package is not refused outright like C: an interactive admin can override it. T20's
  installer must run non-interactively (no UI: fail instead of prompting) and verify the catalog itself.
- D2: for UMDF the catalog is checked **at install only**; HVCI does not re-check the user-mode DLL at load.
  Protection after install rests on the file ACLs of `System32\drivers\UMDF` (admin / TrustedInstaller),
  i.e. an attacker who can replace it is already admin. Acceptable for ADR-0004; T20 must not loosen those ACLs.
- setupapi warns "WDF schema 2.23 when section requires 2.25" (UMDF 2.25 vs this OS's INF schema); install is not affected.

## Open questions
- (Answered 2026-10-22) The WDK NuGet package includes Inf2Cat and devcon: checked in the nuget.org
  catalog file list of `10.0.28000.2526` and `10.0.26100.6584`. Paths in the package:
  `c/bin/<ver>/x86/Inf2Cat.exe` (.NET Framework tool, with its `HardwareWorkflow` DLLs),
  `c/tools/<ver>/x64/devcon.exe`, `c/tools/<ver>/x64/infverif.exe`. No system WDK install needed.
