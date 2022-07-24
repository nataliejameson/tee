use std::fmt::{Debug, Formatter};
use std::fs::File;
use std::io::{Read, Write};
use std::process::Stdio;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use tempfile::NamedTempFile;

struct TeeData {
    temp_file: NamedTempFile,
    stop_signal: Arc<AtomicBool>,
    thread: Option<std::thread::JoinHandle<anyhow::Result<()>>>,
}

impl Debug for TeeData {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        f.write_fmt(format_args!(
            "TeeData(temp_file: \"{}\", stopped: {}",
            self.temp_file.path().to_string_lossy(),
            self.stop_signal.load(Ordering::Relaxed)
        ))
    }
}

impl TeeData {
    fn new() -> anyhow::Result<Self> {
        let temp_file = NamedTempFile::new()?;
        let path = temp_file.path().to_path_buf();
        let stop_signal = Arc::new(AtomicBool::new(false));
        let thread_stop_signal = stop_signal.clone();
        let thread = std::thread::spawn(move || -> anyhow::Result<()> {
            let mut file = std::fs::File::open(path)?;
            let mut buf = [0x00; 1024];
            while !thread_stop_signal.load(Ordering::Relaxed) {
                let read_bytes = file.read(&mut buf)?;
                std::io::stderr().write_all(&buf[0..read_bytes])?;
            }

            Ok(())
        });
        Ok(Self {
            temp_file,
            stop_signal,
            thread: Some(thread),
        })
    }
}

impl Drop for TeeData {
    fn drop(&mut self) {
        self.stop_signal.store(true, Ordering::Relaxed);
        if let Some(thread) = self.thread.take() {
            let _ignore = thread.join();
        }
    }
}

#[derive(Clone, Debug)]
pub struct Tee(Arc<TeeData>);

impl Tee {
    pub fn new() -> anyhow::Result<Self> {
        Ok(Self(Arc::new(TeeData::new()?)))
    }

    pub fn get_output(&self) -> anyhow::Result<Vec<u8>> {
        Ok(std::fs::read(self.0.temp_file.path())?)
    }
}

impl From<Tee> for Stdio {
    fn from(t: Tee) -> Self {
        let file: File = (*t.0.temp_file.as_file()).try_clone().unwrap();
        Stdio::from(file)
    }
}
