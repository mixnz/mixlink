use std::thread;

use mixlink_core::svc_ipc::open_pipe;

use super::*;

/// The tray runs as an interactive user who is not the pipe's owner (the service is SYSTEM).
/// Without the owner-rights ACE, only the interactive-users ACE applies: the client must open
/// with no more than that ACE grants (no `FILE_CREATE_PIPE_INSTANCE` via `GENERIC_WRITE`).
#[test]
fn interactive_user_can_open_without_owner_rights() {
    let name = format!(r"\\.\pipe\mixlink-test-iu-{}", std::process::id());
    let security = PipeSecurity::from_sddl("D:P(A;;0x12008b;;;IU)").unwrap();
    let instance = create_instance(&name, true, &security).unwrap().unwrap();
    // Not joined: if the open fails, nobody ever connects.
    thread::spawn(move || {
        let _ = wait_client(&instance);
    });
    let result = open_pipe(&name);
    assert!(result.is_ok(), "{result:?}");
}
