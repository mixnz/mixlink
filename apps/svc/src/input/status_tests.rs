use mixlink_input::platform::windows::vhid_wire::QueryReply;

use super::*;

fn reply(feeder_held: bool, dropped: u32) -> QueryReply {
    QueryReply {
        version: 1,
        dropped,
        feeder_held,
        sizes: [8, 7, 5, 13],
    }
}

#[test]
fn free_driver() {
    let text = format_status(&reply(false, 0));
    assert!(text.contains("wire version 1"), "{text}");
    assert!(
        text.contains("keyboard 8 B, mouse 7 B / 5 B, gamepad 13 B"),
        "{text}"
    );
    assert!(text.contains("Dropped reports: 0"), "{text}");
    assert!(text.contains("Feeder: free"), "{text}");
}

#[test]
fn held_driver() {
    let text = format_status(&reply(true, 3));
    assert!(text.contains("Dropped reports: 3"), "{text}");
    assert!(text.contains("Feeder: in use"), "{text}");
}
