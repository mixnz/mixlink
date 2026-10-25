//! Platform code of the stream window. Only module tree allowed to use `unsafe`.

#[cfg(windows)]
pub(crate) mod window;
