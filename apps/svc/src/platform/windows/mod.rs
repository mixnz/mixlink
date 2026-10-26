//! Windows-only code; every `unsafe` block of the service lives here.

pub mod catalog;
pub mod ccd;
pub mod cert;
pub mod cert_der;
pub mod console;
pub mod console_process;
pub mod devices;
pub mod dir_security;
pub mod display_agent;
pub mod display_config;
pub mod driver_store;
pub mod drivers_host;
pub mod drivers_reg;
pub mod gpu_name;
pub mod install;
pub mod pad_nodes;
pub mod pipe;
pub mod random;
pub mod raw_input;
pub mod security;
pub mod service;
pub mod sign;
pub mod trust_store;
pub mod vdisplay;
pub mod xinput;
