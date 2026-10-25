use super::*;

fn is_upper_hex40(s: &str) -> bool {
    s.len() == 40
        && s.bytes()
            .all(|b| b.is_ascii_digit() || (b'A'..=b'F').contains(&b))
}

#[test]
fn user_cert_has_subject_and_thumbprint() {
    let mut cert = create_signing_cert(KeyScope::User).unwrap();
    assert!(is_upper_hex40(&cert.thumbprint()), "{}", cert.thumbprint());
    assert_eq!(cert.subject().unwrap(), SUBJECT_NAME);
    cert.delete_key().unwrap();
}

#[test]
fn two_certs_have_different_thumbprints() {
    let mut a = create_signing_cert(KeyScope::User).unwrap();
    let mut b = create_signing_cert(KeyScope::User).unwrap();
    assert_ne!(a.thumbprint(), b.thumbprint());
    a.delete_key().unwrap();
    b.delete_key().unwrap();
}

#[test]
fn delete_key_removes_container() {
    let mut cert = create_signing_cert(KeyScope::User).unwrap();
    assert!(key_exists(KeyScope::User, cert.container()).unwrap());
    cert.delete_key().unwrap();
    assert!(!key_exists(KeyScope::User, cert.container()).unwrap());
    // A second delete is a no-op.
    cert.delete_key().unwrap();
}

#[test]
fn drop_deletes_a_key_left_behind() {
    let container = {
        let cert = create_signing_cert(KeyScope::User).unwrap();
        cert.container().to_owned()
    };
    assert!(!key_exists(KeyScope::User, &container).unwrap());
}
