//! The inbound firewall rule for discovery and sessions (spec 0017): the service runs in Session 0,
//! where Windows shows no firewall prompt, so `install` adds the rule and `uninstall` removes it.

use std::path::Path;

use mixlink_core::config::NetConfig;
use windows::Win32::Foundation::VARIANT_TRUE;
use windows::Win32::NetworkManagement::WindowsFirewall::{
    INetFwPolicy2, INetFwRule, NET_FW_ACTION_ALLOW, NET_FW_IP_PROTOCOL_UDP, NET_FW_PROFILE2_DOMAIN,
    NET_FW_PROFILE2_PRIVATE, NET_FW_RULE_DIR_IN, NetFwPolicy2, NetFwRule,
};
use windows::Win32::System::Com::{CLSCTX_ALL, CoCreateInstance};
use windows::core::BSTR;

use super::com;
use crate::error::SvcError;
use crate::net_check::local_ports;
use crate::strings;

pub const RULE_NAME: &str = "MixLink";

fn policy() -> Result<INetFwPolicy2, SvcError> {
    // SAFETY: the caller holds a COM guard for this thread.
    unsafe { CoCreateInstance(&NetFwPolicy2, None, CLSCTX_ALL) }
        .map_err(SvcError::win32("CoCreateInstance(NetFwPolicy2)"))
}

/// Replaces any rule named `RULE_NAME` with one for `exe` and the ports in `net`.
pub fn add_rule(exe: &Path, net: &NetConfig) -> Result<(), SvcError> {
    let _com = com::init()?;
    // SAFETY: COM is initialized for the guard's lifetime; every BSTR outlives its call.
    unsafe {
        let rules = policy()?
            .Rules()
            .map_err(SvcError::win32("INetFwPolicy2::Rules"))?;
        // Not there yet is fine.
        let _ = rules.Remove(&BSTR::from(RULE_NAME));
        let rule: INetFwRule = CoCreateInstance(&NetFwRule, None, CLSCTX_ALL)
            .map_err(SvcError::win32("CoCreateInstance(NetFwRule)"))?;
        rule.SetName(&BSTR::from(RULE_NAME))
            .map_err(SvcError::win32("INetFwRule::SetName"))?;
        rule.SetDescription(&BSTR::from(strings::FIREWALL_RULE_DESCRIPTION))
            .map_err(SvcError::win32("INetFwRule::SetDescription"))?;
        rule.SetApplicationName(&BSTR::from(exe.as_os_str().to_string_lossy().as_ref()))
            .map_err(SvcError::win32("INetFwRule::SetApplicationName"))?;
        rule.SetProtocol(NET_FW_IP_PROTOCOL_UDP.0)
            .map_err(SvcError::win32("INetFwRule::SetProtocol"))?;
        rule.SetLocalPorts(&BSTR::from(local_ports(net)))
            .map_err(SvcError::win32("INetFwRule::SetLocalPorts"))?;
        rule.SetDirection(NET_FW_RULE_DIR_IN)
            .map_err(SvcError::win32("INetFwRule::SetDirection"))?;
        rule.SetProfiles(NET_FW_PROFILE2_PRIVATE.0 | NET_FW_PROFILE2_DOMAIN.0)
            .map_err(SvcError::win32("INetFwRule::SetProfiles"))?;
        rule.SetAction(NET_FW_ACTION_ALLOW)
            .map_err(SvcError::win32("INetFwRule::SetAction"))?;
        rule.SetEnabled(VARIANT_TRUE)
            .map_err(SvcError::win32("INetFwRule::SetEnabled"))?;
        rules
            .Add(&rule)
            .map_err(SvcError::win32("INetFwRules::Add"))?;
    }
    Ok(())
}

/// Removes the rule; absent is fine.
pub fn remove_rule() -> Result<(), SvcError> {
    let _com = com::init()?;
    // SAFETY: COM is initialized for the guard's lifetime; the BSTR outlives the calls.
    unsafe {
        let rules = policy()?
            .Rules()
            .map_err(SvcError::win32("INetFwPolicy2::Rules"))?;
        let name = BSTR::from(RULE_NAME);
        if rules.Item(&name).is_ok() {
            rules
                .Remove(&name)
                .map_err(SvcError::win32("INetFwRules::Remove"))?;
        }
    }
    Ok(())
}
