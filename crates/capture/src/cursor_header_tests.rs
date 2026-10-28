use super::*;

/// A cursor section in plain memory, written the way the driver does.
struct Mem(Vec<u8>);

impl Mem {
    fn new() -> Mem {
        let mut mem = Mem(vec![0; HEADER_LEN + SHAPE_MAX_BYTES]);
        mem.put(OFF_MAGIC, MAGIC);
        mem
    }

    fn put(&mut self, offset: usize, value: u32) {
        self.0[offset..offset + 4].copy_from_slice(&value.to_le_bytes());
    }

    fn position(&mut self, visible: bool, x: i32, y: i32) {
        self.put(OFF_VISIBLE, u32::from(visible));
        self.put(OFF_X, x as u32);
        self.put(OFF_Y, y as u32);
    }

    fn shape(&mut self, id: u32, width: u32, height: u32, hot: (u32, u32)) {
        let pitch = width * 4;
        self.put(OFF_SHAPE_ID, id);
        self.put(OFF_SHAPE_TYPE, 2);
        self.put(OFF_WIDTH, width);
        self.put(OFF_HEIGHT, height);
        self.put(OFF_PITCH, pitch);
        self.put(OFF_XHOT, hot.0);
        self.put(OFF_YHOT, hot.1);
        self.put(OFF_SHAPE_BYTES, pitch * height);
        for (i, byte) in self.0[HEADER_LEN..HEADER_LEN + (pitch * height) as usize]
            .iter_mut()
            .enumerate()
        {
            *byte = (i % 251) as u8;
        }
    }
}

impl CursorView for Mem {
    fn word(&self, offset: usize) -> u32 {
        u32::from_le_bytes(self.0[offset..offset + 4].try_into().unwrap())
    }

    fn copy_pixels(&self, out: &mut [u8]) {
        out.copy_from_slice(&self.0[HEADER_LEN..HEADER_LEN + out.len()]);
    }
}

fn read(mem: &Mem, last_shape: u32) -> (Snapshot, Vec<u8>) {
    let mut out = vec![0u8; SHAPE_MAX_BYTES];
    let snap = read_snapshot(mem, &mut out, last_shape);
    (snap, out)
}

#[test]
fn header_layout_matches_driver() {
    // drivers/vdisplay/CursorShare.h static_asserts.
    assert_eq!(
        [
            OFF_MAGIC,
            OFF_SEQ,
            OFF_VISIBLE,
            OFF_X,
            OFF_Y,
            OFF_SHAPE_ID,
            OFF_SHAPE_TYPE,
            OFF_WIDTH,
            OFF_HEIGHT,
            OFF_PITCH,
            OFF_XHOT,
            OFF_YHOT,
            OFF_SHAPE_BYTES
        ],
        [0, 4, 8, 12, 16, 20, 24, 28, 32, 36, 40, 44, 48]
    );
    assert_eq!(HEADER_LEN, 64);
    assert_eq!(SHAPE_MAX_BYTES, 256 * 256 * 4);
    assert_eq!(MAGIC.to_le_bytes(), *b"MXC1");
}

#[test]
fn position_is_hotspot() {
    let mut mem = Mem::new();
    mem.position(true, -3, 100);
    mem.shape(7, 32, 32, (4, 9));
    let (snap, _) = read(&mem, 7);
    assert_eq!(
        snap,
        Snapshot::Cursor {
            pos: CursorPos {
                x: 1,
                y: 109,
                visible: true,
                shape_id: 7
            },
            shape: None
        }
    );
}

#[test]
fn shape_only_when_id_changes() {
    let mut mem = Mem::new();
    mem.position(true, 10, 10);
    mem.shape(8, 16, 2, (0, 0));
    let (snap, out) = read(&mem, 7);
    let Snapshot::Cursor {
        shape: Some(info), ..
    } = snap
    else {
        panic!("no shape: {snap:?}");
    };
    assert_eq!(
        info,
        ShapeInfo {
            kind: ShapeType::Alpha,
            width: 16,
            height: 2,
            hot_x: 0,
            hot_y: 0,
            pitch: 64,
            len: 128
        }
    );
    assert_eq!(&out[..128], &mem.0[HEADER_LEN..HEADER_LEN + 128]);
    let (again, _) = read(&mem, 8);
    assert!(matches!(again, Snapshot::Cursor { shape: None, .. }));
}

#[test]
fn masked_color_kind() {
    let mut mem = Mem::new();
    mem.shape(9, 8, 8, (0, 0));
    mem.put(OFF_SHAPE_TYPE, 1);
    let (snap, _) = read(&mem, 0);
    assert!(matches!(
        snap,
        Snapshot::Cursor {
            shape: Some(ShapeInfo {
                kind: ShapeType::MaskedColor,
                ..
            }),
            ..
        }
    ));
}

#[test]
fn no_shape_yet_is_none() {
    let mut mem = Mem::new();
    mem.position(false, 0, 0);
    let (snap, _) = read(&mem, 0);
    assert!(matches!(
        snap,
        Snapshot::Cursor {
            pos: CursorPos {
                visible: false,
                shape_id: 0,
                ..
            },
            shape: None
        }
    ));
}

#[test]
fn torn_read_retries() {
    let mut mem = Mem::new();
    mem.shape(8, 16, 16, (0, 0));
    mem.put(OFF_SEQ, 3);
    assert_eq!(read(&mem, 0).0, Snapshot::Torn);
    mem.put(OFF_SEQ, 4);
    assert!(matches!(read(&mem, 0).0, Snapshot::Cursor { .. }));
}

#[test]
fn magic_zero_is_gone() {
    let mut mem = Mem::new();
    mem.put(OFF_MAGIC, 0);
    assert_eq!(read(&mem, 0).0, Snapshot::Gone);
}

#[test]
fn oversize_shape_rejected() {
    let mut mem = Mem::new();
    mem.shape(8, 16, 16, (0, 0));
    mem.put(OFF_SHAPE_BYTES, (SHAPE_MAX_BYTES + 4) as u32);
    assert_eq!(read(&mem, 0).0, Snapshot::Torn);
    mem.put(OFF_SHAPE_BYTES, 16 * 4 * 16 - 4);
    assert_eq!(read(&mem, 0).0, Snapshot::Torn, "bytes != pitch * height");
}

#[test]
fn unknown_shape_type_rejected() {
    let mut mem = Mem::new();
    mem.shape(8, 16, 16, (0, 0));
    mem.put(OFF_SHAPE_TYPE, 9);
    assert_eq!(read(&mem, 0).0, Snapshot::Torn);
}

#[test]
fn event_and_section_names() {
    assert_eq!(cursor_section_name(2), r"Global\MixLinkCursor-2");
    assert_eq!(cursor_event_name(0), r"Global\MixLinkCursorReady-0");
}
