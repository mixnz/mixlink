//! Logical channels multiplexed on one UDP socket (`docs/architecture/overview.md`).

/// Channel id carried in byte 1 of every packet header. Id 0 is never valid.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[repr(u8)]
pub enum Channel {
    Control = 1,
    Video = 2,
    Audio = 3,
    Cursor = 4,
    Input = 5,
    Feedback = 6,
}

impl Channel {
    /// Every channel in id order.
    pub const ALL: [Channel; 6] = [
        Channel::Control,
        Channel::Video,
        Channel::Audio,
        Channel::Cursor,
        Channel::Input,
        Channel::Feedback,
    ];

    pub const fn id(self) -> u8 {
        self as u8
    }

    pub const fn from_id(id: u8) -> Option<Channel> {
        match id {
            1 => Some(Channel::Control),
            2 => Some(Channel::Video),
            3 => Some(Channel::Audio),
            4 => Some(Channel::Cursor),
            5 => Some(Channel::Input),
            6 => Some(Channel::Feedback),
            _ => None,
        }
    }
}
