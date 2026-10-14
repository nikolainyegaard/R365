//! XDG base directories for r365. Directories are created on first use with owner-only access,
//! since the data dir holds downloaded .rdp profiles and session logs.

use std::os::unix::fs::PermissionsExt;
use std::path::PathBuf;

const APP: &str = "r365";

fn xdg(var: &str, fallback: &[&str]) -> PathBuf {
    let base = std::env::var_os(var).map(PathBuf::from).filter(|p| p.is_absolute()).unwrap_or_else(|| {
        let mut home = std::env::var_os("HOME").map(PathBuf::from).unwrap_or_else(|| PathBuf::from("/tmp"));
        home.extend(fallback);
        home
    });
    base.join(APP)
}

fn ensure(dir: PathBuf) -> PathBuf {
    if std::fs::create_dir_all(&dir).is_ok() {
        let _ = std::fs::set_permissions(&dir, std::fs::Permissions::from_mode(0o700));
    }
    dir
}

pub fn config_dir() -> PathBuf {
    ensure(xdg("XDG_CONFIG_HOME", &[".config"]))
}

pub fn data_dir() -> PathBuf {
    ensure(xdg("XDG_DATA_HOME", &[".local", "share"]))
}

pub fn feed_dir() -> PathBuf {
    ensure(data_dir().join("feed"))
}

pub fn logs_dir() -> PathBuf {
    ensure(data_dir().join("logs"))
}

pub fn config_file() -> PathBuf {
    config_dir().join("config.json")
}

pub fn workspaces_cache_file() -> PathBuf {
    data_dir().join("workspaces.json")
}

pub fn token_fallback_file() -> PathBuf {
    data_dir().join("tokens.json")
}

/// Writes owner-only, via a temp file and rename so a crash never leaves a half-written file.
pub fn write_private(path: &std::path::Path, bytes: &[u8]) -> crate::Result<()> {
    let tmp = path.with_extension(format!("tmp{}", std::process::id()));
    let io = |e: std::io::Error| format!("Could not write {}: {e}", path.display());
    std::fs::write(&tmp, bytes).map_err(io)?;
    std::fs::set_permissions(&tmp, std::fs::Permissions::from_mode(0o600)).map_err(io)?;
    std::fs::rename(&tmp, path).map_err(io)
}

/// Keeps the newest `keep` session logs.
pub fn prune_logs(keep: usize) {
    let Ok(entries) = std::fs::read_dir(logs_dir()) else { return };
    let mut logs: Vec<_> = entries.flatten().filter(|e| e.path().extension().is_some_and(|x| x == "log")).collect();
    logs.sort_by_key(|e| std::cmp::Reverse(e.metadata().and_then(|m| m.modified()).ok()));
    for old in logs.into_iter().skip(keep) {
        let _ = std::fs::remove_file(old.path());
    }
}
