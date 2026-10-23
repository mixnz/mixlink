use super::*;

#[test]
fn lever_names_lists_bits_in_order() {
    assert_eq!(lever_names(0), "none");
    assert_eq!(lever_names(L_GPU), "gpu");
    assert_eq!(lever_names(7), "mmcss+gpu+process");
    assert_eq!(lever_names(L_MMCSS | L_PROCESS), "mmcss+process");
}
