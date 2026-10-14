//! Opening https links in the user's browser.
//!
//! On Wayland a process without an activation token cannot raise an existing window, so a new
//! tab in a background browser stays hidden (KWin's focus stealing prevention). A brand new
//! window is allowed to take focus, so the default browser's own "new-window" desktop action is
//! used when it has one; xdg-open is the fallback.

use std::path::PathBuf;
use std::process::{Command, Stdio};

pub fn open(url: &str) -> crate::Result<()> {
    if !url.starts_with("https://") {
        return Err("Only https links are opened".into());
    }
    let mut cmd = new_window_command(url).unwrap_or_else(|| {
        let mut c = Command::new("xdg-open");
        c.arg(url);
        c
    });
    cmd.stdin(Stdio::null()).stdout(Stdio::null()).stderr(Stdio::null()).spawn().map(|_| ()).map_err(|e| format!("Could not open the browser: {e}"))
}

/// The default browser's `[Desktop Action new-window]` Exec line, with the URL substituted.
fn new_window_command(url: &str) -> Option<Command> {
    let out = Command::new("xdg-settings").args(["get", "default-web-browser"]).stderr(Stdio::null()).output().ok()?;
    let id = String::from_utf8_lossy(&out.stdout).trim().to_string();
    if id.is_empty() || id.contains('/') {
        return None;
    }
    let contents = application_dirs().into_iter().find_map(|d| std::fs::read_to_string(d.join(&id)).ok())?;
    let exec = contents
        .lines()
        .skip_while(|l| l.trim() != "[Desktop Action new-window]")
        .skip(1)
        .take_while(|l| !l.starts_with('['))
        .find_map(|l| l.strip_prefix("Exec="))?;
    let mut words = exec.split_whitespace().map(|w| w.trim_matches('"'));
    let mut cmd = Command::new(words.next()?);
    let mut placed = false;
    for w in words {
        match w {
            "%u" | "%U" | "%f" | "%F" => {
                cmd.arg(url);
                placed = true;
            }
            w if w.starts_with('%') => {}
            w => {
                cmd.arg(w);
            }
        }
    }
    if !placed {
        cmd.arg(url);
    }
    Some(cmd)
}

fn application_dirs() -> Vec<PathBuf> {
    let mut dirs = Vec::new();
    let home = std::env::var_os("HOME").map(PathBuf::from);
    let data_home = std::env::var_os("XDG_DATA_HOME").map(PathBuf::from).or_else(|| home.map(|h| h.join(".local/share")));
    if let Some(d) = data_home {
        dirs.push(d.join("applications"));
    }
    let data_dirs = std::env::var("XDG_DATA_DIRS").unwrap_or_else(|_| "/usr/local/share:/usr/share".into());
    dirs.extend(data_dirs.split(':').filter(|s| !s.is_empty()).map(|s| PathBuf::from(s).join("applications")));
    dirs.push(PathBuf::from("/var/lib/flatpak/exports/share/applications"));
    dirs
}
