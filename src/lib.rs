use std::fmt::Debug;
use std::fmt::Formatter;
use std::fs::File;
use std::io::Seek;
use std::io::Write;
// This only works on unix for right now because we use read_at()
#[cfg(unix)]
use std::os::unix::fs::FileExt;
use std::process::Stdio;
use std::sync::atomic::AtomicBool;
use std::sync::atomic::Ordering;
use std::sync::Arc;
use std::time::Duration;

struct TeeData {
    temp_file: File,
    stop_signal: Arc<AtomicBool>,
    thread: Option<std::thread::JoinHandle<anyhow::Result<()>>>,
}

impl Debug for TeeData {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        f.write_fmt(format_args!(
            "TeeData(stopped: {})",
            self.stop_signal.load(Ordering::Relaxed)
        ))
    }
}

impl TeeData {
    fn new<T: Write + Sync + Send + 'static>(mut stdio: T) -> anyhow::Result<Self> {
        let temp_file = tempfile::tempfile()?;
        let file = temp_file.try_clone()?;
        let stop_signal = Arc::new(AtomicBool::new(false));
        let thread_stop_signal = stop_signal.clone();
        let thread = std::thread::spawn(move || -> anyhow::Result<()> {
            let mut buf = [0x00; 1024];
            let mut offset: u64 = 0;
            while !thread_stop_signal.load(Ordering::Relaxed) {
                let read_bytes = file.read_at(&mut buf, offset)?;
                offset += read_bytes as u64;
                stdio.write_all(&buf[0..read_bytes])?;
                if read_bytes == 0 {
                    // No need to busy loop on this if nothing's going on, a small pause is fine.
                    std::thread::sleep(Duration::from_millis(5));
                }
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

/// Both captures output from a stdio object and relays it to that original object
///
/// Tee is mostly used for things like stderr, where one might want to show the user the output
/// of a command on their terminal, but the program also wants to parse some data from that stderr
/// stream.
///
/// Tee does this by opening an unnamed temporary file and passing that handle to `Stdio`, which
/// can be used by [`std::process::Process`]. Everything that would be written to the stdio object
/// is thus written to this temp file.
///
/// It also spawns a thread in the background that watches this temporary file, and if any new data
/// was written to it, it relays that to the original stdio object.
///
/// When all instances of the [`Tee`] object are dropped, the background thread is stopped, and no
/// more data is written to the original stdio object. If all instances of [`Tee`] *and* all
/// [`Stdio`] that were created with [`Tee::stdio`] are dropped, then the OS will clean up the
/// unnamed temporary file, as all handles to it should have been closed.
#[derive(Clone, Debug)]
pub struct Tee(Arc<TeeData>);

impl Tee {
    pub fn new<T: Write + Sync + Send + 'static>(stdio: T) -> anyhow::Result<Self> {
        Ok(Self(Arc::new(TeeData::new(stdio)?)))
    }

    /// Get the output from the open temporary file
    pub fn get_output(&self) -> anyhow::Result<Vec<u8>> {
        let mut file = self.0.temp_file.try_clone()?;
        let len = file.stream_position()?;
        let mut buf = vec![0x00; len as usize];
        let mut offset = 0;
        while offset < len {
            let read_bytes = self.0.temp_file.read_at(buf.as_mut_slice(), offset)? as u64;
            offset += read_bytes;
        }
        Ok(buf)
    }

    /// Get a [`std::process::Stdio`] instance from this Tee instance.
    ///
    /// Note that this duplicates the underlying temp file handle, so as long as the Tee object
    /// and the Stdio object stick around, that unnamed temporary file will exist.
    pub fn stdio(&self) -> Stdio {
        let file: File = self.0.temp_file.try_clone().unwrap();
        Stdio::from(file)
    }
}

#[cfg(test)]
mod test {
    use crate::Tee;

    #[test]
    fn tee_tees() {
        let tee_err = Tee::new(std::io::stderr()).unwrap();
        let res = std::process::Command::new("sh")
            .args(["-c", "echo \"test failure string\" >&2; exit 1;"])
            .stderr(tee_err.stdio())
            .output()
            .unwrap();
        assert_eq!(0, res.stderr.len());
        let stderr_out = String::from_utf8(tee_err.get_output().unwrap()).unwrap();
        assert_eq!("test failure string\n", stderr_out);
    }
}
