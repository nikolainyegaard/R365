//! Refresh token storage. The user keyring through `secret-tool` (libsecret's CLI, present on
//! GNOME and KDE desktops), else an owner-only file in the data dir. Each signed-in account has
//! its own keyring entry and fallback file.
//! ponytail: shelling out avoids a D-Bus crate and an async runtime; swap for a Secret Service
//! crate if a desktop without secret-tool turns up in practice.

use r365_core::auth::TokenSet;
use std::io::Write;
use std::process::{Command, Stdio};

fn attrs(account_id: &str) -> [&str; 4] {
    ["service", "r365", "account", account_id]
}

fn have_secret_tool() -> bool {
    Command::new("secret-tool").arg("--version").stdout(Stdio::null()).stderr(Stdio::null()).status().is_ok_and(|s| s.success())
}

pub fn store(account_id: &str, tokens: &TokenSet) -> crate::Result<()> {
    let json = serde_json::to_vec(tokens).map_err(|e| format!("Could not serialise tokens: {e}"))?;
    if have_secret_tool() {
        let label = format!("R365 sign-in ({account_id})");
        let mut child = Command::new("secret-tool")
            .args(["store", "--label", &label])
            .args(attrs(account_id))
            .stdin(Stdio::piped())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn()
            .map_err(|e| format!("secret-tool could not start: {e}"))?;
        if let Some(mut stdin) = child.stdin.take() {
            let _ = stdin.write_all(&json);
        }
        if child.wait().is_ok_and(|s| s.success()) {
            let _ = std::fs::remove_file(crate::paths::token_fallback_file(account_id));
            return Ok(());
        }
    }
    crate::paths::write_private(&crate::paths::token_fallback_file(account_id), &json)
}

pub fn load(account_id: &str) -> Option<TokenSet> {
    if have_secret_tool() {
        let out = Command::new("secret-tool").arg("lookup").args(attrs(account_id)).stderr(Stdio::null()).output().ok()?;
        if out.status.success() && !out.stdout.is_empty() {
            return serde_json::from_slice(&out.stdout).ok();
        }
    }
    let bytes = std::fs::read(crate::paths::token_fallback_file(account_id)).ok()?;
    serde_json::from_slice(&bytes).ok()
}

pub fn clear(account_id: &str) {
    if have_secret_tool() {
        let _ = Command::new("secret-tool").arg("clear").args(attrs(account_id)).stdout(Stdio::null()).stderr(Stdio::null()).status();
    }
    let _ = std::fs::remove_file(crate::paths::token_fallback_file(account_id));
}

/// The pre-multi-account entry (`account=default` and `tokens.json`).
pub fn load_legacy() -> Option<TokenSet> {
    if have_secret_tool() {
        let out = Command::new("secret-tool").arg("lookup").args(["service", "r365", "account", "default"]).stderr(Stdio::null()).output().ok()?;
        if out.status.success() && !out.stdout.is_empty() {
            return serde_json::from_slice(&out.stdout).ok();
        }
    }
    let bytes = std::fs::read(crate::paths::legacy_token_fallback_file()).ok()?;
    serde_json::from_slice(&bytes).ok()
}

pub fn clear_legacy() {
    if have_secret_tool() {
        let _ = Command::new("secret-tool")
            .arg("clear")
            .args(["service", "r365", "account", "default"])
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .status();
    }
    let _ = std::fs::remove_file(crate::paths::legacy_token_fallback_file());
}
