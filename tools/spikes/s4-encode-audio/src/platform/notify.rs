//! Default render endpoint changes: the callback only sets a flag; the capture loop reopens the stream
//! (no COM calls on the notification thread).

use std::sync::atomic::{AtomicBool, Ordering};

use windows::Win32::Foundation::PROPERTYKEY;
use windows::Win32::Media::Audio::{
    DEVICE_STATE, EDataFlow, ERole, IMMDeviceEnumerator, IMMNotificationClient,
    IMMNotificationClient_Impl, eConsole, eRender,
};
use windows::core::{PCWSTR, Result, implement};

use crate::platform::wasapi::{com_init, enumerator};
use crate::util::BoxResult;

#[implement(IMMNotificationClient)]
struct Client {
    flag: &'static AtomicBool,
}

impl IMMNotificationClient_Impl for Client_Impl {
    fn OnDeviceStateChanged(&self, _id: &PCWSTR, _state: DEVICE_STATE) -> Result<()> {
        Ok(())
    }
    fn OnDeviceAdded(&self, _id: &PCWSTR) -> Result<()> {
        Ok(())
    }
    fn OnDeviceRemoved(&self, _id: &PCWSTR) -> Result<()> {
        Ok(())
    }
    fn OnDefaultDeviceChanged(&self, flow: EDataFlow, role: ERole, _id: &PCWSTR) -> Result<()> {
        if flow == eRender && role == eConsole {
            self.flag.store(true, Ordering::Release);
        }
        Ok(())
    }
    fn OnPropertyValueChanged(&self, _id: &PCWSTR, _key: &PROPERTYKEY) -> Result<()> {
        Ok(())
    }
}

/// Keeps the callback registered until dropped.
pub struct Registration {
    enumerator: IMMDeviceEnumerator,
    client: IMMNotificationClient,
}

pub fn watch_default_render(flag: &'static AtomicBool) -> BoxResult<Registration> {
    com_init();
    let enumerator = enumerator()?;
    let client: IMMNotificationClient = Client { flag }.into();
    // SAFETY: the client stays alive in the registration until it is unregistered.
    unsafe { enumerator.RegisterEndpointNotificationCallback(&client)? };
    Ok(Registration { enumerator, client })
}

impl Drop for Registration {
    fn drop(&mut self) {
        // SAFETY: unregisters the client registered in `watch_default_render`.
        unsafe {
            let _ = self
                .enumerator
                .UnregisterEndpointNotificationCallback(&self.client);
        }
    }
}
