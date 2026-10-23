//! Test tone: 1 kHz sine at -12 dBFS on the default render endpoint (shared mode, event-driven).

use std::sync::atomic::{AtomicBool, Ordering};

use windows::Win32::Foundation::{CloseHandle, WAIT_OBJECT_0};
use windows::Win32::Media::Audio::{
    AUDCLNT_SHAREMODE_SHARED, AUDCLNT_STREAMFLAGS_EVENTCALLBACK, IAudioClient, IAudioRenderClient,
    eConsole, eRender,
};
use windows::Win32::System::Com::{CLSCTX_ALL, CoTaskMemFree};
use windows::Win32::System::Threading::{CreateEventW, WaitForSingleObject};

use crate::platform::wasapi::{com_init, enumerator};
use crate::util::BoxResult;

const FREQ_HZ: f64 = 1_000.0;
/// -12 dBFS.
const AMPLITUDE: f64 = 0.251;

/// Plays until `stop` is set. Needs a 32-bit float mix format (the shared-mode default).
pub fn play(stop: &AtomicBool) -> BoxResult<()> {
    com_init();
    // SAFETY: COM calls on live interfaces; buffer written between GetBuffer and ReleaseBuffer
    // within the frame count WASAPI granted.
    unsafe {
        let dev = enumerator()?.GetDefaultAudioEndpoint(eRender, eConsole)?;
        let client: IAudioClient = dev.Activate(CLSCTX_ALL, None)?;
        let fmt = client.GetMixFormat()?;
        let (channels, rate, bits) = (
            (*fmt).nChannels as usize,
            (*fmt).nSamplesPerSec,
            (*fmt).wBitsPerSample,
        );
        let init = client.Initialize(
            AUDCLNT_SHAREMODE_SHARED,
            AUDCLNT_STREAMFLAGS_EVENTCALLBACK,
            0,
            0,
            fmt,
            None,
        );
        CoTaskMemFree(Some(fmt as *const _));
        init?;
        if bits != 32 {
            return Err(format!("mix format is {bits}-bit, tone needs 32-bit float").into());
        }
        let event = CreateEventW(None, false, false, None)?;
        client.SetEventHandle(event)?;
        let render: IAudioRenderClient = client.GetService()?;
        let size = client.GetBufferSize()?;
        client.Start()?;
        let step = 2.0 * std::f64::consts::PI * FREQ_HZ / f64::from(rate);
        let mut phase = 0.0f64;
        while !stop.load(Ordering::Acquire) {
            if WaitForSingleObject(event, 200) != WAIT_OBJECT_0 {
                continue;
            }
            let free = size - client.GetCurrentPadding()?;
            if free == 0 {
                continue;
            }
            let data = render.GetBuffer(free)? as *mut f32;
            let out = std::slice::from_raw_parts_mut(data, free as usize * channels);
            for frame in out.chunks_exact_mut(channels) {
                frame.fill((phase.sin() * AMPLITUDE) as f32);
                phase = (phase + step) % (2.0 * std::f64::consts::PI);
            }
            render.ReleaseBuffer(free, 0)?;
        }
        let _ = client.Stop();
        let _ = CloseHandle(event);
    }
    Ok(())
}
