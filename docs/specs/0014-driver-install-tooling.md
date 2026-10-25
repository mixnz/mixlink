# 0014 — Driver install tooling: per-machine signing, install / uninstall, WDK build in gate and CI

**Status:** in-progress
**Date:** 2026-10-25
**Roadmap:** T20
**Plan:** [0014-driver-install-tooling](../plans/0014-driver-install-tooling.md) (local only, written by writing-plans skill)
**Crates touched:** `apps/svc`, `drivers/vdisplay` (INF only), `scripts/`, `.github/workflows/ci.yml`

## Goal
One admin command puts our two UMDF drivers (`vdisplay`, `vhid`) on a host the way ADR-0004 says:
a fresh per-machine certificate signs the catalogs, its private key is deleted, the drivers install
without any Windows prompt. Another command removes drivers and certificate and leaves the machine
clean. A third reports what is installed. The phase 2 installer only calls these commands; T21 / T22
build on a host where the drivers are installed this way. The driver build runs in the gate and in CI.

## Scope
- In:
  - `mixlink-svc drivers install | uninstall | status` (Rust, in `apps/svc`), native Windows APIs only:
    no PowerShell, `pnputil` or `devcon` (spec 0003 D: `pnputil` showed the "Install anyway" prompt).
  - Per-machine certificate: create, sign both catalogs, delete the private key and check it is gone,
    trust (Root + TrustedPublisher), remove on uninstall and on update.
  - Package check before install: every file in the package is a catalog member with the right hash.
  - Install the driver packages, then create the root device nodes (`Root\MixLinkVDisplay`,
    `Root\MixLinkVHid`); update path when already installed.
  - Uninstall that names the app holding a device open (Steam Input holds `vhid`, spec 0008).
  - `vdisplay.inf`: driver isolation (DLL in DIRID 13), so `infverif /w` passes for both drivers.
  - Gate step and CI job that build both drivers and run `infverif /w`.
  - `scripts/build.sh release` builds the drivers and copies the unsigned packages to `dist/drivers/`.
- Out:
  - The installer and its UI, tray button for drivers (phase 2). In phase 1 a developer runs the commands.
  - Adding / removing monitors, EDID, topology (T21); HID report features (T22); ViGEmBus (T12).
  - Removing a held device at next boot: T20 reports the holder and stops (see Open questions).
  - The WDF schema 2.23 / 2.25 setupapi warning (spec 0003): does not affect install.
  - Vietnamese strings (language switch row, not added yet).
  - Changes to `tools/spikes/s1-signing/` (spike code stays as it is).

## Design

