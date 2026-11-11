//! Fetching the workspace feed and .rdp profiles; caching the last good feed on disk.

use crate::http::Http;
use r365_core::feed::{self, TenantFeed, Workspace};
use std::path::{Path, PathBuf};

pub fn discover(http: &Http, token: &str) -> crate::Result<Vec<TenantFeed>> {
    let resp = http.get(feed::DISCOVERY_URL, Some(token), Some(feed::ACCEPT_DISCOVERY), feed::MAX_XML_BYTES)?;
    match resp.status {
        200 => feed::parse_discovery(&resp.text()),
        401 | 403 => Err("Microsoft rejected the sign-in token for the workspace feed. Sign out and in again.".into()),
        s => Err(format!("Workspace feed discovery failed with HTTP {s}")),
    }
}

pub fn fetch_workspace(http: &Http, token: &str, tenant: &TenantFeed) -> crate::Result<Workspace> {
    let resp = http.get(&tenant.feed_url, Some(token), Some(feed::ACCEPT_FEED), feed::MAX_XML_BYTES)?;
    if resp.status != 200 {
        return Err(format!("Feed for {} failed with HTTP {}", tenant.display_name, resp.status));
    }
    feed::parse_feed(&resp.text(), tenant)
}

/// All workspaces, one tenant after another. A failed tenant does not discard the others.
/// ponytail: sequential; users have one to three tenants. Thread per tenant if that changes.
pub fn fetch_all(http: &Http, token: &str) -> crate::Result<Vec<Workspace>> {
    let tenants = discover(http, token)?;
    let mut out = Vec::new();
    let mut last_error = None;
    for t in &tenants {
        match fetch_workspace(http, token, t) {
            Ok(w) => out.push(w),
            Err(e) => last_error = Some(e),
        }
    }
    match (out.is_empty(), last_error) {
        (true, Some(e)) => Err(e),
        _ => Ok(out),
    }
}

/// Whether a feed error means the access token was rejected, so a refresh-then-retry is worth
/// trying, rather than a service or network failure.
pub fn is_auth_error(message: &str) -> bool {
    message.contains("HTTP 401") || message.contains("HTTP 403") || message.contains("rejected the sign-in token")
}

/// Downloads a resource's .rdp profile to the feed dir, owner-only, and returns its path.
pub fn download_rdp(http: &Http, token: &str, rdp_url: &str, file_stem: &str) -> crate::Result<PathBuf> {
    let resp = http.get(rdp_url, Some(token), None, feed::MAX_RDP_BYTES)?;
    if resp.status != 200 {
        return Err(format!("Downloading the remote desktop profile failed with HTTP {}", resp.status));
    }
    let path = crate::paths::feed_dir().join(format!("{file_stem}.rdp"));
    crate::paths::write_private(&path, &resp.body)?;
    Ok(path)
}

pub fn load_cache(account_id: &str) -> Vec<Workspace> {
    std::fs::read(crate::paths::workspaces_cache_file(account_id)).ok().and_then(|b| serde_json::from_slice(&b).ok()).unwrap_or_default()
}

pub fn save_cache(account_id: &str, workspaces: &[Workspace]) {
    if let Ok(json) = serde_json::to_vec(workspaces) {
        let _ = crate::paths::write_private(&crate::paths::workspaces_cache_file(account_id), &json);
    }
}

/// Removes only this account's feed cache. Downloaded profiles live under the shared feed
/// directory and are keyed by tenant and resource, so another account may still be using them.
pub fn clear_cache(account_id: &str) {
    let _ = std::fs::remove_file(crate::paths::workspaces_cache_file(account_id));
}

/// The pre-multi-account cache (`workspaces.json`); read once and migrated.
pub fn load_cache_legacy() -> Vec<Workspace> {
    std::fs::read(crate::paths::legacy_workspaces_cache_file()).ok().and_then(|b| serde_json::from_slice(&b).ok()).unwrap_or_default()
}

pub fn clear_cache_legacy() {
    let _ = std::fs::remove_file(crate::paths::legacy_workspaces_cache_file());
}

pub fn feed_dir() -> PathBuf {
    crate::paths::feed_dir()
}

pub fn is_under_feed_dir(p: &Path) -> bool {
    p.starts_with(crate::paths::feed_dir())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn auth_errors_are_recognised() {
        assert!(is_auth_error("Feed for Contoso failed with HTTP 401"));
        assert!(is_auth_error("Downloading the remote desktop profile failed with HTTP 403"));
        assert!(is_auth_error("Microsoft rejected the sign-in token for the workspace feed. Sign out and in again."));
        assert!(!is_auth_error("Feed for Contoso failed with HTTP 500"));
        assert!(!is_auth_error("network is unreachable"));
    }
}
