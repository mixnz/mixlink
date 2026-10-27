//! Wire reasons → the launcher's close reasons (spec 0020); the launcher turns them into text.

use mixlink_core::ipc::CloseReason;
use mixlink_proto::control::{ByeReason, RejectReason};

pub fn reject_reason(reason: RejectReason) -> CloseReason {
    let text = match reason {
        RejectReason::NotPaired => return CloseReason::NotPaired,
        RejectReason::Busy => "busy",
        RejectReason::BadDisplay => "badDisplay",
        RejectReason::Version => "version",
        // Pairing answers; a session handshake never gets them from a well-behaved host.
        RejectReason::BadPin | RejectReason::PairingClosed => "version",
    };
    CloseReason::Rejected {
        reason: text.to_owned(),
    }
}

pub fn bye_reason(reason: ByeReason) -> &'static str {
    match reason {
        ByeReason::UserClosed => "userClosed",
        ByeReason::Shutdown => "shutdown",
        ByeReason::Timeout => "timeout",
        ByeReason::HostError => "hostError",
    }
}
