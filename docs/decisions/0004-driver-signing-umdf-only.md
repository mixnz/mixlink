# 0004 — Own drivers are UMDF only, signed per machine, never by Microsoft

**Status:** accepted · 2026-10-22

## Context
The host needs drivers: a virtual display (ADR-0006) and virtual HID input devices (ADR-0007).
The project will never pay for Microsoft signing (EV certificate + Partner Center attestation).
- Kernel-mode drivers (KMDF/WDM) load only when Microsoft-signed (Win10 1607+, Secure Boot on).
- User-mode drivers (UMDF 2) install when their catalog (`.cat`) chains to a certificate the
  machine trusts (Trusted Root + Trusted Publisher). No test-signing mode needed.
- Test-signing mode is not an option: kernel anti-cheats (Vanguard, FACEIT…) refuse to run with it.
- Shipping one shared signing certificate is unsafe: whoever gets its private key can sign code
  that every installed machine trusts.

## Decision
- **Our drivers are UMDF 2 only.** No kernel-mode driver of our own, ever.
- **Per-machine signing at install time.** The elevated installer:
  1. creates a fresh key pair in the machine certificate store;
  2. signs the prebuilt driver catalog with it (`Set-AuthenticodeSignature`);
  3. adds the public certificate to Trusted Root and Trusted Publisher;
  4. deletes the private key, so nothing else can ever be signed with it.
- Uninstall removes the drivers, then the certificate from both stores.
- **Drivers are written in C++** on top of the Microsoft WDK samples (MIT): `IddSampleDriver`
  for IddCx, `vhidmini2` (UMDF build) for HID. Rust has no IddCx bindings and its UMDF support
  is immature. This amends ADR-0001 (Rust everywhere) for `drivers/` only.
- Driver sources live in `drivers/<name>/`, built with MSBuild + WDK, outside the Cargo workspace.
  Rust talks to them only through IOCTLs from `platform/` modules.
- Third-party drivers that are already signed by their authors may be used as optional
  components (e.g. ViGEmBus, ADR-0007). We still sign nothing with Microsoft.
- Our `.exe` files stay unsigned.

## Consequences
- Risk to prove first (spike S1): a catalog signed this way installs and loads on Win11 24H2
  with Secure Boot on. If it fails, fallback is an author-signed open-source IddCx driver
  (e.g. Virtual-Display-Driver); HID input then falls back to SendInput.
- Install needs admin rights once (service + drivers + certificate).
- Unsigned `.exe`: SmartScreen warns on first run; Windows 11 Smart App Control, when on,
  blocks the app. Document both; nothing to fix in code.
- Build needs Visual Studio + WDK on dev machines and CI; the gate gets a driver build step.
- Any feature that would need a kernel driver (e.g. native XInput device) is out of reach
  unless a signed third-party driver provides it.
