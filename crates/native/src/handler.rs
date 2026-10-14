//! Registers R365 as the desktop's handler for ms-appx-web links, which is where Entra sends
//! the sign-in result for the Remote Desktop client ID. Done on every start so a moved binary
//! keeps working; the desktop file is only rewritten when its content changes.

use std::path::PathBuf;
use std::process::{Command, Stdio};

const SCHEME: &str = "x-scheme-handler/ms-appx-web";
const DESKTOP: &str = "r365.desktop";

fn applications_dir() -> PathBuf {
    std::env::var_os("XDG_DATA_HOME")
        .map(PathBuf::from)
        .filter(|p| p.is_absolute())
        .unwrap_or_else(|| {
            let mut home = std::env::var_os("HOME").map(PathBuf::from).unwrap_or_else(|| PathBuf::from("/tmp"));
            home.extend([".local", "share"]);
            home
        })
        .join("applications")
}

/// Writes the desktop entry pointing at this executable and makes it the scheme's default.
pub fn register() -> crate::Result<()> {
    let exe = std::env::current_exe().map_err(|e| format!("Could not find the r365 executable: {e}"))?;
    let content = format!(
        "[Desktop Entry]\nType=Application\nName=R365\nComment=Windows 365 and Azure Virtual Desktop client\nExec={} %u\nTerminal=false\nCategories=Network;RemoteAccess;\nMimeType={SCHEME};\nStartupWMClass=r365\n",
        exe.to_string_lossy().replace(' ', "\\ ")
    );
    let dir = applications_dir();
    std::fs::create_dir_all(&dir).map_err(|e| format!("Could not create {}: {e}", dir.display()))?;
    let file = dir.join(DESKTOP);
    if std::fs::read_to_string(&file).ok().as_deref() != Some(content.as_str()) {
        std::fs::write(&file, content).map_err(|e| format!("Could not write {}: {e}", file.display()))?;
    }
    if is_default() {
        return Ok(());
    }
    let ok =
        Command::new("xdg-mime").args(["default", DESKTOP, SCHEME]).stdout(Stdio::null()).stderr(Stdio::null()).status().is_ok_and(|s| s.success());
    if ok && is_default() { Ok(()) } else { Err("xdg-mime could not make R365 the ms-appx-web handler".into()) }
}

fn is_default() -> bool {
    Command::new("xdg-mime")
        .args(["query", "default", SCHEME])
        .stderr(Stdio::null())
        .output()
        .is_ok_and(|o| String::from_utf8_lossy(&o.stdout).trim() == DESKTOP)
}
