//! ViGEmBus backend: one virtual Xbox 360 pad (XInput). ViGEmBus is installed by the user.

use vigem_client::{Client, TargetId, XButtons, XGamepad, Xbox360Wired};

use crate::feed::{FeedError, Feeder};
use crate::input::{Input, pad_to_xinput};

pub struct VigemFeeder {
    target: Xbox360Wired<Client>,
}

fn vigem_err(call: &str, e: vigem_client::Error) -> FeedError {
    FeedError::Os(format!("{call}: {e:?}"))
}

impl VigemFeeder {
    pub fn connect() -> Result<Self, FeedError> {
        let client = Client::connect()
            .map_err(|e| vigem_err("ViGEm connect (is ViGEmBus installed?)", e))?;
        let mut target = Xbox360Wired::new(client, TargetId::XBOX360_WIRED);
        target.plugin().map_err(|e| vigem_err("ViGEm plugin", e))?;
        target
            .wait_ready()
            .map_err(|e| vigem_err("ViGEm wait_ready", e))?;
        Ok(Self { target })
    }

    /// XInput user index (0..3) of the virtual pad.
    pub fn user_index(&mut self) -> Result<u32, FeedError> {
        self.target
            .get_user_index()
            .map_err(|e| vigem_err("ViGEm get_user_index", e))
    }
}

impl Feeder for VigemFeeder {
    // HOT PATH
    fn send(&mut self, input: &Input) -> Result<(), FeedError> {
        let Input::Pad(state) = input else {
            return Err(FeedError::Unsupported(
                "ViGEm backend feeds the gamepad only",
            ));
        };
        let x = pad_to_xinput(state);
        let gamepad = XGamepad {
            buttons: XButtons { raw: x.buttons },
            left_trigger: x.left_trigger,
            right_trigger: x.right_trigger,
            thumb_lx: x.thumb_lx,
            thumb_ly: x.thumb_ly,
            thumb_rx: x.thumb_rx,
            thumb_ry: x.thumb_ry,
        };
        self.target
            .update(&gamepad)
            .map_err(|e| vigem_err("ViGEm update", e))
    }
}
