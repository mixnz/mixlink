use super::*;

fn advert(name: &str) -> Advert {
    Advert {
        id: DeviceId::from_bytes([7; 16]),
        name: name.to_owned(),
        ver: 1,
        caps: CAPS_V1.to_owned(),
        port: 47801,
    }
}

fn pairs(advert: &Advert) -> Vec<(String, String)> {
    advert
        .txt()
        .iter()
        .map(|(k, v)| ((*k).to_owned(), v.clone()))
        .collect()
}

#[test]
fn txt_round_trips() {
    let a = advert("Desk PC");
    assert_eq!(Advert::from_txt(&pairs(&a), 47801), Some(a));
}

#[test]
fn txt_round_trips_unicode_name() {
    // 20 × "ệ" (3 bytes each) + "abc" = 63 bytes, the name limit.
    let name = format!("{}abc", "ệ".repeat(20));
    assert_eq!(name.len(), 63);
    let a = advert(&name);
    assert_eq!(Advert::from_txt(&pairs(&a), 47801), Some(a));
}

#[test]
fn txt_missing_id_is_none() {
    let mut p = pairs(&advert("x"));
    p.retain(|(k, _)| k != "id");
    assert_eq!(Advert::from_txt(&p, 47801), None);
}

#[test]
fn txt_bad_ver_is_none() {
    let mut p = pairs(&advert("x"));
    for (k, v) in &mut p {
        if k == "ver" {
            *v = "300".to_owned();
        }
    }
    assert_eq!(Advert::from_txt(&p, 47801), None);
}

#[test]
fn txt_name_too_long_is_none() {
    let p = pairs(&advert(&"a".repeat(64)));
    assert_eq!(Advert::from_txt(&p, 47801), None);
}

#[test]
fn txt_missing_caps_is_empty() {
    let mut p = pairs(&advert("x"));
    p.retain(|(k, _)| k != "caps");
    let got = Advert::from_txt(&p, 47801).unwrap();
    assert_eq!(got.caps, "");
}

#[test]
fn caps_list_splits_on_commas() {
    let mut a = advert("x");
    a.caps = "v1,h264".to_owned();
    assert_eq!(a.caps_list().collect::<Vec<_>>(), ["v1", "h264"]);
}