### Commands (process contract, extends spec 0012)
| Command | Elevated | Does | Exit codes |
|---|---|---|---|
| `drivers install [--from <dir>]` | yes | install or update both drivers | 0 ok, 1 error |
| `drivers uninstall` | yes | remove devices, packages, certificates | 0 ok or nothing installed, 1 error, 3 device held open |
| `drivers status` | no | one line per driver + certificate state | 0 |
- Package source: `<exe dir>\drivers\<name>\` (the `dist/` layout); `--from <dir>` points at a folder
  holding `<name>\` subfolders, for development (`target\drivers`, the `pkg\` level is resolved).
- Not elevated → "run as administrator", exit 1, no self-elevation (same as `install`, spec 0012).
- Exit code 3 is new; `main.rs` documents it next to 0 / 1 / 2.
- The service does not open the drivers in T20; later rows that do must stop using them before an
  uninstall (noted in the module card).

### Install steps (`drivers install`)
Spec 0003 Verdict and 0008 Verdict set the order. `SetupSetNonInteractiveMode(TRUE)` is called first:
any SetupAPI step that would show UI fails instead.
1. **Stage:** copy each package to `%ProgramData%\MixLink\drivers\<name>\` (recreated each run), folder
   DACL protected, SYSTEM + Administrators full control only. The source folder is never changed.
2. **Certificate:** `CertCreateSelfSignCertificate` with a new CNG key (`NCryptCreatePersistedKey`,
   RSA 3072, machine key), subject `CN=MixLink Driver (this PC)`, code-signing EKU, 20-year validity.
3. **Sign:** both staged `.cat` files with `SignerSignEx3` (SHA-256, no timestamp).
4. **Delete the key:** `NCryptDeleteKey`, then check by container name that `NCryptOpenKey` fails with
   `NTE_BAD_KEYSET` (spec 0003: do not trust `RSACng.Key.UniqueName`). A key that survives → error, stop.
5. **Trust:** add the certificate (public part only) to `LocalMachine\Root` and `TrustedPublisher`.
6. **Verify:** for every file in each staged package, `WinVerifyTrust` with `WTD_CHOICE_CATALOG`
   against that package's catalog, `WTD_UI_NONE`, revocation off. Any file that fails, or a file that
   is not a catalog member → error, roll back steps 5 and 2, stop. This checks hash and chain together.
7. **Install packages:** `DiInstallDriverW` per staged INF (no `DIIRFLAG_FORCE_INF`). Failure → stop.
8. **Device nodes:** only now (spec 0003 C) create a root node per hardware ID that has none yet
   (`SetupDiCreateDeviceInfoW` + `SPDRP_HARDWAREID` + `DIF_REGISTERDEVICE`), then
   `UpdateDriverForPlugAndPlayDevicesW` with `INSTALLFLAG_NONINTERACTIVE`. Existing nodes get the
   update only.
9. **Record and clean up:** write the state (below); on an update, remove the previous packages
   (`DiUninstallDriverW` on their published `oem*.inf`) and the previous certificates, in that order.
- Reruns are updates: a new certificate every time, old one removed only after the new packages are in.
- A failure after step 5 and before step 9 leaves the new certificate trusted and records it, so the
  next `install` or `uninstall` cleans it up. Nothing is left that the state does not name.

### Uninstall steps (`drivers uninstall`)
1. For each device node with our hardware IDs: `CM_Query_And_Remove_SubTreeW` (no UI). A veto →
   message asks to close the program holding it and run again, exit 3, nothing else removed. With
   veto type `PNP_VetoWindowsApp` the message names it (`pszVetoName`); otherwise (e.g. an open handle)
   it names Steam as the usual holder (spec 0008). Then `DiUninstallDevice` for the node.
2. `DiUninstallDriverW` for every published INF in the state.
3. Remove every certificate with our subject from Root and TrustedPublisher (by subject, so leftovers
   from a broken run go too), delete the staged folder and the state.
- Order matters: certificates go last so a driver that could not be removed stays trusted.
- Nothing installed → message, exit 0.

### State
`HKLM\SOFTWARE\MixLink\Drivers`: `CertThumbprint` (hex SHA-1) and per driver `<name>.PublishedInf`
(`oemNN.inf`). Written and read by a small pure module so its parsing has unit tests; the registry
calls live in `platform/windows/`.

### Status (`drivers status`)
Per driver: package published or not (`SetupGetInfDriverStoreLocationW` on the published INF), device
present, `CM_Get_DevNode_Status` problem code. Certificate: present in both stores, thumbprint matches
the state. Output is plain text for people; the tray contract over the pipe comes with T21 / T22.

### Code layout (`apps/svc`)
- `drivers/mod.rs`: command entry, step order, rollback; `drivers/state.rs` (+ `_tests.rs`): state
  record; `drivers/package.rs` (+ `_tests.rs`): locating packages, driver list (name, INF, hardware ID).
- `platform/windows/`: `cert.rs` (create, delete key, trust, remove), `sign.rs` (`SignerSignEx3`),
  `catalog.rs` (`WinVerifyTrust`), `devices.rs` (SetupAPI / CfgMgr32), `drivers_reg.rs` (state in the
  registry), staged-folder DACL reuses `security.rs`. All `unsafe` stays here with `// SAFETY:`.
