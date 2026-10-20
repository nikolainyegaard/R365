//! Opening https links in the user's browser.
//!
//! On Wayland a process without an activation token cannot raise an existing window, so a new
//! tab in a background browser stays hidden (KWin's focus stealing prevention). With a token
//! from our own window (`XDG_ACTIVATION_TOKEN`, or `DESKTOP_STARTUP_ID` on X11) the browser is
//! allowed to come forward. Without one, a brand new window is the next best thing, since the
//! compositor lets new windows take focus; the default browser's own "new-window" desktop action
//! provides that. xdg-open is the last resort.

use std::path::PathBuf;
use std::process::{Command, Stdio};

pub fn open(url: &str, activation_token: Option<&str>) -> crate::Result<()> {
    if !url.starts_with("https://") {
        return Err("Only https links are opened".into());
    }
    let mut cmd = match activation_token {
        Some(_) => desktop_command(url, None),
        None => desktop_command(url, Some("new-window")),
    }
    .unwrap_or_else(|| {
        let mut c = Command::new("xdg-open");
        c.arg(url);
        c
    });
    if let Some(token) = activation_token {
        let var = if std::env::var_os("WAYLAND_DISPLAY").is_some() { "XDG_ACTIVATION_TOKEN" } else { "DESKTOP_STARTUP_ID" };
        cmd.env(var, token);
    }
    cmd.stdin(Stdio::null()).stdout(Stdio::null()).stderr(Stdio::null()).spawn().map(|_| ()).map_err(|e| format!("Could not open the browser: {e}"))
}

/// The default browser's Exec line, from `[Desktop Entry]` or from `[Desktop Action <name>]`,
/// with the URL substituted.
fn desktop_command(url: &str, action: Option<&str>) -> Option<Command> {
    let out = Command::new("xdg-settings").args(["get", "default-web-browser"]).stderr(Stdio::null()).output().ok()?;
    let id = String::from_utf8_lossy(&out.stdout).trim().to_string();
    if id.is_empty() || id.contains('/') {
        return None;
    }
    let contents = application_dirs().into_iter().find_map(|d| std::fs::read_to_string(d.join(&id)).ok())?;
    let section = action.map_or("[Desktop Entry]".to_string(), |a| format!("[Desktop Action {a}]"));
    let exec =
        contents.lines().skip_while(|l| l.trim() != section).skip(1).take_while(|l| !l.starts_with('[')).find_map(|l| l.strip_prefix("Exec="))?;
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
