# Architecture Overview

## Pipeline
```
HOST (mixlink-svc, SYSTEM service)                      CLIENT
IddCx virtual monitor -shared GPU tex-> encode (NVENC/AMF/QSV)   decode (D3D11VA) -> render (flip swapchain)
                                   |                          ^
                                   v                          |
                         proto: packetize + FEC --UDP--> proto: reassemble + FEC recover
audio: WASAPI loopback -> Opus ----UDP-------------> Opus -> WASAPI playback
input -> virtual HID (UMDF) <------UDP------------- input capture (Raw Input / XInput)
                         net: pacing, bitrate control <--feedback (loss, RTT, jitter)
```

## Host processes (ADR-0005)
| Process | Runs as | Role |
|---|---|---|
| `mixlink-svc` (`apps/svc`) | LocalSystem, Session 0, auto-start | Whole host pipeline, network, driver control, display topology |
| `mixlink-host` (`apps/host`) | Interactive user | Tray UI only: status, PIN, paired devices, settings; named pipe to the service |
| User-session agent (later) | Interactive user | Clipboard, game launch; never on the hot path |

## Host drivers (ADR-0004)
UMDF 2 only, C++ on WDK samples, in `drivers/`, signed per machine at install (no Microsoft signing).
| Driver | Role | ADR |
|---|---|---|
| `drivers/vdisplay` (IddCx) | Virtual monitor in the client's exact mode; frames to a shared texture | 0006 |
| `drivers/vhid` (HID minidriver) | Virtual keyboard, mouse, gamepad fed by HID reports | 0007 |

## Display (ADR-0006)
- Stream source is always a virtual monitor, headless or not; mode = client's display.
- Stable EDID per paired client. Virtual monitor exists before login when no physical monitor.
- Session topology: virtual only (default) or extend; previous topology saved and restored, also after a crash.
- Fallback frame path: DXGI Desktop Duplication on the virtual monitor.
- Cursor is sent on its own and drawn by the client.

## Threads (per side)
Each stage runs on its own dedicated OS thread, connected by lock-free SPSC ring buffers.
Async runtime (tokio) is allowed only for control plane: discovery, pairing, settings.

| Host thread | Priority | Client thread | Priority |
|---|---|---|---|
| capture+encode | MMCSS "Games" | net-recv | time-critical |
| net-send (pacer) | time-critical | decode+present | MMCSS "Games" |
| audio | MMCSS "Pro Audio" | audio | MMCSS "Pro Audio" |
| input-inject | time-critical | input-send | time-critical |

## Channels (logical, multiplexed on one UDP socket)
| Channel | Direction | Reliability |
|---|---|---|
| video | host->client | FEC, no retransmit; on loss request intra-refresh / ref invalidation |
| audio | host->client | FEC, drop late packets |
| cursor | host->client | latest-wins: shape on change, position per update |
| input | client->host | redundant: each packet repeats last N events |
| control | both | small reliable layer (ack + resend); handshake carries client displays |
| feedback | client->host | periodic stats: loss, jitter, decode time |

## Phases
1. **LAN**: discovery via mDNS + broadcast, PIN pairing, direct UDP, headless host supported.
2. **Internet**: signaling server (rendezvous), ICE/STUN hole punching, TURN relay fallback.
   Adds `services/signaling/` when started — do not create it before.

## Risks
- Per-machine signed UMDF drivers must load with Secure Boot on (spike S1); fallback: author-signed
  open-source IddCx driver, SendInput for input.
- Unsigned `.exe`: SmartScreen warns; Smart App Control (Win11) blocks the app. Documented only.
- XInput-only games need ViGEmBus (optional, author-signed, retired, flagged by some anti-cheats).
- Anti-cheat: EA Javelin and EAC launch with our drivers (spike S5); gameplay and other anti-cheats
  untested. List and user warning: `compat/anti-cheat.md`.
- Encode and loopback from Session 0 unproven (spike S4); fallback: SYSTEM process in the console session.
- Wi-Fi adds jitter: UI must warn and recommend wired / 5 GHz+.
