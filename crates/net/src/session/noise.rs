//! Noise `KK` parameters and session key derivation (spec 0018).

use hkdf::Hkdf;
use mixlink_core::ids::DeviceId;
use mixlink_proto::control::PublicKey;
use mixlink_proto::crypto::{KEY_LEN, SessionKeys};
use sha2::Sha256;
use snow::{Builder, HandshakeState};

use crate::error::NetError;
use crate::keys::StaticKeyPair;

/// Both sides know each other's static key from pairing.
pub const NOISE_PATTERN: &str = "Noise_KK_25519_AESGCM_SHA256";
const PROLOGUE_LABEL: &[u8; 10] = b"mixlink-v1";
pub const PROLOGUE_LEN: usize = 10 + 16 + 16;

/// `mixlink-v1 ‖ client id ‖ host id`: binds both ids into the handshake.
pub fn prologue(client: DeviceId, host: DeviceId) -> [u8; PROLOGUE_LEN] {
    let mut out = [0u8; PROLOGUE_LEN];
    out[..10].copy_from_slice(PROLOGUE_LABEL);
    out[10..26].copy_from_slice(client.as_bytes());
    out[26..].copy_from_slice(host.as_bytes());
    out
}

/// Per-direction AES-128 keys from the Noise split (`k1` = initiator to responder = client to host).
pub fn derive(raw: ([u8; 32], [u8; 32])) -> SessionKeys {
    SessionKeys {
        c2h: expand(&raw.0, b"mixlink-v1 c2h"),
        h2c: expand(&raw.1, b"mixlink-v1 h2c"),
    }
}

fn expand(ikm: &[u8; 32], info: &[u8]) -> [u8; KEY_LEN] {
    let mut key = [0u8; KEY_LEN];
    // 16 bytes is far below the HKDF-SHA256 limit, so `expand` cannot fail.
    let _ = Hkdf::<Sha256>::new(None, ikm).expand(info, &mut key);
    key
}

fn noise_error(error: snow::Error) -> NetError {
    NetError::Noise(format!("{error}"))
}

fn builder<'a>(
    me: &'a StaticKeyPair,
    peer: &'a PublicKey,
    prologue: &'a [u8],
) -> Result<Builder<'a>, NetError> {
    let params = NOISE_PATTERN.parse().map_err(noise_error)?;
    Builder::new(params)
        .local_private_key(&me.private)
        .and_then(|b| b.remote_public_key(&peer.0))
        .and_then(|b| b.prologue(prologue))
        .map_err(noise_error)
}

pub(crate) fn initiator(
    me: &StaticKeyPair,
    host: &PublicKey,
    prologue: &[u8],
) -> Result<HandshakeState, NetError> {
    builder(me, host, prologue)?
        .build_initiator()
        .map_err(noise_error)
}

pub(crate) fn responder(
    me: &StaticKeyPair,
    client: &PublicKey,
    prologue: &[u8],
) -> Result<HandshakeState, NetError> {
    builder(me, client, prologue)?
        .build_responder()
        .map_err(noise_error)
}

/// Writes one handshake message carrying `payload`.
pub(crate) fn write(state: &mut HandshakeState, payload: &[u8]) -> Result<Vec<u8>, NetError> {
    let mut out = vec![0u8; mixlink_proto::control::MAX_NOISE];
    let len = state
        .write_message(payload, &mut out)
        .map_err(noise_error)?;
    out.truncate(len);
    Ok(out)
}

/// Reads one handshake message; returns its payload.
pub(crate) fn read(state: &mut HandshakeState, message: &[u8]) -> Result<Vec<u8>, NetError> {
    let mut out = vec![0u8; mixlink_proto::control::MAX_NOISE];
    let len = state.read_message(message, &mut out).map_err(noise_error)?;
    out.truncate(len);
    Ok(out)
}
