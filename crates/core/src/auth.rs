//! Entra ID OAuth 2.0 for the Microsoft Remote Desktop public client: device code sign-in,
//! refresh token grant, and id_token claims. See docs/auth.md for why this shape.

use serde::{Deserialize, Serialize};

/// Microsoft's own Remote Desktop / AVD client. FreeRDP hardcodes the same ID, which is what
/// lets its session auth share the user's sign-in.
pub const CLIENT_ID: &str = "a85cf173-4192-42f8-81fa-777a763e6e2c";
pub const SCOPE: &str = "https://www.wvd.microsoft.com/.default offline_access openid profile";
const AUTHORITY: &str = "https://login.microsoftonline.com/organizations/oauth2/v2.0";

pub fn device_code_url() -> String {
    format!("{AUTHORITY}/devicecode")
}

pub fn token_url() -> String {
    format!("{AUTHORITY}/token")
}

pub fn device_code_form() -> Vec<(String, String)> {
    vec![("client_id".into(), CLIENT_ID.into()), ("scope".into(), SCOPE.into())]
}

pub fn poll_form(device_code: &str) -> Vec<(String, String)> {
    vec![
        ("client_id".into(), CLIENT_ID.into()),
        ("grant_type".into(), "urn:ietf:params:oauth:grant-type:device_code".into()),
        ("device_code".into(), device_code.into()),
    ]
}

pub fn refresh_form(refresh_token: &str) -> Vec<(String, String)> {
    vec![
        ("client_id".into(), CLIENT_ID.into()),
        ("grant_type".into(), "refresh_token".into()),
        ("refresh_token".into(), refresh_token.into()),
        ("scope".into(), SCOPE.into()),
    ]
}

#[derive(Debug, Clone, Deserialize)]
pub struct DeviceCode {
    pub user_code: String,
    pub device_code: String,
    pub verification_uri: String,
    pub expires_in: u64,
    #[serde(default = "default_interval")]
    pub interval: u64,
}

fn default_interval() -> u64 {
    5
}

#[derive(Debug, Clone, Deserialize)]
pub struct TokenResponse {
    pub access_token: String,
    #[serde(default)]
    pub refresh_token: Option<String>,
    #[serde(default)]
    pub id_token: Option<String>,
    #[serde(default)]
    pub expires_in: u64,
}

#[derive(Debug, Clone, Deserialize)]
struct ErrorBody {
    #[serde(default)]
    error: String,
    #[serde(default)]
    error_description: String,
}

/// What one poll of the token endpoint during device code sign-in means.
#[derive(Debug)]
pub enum Poll {
    Pending,
    SlowDown,
    Done(TokenResponse),
    Failed(String),
}

pub fn parse_poll(status: u16, body: &str) -> Poll {
    if status == 200 {
        return match serde_json::from_str::<TokenResponse>(body) {
            Ok(t) => Poll::Done(t),
            Err(e) => Poll::Failed(format!("Token response was not understood: {e}")),
        };
    }
    let err = serde_json::from_str::<ErrorBody>(body).unwrap_or(ErrorBody { error: String::new(), error_description: String::new() });
    match err.error.as_str() {
        "authorization_pending" => Poll::Pending,
        "slow_down" => Poll::SlowDown,
        "expired_token" => Poll::Failed("The sign-in code expired. Start again.".into()),
        "authorization_declined" => Poll::Failed("Sign-in was declined in the browser.".into()),
        _ => Poll::Failed(describe_error(status, &err)),
    }
}

pub fn parse_token_response(status: u16, body: &str) -> crate::Result<TokenResponse> {
    if status == 200 {
        return serde_json::from_str(body).map_err(|e| format!("Token response was not understood: {e}"));
    }
    let err = serde_json::from_str::<ErrorBody>(body).unwrap_or(ErrorBody { error: String::new(), error_description: String::new() });
    Err(describe_error(status, &err))
}

fn describe_error(status: u16, err: &ErrorBody) -> String {
    let first_line = err.error_description.lines().next().unwrap_or("").trim();
    match (err.error.is_empty(), first_line.is_empty()) {
        (true, _) => format!("Microsoft sign-in returned HTTP {status}"),
        (false, true) => format!("Microsoft sign-in failed: {}", err.error),
        (false, false) => format!("Microsoft sign-in failed: {first_line}"),
    }
}

/// Identity claims from the id_token; only what the UI and FreeRDP need.
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq, Eq)]
pub struct Claims {
    #[serde(default)]
    pub upn: Option<String>,
    #[serde(default)]
    pub preferred_username: Option<String>,
    #[serde(default)]
    pub name: Option<String>,
    #[serde(default)]
    pub email: Option<String>,
    #[serde(default)]
    pub tid: Option<String>,
}

impl Claims {
    /// What FreeRDP gets as `/u:`.
    pub fn rdp_username(&self) -> String {
        first_present(&[&self.upn, &self.preferred_username, &self.email])
    }

