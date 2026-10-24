# net-probe

Dev tool for the UDP transport (spec 0011): handshake, synthetic video and input pings over
`mixlink-net`, on one machine or between two PCs. English output only (dev tool).

## Build
```
cargo build -p mixlink-net-probe --release
```
The binary is `target/release/net-probe.exe`. Copy it to the other PC (no install needed).

## Two PCs (wired LAN)
1. Host PC: `net-probe serve` (listens on UDP 47801; `--port P` to change).
   Windows Firewall asks the first time: allow it on **Private** networks. If no prompt shows and
   nothing arrives, check the network is Private and that an inbound UDP rule for the exe exists.
2. Client PC: `net-probe run <host-ip>` (default 30 s, 50 Mbps, 120 fps).
   Options: `--port P`, `--seconds N`, `--mbps N`, `--fps N`.
3. `serve` keeps running for the next client; stop it with Ctrl+C.

## One machine
`net-probe serve --port 47901` in one terminal, `net-probe run 127.0.0.1 --port 47901 --seconds 5`
in another.

## Output (`run`)
- `handshake`: time from the first `Hello` to `Welcome`.
- `video`: frames queued, frames the host acked (fully reassembled), lost = sent − acked;
  `abandoned` = frames dropped on the client because the video lane was full.
- `pings`: one per ms on `Input`, echoed on `Feedback`; RTT measured on the client clock
  (send-time stamp → client recv thread). Includes both machines' lane hand-offs.
- `transport`: lane counters (`tx` / `rx` by channel id 1-6: control, video, audio, cursor, input,
  feedback), socket errors, `rx_bad`, applied buffers, MMCSS.

`serve` prints its own counters when the client says bye. No pacing yet: each video frame
leaves as one burst, as in spec 0011.
