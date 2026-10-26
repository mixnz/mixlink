# Anti-cheat compatibility

Which anti-cheat games start with the MixLink drivers installed, and the warning users see about it.
Source: spike S5, [spec 0008](../specs/0008-spike-anticheat-launch.md) → Results. One PC, no gameplay tested.

## Launch table
"Launches" = the anti-cheat starts and the game reaches its main menu with `vdisplay` + `vhid` installed,
also with the virtual monitor as main display, and still launches after the drivers are removed.

| Game | Anti-cheat | Launches with MixLink drivers | Tested on |
|---|---|---|---|
| Apex Legends | EA Javelin | yes | 2026-10-24, Win11 26H2, Secure Boot + HVCI on |
| Elden Ring | Easy Anti-Cheat | yes | 2026-10-24, Win11 26H2, Secure Boot + HVCI on |
| — | Riot Vanguard | not tested | — |
| — | BattlEye | not tested | — |
| — | FACEIT | not tested | — |

Not tested for any game: playing a match, input from the MixLink virtual devices, delayed (server-side)
bans. The virtual Xbox pads (ADR-0009, VID `045E` PID `0B13`) did not exist during spike S5. A new row needs the same four cases (A–D) as spec 0008.

## Warning (user-facing, EN)
Final copy for the installer and the client. Shown before the drivers are installed, and linked from the
game compatibility list. Each paragraph is one i18n key when the UI task adds it (phase 2); `vi` is written
natively then.

**Title:** Games with anti-cheat

**Body:**
- MixLink adds a virtual display and a virtual keyboard, mouse and gamepad to this PC.
- Your gamepad shows up on this PC as an Xbox controller, with Microsoft's device ID, so games and
  Steam pick it up without setup. Some anti-cheats check for virtual controllers.
- We checked that Apex Legends and Elden Ring still start with them installed. We haven't tested
  Valorant, BattlEye games or FACEIT.
- No anti-cheat maker has approved MixLink. Playing online over a stream is at your own risk. Some
  anti-cheats ban the whole PC, not just the account.
- MixLink never reads or changes a game's memory or files.
- If a game won't start, uninstall MixLink. That removes its drivers.

**Button:** Got it
