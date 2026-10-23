//! WASAPI: shared-mode loopback capture of the default render endpoint (event-driven), endpoint list.

use windows::Win32::Devices::FunctionDiscovery::PKEY_Device_FriendlyName;
use windows::Win32::Foundation::{CloseHandle, HANDLE, WAIT_OBJECT_0};
use windows::Win32::Media::Audio::{
    AUDCLNT_SHAREMODE_SHARED, AUDCLNT_STREAMFLAGS_EVENTCALLBACK, AUDCLNT_STREAMFLAGS_LOOPBACK,
    DEVICE_STATE_ACTIVE, IAudioCaptureClient, IAudioClient, IAudioClient3, IMMDevice,
    IMMDeviceEnumerator, MMDeviceEnumerator, WAVEFORMATEX, eConsole, eRender,
};
use windows::Win32::System::Com::StructuredStorage::PropVariantToStringAlloc;
use windows::Win32::System::Com::{
    CLSCTX_ALL, COINIT_MULTITHREADED, CoCreateInstance, CoInitializeEx, CoTaskMemFree, STGM_READ,
};
use windows::Win32::System::Threading::{CreateEventW, WaitForSingleObject};
use windows::core::Interface;

use crate::util::BoxResult;

const WAVE_FORMAT_IEEE_FLOAT: u16 = 3;
const WAVE_FORMAT_EXTENSIBLE: u16 = 0xFFFE;

/// Joins the multithreaded apartment for this thread (already joined = fine).
pub fn com_init() {
    // SAFETY: plain COM init; the result only says whether this call joined.
    let _ = unsafe { CoInitializeEx(None, COINIT_MULTITHREADED) };
}

pub fn enumerator() -> BoxResult<IMMDeviceEnumerator> {
    // SAFETY: standard COM creation of the MMDevice enumerator.
    Ok(unsafe { CoCreateInstance(&MMDeviceEnumerator, None, CLSCTX_ALL)? })
}

fn friendly_name(dev: &IMMDevice) -> String {
    // SAFETY: property store read; the allocated string is freed with CoTaskMemFree.
    unsafe {
        let Ok(store) = dev.OpenPropertyStore(STGM_READ) else {
            return "?".into();
        };
        let Ok(value) = store.GetValue(&PKEY_Device_FriendlyName) else {
            return "?".into();
        };
        let Ok(text) = PropVariantToStringAlloc(&value) else {
            return "?".into();
        };
        let name = text.to_string().unwrap_or_default();
        CoTaskMemFree(Some(text.0 as *const _));
        name
    }
}

/// Active render endpoints; the default one is marked `*`.
pub fn endpoints() -> BoxResult<Vec<String>> {
    com_init();
    let en = enumerator()?;
    // SAFETY: COM calls on live interfaces.
    unsafe {
        let default = en
            .GetDefaultAudioEndpoint(eRender, eConsole)
            .map(|d| friendly_name(&d))
            .ok();
        let all = en.EnumAudioEndpoints(eRender, DEVICE_STATE_ACTIVE)?;
        let mut list = Vec::new();
        for i in 0..all.GetCount()? {
            let name = friendly_name(&all.Item(i)?);
            let mark = if default.as_deref() == Some(name.as_str()) {
                "*"
            } else {
                " "
            };
            list.push(format!("{mark} {name}"));
        }
        Ok(list)
    }
}

/// Mix format of `client`, checked to be 32-bit float (the shared-mode engine format).
/// Returns (format pointer to free with CoTaskMemFree, channels, sample rate).
fn float_mix_format(client: &IAudioClient) -> BoxResult<(*mut WAVEFORMATEX, u16, u32)> {
    // SAFETY: GetMixFormat returns a CoTaskMem block we read and later free.
    unsafe {
        let fmt = client.GetMixFormat()?;
        let f = *fmt;
        let tag = f.wFormatTag;
        let bits = f.wBitsPerSample;
        if bits != 32 || (tag != WAVE_FORMAT_IEEE_FLOAT && tag != WAVE_FORMAT_EXTENSIBLE) {
            CoTaskMemFree(Some(fmt as *const _));
            return Err(
                format!("mix format is not 32-bit float (tag {tag:#x}, {bits} bit)").into(),
            );
        }
        Ok((fmt, f.nChannels, f.nSamplesPerSec))
    }
}

/// Per-packet callback: (samples, frames, flags, device QPC position in 100 ns).
pub type PacketSink<'a> = dyn FnMut(&[f32], u32, u32, u64) + 'a;

