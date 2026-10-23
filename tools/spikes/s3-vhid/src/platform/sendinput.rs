//! `SendInput` backend (keyboard and mouse only): the comparison baseline.

use windows::Win32::UI::Input::KeyboardAndMouse::{
    INPUT, INPUT_0, INPUT_KEYBOARD, INPUT_MOUSE, KEYBD_EVENT_FLAGS, KEYBDINPUT, KEYEVENTF_KEYUP,
    MOUSEEVENTF_ABSOLUTE, MOUSEEVENTF_MOVE, MOUSEINPUT, SendInput, VIRTUAL_KEY,
};

use crate::feed::{FeedError, Feeder};
use crate::input::{Input, usage_to_vk};

pub struct SendInputFeeder;

fn send_one(input: INPUT) -> Result<(), FeedError> {
    // SAFETY: one fully initialised INPUT, size matches the struct.
    let sent = unsafe { SendInput(&[input], std::mem::size_of::<INPUT>() as i32) };
    if sent == 1 {
        Ok(())
    } else {
        Err(FeedError::Os(format!(
            "SendInput: {}",
            windows::core::Error::from_thread()
        )))
    }
}

fn mouse(
    dx: i32,
    dy: i32,
    flags: windows::Win32::UI::Input::KeyboardAndMouse::MOUSE_EVENT_FLAGS,
) -> INPUT {
    INPUT {
        r#type: INPUT_MOUSE,
        Anonymous: INPUT_0 {
            mi: MOUSEINPUT {
                dx,
                dy,
                mouseData: 0,
                dwFlags: flags,
                time: 0,
                dwExtraInfo: 0,
            },
        },
    }
}

impl Feeder for SendInputFeeder {
    // HOT PATH
    fn send(&mut self, input: &Input) -> Result<(), FeedError> {
        match *input {
            Input::Key { usage, down } => {
                let vk =
                    usage_to_vk(usage).ok_or(FeedError::Unsupported("key without a VK mapping"))?;
                let flags = if down {
                    KEYBD_EVENT_FLAGS(0)
                } else {
                    KEYEVENTF_KEYUP
                };
                send_one(INPUT {
                    r#type: INPUT_KEYBOARD,
                    Anonymous: INPUT_0 {
                        ki: KEYBDINPUT {
                            wVk: VIRTUAL_KEY(vk),
                            wScan: 0,
                            dwFlags: flags,
                            time: 0,
                            dwExtraInfo: 0,
                        },
                    },
                })
            }
            Input::MouseMove { dx, dy } => {
                send_one(mouse(i32::from(dx), i32::from(dy), MOUSEEVENTF_MOVE))
            }
            Input::MouseAbs { x, y } => {
                // Descriptor range 0..32767 -> SendInput's 0..65535.
                let scale = |v: u16| i32::from(v) * 65535 / 32767;
                send_one(mouse(
                    scale(x),
                    scale(y),
                    MOUSEEVENTF_MOVE | MOUSEEVENTF_ABSOLUTE,
                ))
            }
            Input::Pad(_) => Err(FeedError::Unsupported("SendInput has no gamepad")),
        }
    }
}