- `error.rs`: `SvcError` gains driver variants with the failing step and Win32 / HRESULT code.
- `strings.rs`: every message (natural-copy), `en` only.
- New `windows` features for `apps/svc`, checked against 0.62.2 docs while planning:
  `Win32_Security_Cryptography`, `Win32_Security_Cryptography_Catalog`, `Win32_Security_WinTrust`,
  `Win32_Devices_DeviceAndDriverInstallation`, `Win32_System_Registry`. `SignerSignEx3` and its
  `SIGNER_*` structs are in `Win32_Security_Cryptography` (checked in 0.62.2).

### Driver package fix: isolation
`vdisplay.inf` copies `vdisplay.dll` to `%12%\UMDF`; change to DIRID 13 (run from the driver store) like
`vhid.inf` already does: `ServiceBinary=%13%\vdisplay.dll`, no copy section to `drivers\UMDF`. Then
`infverif /w` passes for both. Spec 0003 D2 relied on `System32\drivers\UMDF` ACLs; the driver store
is protected the same way (TrustedInstaller), so ADR-0004's protection does not change.

### Build in gate and CI
- `scripts/check-drivers.sh`: finds the WDK toolset (`vswhere` + `WindowsUserModeDriver10.0`); found →
  `drivers/build.ps1 -Driver all` + `infverif /w` on both INFs, errors fail; not found → `WARN` and skip.
  Runs in the full gate only (not `--fast`).
- CI: new job `drivers` on `windows-latest` (input option `drivers`), NuGet packages cached by
  `packages.config` hash, runs `check-drivers.sh` with skip turned into an error (`MIXLINK_REQUIRE_WDK=1`).
- `scripts/build.sh release`: runs `drivers/build.ps1`, copies `target/drivers/<name>/pkg/*` to
  `dist/drivers/<name>/`. A failed driver build fails the release build.

## Latency impact
None: install-time only, no hot path touched. DIRID 13 changes where `vdisplay.dll` loads from, not how.

## Acceptance criteria
Machine checks run by the user in an elevated terminal on the main PC (Secure Boot on, HVCI on,
test-signing off), logged in the spec's Results like spec 0003.
- [ ] Clean machine: `drivers install` exits 0 with no Windows prompt; both devices OK, problem code 0;
  the virtual monitor appears; the certificate is in Root and TrustedPublisher; no private key exists
  for it (`certutil -store my` / key container check shows none).
- [ ] After a reboot both devices are still OK.
- [ ] `drivers install` again (update): exits 0, one certificate and one published INF per driver left.
- [ ] A package copy with one byte appended to its `.dll` (`--from` that copy) → install fails at verify,
  no prompt, no device node, no certificate left.
- [ ] Steam running with Steam Input: `drivers uninstall` exits 3 and asks to close Steam (by name or
  as the usual holder); after closing Steam it exits 0.
- [ ] After `drivers uninstall`: no device, no `oem*.inf` for our drivers in the driver store, no
  certificate with our subject in either store, no `%ProgramData%\MixLink\drivers`, no state key.
  `drivers uninstall` again exits 0 ("nothing installed").
- [ ] `drivers status` shows each state above correctly and works without elevation.
- [ ] Not elevated: `install` / `uninstall` print "run as administrator", exit 1, change nothing.
- [ ] `infverif /w` passes for `vdisplay.inf` and `vhid.inf`; the CI `drivers` job builds both.
- [ ] `build.sh release` puts both unsigned packages in `dist/drivers/`, and `drivers install` run from
  `dist/` uses them.

## Open questions
- Does `windows-latest` have the `WindowsUserModeDriver10.0` toolset (WDK VS component)? Checked with the
  first CI dispatch; if missing, the job adds the component with the VS installer before building.
- Remove a held `vhid` at next boot instead of asking the user to close the app: decide when the
  installer UI exists (phase 2); T20 only reports the holder.
- Does `vdisplay` (IddCx) behave the same from DIRID 13? Expected yes (current Microsoft samples use it);
  the first two acceptance checks confirm it.
