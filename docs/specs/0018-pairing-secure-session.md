# 0018 — Pairing (PIN) and the encrypted session

**Status:** in-progress
**Date:** 2026-10-27
**Roadmap:** T6
**Plan:** [0018-pairing-secure-session](../plans/0018-pairing-secure-session.md) (local only, written by writing-plans skill)
**Crates touched:** `crates/proto`, `crates/net`, `crates/core`, `apps/svc`, `apps/host`, `apps/client`, `tools/net-probe`

## Goal
A client pairs with a host once by typing the 6-digit PIN the host tray shows. After that every
connection is mutually authenticated with the stored keys (no PIN), and every session datagram is
encrypted and authenticated with AES-128-GCM. Someone on the LAN without a pairing cannot connect,
read the stream, or inject input.

## Scope
- In:
  - Device keys: host static X25519 key pair (service, protected folder); client device id + key pair
    (launcher, per user). Paired-client list on the host, known-host list on the client.
  - Pairing over the session port: request → PIN on the host tray → PIN typed in the launcher →
    SPAKE2 + key confirmation → both sides store the peer's public key.
  - Session handshake: Noise `KK` (both static keys known) carrying `Hello` / `Welcome` inside, then
    per-direction AES-128-GCM keys; sealed `Ping` / `Pong` proves the session and keeps it alive.
  - Datagram sealing in `crates/proto` (header = AAD, tag appended), replay window, seq wrap.
  - Service: session-port listener thread that answers pairing and session handshakes.
  - Tray: pairing prompt with PIN (cancel), paired-device list with Remove. Pipe server check (spec 0012
    open question) before the tray trusts the pipe.
  - Launcher: rows show paired / not paired; Pair button → PIN entry → result; Forget on a paired row.
  - `net-probe pair` / `net-probe connect` for two-PC checks (connect prints Ping RTT).
- Out:
  - Streaming over the session (virtual monitor, video, input): T9 takes `SecureSession` from here.
    The launcher's demo Connect button stays as it is.
  - Rekeying (not reachable, see Crypto), key rotation, DPAPI wrapping of private keys, telling the host
    when a client forgets it (the host entry stays until Remove in the tray), more than one client session
    at a time (`Busy`).
  - Vietnamese strings (language switch row).

## Design

### Dependencies (versions checked 2026-10-27; plan re-checks)
| Crate | Version | Where | Why |
|---|---|---|---|
| `aes-gcm` | 0.11.1 | proto | Datagram AEAD, in place, AES-NI + CLMUL (ADR-0001) |
| `snow` | 0.10.0 | net | Noise `KK_25519_AESGCM_SHA256` handshake state machine (not hand-written); `dangerously_get_raw_split` gives the keys |
| `spake2` | 0.4.0 | net | PAKE for the PIN: one online guess per attempt, no offline brute force |
| `hkdf`, `sha2` | 0.13 / 0.11 | proto, net | Key derivation, transcript hash, HMAC confirm |

A PIN in a plain HMAC confirm would be brute-forced offline from one exchange (10^6 tries), so the
"HMAC" option in `protocol.md` is dropped for SPAKE2.

### Keys and files
| File | Side | Content | Protection |
|---|---|---|---|
| `%ProgramData%\MixLink\identity.json` | host | `id` (unchanged, public) | as today |
| `%ProgramData%\MixLink\secrets\host_key.json` | host | X25519 private + public | `dir_security::secure_dir` (SYSTEM + Admins) |
| `%ProgramData%\MixLink\secrets\paired.json` | host | `[{ device, name, key, paired_at }]` | same (a user must not add a key) |
| `%LOCALAPPDATA%\MixLink\identity.json` | client | `id`, X25519 private + public | per-user profile ACL |
| `%LOCALAPPDATA%\MixLink\known_hosts.json` | client | `[{ host, name, key }]` | same |
- Keys as hex in JSON, written temp-then-rename (same as `identity.rs`). Bad / missing key file → new
  key, warning; paired lists then no longer match and peers get `NotPaired` (re-pair).
- Pairing a device / host again replaces its entry. Remove in the tray deletes the entry; that client
  gets `NotPaired` next time and its launcher marks the host not paired.

### Pairing protocol (control channel, plaintext JSON, `proto::control`)
1. C→H `PairRequest { proto, device, host, name, key }`: `host` = the id from discovery; wrong id →
   `Reject(PairingClosed)`. Opens a prompt: random PIN (OS RNG, uniform 000000–999999), 120 s, shown
   in the tray. Another device's prompt open → `Reject(Busy)`. Reply `PairPrompt { expires_s }`.
