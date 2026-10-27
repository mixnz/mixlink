//! Host side of pairing (spec 0018): pure state, one prompt at a time, time passed in.

use mixlink_core::ids::DeviceId;
use mixlink_proto::control::{Blob, ControlMessage, PublicKey, RejectReason};
use mixlink_proto::header::WIRE_VERSION;
use spake2::{Ed25519Group, Identity, Password, Spake2};

use super::transcript::{self, Parts};
use super::{MAX_ATTEMPTS, Now, PROMPT_TIME, PairedDevice, PromptView, pin_text};
use crate::error::NetError;
use crate::keys::StaticKeyPair;

/// The reply to send, and the device to store when pairing just finished.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HostPairEvent {
    pub reply: ControlMessage,
    pub paired: Option<PairedDevice>,
}

/// The fields of a `PairRequest`.
#[derive(Debug, Clone, Copy)]
struct Request<'a> {
    proto: u8,
    device: DeviceId,
    host: DeviceId,
    name: &'a str,
    key: PublicKey,
}

#[derive(Debug)]
struct Prompt {
    device: DeviceId,
    name: String,
    client_key: PublicKey,
    pin: u32,
    opened: Now,
    attempts: u8,
    start: Option<Started>,
}

/// The answered `PairStart`, kept so a resend gets the same reply.
#[derive(Debug)]
struct Started {
    spake_a: Vec<u8>,
    reply: ControlMessage,
    transcript: [u8; 32],
    confirm_c: [u8; 32],
}

#[derive(Debug)]
pub struct HostPairing {
    host: DeviceId,
    keys: StaticKeyPair,
    prompt: Option<Prompt>,
    /// Last finished pairing (device, client MAC): a resent `PairConfirm` gets `PairDone` again.
    done: Option<(DeviceId, Vec<u8>)>,
}

impl HostPairing {
    pub fn new(host: DeviceId, keys: StaticKeyPair) -> HostPairing {
        HostPairing {
            host,
            keys,
            prompt: None,
            done: None,
        }
    }

    /// Handles a pairing message from `peer`; `None` for messages that are not pairing ones.
    /// `pin` runs only when a new prompt opens.
    pub fn handle(
        &mut self,
        msg: &ControlMessage,
        now: Now,
        pin: impl FnOnce() -> Result<u32, NetError>,
    ) -> Option<HostPairEvent> {
        self.expire(now);
        let reply = match msg {
            ControlMessage::PairRequest {
                proto,
                device,
                host,
                name,
                key,
            } => {
                let request = Request {
                    proto: *proto,
                    device: *device,
                    host: *host,
                    name,
                    key: *key,
                };
                self.on_request(&request, now, pin)
            }
            ControlMessage::PairStart { device, spake } => self.on_start(*device, spake),
            ControlMessage::PairConfirm { device, mac } => {
                return Some(self.on_confirm(*device, mac, now));
            }
            _ => return None,
        };
        Some(HostPairEvent {
            reply,
            paired: None,
        })
    }

    /// The open prompt, for the tray.
    pub fn prompt(&self, now: Now) -> Option<PromptView> {
        let prompt = self.prompt.as_ref()?;
        let left = remaining_s(prompt, now)?;
        Some(PromptView {
            device_name: prompt.name.clone(),
            pin: pin_text(prompt.pin),
            expires_s: left,
        })
    }

    /// Closes the prompt; whether one was open.
    pub fn cancel(&mut self) -> bool {
        self.prompt.take().is_some()
    }

    /// Whether a prompt is open (expired ones count until `expire` runs).
    pub fn is_open(&self) -> bool {
        self.prompt.is_some()
    }

    /// Closes a prompt whose time is up.
    pub fn expire(&mut self, now: Now) {
        if let Some(prompt) = &self.prompt
            && remaining_s(prompt, now).is_none()
        {
            self.prompt = None;
        }
    }

