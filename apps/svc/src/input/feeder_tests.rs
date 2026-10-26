use mixlink_input::hid::report::Target;

use super::*;

#[test]
fn claim_skips_absent_pads() {
    let targets: Vec<Target> = claim_reports(0b0101).iter().map(|r| r.target()).collect();
    assert_eq!(
        targets,
        [
            Target::Keyboard,
            Target::MouseRel,
            Target::Pad(0),
            Target::Pad(2)
        ]
    );
}

#[test]
fn claim_without_pads_is_keyboard_and_mouse() {
    let targets: Vec<Target> = claim_reports(0).iter().map(|r| r.target()).collect();
    assert_eq!(targets, [Target::Keyboard, Target::MouseRel]);
}
