//! Virtual input (spec 0016): the `vhid` feeder the service holds, virtual Xbox pads per slot, and
//! the `input ...` dev commands.

pub mod commands;
pub mod feeder;
pub mod pad_ids;
pub mod padmap;
pub mod pads;
pub mod probe;
pub mod probe_run;
pub mod script;
pub mod status;

pub use feeder::open_feeder;
