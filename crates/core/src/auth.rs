//! Entra ID OAuth 2.0 for the Microsoft Remote Desktop public client: authorization code with
//! PKCE in the system browser, refresh token grant, and id_token claims. See docs/auth.md.

use serde::{Deserialize, Serialize};
use sha2::Digest;

/// Microsoft's own Remote Desktop / AVD client. FreeRDP hardcodes the same ID, which is what
/// lets its session auth share the user's sign-in.
pub const CLIENT_ID: &str = "a85cf173-4192-42f8-81fa-777a763e6e2c";
pub const SCOPE: &str = "https://www.wvd.microsoft.com/.default offline_access openid profile";
const AUTHORITY: &str = "https://login.microsoftonline.com/organizations/oauth2/v2.0";
/// The Entra broker redirect registered for the client ID. A browser hands it to whatever owns
/// the ms-appx-web scheme on the desktop; R365 registers itself as that handler.
pub const REDIRECT_BROKER: &str = "ms-appx-web://Microsoft.AAD.BrokerPlugin/a85cf173-4192-42f8-81fa-777a763e6e2c";
/// The redirect the user has to copy out of the address bar by hand; used when the scheme
/// handler could not be registered.
pub const REDIRECT_NATIVE: &str = "https://login.microsoftonline.com/common/oauth2/nativeclient";

pub fn token_url() -> String {
    format!("{AUTHORITY}/token")
}

/// One sign-in attempt's secrets: the PKCE verifier and the state the redirect must echo.
#[derive(Debug, Clone)]
pub struct Pkce {
    pub verifier: String,
    pub challenge: String,
    pub state: String,
}

impl Pkce {
    /// `random` must be at least 48 bytes from the OS; 32 go to the verifier, 16 to the state.
    pub fn new(random: &[u8]) -> crate::Result<Self> {
        let (v, s) = (random.get(..32), random.get(32..48));
        let (Some(v), Some(s)) = (v, s) else { return Err("Not enough randomness for sign-in".into()) };
        let verifier = base64url_encode(v);
        let challenge = base64url_encode(&sha2::Sha256::digest(verifier.as_bytes()));
        Ok(Self { verifier, challenge, state: base64url_encode(s) })
    }
}

pub fn authorize_url(pkce: &Pkce, redirect_uri: &str) -> String {
    format!(
        "{AUTHORITY}/authorize?client_id={CLIENT_ID}&response_type=code&response_mode=query&redirect_uri={}&scope={}&code_challenge={}&code_challenge_method=S256&state={}",
        percent_encode(redirect_uri),
        percent_encode(SCOPE),
        pkce.challenge,
        pkce.state
    )
}

