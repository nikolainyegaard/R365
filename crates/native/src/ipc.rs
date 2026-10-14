//! One running instance. A second `r365 <url>` process, started by the browser for an
//! ms-appx-web link, hands the URL to the first over a Unix socket and exits.

use std::io::{BufRead, BufReader, Write};
use std::os::unix::net::{UnixListener, UnixStream};
use std::path::PathBuf;
use std::sync::mpsc::Sender;
use std::time::Duration;

fn socket_path() -> PathBuf {
    std::env::var_os("XDG_RUNTIME_DIR").map(PathBuf::from).filter(|p| p.is_dir()).unwrap_or_else(std::env::temp_dir).join("r365.sock")
}

/// Delivers `url` to a running instance. False when none is listening.
pub fn send_to_running(url: &str) -> bool {
    let Ok(mut stream) = UnixStream::connect(socket_path()) else { return false };
    let _ = stream.set_write_timeout(Some(Duration::from_secs(2)));
    stream.write_all(format!("{url}\n").as_bytes()).is_ok()
}

/// Binds the socket and forwards every line received to `tx`. Returns false if another
/// instance already holds it.
pub fn listen(tx: Sender<String>) -> bool {
    let path = socket_path();
    if UnixStream::connect(&path).is_ok() {
        return false;
    }
    let _ = std::fs::remove_file(&path);
    let Ok(listener) = UnixListener::bind(&path) else { return false };
    std::thread::Builder::new()
        .name("ipc".into())
        .spawn(move || {
            for stream in listener.incoming().flatten() {
                let _ = stream.set_read_timeout(Some(Duration::from_secs(2)));
                let mut line = String::new();
                if BufReader::new(stream).read_line(&mut line).is_ok() {
                    let line = line.trim();
                    if !line.is_empty() && line.len() < 16 * 1024 && tx.send(line.to_string()).is_err() {
                        return;
                    }
                }
            }
        })
        .is_ok()
}
