//! Writes access units to a file on its own thread, so the frame loop never blocks on disk.
//!
//! Buffers are allocated once and move between two `rtrb` rings: **filled** (frame loop → writer)
//! and **free** (writer → frame loop). `push` copies the AU into a free buffer; nothing allocates.

use std::fs::File;
use std::io::{self, BufWriter, Write};
use std::path::Path;
use std::thread::{self, JoinHandle, Thread};
use std::time::Duration;

use rtrb::{Consumer, Producer, RingBuffer};

/// Largest access unit kept (as the encoder's buffer).
pub const AU_CAPACITY: usize = 4 * 1024 * 1024;
/// Buffers in flight.
pub const SLOTS: usize = 8;
const IDLE_WAIT: Duration = Duration::from_millis(5);

#[derive(Debug)]
struct Buf {
    data: Box<[u8]>,
    len: usize,
}

/// Frame-loop side. Dropping it lets the writer finish what is queued and exit.
#[derive(Debug)]
pub struct WriterTx {
    filled: Producer<Buf>,
    free: Consumer<Buf>,
    writer: Thread,
    dropped: u64,
}

impl WriterTx {
    // HOT PATH
    /// Queues a copy of `au`; `false` (counted) when it is too large or every buffer is in use.
    pub fn push(&mut self, au: &[u8]) -> bool {
        if au.len() > AU_CAPACITY {
            self.dropped += 1;
            return false;
        }
        let Ok(mut buf) = self.free.pop() else {
            self.dropped += 1;
            return false;
        };
        buf.data[..au.len()].copy_from_slice(au);
        buf.len = au.len();
        // Cannot be full: there are only SLOTS buffers and this one came from `free`.
        let _ = self.filled.push(buf);
        self.writer.unpark();
        true
    }

    pub fn dropped(&self) -> u64 {
        self.dropped
    }
}

#[derive(Debug)]
pub struct Writer;

impl Writer {
    /// Creates `path` (truncates) and starts thread `cap-writer`. The handle returns the bytes written.
    pub fn spawn(path: &Path) -> io::Result<(WriterTx, JoinHandle<io::Result<u64>>)> {
        let file = BufWriter::new(File::create(path)?);
        let (filled_tx, filled_rx) = RingBuffer::new(SLOTS);
        let (mut free_tx, free_rx) = RingBuffer::new(SLOTS);
        for _ in 0..SLOTS {
            let _ = free_tx.push(Buf {
                data: vec![0; AU_CAPACITY].into_boxed_slice(),
                len: 0,
            });
        }
        let handle = thread::Builder::new()
            .name("cap-writer".into())
            .spawn(move || write_loop(file, filled_rx, free_tx))?;
        let tx = WriterTx {
            filled: filled_tx,
            free: free_rx,
            writer: handle.thread().clone(),
            dropped: 0,
        };
        Ok((tx, handle))
    }
}

fn write_loop(
    mut file: BufWriter<File>,
    mut filled: Consumer<Buf>,
    mut free: Producer<Buf>,
) -> io::Result<u64> {
    let mut written = 0u64;
    loop {
        match filled.pop() {
            Ok(buf) => {
                file.write_all(&buf.data[..buf.len])?;
                written += buf.len as u64;
                let _ = free.push(buf);
            }
            Err(_) if filled.is_abandoned() => {
                // The sender is gone; anything it pushed before is already drained above.
                if filled.is_empty() {
                    break;
                }
            }
            Err(_) => thread::park_timeout(IDLE_WAIT),
        }
    }
    file.flush()?;
    Ok(written)
}

#[cfg(test)]
#[path = "writer_tests.rs"]
mod tests;
