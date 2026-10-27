//! Shader helpers shared by the draw passes: compile HLSL at start with `D3DCompile` (ships with
//! Windows, no shader tools in the build) and unwrap `windows` out-parameters.

use windows::Win32::Graphics::Direct3D::Fxc::{D3DCOMPILE_OPTIMIZATION_LEVEL3, D3DCompile};
use windows::Win32::Graphics::Direct3D::ID3DBlob;
use windows::core::PCSTR;

use crate::RenderError;

pub(crate) fn win32(context: &'static str) -> impl FnOnce(windows::core::Error) -> RenderError {
    move |source| RenderError::Win32 { context, source }
}

/// Unwraps a `windows` out-parameter that a successful call must have filled.
pub(crate) fn filled<T>(value: Option<T>, context: &'static str) -> Result<T, RenderError> {
    value.ok_or_else(|| win32(context)(windows::core::Error::empty()))
}

/// Compiles `entry` of `source` for `target` (`vs_4_0`, `ps_4_0`); `name` shows in errors.
pub(crate) fn compile(
    source: &str,
    name: PCSTR,
    entry: PCSTR,
    target: PCSTR,
) -> Result<ID3DBlob, RenderError> {
    let mut code = None;
    // SAFETY: the source slice is valid for its length; `code` is a valid out-pointer.
    unsafe {
        D3DCompile(
            source.as_ptr().cast(),
            source.len(),
            name,
            None,
            None,
            entry,
            target,
            D3DCOMPILE_OPTIMIZATION_LEVEL3,
            0,
            &mut code,
            None,
        )
    }
    .map_err(win32("D3DCompile"))?;
    filled(code, "D3DCompile")
}

pub(crate) fn blob_bytes(blob: &ID3DBlob) -> &[u8] {
    // SAFETY: the blob owns `GetBufferSize` bytes at `GetBufferPointer` while borrowed.
    unsafe { std::slice::from_raw_parts(blob.GetBufferPointer().cast(), blob.GetBufferSize()) }
}
