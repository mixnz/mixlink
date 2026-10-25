//! What the display session logic needs from the system, as traits so it is tested with fakes (spec 0015).

use super::wire::{AddMonitor, QueryReply};
use crate::error::SvcError;

/// The `vdisplay` control channel.
pub trait DisplayDriver {
    /// Returns the new monitor's id.
    fn add_monitor(&mut self, req: &AddMonitor) -> Result<u32, SvcError>;
    fn remove_monitor(&mut self, id: u32) -> Result<(), SvcError>;
    fn query(&mut self) -> Result<QueryReply, SvcError>;
}
