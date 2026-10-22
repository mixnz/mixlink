use super::*;
use crate::report::PadState;

const ALL: [Scenario; 6] = [
    Scenario::Walk,
    Scenario::Look,
    Scenario::Menu,
    Scenario::PadWalk,
    Scenario::PadLook,
    Scenario::PadMenu,
];

#[test]
fn scenarios_are_sorted_and_end_released() {
    for scenario in ALL {
        let steps = steps(scenario);
        assert!(!steps.is_empty(), "{scenario:?}");
        assert!(
            steps.windows(2).all(|w| w[0].at_ms <= w[1].at_ms),
            "{scenario:?} not sorted"
        );

        let mut held = std::collections::BTreeSet::new();
        let mut pad = PadState::CENTERED;
        for step in &steps {
            match step.action {
                Action::Key { usage, down: true } => {
                    held.insert(usage);
                }
                Action::Key { usage, down: false } => {
                    held.remove(&usage);
                }
                Action::MouseMove { .. } | Action::MouseAbs { .. } => {}
                Action::Pad(state) => pad = state,
            }
        }
        assert!(held.is_empty(), "{scenario:?} leaves keys down: {held:?}");
        assert_eq!(
            pad,
            PadState::CENTERED,
            "{scenario:?} leaves the pad off-center"
        );
    }
}

#[test]
fn look_returns_the_cursor() {
    let total: i32 = steps(Scenario::Look)
        .iter()
        .map(|s| match s.action {
            Action::MouseMove { dx, .. } => i32::from(dx),
            _ => 0,
        })
        .sum();
    assert_eq!(total, 0);
}

#[test]
fn parse_names() {
    assert_eq!(Scenario::parse("Walk"), Some(Scenario::Walk));
    assert_eq!(Scenario::parse("padmenu"), Some(Scenario::PadMenu));
    assert_eq!(Scenario::parse("Jump"), None);
}

#[test]
fn keyboard_scenarios_use_only_keys_and_mouse() {
    for scenario in [Scenario::Walk, Scenario::Look, Scenario::Menu] {
        assert!(
            steps(scenario)
                .iter()
                .all(|s| !matches!(s.action, Action::Pad(_))),
            "{scenario:?}"
        );
    }
}
