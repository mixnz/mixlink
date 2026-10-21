# Architecture Overview

## Pipeline
```
HOST                                                    CLIENT
capture (WGC/DXGI) -GPU tex-> encode (NVENC/AMF/QSV)    decode (D3D11VA) -> render (flip swapchain)
                                   |                          ^
                                   v                          |
                         proto: packetize + FEC --UDP--> proto: reassemble + FEC recover
audio: WASAPI loopback -> Opus ----UDP-------------> Opus -> WASAPI playback
input inject (SendInput / virtual pad) <--UDP------ input capture (Raw Input / XInput)
                         net: pacing, bitrate control <--feedback (loss, RTT, jitter)
```

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
| input | client->host | redundant: each packet repeats last N events |
| control | both | small reliable layer (ack + resend) on top of UDP |
| feedback | client->host | periodic stats: loss, jitter, decode time |

## Phases
1. **LAN**: discovery via mDNS + broadcast, PIN pairing, direct UDP.
2. **Internet**: signaling server (rendezvous), ICE/STUN hole punching, TURN relay fallback.
   Adds `services/signaling/` when started — do not create it before.

## Risks
- Virtual gamepad: ViGEmBus is retired (still works). Alternative: own UMDF driver later.
- Anti-cheat may block injected input (SendInput). Document per game; not solvable in app.
- Wi-Fi adds jitter: UI must warn and recommend wired / 5 GHz+.
- Exclusive-fullscreen games may break WGC capture; keep DXGI Desktop Duplication fallback.