pub struct Loopback {
    client: IAudioClient,
    capture: IAudioCaptureClient,
    event: HANDLE,
    pub channels: u16,
    pub rate: u32,
    /// Endpoint, init path and period, for the summary.
    pub info: String,
}

impl Loopback {
    /// Opens loopback on the current default render endpoint: `IAudioClient3` at the minimum engine
    /// period first, plain `IAudioClient::Initialize` with the default period if that is refused.
    pub fn open_default() -> BoxResult<Self> {
        com_init();
        let flags = AUDCLNT_STREAMFLAGS_LOOPBACK | AUDCLNT_STREAMFLAGS_EVENTCALLBACK;
        // SAFETY: COM calls on live interfaces; the format block is freed before returning.
        unsafe {
            let dev = enumerator()?.GetDefaultAudioEndpoint(eRender, eConsole)?;
            let name = friendly_name(&dev);
            let c3: IAudioClient3 = dev.Activate(CLSCTX_ALL, None)?;
            let (fmt, channels, rate) = float_mix_format(&c3.cast()?)?;
            let (mut def, mut fund, mut min, mut max) = (0, 0, 0, 0);
            c3.GetSharedModeEnginePeriod(fmt, &mut def, &mut fund, &mut min, &mut max)?;
            let (client, path) = match c3.InitializeSharedAudioStream(flags, min, fmt, None) {
                Ok(()) => (c3.cast()?, format!("IAudioClient3 period {min} frames")),
                Err(e) => {
                    let client: IAudioClient = dev.Activate(CLSCTX_ALL, None)?;
                    let init = client.Initialize(AUDCLNT_SHAREMODE_SHARED, flags, 0, 0, fmt, None);
                    if let Err(e2) = init {
                        CoTaskMemFree(Some(fmt as *const _));
                        return Err(e2.into());
                    }
                    let (mut dp, mut mp) = (0i64, 0i64);
                    client.GetDevicePeriod(Some(&mut dp), Some(&mut mp))?;
                    let why = format!(
                        "IAudioClient3 refused: {:#010x} {}",
                        e.code().0,
                        e.message()
                    );
                    (
                        client,
                        format!(
                            "IAudioClient default period {} us (min {} us); {why}",
                            dp / 10,
                            mp / 10
                        ),
                    )
                }
            };
            CoTaskMemFree(Some(fmt as *const _));
            let event = CreateEventW(None, false, false, None)?;
            client.SetEventHandle(event)?;
            let capture: IAudioCaptureClient = client.GetService()?;
            client.Start()?;
            let info = format!(
                "endpoint \"{name}\", {rate} Hz x{channels}, {path}; engine period frames default {def} fundamental {fund} min {min} max {max}"
            );
            Ok(Self {
                client,
                capture,
                event,
                channels,
                rate,
                info,
            })
        }
    }

    /// Waits for the next period event; false on timeout.
    pub fn wait(&self, ms: u32) -> bool {
        // SAFETY: the event handle is alive.
        unsafe { WaitForSingleObject(self.event, ms) == WAIT_OBJECT_0 }
    }

    /// Hands every queued packet to `f(samples, frames, flags, qpc_100ns)`; `samples` is empty for a
    /// silent packet (`frames` still counts it).
    // HOT PATH
    pub fn read(&mut self, f: &mut PacketSink) -> BoxResult<()> {
        // SAFETY: buffer valid between GetBuffer and ReleaseBuffer; frames x channels float samples.
        unsafe {
            while self.capture.GetNextPacketSize()? > 0 {
                let (mut data, mut frames, mut flags, mut qpc) =
                    (std::ptr::null_mut(), 0u32, 0u32, 0u64);
                self.capture
                    .GetBuffer(&mut data, &mut frames, &mut flags, None, Some(&mut qpc))?;
                let silent = flags & 2 != 0;
                let samples = if silent || data.is_null() {
                    &[][..]
                } else {
                    std::slice::from_raw_parts(
                        data as *const f32,
                        frames as usize * usize::from(self.channels),
                    )
                };
                f(samples, frames, flags, qpc);
                self.capture.ReleaseBuffer(frames)?;
            }
        }
        Ok(())
    }
}

impl Drop for Loopback {
    fn drop(&mut self) {
        // SAFETY: stop the live client and close our event.
        unsafe {
            let _ = self.client.Stop();
            let _ = CloseHandle(self.event);
        }
    }
}
