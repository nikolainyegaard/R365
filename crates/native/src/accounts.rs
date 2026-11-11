//! Saved account metadata in `accounts.json`. Only identity is stored here; tokens live in the
//! keyring (or owner-only per-account fallback files).

use r365_core::account::AccountInfo;

pub fn load() -> Vec<AccountInfo> {
    let mut accounts: Vec<AccountInfo> =
        std::fs::read(crate::paths::accounts_file()).ok().and_then(|b| serde_json::from_slice(&b).ok()).unwrap_or_default();
    let mut seen = std::collections::HashSet::new();
    accounts.retain(|a| !a.id.trim().is_empty() && seen.insert(a.id.clone()));
    accounts
}

pub fn save(accounts: &[AccountInfo]) -> crate::Result<()> {
    let json = serde_json::to_vec_pretty(accounts).map_err(|e| format!("Could not serialise accounts: {e}"))?;
    crate::paths::write_private(&crate::paths::accounts_file(), &json)
}

/// Adds or updates one account and returns its index.
pub fn upsert(accounts: &mut Vec<AccountInfo>, info: AccountInfo) -> usize {
    if let Some(index) = accounts.iter().position(|a| a.id == info.id) {
        if let Some(account) = accounts.get_mut(index) {
            *account = info;
        }
        index
    } else {
        accounts.push(info);
        accounts.len().saturating_sub(1)
    }
}

pub fn remove(accounts: &mut Vec<AccountInfo>, id: &str) -> Option<AccountInfo> {
    let index = accounts.iter().position(|a| a.id == id)?;
    Some(accounts.remove(index))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn info(id: &str) -> AccountInfo {
        AccountInfo { id: id.into(), upn: format!("{id}@example.com"), display_name: String::new() }
    }

    #[test]
    fn upsert_keeps_one_entry_per_id() {
        let mut list = vec![info("a")];
        assert_eq!(upsert(&mut list, info("b")), 1);
        assert_eq!(upsert(&mut list, info("a")), 0);
        assert_eq!(list.len(), 2);
        assert_eq!(list[0].id, "a");
    }

    #[test]
    fn remove_reports_the_removed_account() {
        let mut list = vec![info("a"), info("b")];
        assert_eq!(remove(&mut list, "a").map(|a| a.id), Some("a".into()));
        assert!(remove(&mut list, "missing").is_none());
        assert_eq!(list.len(), 1);
    }
}
