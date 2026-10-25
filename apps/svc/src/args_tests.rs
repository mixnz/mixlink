use std::path::PathBuf;

use mixlink_core::svc_ipc::PIPE_NAME;

use super::*;

fn parse(args: &[&str]) -> Result<Mode, String> {
    parse_args(args.iter().map(|arg| (*arg).to_owned()))
}

#[test]
fn no_args_is_service() {
    assert_eq!(parse(&[]).unwrap(), Mode::Service);
}

#[test]
fn console_uses_default_pipe() {
    assert_eq!(
        parse(&["--console"]).unwrap(),
        Mode::Console {
            pipe: PIPE_NAME.to_owned()
        }
    );
}

#[test]
fn console_with_pipe() {
    assert_eq!(
        parse(&["--console", "--pipe", "x"]).unwrap(),
        Mode::Console {
            pipe: "x".to_owned()
        }
    );
}

#[test]
fn install_and_uninstall() {
    assert_eq!(parse(&["install"]).unwrap(), Mode::Install);
    assert_eq!(parse(&["uninstall"]).unwrap(), Mode::Uninstall);
}

#[test]
fn bad_args_are_errors() {
    assert!(parse(&["--pipe", "x"]).is_err());
    assert!(parse(&["bogus"]).is_err());
    assert!(parse(&["--console", "--pipe"]).is_err());
    assert!(parse(&["install", "extra"]).is_err());
}

#[test]
fn drivers_install() {
    assert_eq!(
        parse(&["drivers", "install"]).unwrap(),
        Mode::Drivers(DriversCmd::Install { from: None })
    );
}

#[test]
fn drivers_install_from() {
    assert_eq!(
        parse(&["drivers", "install", "--from", r"D:\My Builds"]).unwrap(),
        Mode::Drivers(DriversCmd::Install {
            from: Some(PathBuf::from(r"D:\My Builds"))
        })
    );
}

#[test]
fn drivers_uninstall_and_status() {
    assert_eq!(
        parse(&["drivers", "uninstall"]).unwrap(),
        Mode::Drivers(DriversCmd::Uninstall)
    );
    assert_eq!(
        parse(&["drivers", "status"]).unwrap(),
        Mode::Drivers(DriversCmd::Status)
    );
}

#[test]
fn drivers_unknown() {
    assert!(parse(&["drivers", "x"]).is_err());
    assert!(parse(&["drivers"]).is_err());
    assert!(parse(&["drivers", "install", "--from"]).is_err());
    assert!(parse(&["drivers", "status", "extra"]).is_err());
}
