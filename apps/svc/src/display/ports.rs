//! What the display session logic needs from the system, as traits so it is tested with fakes (spec 0015).

use mixlink_core::config::Topology;
use mixlink_proto::control::DisplayInfo;

use super::topology::{RestorePlan, Snapshot};
use super::wire::{AddMonitor, QueryReply};
use crate::error::SvcError;

/// Windows' screen layout (CCD API, or a helper process that calls it; spec 0015).
#[expect(
    dead_code,
    reason = "used by the display session flow next (spec 0015)"
)]
pub trait DisplayConfig {
    /// The active displays now.
    fn snapshot(&mut self) -> Result<Snapshot, SvcError>;
    /// Device path of the MixLink monitor with this EDID product code, once Windows lists it.
    fn find_mixlink(&mut self, product: u16) -> Result<Option<String>, SvcError>;
    /// Whether any active display is not a MixLink monitor.
    fn has_physical_active(&mut self) -> Result<bool, SvcError>;
    /// Makes `monitor_path` the main display at `mode`; others off or to its right.
    fn switch_to(
        &mut self,
        monitor_path: &str,
        mode: &DisplayInfo,
        topology: Topology,
    ) -> Result<(), SvcError>;
    fn restore(&mut self, plan: &RestorePlan) -> Result<(), SvcError>;
}

/// The `vdisplay` control channel.
pub trait DisplayDriver {
    /// Returns the new monitor's id.
    fn add_monitor(&mut self, req: &AddMonitor) -> Result<u32, SvcError>;
    fn remove_monitor(&mut self, id: u32) -> Result<(), SvcError>;
    fn query(&mut self) -> Result<QueryReply, SvcError>;
}
