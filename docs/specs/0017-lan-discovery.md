# 0017 — LAN discovery: mDNS + broadcast in the service, host list in the launcher

**Status:** in-progress
**Date:** 2026-10-26
**Roadmap:** T5
**Plan:** [0017-lan-discovery](../plans/0017-lan-discovery.md) (local only, written by writing-plans skill)
**Crates touched:** `crates/net`, `crates/core`, `apps/svc`, `apps/client`, `apps/host`

## Goal
A client on the same LAN sees every running MixLink host in the launcher within a few seconds,
with no setup and no address typing. A host that stops or leaves the network drops off the list
within 5 s.

## Scope
- In:
  - Stable host identity: a random `DeviceId` made on the service's first start, kept in
    `%ProgramData%\MixLink\identity.json`.
  - mDNS / DNS-SD through the Windows DNS-SD API: the host registers, the client browses and resolves.
  - Broadcast fallback on UDP `discovery_port` (47800): probe / reply format, a host responder,
    a client prober that sends to every IPv4 interface's directed broadcast address.
  - Client-side merge of both sources by host id, with a 5 s expiry.
  - `mixlink-svc install` adds a Windows Firewall rule (inbound UDP, discovery + session port,
    Private + Domain profiles); `uninstall` removes it.
  - Host tray shows a one-line hint when the host's network is Public.
  - Launcher host list: name, IP, "needs update" for a protocol mismatch, empty state with a hint.
- Out:
  - Client identity (`Hello` device id): T6 adds it with the key pair.
  - Connecting to a chosen host (T6 pairing, T9 streaming); the demo Connect button stays as it is.
  - Adding a host by IP (different subnet): a later roadmap row if wanted.
  - IPv6 for the broadcast path (mDNS covers IPv6 on its own); renaming the host while the service runs;
    a `busy` flag; codec values in `caps` (T7).
  - Vietnamese strings (language switch row, not added yet).

## Design

### Identity (`apps/svc`)
- `identity.json` = `{ "version": 1, "id": "<32 hex>" }`, written once with a random `DeviceId`
  (OS RNG), same folder ACL as `host.json`. Missing or unreadable at start → new id, warning logged.
- T6 adds the X25519 key pair to the same file. The id is not user config, so it stays out of `host.json`.

### Advertised fields (both paths)
| Field | Value |
|---|---|
| `id` | host `DeviceId`, 32 hex |
| `name` | `host.json` `name` (≤ 63 UTF-8 bytes, already enforced) |
| `ver` | `proto::header::WIRE_VERSION` as decimal |
| `caps` | comma list; `v1` for now; receivers ignore values they do not know |
| `port` | `host.json` `net.session_port` (mDNS: SRV record; broadcast: JSON field) |

### mDNS (`crates/net/src/discovery/platform/windows/`)
- Host: `DnsServiceRegister` instance `<name>._mixlink._udp.local` with TXT `id`, `name`, `ver`,
  `caps`, SRV port = session port; `DnsServiceDeRegister` on stop, so a goodbye goes out.
  Instance name clash: Windows renames the instance; clients key on `id`, not on the instance name.
- Client: `DnsServiceBrowse` for `_mixlink._udp.local`, then `DnsServiceResolve` per instance for
  address + port + TXT. A record with TTL 0 removes the host from the mDNS source.
- Goes through the Windows mDNS responder (Dnscache, port 5353): no socket of ours on 5353, and the
  built-in "mDNS (UDP-In)" firewall rule covers it.
- To check first in the plan (small probe): Browse reports goodbyes; Register works from Session 0
  as LocalSystem. If goodbyes do not arrive, the 5 s expiry below still removes the host.

### Broadcast fallback (`crates/net/src/discovery/wire.rs`)
- Probe: `MXLD` + version byte `1` (5 B). Reply: same 5 B + UTF-8 JSON `{id, name, ver, caps, port}`,
  ≤ 1200 B. Bad magic, unknown version or bad JSON → dropped without a reply, no log above `debug`.
- Host responder: one socket on `0.0.0.0:discovery_port`, replies unicast to the probe's source.
  Rate limit: at most one reply per source address per 250 ms.
