use super::*;

const THUMB: &str = "0123456789ABCDEF0123456789ABCDEF01234567";

fn values(pairs: &[(&str, &str)]) -> Vec<(String, String)> {
    pairs
        .iter()
        .map(|(k, v)| ((*k).to_owned(), (*v).to_owned()))
        .collect()
}

fn full_state() -> DriverState {
    let mut state = DriverState {
        cert_thumbprint: Some(THUMB.to_owned()),
        ..DriverState::default()
    };
    state
        .published
        .insert("vdisplay".to_owned(), "oem10.inf".to_owned());
    state
        .published
        .insert("vhid".to_owned(), "oem11.inf".to_owned());
    state
}

#[test]
fn round_trip() {
    let state = full_state();
    let back = DriverState::from_values(state.to_values()).unwrap();
    assert_eq!(back, state);
    assert!(!back.is_empty());
}

#[test]
fn value_names() {
    let mut names: Vec<_> = full_state()
        .to_values()
        .into_iter()
        .map(|(k, _)| k)
        .collect();
    names.sort();
    assert_eq!(
        names,
        [
            "CertThumbprint",
            "vdisplay.PublishedInf",
            "vhid.PublishedInf"
        ]
    );
}

#[test]
fn empty_state_has_no_values() {
    let state = DriverState::default();
    assert!(state.to_values().is_empty());
    assert!(state.is_empty());
    assert_eq!(DriverState::from_values(Vec::new()).unwrap(), state);
}

#[test]
fn rejects_bad_thumbprint() {
    for bad in [
        "abc",
        &THUMB.to_lowercase(),
        &format!("{THUMB}0"),
        "Z123456789ABCDEF0123456789ABCDEF01234567",
    ] {
        let err = DriverState::from_values(values(&[("CertThumbprint", bad)])).unwrap_err();
        assert!(matches!(err, SvcError::StateInvalid(_)), "{bad}: {err:?}");
    }
}

#[test]
fn rejects_bad_published() {
    for bad in ["C:\\x.inf", "oem.inf", "oem12.inf.bak", "oemx.inf", ""] {
        let res = DriverState::from_values(values(&[("vhid.PublishedInf", bad)]));
        assert!(matches!(res, Err(SvcError::StateInvalid(_))), "{bad}");
    }
    for good in ["oem12.inf", "OEM7.INF"] {
        let state = DriverState::from_values(values(&[("vhid.PublishedInf", good)])).unwrap();
        assert_eq!(state.published["vhid"], good);
    }
}

#[test]
fn rejects_unknown_value_name() {
    let res = DriverState::from_values(values(&[("Other", "x")]));
    assert!(matches!(res, Err(SvcError::StateInvalid(_))));
}

#[test]
fn rejects_unknown_driver_name() {
    let res = DriverState::from_values(values(&[("foo.PublishedInf", "oem1.inf")]));
    assert!(matches!(res, Err(SvcError::StateInvalid(_))));
}
