//! Token flows over plain HTTPS. No browser engine anywhere: the first sign-in is the device
//! code grant in the user's own browser, everything after is the refresh token.

use crate::http::Http;
use r365_core::auth::{self, DeviceCode, Poll, TokenSet};

pub fn start_device_code(http: &Http) -> crate::Result<DeviceCode> {
    let resp = http.post_form(&auth::device_code_url(), &auth::device_code_form())?;
    if resp.status != 200 {
        return Err(format!("Microsoft refused to start sign-in (HTTP {})", resp.status));
    }
    serde_json::from_slice(&resp.body).map_err(|e| format!("Device code response was not understood: {e}"))
}

pub fn poll_device_code(http: &Http, code: &DeviceCode) -> Poll {
    match http.post_form(&auth::token_url(), &auth::poll_form(&code.device_code)) {
        Ok(resp) => auth::parse_poll(resp.status, &resp.text()),
        Err(e) => Poll::Failed(e),
    }
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
