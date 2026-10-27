use super::*;
use crate::error::ConfigError;

fn host_with(edit: impl FnOnce(&mut HostConfig)) -> HostConfig {
    let mut config = HostConfig::default();
    edit(&mut config);
    config
}

#[test]
fn defaults_match_spec() {
    let host = HostConfig::default();
    assert_eq!(host.version, 1);
    assert_eq!(host.net.discovery_port, 47800);
    assert_eq!(host.net.session_port, 47801);
    assert_eq!(host.video.max_fps, 120);
    assert_eq!(host.video.bitrate_kbps, 50_000);
    assert!(host.validate().is_ok());
    let client = ClientConfig::default();
    assert_eq!(client.version, 1);
    assert_eq!(client.net, NetConfig::default());
    assert!(client.validate().is_ok());
}

#[test]
fn default_round_trips() {
    let host = HostConfig::default();
    assert_eq!(
        HostConfig::from_json(&host.to_json_pretty().unwrap()).unwrap(),
        host
    );
    let client = ClientConfig::default();
    assert_eq!(
        ClientConfig::from_json(&client.to_json_pretty().unwrap()).unwrap(),
        client
    );
}

#[test]
fn empty_object_is_default() {
    assert_eq!(HostConfig::from_json("{}").unwrap(), HostConfig::default());
    assert_eq!(
        ClientConfig::from_json("{}").unwrap(),
        ClientConfig::default()
    );
}

#[test]
fn partial_section_keeps_other_defaults() {
    let host = HostConfig::from_json(r#"{"net":{"session_port":50000}}"#).unwrap();
    assert_eq!(host.net.session_port, 50000);
    assert_eq!(host.net.discovery_port, 47800);
}

#[test]
fn unknown_fields_ignored() {
    let host = HostConfig::from_json(r#"{"future":true,"net":{"x":1}}"#).unwrap();
    assert_eq!(host, HostConfig::default());
}

#[test]
fn wrong_version_rejected() {
    assert!(matches!(
        HostConfig::from_json(r#"{"version":2}"#),
        Err(ConfigError::UnsupportedVersion {
            found: 2,
            expected: 1
        })
    ));
}

#[test]
fn zero_port_rejected() {
    let config = host_with(|c| c.net.discovery_port = 0);
    assert!(matches!(
        config.validate(),
        Err(ConfigError::ZeroPort {
            field: "discovery_port"
        })
    ));
    let config = host_with(|c| c.net.session_port = 0);
    assert!(matches!(
        config.validate(),
        Err(ConfigError::ZeroPort {
            field: "session_port"
        })
    ));
}

#[test]
fn same_ports_rejected() {
    let config = host_with(|c| c.net.discovery_port = 47801);
    assert!(matches!(
        config.validate(),
        Err(ConfigError::PortClash { port: 47801 })
    ));
}

#[test]
fn fps_limits() {
    for bad in [0, 241] {
        let config = host_with(|c| c.video.max_fps = bad);
        assert!(matches!(
            config.validate(),
            Err(ConfigError::FpsOutOfRange { value }) if value == bad
        ));
    }
    for good in [1, 240] {
        assert!(host_with(|c| c.video.max_fps = good).validate().is_ok());
    }
}

#[test]
fn bitrate_limits() {
    for bad in [999, 500_001] {
        let config = host_with(|c| c.video.bitrate_kbps = bad);
        assert!(matches!(
            config.validate(),
            Err(ConfigError::BitrateOutOfRange { value }) if value == bad
        ));
    }
    for good in [1_000, 500_000] {
        assert!(
            host_with(|c| c.video.bitrate_kbps = good)
                .validate()
                .is_ok()
        );
    }
}

#[test]
fn blank_name_rejected() {
    for bad in ["", "   "] {
        let config = host_with(|c| c.name = bad.to_owned());
        assert!(matches!(config.validate(), Err(ConfigError::EmptyName)));
        let client = ClientConfig {
            name: bad.to_owned(),
            ..ClientConfig::default()
        };
        assert!(matches!(client.validate(), Err(ConfigError::EmptyName)));
    }
}

#[test]
fn name_limit_counts_bytes() {
    assert!(host_with(|c| c.name = "a".repeat(63)).validate().is_ok());
    assert!(matches!(
        host_with(|c| c.name = "a".repeat(64)).validate(),
        Err(ConfigError::NameTooLong { len: 64 })
    ));
    assert!(matches!(
        host_with(|c| c.name = "ệ".repeat(22)).validate(),
        Err(ConfigError::NameTooLong { len: 66 })
    ));
}

#[test]
fn out_of_range_port_is_parse_error() {
    for port in ["70000", "-1"] {
        let json = format!(r#"{{"net":{{"session_port":{port}}}}}"#);
        assert!(matches!(
            HostConfig::from_json(&json),
            Err(ConfigError::Parse(_))
        ));
    }
}

#[test]
fn bad_json_is_parse_error() {
    assert!(matches!(
        HostConfig::from_json("not json"),
        Err(ConfigError::Parse(_))
    ));
    assert!(matches!(
        ClientConfig::from_json("not json"),
        Err(ConfigError::Parse(_))
    ));
}

#[test]
fn topology_default_and_json_names() {
    assert_eq!(Topology::default(), Topology::VirtualOnly);
    assert_eq!(
        serde_json::to_string(&Topology::VirtualOnly).unwrap(),
        "\"virtualOnly\""
    );
    assert_eq!(
        serde_json::from_str::<Topology>("\"extend\"").unwrap(),
        Topology::Extend
    );
}

#[test]
fn display_topology_default_virtual_only() {
    assert_eq!(
        HostConfig::default().display.topology,
        Topology::VirtualOnly
    );
}

#[test]
fn display_topology_extend_parses() {
    let host = HostConfig::from_json(r#"{"display":{"topology":"extend"}}"#).unwrap();
    assert_eq!(host.display.topology, Topology::Extend);
}

#[test]
fn old_config_without_display_loads() {
    let host = HostConfig::from_json(r#"{"version":1,"name":"Den PC"}"#).unwrap();
    assert_eq!(host.display, DisplaySettings::default());
    assert_eq!(host.name, "Den PC");
}

#[test]
fn video_codec_defaults_to_auto() {
    assert_eq!(HostConfig::default().video.codec, CodecChoice::Auto);
    let host = HostConfig::from_json(r#"{"video":{"max_fps":60}}"#).unwrap();
    assert_eq!(host.video.codec, CodecChoice::Auto);
}

#[test]
fn codec_hevc_parses() {
    let host = HostConfig::from_json(r#"{"video":{"codec":"hevc"}}"#).unwrap();
    assert_eq!(host.video.codec, CodecChoice::Hevc);
    assert_eq!(
        serde_json::to_string(&CodecChoice::H264).unwrap(),
        r#""h264""#
    );
}