    fn on_request(
        &mut self,
        request: &Request<'_>,
        now: Now,
        pin: impl FnOnce() -> Result<u32, NetError>,
    ) -> ControlMessage {
        let Request {
            proto,
            device,
            host,
            name,
            key,
        } = *request;
        if proto != WIRE_VERSION {
            return reject(RejectReason::Version);
        }
        if host != self.host {
            return reject(RejectReason::PairingClosed);
        }
        if let Some(prompt) = &mut self.prompt {
            if prompt.device != device {
                return reject(RejectReason::Busy);
            }
            if prompt.start.is_none() {
                prompt.name = name.to_owned();
                prompt.client_key = key;
            }
            return prompt_reply(prompt, now);
        }
        let pin = match pin() {
            Ok(pin) => pin,
            Err(error) => {
                log::error!("no PIN for pairing: {error}");
                return reject(RejectReason::PairingClosed);
            }
        };
        let prompt = self.prompt.insert(Prompt {
            device,
            name: name.to_owned(),
            client_key: key,
            pin,
            opened: now,
            attempts: 0,
            start: None,
        });
        log::info!("pairing prompt for {device}");
        prompt_reply(prompt, now)
    }

    fn on_start(&mut self, device: DeviceId, spake_a: &Blob) -> ControlMessage {
        let Some(prompt) = self.prompt.as_mut().filter(|p| p.device == device) else {
            return reject(RejectReason::PairingClosed);
        };
        if let Some(start) = &prompt.start
            && start.spake_a == spake_a.0
        {
            return start.reply.clone();
        }
        prompt.attempts += 1;
        if prompt.attempts > MAX_ATTEMPTS {
            log::info!("pairing prompt for {device} closed after {MAX_ATTEMPTS} attempts");
            self.prompt = None;
            return reject(RejectReason::PairingClosed);
        }
        let (state, spake_b) = Spake2::<Ed25519Group>::start_b(
            &Password::new(pin_text(prompt.pin).as_bytes()),
            &Identity::new(device.as_bytes()),
            &Identity::new(self.host.as_bytes()),
        );
        let Ok(k) = state.finish(&spake_a.0) else {
            return reject(RejectReason::BadPin);
        };
        let t = transcript::transcript(&Parts {
            device: &device,
            host: &self.host,
            client_key: &prompt.client_key,
            host_key: &self.keys.public,
            spake_a: &spake_a.0,
            spake_b: &spake_b,
        });
        let (confirm_h, confirm_c) = transcript::confirm_keys(&k, &t);
        let reply = ControlMessage::PairReply {
            spake: Blob(spake_b),
            key: self.keys.public,
            mac: Blob(transcript::mac(&confirm_h, &t).to_vec()),
        };
        prompt.start = Some(Started {
            spake_a: spake_a.0.clone(),
            reply: reply.clone(),
            transcript: t,
            confirm_c,
        });
        reply
    }

    fn on_confirm(&mut self, device: DeviceId, mac: &Blob, now: Now) -> HostPairEvent {
        if let Some((done, done_mac)) = &self.done
            && *done == device
            && *done_mac == mac.0
        {
            return event(ControlMessage::PairDone);
        }
        let Some(prompt) = self.prompt.as_mut().filter(|p| p.device == device) else {
            return event(reject(RejectReason::PairingClosed));
        };
        let Some(start) = &prompt.start else {
            return event(reject(RejectReason::PairingClosed));
        };
        if !transcript::verify(&start.confirm_c, &start.transcript, &mac.0) {
            // The attempt is spent; the next try needs a new `PairStart`.
            prompt.start = None;
            return event(reject(RejectReason::BadPin));
        }
        let paired = PairedDevice {
            device,
            name: prompt.name.clone(),
            key: prompt.client_key,
            paired_at: now.unix_s,
        };
        self.prompt = None;
        self.done = Some((device, mac.0.clone()));
        log::info!("paired with {device}");
        HostPairEvent {
            reply: ControlMessage::PairDone,
            paired: Some(paired),
        }
    }
}

fn remaining_s(prompt: &Prompt, now: Now) -> Option<u32> {
    let elapsed = now.mono.saturating_duration_since(prompt.opened.mono);
    let left = PROMPT_TIME.checked_sub(elapsed).filter(|d| !d.is_zero())?;
    Some(u32::try_from(left.as_secs().max(1)).unwrap_or(u32::MAX))
}

fn prompt_reply(prompt: &Prompt, now: Now) -> ControlMessage {
    ControlMessage::PairPrompt {
        expires_s: remaining_s(prompt, now).unwrap_or(1),
    }
}

fn reject(reason: RejectReason) -> ControlMessage {
    ControlMessage::Reject { reason }
}

fn event(reply: ControlMessage) -> HostPairEvent {
    HostPairEvent {
        reply,
        paired: None,
    }
}

#[cfg(test)]
#[path = "host_tests.rs"]
mod tests;
