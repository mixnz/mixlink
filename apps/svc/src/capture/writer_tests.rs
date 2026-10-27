use super::*;

fn temp_path(name: &str) -> std::path::PathBuf {
    std::env::temp_dir().join(format!("mixlink-writer-{}-{name}", std::process::id()))
}

#[test]
fn writes_aus_in_order() {
    let path = temp_path("order.h264");
    let (mut tx, handle) = Writer::spawn(&path).unwrap();
    // More AUs than buffers: a full queue refuses (counted), so wait for the writer and resend.
    for i in 0u8..20 {
        while !tx.push(&[i; 3]) {
            std::thread::sleep(std::time::Duration::from_millis(1));
        }
    }
    drop(tx);
    let written = handle.join().unwrap().unwrap();
    let data = std::fs::read(&path).unwrap();
    let _ = std::fs::remove_file(&path);
    let want: Vec<u8> = (0u8..20).flat_map(|i| [i; 3]).collect();
    assert_eq!(data, want);
    assert_eq!(written, 60);
}

#[test]
fn oversized_au_counted() {
    let path = temp_path("big.h264");
    let (mut tx, handle) = Writer::spawn(&path).unwrap();
    let big = vec![0u8; 5 * 1024 * 1024];
    assert!(!tx.push(&big));
    assert_eq!(tx.dropped(), 1);
    drop(tx);
    assert_eq!(handle.join().unwrap().unwrap(), 0);
    assert_eq!(std::fs::metadata(&path).unwrap().len(), 0);
    let _ = std::fs::remove_file(&path);
}
