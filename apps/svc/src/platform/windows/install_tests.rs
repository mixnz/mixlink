use std::path::Path;

use super::*;

#[test]
fn image_path_is_quoted() {
    assert_eq!(
        image_path(Path::new(r"C:\a b\mixlink-svc.exe")),
        "\"C:\\a b\\mixlink-svc.exe\""
    );
}
