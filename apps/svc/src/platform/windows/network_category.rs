//! Is a connected network Public (spec 0017)? Our firewall rule covers Private and Domain only.

use windows::Win32::Networking::NetworkListManager::{
    INetwork, INetworkListManager, NLM_ENUM_NETWORK_CONNECTED, NLM_NETWORK_CATEGORY_PUBLIC,
    NetworkListManager,
};
use windows::Win32::System::Com::{CLSCTX_ALL, CoCreateInstance};

use super::com;
use crate::error::SvcError;

pub fn any_public_network() -> Result<bool, SvcError> {
    let _com = com::init()?;
    // SAFETY: COM is initialized on this thread for the guard's lifetime.
    let manager: INetworkListManager =
        unsafe { CoCreateInstance(&NetworkListManager, None, CLSCTX_ALL) }
            .map_err(SvcError::win32("CoCreateInstance(NetworkListManager)"))?;
    // SAFETY: valid interface from above.
    let networks = unsafe { manager.GetNetworks(NLM_ENUM_NETWORK_CONNECTED) }
        .map_err(SvcError::win32("GetNetworks"))?;
    loop {
        let mut item: [Option<INetwork>; 1] = [None];
        let mut fetched = 0u32;
        // SAFETY: one-element output slice and a live counter.
        unsafe { networks.Next(&mut item, Some(&mut fetched)) }
            .map_err(SvcError::win32("IEnumNetworks::Next"))?;
        let [Some(network)] = item else {
            return Ok(false);
        };
        if fetched == 0 {
            return Ok(false);
        }
        // SAFETY: valid interface returned by the enumerator.
        let category =
            unsafe { network.GetCategory() }.map_err(SvcError::win32("INetwork::GetCategory"))?;
        if category == NLM_NETWORK_CATEGORY_PUBLIC {
            return Ok(true);
        }
    }
}
