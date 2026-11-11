//! XDG autostart entry for R365.

use std::path::PathBuf;

pub fn path() -> PathBuf {
    let base = std::env::var_os("XDG_CONFIG_HOME")
        .map(PathBuf::from)
        .filter(|p| p.is_absolute())
        .unwrap_or_else(|| std::env::var_os("HOME").map(PathBuf::from).unwrap_or_else(|| PathBuf::from("/tmp")).join(".config"));
    base.join("autostart").join("r365.desktop")
}

/// Creates or removes the login entry. `minimized` makes the login instance start hidden.
pub fn apply(enabled: bool, minimized: bool) -> crate::Result<()> {
    let path = path();
    if !enabled {
        let _ = std::fs::remove_file(path);
        return Ok(());
    }
    let exe = std::env::current_exe().map_err(|e| format!("Could not find the r365 executable: {e}"))?;
    let exec = exec_line(&exe.to_string_lossy(), minimized);
    let content = format!(
        "[Desktop Entry]\nType=Application\nName=R365\nComment=Windows 365 and AVD client\nExec={exec}\nTerminal=false\nX-GNOME-Autostart-enabled=true\n"
    );
    if let Some(dir) = path.parent() {
        std::fs::create_dir_all(dir).map_err(|e| format!("Could not create {}: {e}", dir.display()))?;
    }
    crate::paths::write_private(&path, content.as_bytes())
}

fn exec_line(exe: &str, minimized: bool) -> String {
    // Desktop Entry Exec is not a shell command line: quote the binary and escape field codes.
    let escaped = exe.replace('\\', "\\\\").replace('"', "\\\"").replace('%', "%%");
    if minimized { format!("\"{escaped}\" --minimized") } else { format!("\"{escaped}\"") }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn exec_is_quoted_and_escaped() {
        assert_eq!(exec_line("/home/me/R365", false), "\"/home/me/R365\"");
        assert_eq!(exec_line("/home/me/R365", true), "\"/home/me/R365\" --minimized");
        assert_eq!(exec_line("/home/100%/r365", true), "\"/home/100%%/r365\" --minimized");
    }
}
