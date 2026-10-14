//! Token flows over plain HTTPS. No browser engine anywhere: sign-in is the authorization code
//! grant with PKCE in the user's own browser, everything after is the refresh token.

use crate::http::Http;
use r365_core::auth::{self, Pkce, TokenSet};

/// 48 bytes from the kernel for a new PKCE verifier and state.
pub fn new_pkce() -> crate::Result<Pkce> {
    let mut buf = [0u8; 48];
    std::fs::File::open("/dev/urandom")
        .and_then(|mut f| std::io::Read::read_exact(&mut f, &mut buf))
        .map_err(|e| format!("Could not read randomness for sign-in: {e}"))?;
    Pkce::new(&buf)
}

pub fn exchange_code(http: &Http, code: &str, pkce: &Pkce, redirect_uri: &str) -> crate::Result<TokenSet> {
    let resp = http.post_form(&auth::token_url(), &auth::code_form(code, &pkce.verifier, redirect_uri))?;
    let tokens = auth::parse_token_response(resp.status, &resp.text())?;
    TokenSet::from_response(tokens, None, crate::now_unix())
}

pub fn refresh(http: &Http, previous: &TokenSet) -> crate::Result<TokenSet> {
    let resp = http.post_form(&auth::token_url(), &auth::refresh_form(&previous.refresh_token))?;
    let tokens = auth::parse_token_response(resp.status, &resp.text())?;
    TokenSet::from_response(tokens, Some(previous), crate::now_unix())
}

/// Returns a token set that is good for at least five minutes, refreshing and persisting if
/// needed. `changed` tells the caller whether to update its copy.
pub fn ensure_fresh(http: &Http, current: &TokenSet) -> crate::Result<(TokenSet, bool)> {
    if !current.is_expiring_soon(crate::now_unix()) {
        return Ok((current.clone(), false));
    }
    let fresh = refresh(http, current)?;
    crate::secret::store(&fresh)?;
    Ok((fresh, true))
}
