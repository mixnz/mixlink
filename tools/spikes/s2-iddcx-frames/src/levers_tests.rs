use super::*;
use crate::header::S2Header;

#[test]
fn lever_names_none_and_combo() {
    assert_eq!(lever_names(0), "none");
    assert_eq!(lever_names(0x5), "L1+L3");
    assert_eq!(
        lever_names(L1_CPU | L2_GPU | L3_NO_COPY | L4_D3D12),
        "L1+L2+L3+L4"
    );
}

#[test]
fn lever_report_lists_failures() {
    let mut h = S2Header::zeroed();
    h.levers_requested = 0x3;
    h.levers_active = 0x1;
    h.lever_hr[1] = 0x8007_0005_u32 as i32;
    assert_eq!(
        lever_report(&h),
        "levers requested L1+L2, active L1, failed L2 hr 0x80070005"
    );
}

#[test]
fn lever_report_without_failures() {
    let mut h = S2Header::zeroed();
    h.levers_requested = 0x1;
    h.levers_active = 0x1;
    assert_eq!(lever_report(&h), "levers requested L1, active L1");
}
