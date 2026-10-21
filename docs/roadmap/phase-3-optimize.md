# Phase 3 — Optimize

**Goal:** FPS-grade latency under real conditions: the budget in `docs/architecture/latency-budget.md`
at 1440p120 on wired LAN, playable with packet loss and Wi-Fi jitter.
**Done when:** `latency-probe` shows every stage within budget and `packet-sim` at 2 % loss stays playable.

Tasks are split with the user when this phase starts (bitrate control, intra-refresh / reference
invalidation, slice-based send, thread priorities, AMF/QSV encoders, stats HUD, packet-sim).

| ID | Task | Crates | Needs | Spec | Status |
|---|---|---|---|---|---|