- Client prober: enumerates up IPv4 interfaces (`GetAdaptersAddresses`), sends one probe to each
  directed broadcast address (`addr | !mask`) from one ephemeral socket every 1.5 s while the
  launcher is open. `255.255.255.255` only leaves the default interface on Windows, so it is not used.
- Windows Firewall lets unicast replies to a broadcast through for 3 s by default: the client needs no rule.

### Host list (`crates/net/src/discovery/list.rs`, pure)
- `HostList` keyed by `id`. Each entry: name, `ver`, `caps`, port, addresses with last-seen time
  per source (mDNS, broadcast).
- Shown address: the newest address on the same subnet as a client interface, else the newest one.
- An entry is removed when no source has seen it for 5 s (`protocol.md`). Name / port changes take the
  newest report.
- Tests use a fake clock: merge, expiry, address choice, mismatched `ver`.

### Threads
- No tokio: plain threads, control plane only.
- Host: `Advertiser` (`crates/net`) = mDNS registration + responder thread `svc-discovery`, started
  in `apps/svc/src/run.rs`, stopped through `StopSignal` (socket read timeout, then deregister).
  Discovery failing to start logs an error; the service keeps running (pipe, drivers, display).
- Client: `Browser` (`crates/net`) = browse callbacks + prober thread; publishes a new snapshot
  when the list changes. The launcher starts it on window open and stops it on close.

### Firewall (`apps/svc/src/platform/windows/firewall.rs`)
- `install` adds rule "MixLink" through `INetFwPolicy2`: inbound, allow, UDP, program = service exe,
  local ports = `discovery_port` + `session_port` from `host.json` at install time, profiles
  Private + Domain. An existing rule with that name is replaced. `uninstall` removes it; absent = ok.
- Changing ports in `host.json` needs `install` again (documented in the svc card).
- Public network hint: the service checks the categories of connected networks
  (`INetworkListManager`) when the tray asks for status; `SvcToTray::Status` gains
  `network_public: bool` (unknown fields are ignored by older trays, so `PIPE_VERSION` stays).
  The tray shows the hint text written with `natural-copy`.

### Launcher (`apps/client`)
- Tauri command `list_hosts` (current snapshot) and event `hosts-changed` (new snapshot).
  Row: `{ id, name, address, port, compatible }`; `compatible = ver == WIRE_VERSION`.
- UI: one row per host, name + IP; incompatible rows dimmed with a "needs update" label; clicking a
  row selects it (no connect yet). Empty state: hint to check the same network and that MixLink runs
  on the host. Strings through `@mixlink/i18n` (`en`), written with `natural-copy`.

## Latency impact
None on the stream path: discovery is control plane, off every hot path. The responder thread
sleeps in `recv` and the prober sends a few bytes every 1.5 s. Advertising keeps running during a
stream; a 5 B probe per 1.5 s is negligible on the host.

## Acceptance criteria
- [ ] First service start writes `identity.json`; later starts keep the same id.
- [ ] Two PCs on one wired LAN, both default firewall, host network Private: the launcher lists the
      host (name + IP) within 3 s of opening.
- [ ] Same, with mDNS blocked (Dnscache mDNS off or rule disabled): the host still appears via broadcast.
- [ ] Same, with broadcast blocked (firewall rule removed for 47800): the host still appears via mDNS.
- [ ] Stopping the service removes the host from the list within 5 s; starting it brings it back.
- [ ] A host with a second adapter (Hyper-V vSwitch or VPN) shows once, with the LAN address.
- [ ] A host reporting a different `ver` is listed, dimmed, with "needs update".
- [ ] `install` creates the firewall rule (`netsh advfirewall firewall show rule name=MixLink`);
      `uninstall` removes it; running either twice is fine.
- [ ] Host network set to Public: the tray shows the hint; back to Private: the hint goes.
- [ ] Malformed probes (fuzzed in unit tests) never get a reply and never panic the responder.

## Open questions
- None blocking. Whether `DnsServiceBrowse` reports goodbyes is settled by the plan's first probe.
