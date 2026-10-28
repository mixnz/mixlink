use super::*;

const L_CTRL: u8 = 0xE0;
const L_SHIFT: u8 = 0xE1;
const L_ALT: u8 = 0xE2;
const R_CTRL: u8 = 0xE4;
const R_ALT: u8 = 0xE6;
const Z: u8 = 0x1D;
const F: u8 = 0x09;
const Q: u8 = 0x14;

fn hold(s: &mut Shortcuts, keys: &[u8]) {
    for &k in keys {
        assert_eq!(s.key(k, true), KeyUse::Forward);
    }
}

#[test]
fn ctrl_alt_shift_z_releases() {
    let mut s = Shortcuts::default();
    hold(&mut s, &[L_CTRL, L_ALT, L_SHIFT]);
    assert_eq!(s.key(Z, true), KeyUse::Run(Shortcut::Release));
    assert_eq!(s.key(Z, false), KeyUse::Drop);
    assert_eq!(s.key(L_CTRL, false), KeyUse::Forward);
}

#[test]
fn right_side_modifiers_count() {
    let mut s = Shortcuts::default();
    hold(&mut s, &[R_CTRL, R_ALT, 0xE5]);
    assert_eq!(s.key(F, true), KeyUse::Run(Shortcut::Fullscreen));
    assert_eq!(s.key(Q, true), KeyUse::Run(Shortcut::Close));
}

#[test]
fn z_alone_is_not_a_shortcut() {
    let mut s = Shortcuts::default();
    assert_eq!(s.key(Z, true), KeyUse::Forward);
    hold(&mut s, &[L_CTRL, L_ALT]);
    assert_eq!(s.key(F, true), KeyUse::Forward);
}

#[test]
fn modifier_released_ends_the_combo() {
    let mut s = Shortcuts::default();
    hold(&mut s, &[L_CTRL, R_CTRL, L_ALT, L_SHIFT]);
    s.key(L_CTRL, false);
    assert_eq!(s.key(Z, true), KeyUse::Run(Shortcut::Release));
    s.key(R_CTRL, false);
    assert_eq!(s.key(Q, true), KeyUse::Forward);
}

#[test]
fn reset_forgets_held_keys() {
    let mut s = Shortcuts::default();
    hold(&mut s, &[L_CTRL, L_ALT, L_SHIFT]);
    s.reset();
    assert_eq!(s.key(Z, true), KeyUse::Forward);
}