    /// What the header pill shows.
    pub fn display_identity(&self) -> String {
        let id = first_present(&[&self.upn, &self.preferred_username, &self.email, &self.name]);
        if id.is_empty() { "Signed in".into() } else { id }
    }
}

fn first_present(candidates: &[&Option<String>]) -> String {
    candidates.iter().filter_map(|c| c.as_deref()).map(str::trim).find(|s| !s.is_empty()).unwrap_or("").to_string()
}

/// Decodes the payload segment of a JWT. Signature is not checked: the token came to us over
/// TLS from the token endpoint and only feeds display text and the RDP username.
pub fn parse_id_token(jwt: &str) -> Option<Claims> {
    let payload = jwt.split('.').nth(1)?;
    let bytes = base64url_decode(payload)?;
    serde_json::from_slice(&bytes).ok()
}

fn base64url_decode(input: &str) -> Option<Vec<u8>> {
    let mut out = Vec::with_capacity(input.len() * 3 / 4);
    let mut buf: u32 = 0;
    let mut bits = 0;
    for c in input.bytes() {
        let v = match c {
            b'A'..=b'Z' => c - b'A',
            b'a'..=b'z' => c - b'a' + 26,
            b'0'..=b'9' => c - b'0' + 52,
            b'-' | b'+' => 62,
            b'_' | b'/' => 63,
            b'=' => break,
            _ => return None,
        };
        buf = (buf << 6) | u32::from(v);
        bits += 6;
        if bits >= 8 {
            bits -= 8;
            out.push(((buf >> bits) & 0xff) as u8);
        }
    }
    Some(out)
}

/// What gets stored between runs.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TokenSet {
    pub access_token: String,
    pub refresh_token: String,
    /// Unix seconds.
    pub expires_at: u64,
    #[serde(default)]
    pub claims: Claims,
}

impl TokenSet {
    pub fn from_response(resp: TokenResponse, previous: Option<&TokenSet>, now: u64) -> crate::Result<TokenSet> {
        let refresh_token = resp
            .refresh_token
            .or_else(|| previous.map(|p| p.refresh_token.clone()))
            .ok_or_else(|| "Microsoft did not return a refresh token; offline_access may be blocked by policy".to_string())?;
        let claims = resp
            .id_token
            .as_deref()
            .and_then(parse_id_token)
            .or_else(|| previous.map(|p| p.claims.clone()))
            .unwrap_or_default();
        Ok(TokenSet { access_token: resp.access_token, refresh_token, expires_at: now.saturating_add(resp.expires_in), claims })
    }

    /// Refresh a few minutes early so a session launch never races expiry.
    pub fn is_expiring_soon(&self, now: u64) -> bool {
        self.expires_at <= now.saturating_add(300)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn poll_states() {
        assert!(matches!(parse_poll(400, r#"{"error":"authorization_pending"}"#), Poll::Pending));
        assert!(matches!(parse_poll(400, r#"{"error":"slow_down"}"#), Poll::SlowDown));
        assert!(matches!(parse_poll(400, r#"{"error":"expired_token"}"#), Poll::Failed(_)));
        assert!(matches!(parse_poll(200, r#"{"access_token":"a","expires_in":3600}"#), Poll::Done(_)));
        assert!(matches!(parse_poll(500, "not json"), Poll::Failed(m) if m.contains("HTTP 500")));
    }

    #[test]
    fn id_token_claims() {
        // {"upn":"a@b.com","name":"A B","tid":"t"} in base64url, with a fake header and signature
        let jwt = "eyJhbGciOiJub25lIn0.eyJ1cG4iOiJhQGIuY29tIiwibmFtZSI6IkEgQiIsInRpZCI6InQifQ.sig";
        let c = parse_id_token(jwt).unwrap();
        assert_eq!(c.upn.as_deref(), Some("a@b.com"));
        assert_eq!(c.rdp_username(), "a@b.com");
        assert_eq!(c.display_identity(), "a@b.com");
        assert!(parse_id_token("garbage").is_none());
    }

    #[test]
    fn token_set_keeps_previous_refresh_token() {
        let prev = TokenSet { access_token: "old".into(), refresh_token: "r1".into(), expires_at: 0, claims: Claims::default() };
        let resp = TokenResponse { access_token: "new".into(), refresh_token: None, id_token: None, expires_in: 60 };
        let set = TokenSet::from_response(resp, Some(&prev), 1000).unwrap();
        assert_eq!(set.refresh_token, "r1");
        assert_eq!(set.expires_at, 1060);
        assert!(set.is_expiring_soon(1000));
        let bare = TokenResponse { access_token: "new".into(), refresh_token: None, id_token: None, expires_in: 60 };
        assert!(TokenSet::from_response(bare, None, 0).is_err());
    }
}
