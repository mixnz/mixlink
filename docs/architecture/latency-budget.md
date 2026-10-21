# Latency Budget (wired LAN, 1080p/1440p @ 120 fps)

Measured "glass-to-glass": game frame presented on host -> visible on client display.

| Stage | Target | Technique |
|---|---|---|
| Capture | <= 1 ms | IddCx frame copied once into a shared GPU texture + fence, no CPU readback |
| Encode | <= 3 ms | HW encoder ultra-low-latency, no B-frames, 1-frame VBV, intra-refresh |
| Packetize + send | <= 0.5 ms | Send slices as soon as encoded, pacing to avoid burst loss |
| Network (LAN) | <= 1 ms | Single UDP socket, DSCP EF, large socket buffers |
| Decode | <= 3 ms | D3D11VA hardware decode, low-delay mode |
| Present | <= 1 frame | Flip-model swapchain, waitable object, allow tearing / VRR |
| **Total** | **<= 10 ms + display** | |

Input path target: client event -> injected on host <= 2 ms on LAN.

## Rules for code on the path
- Never wait for a full frame if a slice is ready; never buffer video for smoothness by default.
  Client renders the newest decoded frame; older ones are dropped.
- Loss handling prefers FEC + intra-refresh over retransmit (retransmit costs >= 1 RTT).
- Mouse uses raw relative deltas (pointer lock on client), sent on every event, not per frame.
- Every stage records timestamps (`core::clock`) so `tools/latency-probe` reports per-stage cost.
- A change that adds > 0.5 ms to any stage needs measurements in its spec.
