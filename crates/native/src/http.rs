//! One blocking HTTPS client. Redirects are followed by hand so the bearer token never crosses
//! to another origin and never leaves Microsoft's hosts.

use std::time::Duration;

pub struct Http {
    agent: ureq::Agent,
}

pub struct Response {
    pub status: u16,
    pub body: Vec<u8>,
}

impl Response {
    pub fn text(&self) -> String {
        String::from_utf8_lossy(&self.body).into_owned()
    }
}

const MAX_REDIRECTS: usize = 5;

impl Default for Http {
    fn default() -> Self {
        Self::new()
    }
}

impl Http {
    pub fn new() -> Self {
        let agent = ureq::Agent::config_builder()
            .tls_config(ureq::tls::TlsConfig::builder().root_certs(ureq::tls::RootCerts::PlatformVerifier).build())
            .timeout_connect(Some(Duration::from_secs(20)))
            .timeout_global(Some(Duration::from_secs(60)))
            .http_status_as_error(false)
            .max_redirects(0)
            .max_redirects_will_error(false)
            .user_agent(r365_core::feed::USER_AGENT)
            .build()
            .new_agent();
        Self { agent }
    }

    /// GET with an optional bearer token, following up to five redirects inside Microsoft hosts.
    pub fn get(&self, url: &str, bearer: Option<&str>, accept: Option<&str>, limit: usize) -> crate::Result<Response> {
        let mut current =
            r365_core::feed::allowed_url(url.to_string()).ok_or_else(|| format!("Refusing to request a non-Microsoft address: {url}"))?;
        let origin = origin_of(&current);
        for _ in 0..=MAX_REDIRECTS {
            let mut req = self.agent.get(&current).header("X-MS-User-Agent", r365_core::feed::USER_AGENT);
            if let Some(a) = accept {
                req = req.header("Accept", a);
            }
            if let (Some(t), true) = (bearer, origin_of(&current) == origin) {
                req = req.header("Authorization", &format!("Bearer {t}"));
            }
            let mut resp = req.call().map_err(|e| format!("Request to {} failed: {e}", host_of(&current)))?;
            let status = resp.status().as_u16();
            if (300..400).contains(&status) {
                let Some(location) = resp.headers().get("location").and_then(|v| v.to_str().ok()).map(str::to_string) else {
                    return Ok(Response { status, body: Vec::new() });
                };
                let next = if location.starts_with("https://") { location } else { resolve_relative(&current, &location) };
                current = r365_core::feed::allowed_url(next).ok_or_else(|| "Feed redirected to a non-Microsoft address".to_string())?;
                continue;
            }
            let body = resp.body_mut().with_config().limit(limit as u64).read_to_vec().map_err(|e| format!("Reading the response failed: {e}"))?;
            return Ok(Response { status, body });
        }
        Err("Too many redirects".into())
    }

    /// POST application/x-www-form-urlencoded, as the token endpoints want.
    pub fn post_form(&self, url: &str, fields: &[(String, String)]) -> crate::Result<Response> {
        let pairs: Vec<(&str, &str)> = fields.iter().map(|(k, v)| (k.as_str(), v.as_str())).collect();
        let mut resp = self.agent.post(url).send_form(pairs).map_err(|e| format!("Request to {} failed: {e}", host_of(url)))?;
        let status = resp.status().as_u16();
        let body = resp.body_mut().with_config().limit(1024 * 1024).read_to_vec().map_err(|e| format!("Reading the response failed: {e}"))?;
        Ok(Response { status, body })
    }
}

fn origin_of(url: &str) -> String {
    host_of(url).to_ascii_lowercase()
}

fn host_of(url: &str) -> &str {
    url.strip_prefix("https://").or_else(|| url.strip_prefix("http://")).unwrap_or(url).split(['/', '?', '#']).next().unwrap_or("")
}

fn resolve_relative(base: &str, location: &str) -> String {
    let host = host_of(base);
    if location.starts_with('/') {
        format!("https://{host}{location}")
    } else {
        let dir = base.rsplit_once('/').map_or(base, |(d, _)| d);
        format!("{dir}/{location}")
    }
}
