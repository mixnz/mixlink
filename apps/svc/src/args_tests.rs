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

const CLIENT: &str = "0123456789abcdef0123456789abcdef";

fn try_mode(width: u32, height: u32, refresh_mhz: u32) -> DisplayInfo {
    DisplayInfo {
        width,
        height,
        refresh_mhz,
        hdr: false,
        dpi: 96,
    }
}

#[test]
fn display_status() {
    assert_eq!(
        parse(&["display", "status"]).unwrap(),
        Mode::Display(DisplayCmd::Status)
    );
}

#[test]
fn display_try_keep() {
    let args = [
        "display",
        "try",
        "--client",
        CLIENT,
        "--mode",
        "2560x1440@120",
        "--topology",
        "keep",
    ];
    assert_eq!(
        parse(&args).unwrap(),
        Mode::Display(DisplayCmd::Try {
            client: CLIENT.parse().unwrap(),
            mode: try_mode(2560, 1440, 120_000),
            topology: TryTopology::Keep,
        })
    );
}

#[test]
fn display_try_fractional_refresh() {
    let args = [
        "display",
        "try",
        "--client",
        CLIENT,
        "--mode",
        "1920x1080@59.94",
        "--topology",
        "keep",
    ];
    let Mode::Display(DisplayCmd::Try { mode, .. }) = parse(&args).unwrap() else {
        panic!("not a try command");
    };
    assert_eq!(mode, try_mode(1920, 1080, 59_940));
}

#[test]
fn display_try_bad_mode() {
    for mode in [
        "2560x1440",
        "0x0@60",
        "2560x1440@abc",
        "2560x1440@0",
        "x1440@60",
        "2560x1440@60.1234",
    ] {
        let args = [
            "display",
            "try",
            "--client",
            CLIENT,
            "--mode",
            mode,
            "--topology",
            "keep",
        ];
        assert!(parse(&args).is_err(), "{mode} should not parse");
    }
}

#[test]
fn display_try_bad_client() {
    let args = [
        "display",
        "try",
        "--client",
        &CLIENT[1..],
        "--mode",
        "1920x1080@60",
        "--topology",
        "keep",
    ];
    assert!(parse(&args).is_err());
}

#[test]
fn display_reject_check() {
    assert_eq!(
        parse(&["display", "reject-check"]).unwrap(),
        Mode::Display(DisplayCmd::RejectCheck)
    );
}
