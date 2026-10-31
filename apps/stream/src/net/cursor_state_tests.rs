use mixlink_render::cursor::{CursorKind, convert_shape};

use super::*;

fn draw(x: i32, y: i32, visible: bool) -> CursorDraw {
    CursorDraw { x, y, visible }
}

fn image() -> CursorImage {
    convert_shape(CursorKind::Alpha, 1, 1, (0, 0), 4, &[1, 2, 3, 255]).unwrap()
}

#[test]
fn latest_position_wins() {
    let (mut tx, mut rx) = cursor_channel();
    assert!(rx.take().is_none());
    let now = Instant::now();
    tx.push_shape(4, image());
    tx.set_position(draw(1, 2, true), 4, now);
    tx.set_position(draw(-30, 700, false), 4, now);
    let change = rx.take().unwrap();
    assert_eq!(change.draw, draw(-30, 700, false));
    assert_eq!(change.shape_id, 4);
    assert_eq!(change.new_shape.map(|(id, _)| id), Some(4));
    assert!(rx.take().is_none(), "nothing new");
}

#[test]
fn unknown_shape_requested_once() {
    let (mut tx, _rx) = cursor_channel();
    let t0 = Instant::now();
    assert_eq!(tx.set_position(draw(0, 0, true), 9, t0), Some(9));
    assert_eq!(
        tx.set_position(draw(1, 0, true), 9, t0 + Duration::from_millis(50)),
        None
    );
    assert_eq!(
        tx.set_position(draw(2, 0, true), 9, t0 + SHAPE_REQUEST_GAP),
        Some(9),
        "asked again after the gap"
    );
    tx.push_shape(9, image());
    assert_eq!(
        tx.set_position(draw(3, 0, true), 9, t0 + SHAPE_REQUEST_GAP * 5),
        None
    );
}

#[test]
fn no_shape_id_asks_nothing() {
    let (mut tx, _rx) = cursor_channel();
    assert_eq!(tx.set_position(draw(0, 0, false), 0, Instant::now()), None);
}

#[test]
fn newest_shape_kept() {
    let (mut tx, mut rx) = cursor_channel();
    tx.push_shape(1, image());
    tx.push_shape(2, image());
    let change = rx.take().unwrap();
    assert_eq!(change.new_shape.map(|(id, _)| id), Some(2));
}

#[test]
fn local_pointer_overrides_host_position() {
    let (mut tx, mut rx) = cursor_channel();
    let local = rx.local_pointer();
    tx.set_position(draw(5, 6, true), 0, Instant::now());
    local.set(100, 200);
    let change = rx.take().unwrap();
    assert_eq!(change.draw, draw(100, 200, true));
}

#[test]
fn local_move_alone_is_a_change() {
    let (_tx, mut rx) = cursor_channel();
    let local = rx.local_pointer();
    assert!(rx.take().is_none());
    local.set(7, 8);
    assert_eq!(rx.take().unwrap().draw, draw(7, 8, false));
}

#[test]
fn cleared_local_uses_host() {
    let (mut tx, mut rx) = cursor_channel();
    let local = rx.local_pointer();
    local.set(100, 200);
    local.clear();
    tx.set_position(draw(5, 6, true), 0, Instant::now());
    assert_eq!(rx.take().unwrap().draw, draw(5, 6, true));
}

#[test]
fn visibility_hook_fires_on_change_only() {
    use std::sync::Arc;
    use std::sync::atomic::{AtomicUsize, Ordering};
    let (mut tx, _rx) = cursor_channel();
    let calls = Arc::new(AtomicUsize::new(0));
    let seen = Arc::clone(&calls);
    tx.on_visibility(Box::new(move |_| {
        seen.fetch_add(1, Ordering::Relaxed);
    }));
    let now = Instant::now();
    tx.set_position(draw(0, 0, true), 0, now);
    tx.set_position(draw(1, 0, true), 0, now);
    tx.set_position(draw(1, 0, false), 0, now);
    assert_eq!(calls.load(Ordering::Relaxed), 2);
}