pub fn code_form(code: &str, verifier: &str, redirect_uri: &str) -> Vec<(String, String)> {
    vec![
        ("client_id".into(), CLIENT_ID.into()),
        ("grant_type".into(), "authorization_code".into()),
        ("code".into(), code.into()),
        ("code_verifier".into(), verifier.into()),
        ("redirect_uri".into(), redirect_uri.into()),
        ("scope".into(), SCOPE.into()),
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

/// The code and state carried by a redirect URL, whatever its scheme. Entra's own error
/// parameters become a readable message.
pub fn parse_redirect(url: &str) -> crate::Result<(String, String)> {
    let query = url.trim().split_once('?').map(|(_, q)| q).unwrap_or("");
    let mut code = None;
    let mut state = None;
    let mut error = None;
    let mut description = None;
    for pair in query.split(['&', '#']) {
        let (k, v) = pair.split_once('=').unwrap_or((pair, ""));
        let v = percent_decode(v);
        match k {
            "code" => code = Some(v),
            "state" => state = Some(v),
            "error" => error = Some(v),
            "error_description" => description = Some(v),
            _ => {}
        }
    }
    if let Some(e) = error {
        let first = description.unwrap_or_default();
        let first = first.lines().next().unwrap_or("").trim().to_string();
        return Err(if first.is_empty() { format!("Microsoft sign-in failed: {e}") } else { format!("Microsoft sign-in failed: {first}") });
    }
    match (code, state) {
        (Some(c), Some(s)) if !c.is_empty() => Ok((c, s)),
        _ => Err("That address has no sign-in code in it".into()),
    }
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

pub fn parse_token_response(status: u16, body: &str) -> crate::Result<TokenResponse> {
    if status == 200 {
        return serde_json::from_str(body).map_err(|e| format!("Token response was not understood: {e}"));
    }
    let err = serde_json::from_str::<ErrorBody>(body).unwrap_or(ErrorBody { error: String::new(), error_description: String::new() });
    let first_line = err.error_description.lines().next().unwrap_or("").trim();
    Err(match (err.error.is_empty(), first_line.is_empty()) {
        (true, _) => format!("Microsoft sign-in returned HTTP {status}"),
        (false, true) => format!("Microsoft sign-in failed: {}", err.error),
        (false, false) => format!("Microsoft sign-in failed: {first_line}"),
    })
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

const B64: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789-_";

fn base64url_encode(bytes: &[u8]) -> String {
    let mut out = String::with_capacity(bytes.len().div_ceil(3) * 4);
    for chunk in bytes.chunks(3) {
        let (b0, b1, b2) = (chunk.first().copied().unwrap_or(0), chunk.get(1).copied(), chunk.get(2).copied());
        let n = (u32::from(b0) << 16) | (u32::from(b1.unwrap_or(0)) << 8) | u32::from(b2.unwrap_or(0));
        let sym = |shift: u32| char::from(B64.get(((n >> shift) & 63) as usize).copied().unwrap_or(b'A'));
        out.push(sym(18));
        out.push(sym(12));
        if b1.is_some() {
            out.push(sym(6));
        }
        if b2.is_some() {
            out.push(sym(0));
        }
    }
    out
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

pub fn percent_encode(s: &str) -> String {
    let mut out = String::with_capacity(s.len() * 3);
    for b in s.bytes() {
        if b.is_ascii_alphanumeric() || matches!(b, b'-' | b'_' | b'.' | b'~') {
            out.push(char::from(b));
        } else {
            out.push_str(&format!("%{b:02X}"));
        }
    }
    out
}

pub fn percent_decode(s: &str) -> String {
    let bytes = s.as_bytes();
    let mut out = Vec::with_capacity(bytes.len());
    let mut i = 0;
    while i < bytes.len() {
        let b = bytes.get(i).copied().unwrap_or(0);
        let hex = || bytes.get(i + 1..i + 3).and_then(|h| std::str::from_utf8(h).ok()).and_then(|h| u8::from_str_radix(h, 16).ok());
        match b {
            b'%' if hex().is_some() => {
                out.push(hex().unwrap_or(b'%'));
                i += 3;
            }
            b'+' => {
                out.push(b' ');
                i += 1;
            }
            _ => {
                out.push(b);
                i += 1;
            }
        }
    }
    String::from_utf8_lossy(&out).into_owned()
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
        let claims = resp.id_token.as_deref().and_then(parse_id_token).or_else(|| previous.map(|p| p.claims.clone())).unwrap_or_default();
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
    fn pkce_challenge_is_rfc7636_s256() {
        // RFC 7636 appendix B: this verifier hashes to this challenge.
        let verifier = "dBjftJeZ4CVP-mB92K27uhbUJU1p1r_wW1gFWFOEjXk";
        assert_eq!(base64url_encode(&sha2::Sha256::digest(verifier.as_bytes())), "E9Melhoa2OwvFrEMTJguCHaoeK1t8URWbuGJSstw-cM");
        let random: Vec<u8> = (0..48).collect();
        let p = Pkce::new(&random).unwrap();
        assert_eq!(p.verifier.len(), 43);
        assert_eq!(base64url_decode(&p.verifier).unwrap(), random[..32]);
        assert!(Pkce::new(&random[..40]).is_err());
        let url = authorize_url(&p, REDIRECT_BROKER);
        assert!(url.contains("redirect_uri=ms-appx-web%3A%2F%2FMicrosoft.AAD.BrokerPlugin%2Fa85cf173"));
        assert!(url.contains("code_challenge_method=S256"));
    }

    #[test]
    fn redirect_parsing() {
        let (c, s) = parse_redirect("ms-appx-web://Microsoft.AAD.BrokerPlugin/x?code=abc%2Fdef&state=st&session_state=z").unwrap();
        assert_eq!((c.as_str(), s.as_str()), ("abc/def", "st"));
        let err = parse_redirect("https://login.microsoftonline.com/common/oauth2/nativeclient?error=access_denied&error_description=AADSTS65004%3A+User+declined.&state=st").unwrap_err();
        assert!(err.contains("AADSTS65004"));
        assert!(parse_redirect("https://example/?state=st").is_err());
    }

    #[test]
    fn token_errors() {
        assert!(parse_token_response(200, r#"{"access_token":"a","expires_in":3600}"#).is_ok());
        let e = parse_token_response(400, r#"{"error":"invalid_grant","error_description":"AADSTS70008: expired.\nTrace"}"#).unwrap_err();
        assert_eq!(e, "Microsoft sign-in failed: AADSTS70008: expired.");
        assert!(parse_token_response(500, "not json").unwrap_err().contains("HTTP 500"));
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
