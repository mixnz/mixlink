//! Video facts shared by the decoder (codec) and the presenter (render), which never depend on each other.

/// YCbCr → RGB matrix of a stream (limited range, 8-bit). The host tags NVENC streams BT.601: NVENC
/// converts BGRA with that matrix (spec 0019).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum ColorMatrix {
    Bt601,
    /// Also what an untagged stream gets.
    #[default]
    Bt709,
}
