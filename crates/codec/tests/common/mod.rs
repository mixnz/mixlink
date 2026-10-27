//! Test helpers: D3D11 devices and the tiny test bitstreams split into access units.
#![allow(dead_code)]

use mixlink_codec::{AccessUnitParser, CodecError, VideoCodec, has_hardware_decoder};
use windows::Win32::Foundation::HMODULE;
use windows::Win32::Graphics::Direct3D::{
    D3D_DRIVER_TYPE, D3D_DRIVER_TYPE_HARDWARE, D3D_DRIVER_TYPE_WARP,
};
use windows::Win32::Graphics::Direct3D11::{
    D3D11_CREATE_DEVICE_BGRA_SUPPORT, D3D11_CREATE_DEVICE_FLAG, D3D11_CREATE_DEVICE_VIDEO_SUPPORT,
    D3D11_SDK_VERSION, D3D11CreateDevice, ID3D11Device,
};

pub mod pixels;

pub const TINY_H264: &[u8] = include_bytes!("../data/tiny.h264");
pub const TINY_HEVC: &[u8] = include_bytes!("../data/tiny.hevc");

fn create(driver: D3D_DRIVER_TYPE, flags: D3D11_CREATE_DEVICE_FLAG) -> Option<ID3D11Device> {
    let mut device = None;
    // SAFETY: valid out-pointer; no adapter, module or feature levels passed.
    unsafe {
        D3D11CreateDevice(
            None,
            driver,
            HMODULE::default(),
            flags,
            None,
            D3D11_SDK_VERSION,
            Some(&mut device),
            None,
            None,
        )
    }
    .ok()?;
    device
}

/// Hardware device that can decode `codec`, or `None` (CI has no GPU).
pub fn hw_device_with_decoder(codec: VideoCodec) -> Option<ID3D11Device> {
    let flags = D3D11_CREATE_DEVICE_FLAG(
        D3D11_CREATE_DEVICE_BGRA_SUPPORT.0 | D3D11_CREATE_DEVICE_VIDEO_SUPPORT.0,
    );
    create(D3D_DRIVER_TYPE_HARDWARE, flags).filter(|d| has_hardware_decoder(d, codec))
}

pub fn warp_device() -> Option<ID3D11Device> {
    create(D3D_DRIVER_TYPE_WARP, D3D11_CREATE_DEVICE_BGRA_SUPPORT)
}

pub fn stream(codec: VideoCodec) -> &'static [u8] {
    match codec {
        VideoCodec::H264 => TINY_H264,
        VideoCodec::Hevc => TINY_HEVC,
    }
}

/// The 10 access units of a tiny test stream, each owned.
pub fn access_units(codec: VideoCodec) -> Result<Vec<Vec<u8>>, CodecError> {
    let mut parser = AccessUnitParser::new(codec)?;
    let mut aus = Vec::new();
    let mut input = stream(codec);
    while !input.is_empty() {
        if let Some(au) = parser.push(&mut input)? {
            aus.push(au.to_vec());
        }
    }
    while let Some(au) = parser.flush()? {
        aus.push(au.to_vec());
    }
    Ok(aus)
}

/// Hardware device or a skip message; use as `let Some(d) = device_or_skip(codec) else { return };`.
pub fn device_or_skip(codec: VideoCodec) -> Option<ID3D11Device> {
    let device = hw_device_with_decoder(codec);
    if device.is_none() {
        eprintln!("skip: no D3D11VA decoder for {codec:?}");
    }
    device
}
