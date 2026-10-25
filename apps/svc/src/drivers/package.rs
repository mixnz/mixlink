//! Driver packages we ship and where they are found (spec 0014).

use std::path::{Path, PathBuf};

use crate::error::SvcError;

/// One of our UMDF driver packages and the root device it binds to.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct DriverSpec {
    pub name: &'static str,
    pub inf: &'static str,
    pub cat: &'static str,
    pub hardware_id: &'static str,
    /// Setup class of the device node we create.
    pub class_name: &'static str,
    pub class_guid: &'static str,
}

/// Install order: display first, then input.
pub const DRIVERS: [DriverSpec; 2] = [
    DriverSpec {
        name: "vdisplay",
        inf: "vdisplay.inf",
        cat: "vdisplay.cat",
        hardware_id: r"Root\MixLinkVDisplay",
        class_name: "Display",
        class_guid: "{4D36E968-E325-11CE-BFC1-08002BE10318}",
    },
    DriverSpec {
        name: "vhid",
        inf: "vhid.inf",
        cat: "vhid.cat",
        hardware_id: r"Root\MixLinkVHid",
        class_name: "HIDClass",
        class_guid: "{745A17A0-74D3-11D0-B6FE-00A0C90F57DA}",
    },
];

/// Folder holding `spec`'s unsigned package. With `from` (development): `<from>\<name>\pkg`, then
/// `<from>\<name>`; without: `<exe_dir>\drivers\<name>` (the `dist/` layout).
pub fn resolve_source(
    exe_dir: &Path,
    from: Option<&Path>,
    spec: &DriverSpec,
) -> Result<PathBuf, SvcError> {
    let candidates = match from {
        Some(from) => vec![from.join(spec.name).join("pkg"), from.join(spec.name)],
        None => vec![exe_dir.join("drivers").join(spec.name)],
    };
    for dir in &candidates {
        if dir.join(spec.inf).is_file() {
            return Ok(dir.clone());
        }
    }
    let path = candidates.into_iter().next().unwrap_or_default();
    Err(SvcError::PackageMissing { path })
}

/// Where `install` copies and signs a package: `<ProgramData>\MixLink\drivers\<name>`.
pub fn stage_dir(program_data: &Path, name: &str) -> PathBuf {
    program_data.join("MixLink").join("drivers").join(name)
}

#[cfg(test)]
#[path = "package_tests.rs"]
mod tests;
