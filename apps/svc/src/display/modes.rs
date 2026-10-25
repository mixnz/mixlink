//! Mode list and add request for one virtual monitor (spec 0015).

use mixlink_core::ids::DeviceId;
use mixlink_proto::control::DisplayInfo;

use super::edid::{build, container_id};
use super::wire::{AddMonitor, MAX_MODES, WireMode};

/// Fallback modes Windows may pick besides the client's own (w, h, mHz).
const STANDARD: [(u32, u32, u32); 11] = [
    (3840, 2160, 60_000),
    (2560, 1440, 144_000),
    (2560, 1440, 120_000),
    (2560, 1440, 90_000),
    (2560, 1440, 60_000),
    (1920, 1080, 144_000),
    (1920, 1080, 90_000),
    (1920, 1080, 60_000),
    (1600, 900, 60_000),
    (1024, 768, 75_000),
    (1024, 768, 60_000),
];

/// The client mode first (preferred), then the standard modes that fit inside it.
pub fn mode_list(client: &DisplayInfo) -> Vec<WireMode> {
    let first = WireMode {
        width: client.width,
        height: client.height,
        refresh_mhz: client.refresh_mhz,
    };
    let rest = STANDARD
        .iter()
        .map(|&(width, height, refresh_mhz)| WireMode {
            width,
            height,
            refresh_mhz,
        })
        .filter(|m| *m != first && m.width <= first.width && m.height <= first.height);
    std::iter::once(first).chain(rest).take(MAX_MODES).collect()
}

/// `ADD_MONITOR` request for client `id` (or the headless monitor) at `mode`.
pub fn add_request(id: &DeviceId, product: u16, mode: &DisplayInfo) -> AddMonitor {
    AddMonitor {
        container: container_id(id),
        edid: build(id, product, mode).to_bytes(),
        modes: mode_list(mode),
        preferred: 0,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::display::edid::PRODUCT_CLIENT;

    fn info(width: u32, height: u32, refresh_mhz: u32) -> DisplayInfo {
        DisplayInfo {
            width,
            height,
            refresh_mhz,
            hdr: false,
            dpi: 96,
        }
    }

    #[test]
    fn mode_list_client_first() {
        let list = mode_list(&info(2560, 1440, 120_000));
        assert_eq!(
            list[0],
            WireMode {
                width: 2560,
                height: 1440,
                refresh_mhz: 120_000
            }
        );
        assert!(list.len() <= MAX_MODES);
        assert!(list.iter().all(|m| m.width <= 2560 && m.height <= 1440));
        for (i, a) in list.iter().enumerate() {
            assert!(list[i + 1..].iter().all(|b| b != a));
        }
        assert!(list.len() > 5);
    }

    #[test]
    fn mode_list_small_client() {
        let list = mode_list(&info(1280, 720, 60_000));
        let sizes: Vec<(u32, u32, u32)> = list
            .iter()
            .map(|m| (m.width, m.height, m.refresh_mhz))
            .collect();
        // 1024x768 is taller than 720, so no standard mode fits.
        assert_eq!(sizes, vec![(1280, 720, 60_000)]);
    }

    #[test]
    fn add_request_uses_edid_and_container() {
        let id = DeviceId::from_bytes([9; 16]);
        let req = add_request(&id, PRODUCT_CLIENT, &info(1920, 1080, 60_000));
        assert_eq!(req.preferred, 0);
        assert_eq!(req.edid.len(), 128);
        assert_eq!(req.container, container_id(&id));
        assert!(req.encode().is_ok());
    }
}
