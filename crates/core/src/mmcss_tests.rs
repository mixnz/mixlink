use super::{Mmcss, MmcssTask};

#[test]
fn enter_and_leave_on_same_thread() {
    for task in [MmcssTask::ProAudio, MmcssTask::Games] {
        let guard: Option<Mmcss> = Mmcss::enter(task);
        if cfg!(windows) {
            assert!(guard.is_some(), "{task:?} not applied");
        }
        drop(guard);
    }
}

#[test]
fn enter_twice_nests() {
    let outer = Mmcss::enter(MmcssTask::Games);
    let inner = Mmcss::enter(MmcssTask::ProAudio);
    drop(inner);
    drop(outer);
}
