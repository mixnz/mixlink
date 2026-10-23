//! Line log for the service (never written per frame in timing mode).

use std::fs::File;
use std::io::{BufWriter, Write};
use std::path::PathBuf;

use crate::qpc;
use crate::util::{BoxResult, now_stamp, out_dir};

pub struct RunLog {
    file: BufWriter<File>,
    pub path: PathBuf,
}

impl RunLog {
    pub fn create(case: &str, name: &str) -> BoxResult<Self> {
        let path = out_dir()?.join(format!("{}-{case}-{name}.log", now_stamp()));
        Ok(Self {
            file: BufWriter::new(File::create(&path)?),
            path,
        })
    }

    pub fn line(&mut self, msg: &str) {
        let _ = writeln!(self.file, "{} qpc={} {msg}", now_stamp(), qpc::now());
        let _ = self.file.flush();
    }
}
