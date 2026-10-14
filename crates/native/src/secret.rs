//! Refresh token storage. The user keyring through `secret-tool` (libsecret's CLI, present on
//! GNOME and KDE desktops), else an owner-only file in the data dir.
//! ponytail: shelling out avoids a D-Bus crate and an async runtime; swap for a Secret Service
//! crate if a desktop without secret-tool turns up in practice.

use r365_core::auth::TokenSet;
use std::io::Write;
use std::process::{Command, Stdio};

const ATTRS: [&str; 4] = ["service", "r365", "account", "default"];

fn have_secret_tool() -> bool {
    Command::new("secret-tool").arg("--version").stdout(Stdio::null()).stderr(Stdio::null()).status().is_ok_and(|s| s.success())
}

pub fn store(tokens: &TokenSet) -> crate::Result<()> {
    let json = serde_json::to_vec(tokens).map_err(|e| format!("Could not serialise tokens: {e}"))?;
    if have_secret_tool() {
        let mut child = Command::new("secret-tool")
            .args(["store", "--label", "R365 sign-in"])
            .args(ATTRS)
            .stdin(Stdio::piped())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn()
            .map_err(|e| format!("secret-tool could not start: {e}"))?;
        if let Some(mut stdin) = child.stdin.take() {
            let _ = stdin.write_all(&json);
        }
        if child.wait().is_ok_and(|s| s.success()) {
            let _ = std::fs::remove_file(crate::paths::token_fallback_file());
            return Ok(());
        }
    }
    crate::paths::write_private(&crate::paths::token_fallback_file(), &json)
}

pub fn load() -> Option<TokenSet> {
    if have_secret_tool() {
        let out = Command::new("secret-tool").arg("lookup").args(ATTRS).stderr(Stdio::null()).output().ok()?;
        if out.status.success() && !out.stdout.is_empty() {
            return serde_json::from_slice(&out.stdout).ok();
        }
    }
    let bytes = std::fs::read(crate::paths::token_fallback_file()).ok()?;
    serde_json::from_slice(&bytes).ok()
}

pub fn clear() {
    if have_secret_tool() {
        let _ = Command::new("secret-tool").arg("clear").args(ATTRS).stdout(Stdio::null()).stderr(Stdio::null()).status();
    }
    let _ = std::fs::remove_file(crate::paths::token_fallback_file());
}