2. User types the PIN. C→H `PairStart { device, spake }` (SPAKE2 `start_a`, ids = client id, host id).
3. H→C `PairReply { spake, key, mac }`: host finishes SPAKE2 → `K`; `mac = HMAC(K_h, T)`.
4. Client finishes, checks `mac` (wrong → "wrong PIN" shown; the attempt is spent). C→H `PairConfirm
   { mac = HMAC(K_c, T) }`. Host checks → stores the client → `PairDone`; else `Reject(BadPin)`.
- `T = SHA-256("mixlink-pair-v1" ‖ device ‖ host ‖ client key ‖ host key ‖ spake A ‖ spake B)`;
  `K_h`, `K_c` = HKDF-SHA256(`K`, salt `T`, info `"confirm h"` / `"confirm c"`). Both static keys are
  bound, so an attacker who swaps a key fails the confirm.
- Each `PairStart` with a new SPAKE message is one attempt; 3 attempts or expiry close the prompt
  (`Reject(PairingClosed)`); the user asks again from the launcher. Odds of a guessed PIN ≤ 3 in 10^6.
- Lost packets: the client resends each step with `ResendTimer` (250 ms, 10 s; prompt step waits up
  to 120 s for the user). The host caches its last reply per step and repeats it for a repeated
  request (same bytes), so resends never count as attempts.
- New `RejectReason` values: `BadPin`, `PairingClosed`.

### Session handshake (Noise `KK`, `net::session`)
- Prologue `"mixlink-v1" ‖ client id ‖ host id`. Client → `SessionInit { proto, device, host, noise }`
  where `noise` = message 1 (`e, es, ss`), its payload the `Hello` JSON (encrypted).
- Host: unknown `device` → plaintext `Reject(NotPaired)`; version → `Reject(Version)`; busy →
  `Reject(Busy)`. Else `SessionAccept { noise }` = message 2 (`e, ee, se`), payload `Welcome` or a
  `Reject` (bad display) JSON. Host picks a random `SessionId`.
- Keys: raw split `(k1, k2)` → `c2h = HKDF(k1, info "mixlink-v1 c2h")[..16]`, `h2c` likewise.
- Message 1 can be replayed (Noise KK property), so the host commits a session (T9: builds the
  virtual monitor) only on the first valid **sealed** datagram from the client: the client sends
  sealed `Ping { n }` every 1 s; the host answers `Pong { n }`. No valid datagram for 10 s → session
  dropped (host) / `Timeout` (client). `Bye` (sealed) ends it at once. A repeated `SessionInit` (same
  bytes) gets the cached `SessionAccept`; a new one from the same device replaces its old session.
- Result types: `SecureSession { id, peer, keys, welcome }` (client), `HostSession` (host). T9 builds on them.

### Datagram crypto (`crates/proto/src/crypto.rs`) — `// HOT PATH`
- Sealed datagram = 16 B header (plain, AAD) ‖ payload (encrypted) ‖ 16 B tag ≤ 1232 B (ADR-0008).
- Nonce (12 B) = `channel` ‖ 3 zero ‖ `roc` (u32 LE) ‖ `seq` (u32 LE). Keys differ per direction and
  per session, so the nonce is unique as long as each (channel, direction) never repeats `seq` + `roc`.
