use mixlink_input::platform::windows::vhid_wire::QueryReply;

use super::*;

fn reply(feeder_held: bool, dropped: u32, pads_present: u8) -> QueryReply {
    QueryReply {
        version: 2,
        dropped,
        feeder_held,
        sizes: [16, 7, 5, 16],
        pads_present,
    }
}

#[test]
fn free_driver() {
    let text = format_status(&reply(false, 0, 0));
    assert!(text.contains("wire version 2"), "{text}");
    assert!(
        text.contains("keyboard 16 B, mouse 7 B / 5 B, Xbox pad 16 B"),
        "{text}"
    );
    assert!(text.contains("Dropped reports: 0"), "{text}");
    assert!(text.contains("Feeder: free"), "{text}");
    assert!(text.contains("Virtual Xbox controllers: none"), "{text}");
}

#[test]
fn held_driver() {
    let text = format_status(&reply(true, 3, 0));
    assert!(text.contains("Dropped reports: 3"), "{text}");
    assert!(text.contains("Feeder: in use"), "{text}");
}

#[test]
fn pads_listed_one_based() {
    let text = format_status(&reply(false, 0, 0b0101));
    assert!(text.contains("Virtual Xbox controllers: 1, 3"), "{text}");
}
