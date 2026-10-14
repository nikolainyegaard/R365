use std::process::{Command, Stdio};

/// Opens a URL in the user's default browser. Only https, so a hostile feed value can never
/// launch a local handler.
pub fn open(url: &str) -> crate::Result<()> {
    if !url.starts_with("https://") {
        return Err("Only https links are opened".into());
    }
    Command::new("xdg-open")
        .arg(url)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .map(|_| ())
        .map_err(|e| format!("Could not open the browser: {e}"))
}
