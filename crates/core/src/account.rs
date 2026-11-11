//! Local identity for a signed-in Microsoft account.
//!
//! One Entra ID sign-in can see several tenants, so the tenant is not the account. The stable
//! key here is the user principal name plus the tenant that issued the id_token. It is also used
//! as the keyring attribute and as a file-name component, so it is sanitised and lowercased.

use crate::auth::Claims;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AccountInfo {
    pub id: String,
    pub upn: String,
    #[serde(default)]
    pub display_name: String,
}

impl AccountInfo {
    pub fn from_claims(claims: &Claims) -> Self {
        Self { id: account_id(claims), upn: account_name(claims), display_name: claims.name.clone().unwrap_or_default() }
    }

    /// Header and account-menu label. The UPN is what tells two company accounts apart.
    pub fn label(&self) -> &str {
        if !self.upn.trim().is_empty() {
            &self.upn
        } else if !self.display_name.trim().is_empty() {
            &self.display_name
        } else {
            "Signed in"
        }
    }
}

/// `upn@tenant`, sanitised for the keyring and file names. Falls back to the display name and,
/// only for a token with no identity at all, to `default`.
pub fn account_id(claims: &Claims) -> String {
    let name = account_name(claims);
    let mut basis = name.to_ascii_lowercase();
    if let Some(tid) = claims.tid.as_deref().map(str::trim).filter(|t| !t.is_empty()) {
        basis.push('@');
        basis.push_str(&tid.to_ascii_lowercase());
    }
    let mut id: String =
        basis.chars().map(|c| if c.is_ascii_alphanumeric() || matches!(c, '@' | '.' | '_' | '-') { c } else { '_' }).take(120).collect();
    if id.trim_matches(['@', '.', '_', '-']).is_empty() {
        id = "default".into();
    }
    id
}

fn account_name(claims: &Claims) -> String {
    [&claims.upn, &claims.preferred_username, &claims.email, &claims.name]
        .into_iter()
        .filter_map(|v| v.as_deref())
        .map(str::trim)
        .find(|v| !v.is_empty())
        .unwrap_or("")
        .to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn claims(upn: Option<&str>, tid: Option<&str>) -> Claims {
        Claims { upn: upn.map(str::to_string), tid: tid.map(str::to_string), ..Claims::default() }
    }

    #[test]
    fn id_is_stable_and_file_safe() {
        let c = claims(Some("Ana.Perez@Contoso.com"), Some("1111-2222"));
        assert_eq!(account_id(&c), "ana.perez@contoso.com@1111-2222");
        assert!(account_id(&c).chars().all(|ch| ch.is_ascii_alphanumeric() || matches!(ch, '@' | '.' | '_' | '-')));
        assert_eq!(account_id(&c), account_id(&c));
    }

    #[test]
    fn id_falls_back_through_claims() {
        let mut c = claims(None, None);
        c.preferred_username = Some("alias@fabrikam.com".into());
        assert!(account_id(&c).starts_with("alias@fabrikam.com"));
        c.preferred_username = None;
        c.email = Some("mail@example.com".into());
        assert!(account_id(&c).starts_with("mail@example.com"));
        c.email = None;
        c.name = Some("A Person".into());
        assert_eq!(account_id(&c), "a_person");
        assert_eq!(account_id(&Claims::default()), "default");
    }

    #[test]
    fn info_label_prefers_upn() {
        let info = AccountInfo::from_claims(&claims(Some("ana@contoso.com"), Some("t1")));
        assert_eq!(info.label(), "ana@contoso.com");
        assert_eq!(info.id, "ana@contoso.com@t1");
        let mut empty = AccountInfo { id: "x".into(), upn: String::new(), display_name: "Ana".into() };
        assert_eq!(empty.label(), "Ana");
        empty.display_name.clear();
        assert_eq!(empty.label(), "Signed in");
    }
}