- `Sealer` (one per channel, sender's thread): seals in place; `seq` must be newer than the last sealed
  one (RFC 1982), else error (never reuse a nonce); a wrap bumps `roc`.
- `Opener` (one per channel, receiver's thread): guesses `roc` from the newest seen seq (SRTP style),
  opens in place, then a 1024-entry replay window drops repeats and too-old packets (counted).
  Failed tag → dropped, counted, no log above `trace`.
- No allocation: `encrypt_in_place_detached` / `decrypt_in_place_detached` on the slot buffer.
- `net` lanes: `TxSlot::seal_commit(&mut Sealer, len, peer)`, `RxDatagram::open(&mut Opener) ->
  Result<&[u8]>` (slot data needs `&mut`).
- Control channel: before a session only the plaintext messages above are accepted; after it, only
  sealed. A plaintext `SessionInit` / `PairRequest` from a peer with a session is still handled
  (client restarted). Other channels are always sealed.
- Rekey: one `roc` step per 2^32 datagrams per channel; at 500 k datagrams/s a 2^64 wrap is never reached.

### Service (`apps/svc`)
- Thread `svc-session` owns a `Transport` on `0.0.0.0:session_port`, serves the control rx lane with the
  pure state machines `net::pairing::HostPairing` and `net::session::HostSessions` (fake clock in tests).
  Started in `run.rs` next to discovery; failure logs an error and the service keeps running.
- Pairing prompt state is shared with the pipe (`Arc` + mutex, control plane only).
- Pipe (`core::svc_ipc`, `PIPE_VERSION` → 2): `Status` gains `pairing: Option<{ device_name, pin,
  expires_s }>`; new requests `CancelPairing`, `ListPaired` → `Paired { devices }`, `RemovePaired { device }`.

### Tray (`apps/host`)
- Pipe server check: after opening the pipe, `GetNamedPipeServerProcessId` must equal the `MixLink`
  service PID (SCM `QueryServiceStatusEx`); otherwise `LinkState::untrusted` and nothing is shown or
  sent. A `--console` service is therefore not trusted by the tray (dev: run the installed service).
- A new prompt shows the window in front with device name, PIN, countdown, Cancel. Paired-device list
  with Remove. Strings through `@mixlink/i18n`, written with `natural-copy`.

### Launcher (`apps/client`)
- `HostRow` gains `paired` (host id + key in `known_hosts.json`). Pair button on a selected unpaired
  row → `pair_request(id)`; the UI shows the PIN field when `pair-state` says `awaitingPin`;
  `pair_submit(pin)`, `pair_cancel()`. States: `requesting`, `awaitingPin`, `checking`, `paired`,
  `failed { reason }` (wrong PIN, closed, busy, no answer). Thread `client-pair` with its own `Transport`.
- Forget on a paired row (with a confirm) → `forget_host(id)` deletes the entry from `known_hosts.json`;
  the row shows not paired. Local only: the host keeps the client until Remove in the tray (harmless, the
  client no longer has the host's key). A `NotPaired` reject also forgets the host.

### `tools/net-probe`
- `pair <ip:port> <host id>` (reads the PIN from stdin), `connect <ip:port> <host id> [s]` (session,
  Ping every 1 s, prints RTT p50 / p99, then `Bye`). `--store <dir>` (default: the launcher's folder).

## Latency impact
- Per datagram: seal on the sender's packetizing thread, open on the consumer thread. Expected with
  AES-NI ~0.3–0.6 µs per 1.2 KB datagram; a 1440p120 P-frame (~130 datagrams) ≈ 50–80 µs, a 4 MiB
  keyframe (3500) ≈ 2 ms. Measured by a seal + open benchmark over 1232 B datagrams (p50 / p99 per
  datagram, MB/s) on the main PC and the laptop; recorded in Results. Target: p99 ≤ 2 µs per datagram.
- `tests/no_alloc.rs` (proto, net) cover `seal` / `open`. Handshake and pairing are control plane.

## Acceptance criteria
- [ ] Pair from the launcher on a second PC: the tray pops up with the device name and PIN; the right
      PIN gives "Paired" on both sides; files on both sides hold the peer's key.
- [ ] A wrong PIN shows "wrong PIN"; after 3 wrong PINs or 120 s the prompt closes and a new request is needed.
- [ ] Cancel in the tray: the launcher shows the pairing was cancelled.
- [ ] `net-probe connect` from a paired client: session up, Ping RTT printed, `Bye` ends it; the host log
      shows the session start and end.
- [ ] Unpaired client, or a client removed in the tray: `connect` gets `NotPaired`; launcher shows not paired.
- [ ] Forget in the launcher: the row shows not paired, `known_hosts.json` loses the entry; Pair works again.
- [ ] A client with a forged id (real id, other key): handshake fails, no session on the host.
- [ ] Tampered, replayed, or other-session datagrams are dropped and counted (unit tests; seq wrap included).
- [ ] Another process holding `\\.\pipe\mixlink-svc` before the service: the tray shows untrusted, no PIN.
- [ ] Standard user cannot read or write `secrets\` (`icacls`).
- [ ] Benchmark numbers in Results, p99 ≤ 2 µs per datagram on the main PC.

## Open questions
Calls made in this spec (review them):
- Noise `KK` via `snow` + SPAKE2 via `spake2` 0.4 instead of hand-written X25519 + HMAC.
- Request-first pairing: the PIN exists only while a client asks; one prompt at a time.
- Tray paired list + Remove is in scope (otherwise no way to revoke); launcher Forget is local only.
- Demo Connect unchanged; the encrypted session is proven by `net-probe connect` and tests until T9.
- The tray trusts only the installed service (PID check), also in debug builds.
- Private keys in ACL-protected files, no DPAPI. `PIPE_VERSION` 2 (tray and service ship together).
- Size: about 6 plan tasks (crypto, pairing + session logic, svc, pipe + tray, launcher, net-probe +
  runs). If too big, split tray + launcher UI into a new row.
