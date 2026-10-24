# 0008 — Spike S5: anti-cheat titles launch with our drivers installed

**Status:** in-progress
**Date:** 2026-10-23
**Roadmap:** T18
**Plan:** [0008-spike-anticheat-launch](../plans/0008-spike-anticheat-launch.md) (local only, written by writing-plans skill)
**Crates touched:** none. New: `tools/spikes/s5-anticheat/`, `anti-cheat.md` in `docs/compat/`

## Goal
Find out whether EA Javelin (Apex Legends) and Easy Anti-Cheat (Elden Ring) start and their games reach
the main menu while our per-machine signed drivers (`vdisplay`, `vhid`, ADR-0004) and certificates are
installed. Vanguard, BattlEye and FACEIT stay untested. Gameplay and input acceptance are not tested: they
become a warning shown to users (Design → Warning).

## Scope
- In:
  - Launch checks of **Apex Legends** (EA Javelin Anticheat) and **Elden Ring** (Easy Anti-Cheat, normal
    start, not offline mode), each in every case, on the user's one PC (the only machine; no spare PC,
    no spare hardware ID). Apex was first taken for EAC; it ships Javelin instead.
  - `tools/spikes/s5-anticheat/`: `run-all.ps1` runs A-D from one elevated command (installs / removes
    drivers, resumes after the reboot, switches the main display); `report.ps1` records machine and
    anti-cheat state per case (Design).
    Drivers are installed with the existing S1 (`vdisplay`) and S3 (`vhid`) scripts.
  - Results in this spec (S1 format) and the verdict for ADR-0007's S5 risk.
  - `anti-cheat.md` in `docs/compat/`: launch table (Javelin and EAC tested, others "not tested") + the user
    warning text (EN), written with the `natural-copy` skill. Row in `docs/README.md`.
- Out:
  - Any match, mode or lobby with other players (Firing Range included: it needs no other players, but it
    is gameplay), loading an Elden Ring save; ranked anything. Each game stays in its main menu.
  - Input from `vhid` or `s3-feed` inside the game, scripted or not. The user uses their own physical
    keyboard and mouse only, and only in menus.
  - Vanguard, BattlEye, FACEIT titles. Long-play or delayed (server-side) ban checks. Not tested now or
    later (no spare machine / account per HWID); covered by the warning only. No new roadmap row.
  - ViGEmBus: not installed during this spike.
  - Any attempt to hide our drivers from, or work around, an anti-cheat. A refusal is recorded as is.
  - Showing the warning in the app UI (i18n keys): the installer / client UI task that needs it (phase 2).

## Design

### Cases
| Case | Machine state | Check |
|---|---|---|
| A. Baseline | no MixLink driver or certificate (`report.ps1` confirms) | anti-cheat starts, main menu reached. Fails → game / machine issue, not ours |
| B. Installed | `vdisplay` + `vhid` installed and signed per machine, rebooted once, physical monitor main | anti-cheat starts, main menu reached, no anti-cheat warning; game left at the menu 5 min |
| C. Stream layout | as B, virtual monitor set as main display (the stream topology, ADR-0006) | same as B, game shows on the virtual monitor |
| D. Clean-up | both drivers uninstalled (`RESULT CLEAN`) | both games still launch (no lasting flag on the machine) |

B runs after a reboot because kernel anti-cheats may check drivers at boot. Any refusal message is copied
word for word with a screenshot; the anti-cheat's own logs are saved when it writes any.

### `report.ps1 -Case <name>` (no elevation needed where possible)
- Secure Boot, test-signing, HVCI, SAC state; our devices and their problem codes; our certificates in
  Trusted Root / Trusted Publisher.
- Per game: anti-cheat service present and its state (`EAAntiCheatService`, `EasyAntiCheat*`), version;
  newest logs (`%ProgramData%\eaanticheat`, `%AppData%\EasyAntiCheat`) copied to `out/`.
- Writes `out/<time>-<case>.log` (gitignored). Reads only; changes nothing.

### Warning (in `docs/compat/` `anti-cheat.md`, later shown in the installer / client)
Content to cover, final copy written with `natural-copy`:
- MixLink installs a virtual display and a virtual input device; the table says which games launched with them.
- Playing online through any remote-play tool is at the player's own risk; anti-cheats may flag or ban,
  sometimes per machine. MixLink never touches game memory or processes.
- A game marked "refused" needs the MixLink drivers removed (how) to play locally.

## Latency impact
None. No hot path is touched.

## Acceptance criteria
- [ ] A–D run for Apex Legends and Elden Ring; table in Results.
- [ ] Every refusal recorded word for word with the anti-cheat log name (if any).
- [ ] D shows the machine clean and both games still launching after uninstall.
- [ ] `docs/compat/` `anti-cheat.md` holds the launch table and the warning text.
- [ ] Verdict per anti-cheat (Javelin, EAC): launch risk of ADR-0007 S5 passes or fails; others untested.
