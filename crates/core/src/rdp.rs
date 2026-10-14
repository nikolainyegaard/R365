//! FreeRDP command line, .rdp profile adjustments and stdout interpretation. The argument list
//! follows JW365's, which spent a year finding what works against the AVD gateway (see NOTICE).

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DisplayMode {
    Default,
    Fullscreen,
    Windowed,
    MultiMonitor,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LaunchOptions {
    pub fullscreen: bool,
    pub multi_monitor: bool,
    pub scale_percent: u32,
    pub sound: bool,
    pub microphone: bool,
    pub usb_redirection: bool,
    pub smartcard: bool,
    pub clipboard: bool,
    pub dynamic_resolution: bool,
    pub gfx_progressive: bool,
    pub async_channels: bool,
    pub auto_reconnect: bool,
    pub prevent_session_lock: bool,
    pub ignore_cert: bool,
    pub extra_args: Vec<String>,
}

/// Rewrites the downloaded .rdp so FreeRDP does not poll absent smartcard readers or forward
/// devices the user switched off. Lines are trimmed and re-joined with CRLF as the format wants.
pub fn prepare_profile(contents: &str, opts: &LaunchOptions) -> String {
    let mut out = String::with_capacity(contents.len());
    for line in contents.lines().map(str::trim).filter(|l| !l.is_empty()) {
        let key = line.split_once(':').map(|(k, _)| k.to_ascii_lowercase()).unwrap_or_default();
        let replacement = match key.as_str() {
            "redirectsmartcards" if !opts.smartcard => Some("redirectsmartcards:i:0"),
            "usbdevicestoredirect" if !opts.usb_redirection => Some("usbdevicestoredirect:s:"),
            "devicestoredirect" if !opts.usb_redirection => Some("devicestoredirect:s:"),
            "redirectclipboard" if !opts.clipboard => Some("redirectclipboard:i:0"),
            _ => None,
        };
        out.push_str(replacement.unwrap_or(line));
        out.push_str("\r\n");
    }
    out
}

/// Arguments after the FreeRDP binary. The .rdp file comes first, as FreeRDP requires.
pub fn build_args(rdp_file: &str, username: &str, title: &str, opts: &LaunchOptions) -> crate::Result<Vec<String>> {
    let mut a: Vec<String> = vec![rdp_file.into(), "/gateway:type:arm".into(), "/sec:aad".into()];
    if !username.trim().is_empty() {
        a.push(format!("/u:{username}"));
    }
    if opts.sound {
        a.push("/sound:sys:pulse,rate:48000,channel:2,quality:high".into());
    }
    if opts.microphone {
        a.push("/microphone:sys:pulse,rate:48000".into());
    }
    if opts.usb_redirection {
        a.push("/usb:auto".into());
    }
    if opts.smartcard {
        a.push("/smartcard".into());
    }
    let whole_screen = opts.fullscreen || opts.multi_monitor;
    if opts.multi_monitor {
        a.push("/f".into());
        a.push("/multimon:force".into());
    } else if opts.fullscreen {
        a.push("/f".into());
    }
    if opts.scale_percent > 0 && !whole_screen {
        a.push(format!("/scale-desktop:{}", opts.scale_percent));
    }
    if opts.ignore_cert {
        a.push("/cert:ignore".into());
    }
    if opts.clipboard {
        a.push("+clipboard".into());
    }
    if opts.dynamic_resolution && !whole_screen {
        a.push("+dynamic-resolution".into());
    }
    a.push("/network:auto".into());
    a.push("+compression".into());
    a.push("+fonts".into());
    a.push("+aero".into());
    if opts.async_channels {
        a.push("+async-channels".into());
    }
    if opts.auto_reconnect {
        a.push("+auto-reconnect".into());
        a.push("/auto-reconnect-max-retries:10".into());
    }
    if opts.prevent_session_lock && !opts.extra_args.iter().any(|x| x == "/prevent-session-lock" || x.starts_with("/prevent-session-lock:")) {
        a.push("/prevent-session-lock:120".into());
    }
    if opts.gfx_progressive {
        a.push("/gfx:AVC420,progressive".into());
        a.push("+rfx".into());
    }
    a.push("/gdi:sw".into());
    a.push("/bpp:32".into());
    a.push("/log-level:info".into());
    a.push(format!("/t:{title}"));
    a.push("/wm-class:r365".into());
    for extra in &opts.extra_args {
        if extra.contains(['\0', '\n', '\r']) {
            return Err("Extra FreeRDP arguments cannot contain control characters".into());
        }
        a.push(extra.clone());
    }
    Ok(a)
}

/// The URL FreeRDP asks the user to visit for Entra ID session auth, if this line carries one.
pub fn auth_url_from_line(line: &str) -> Option<String> {
    let rest = line.split_once("Browse to: ")?.1;
    let clean = strip_ansi(rest);
    let url = clean.split_whitespace().next()?.trim();
    url.starts_with("https://").then(|| url.to_string())
}

fn strip_ansi(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    let mut chars = s.chars().peekable();
    while let Some(c) = chars.next() {
        if c == '\u{1b}' && chars.peek() == Some(&'[') {
            chars.next();
            for d in chars.by_ref() {
                if d.is_ascii_alphabetic() {
                    break;
                }
            }
        } else {
            out.push(c);
        }
    }
    out
}

pub fn is_connected_marker(line: &str) -> bool {
    let l = line.to_ascii_lowercase();
    [
        "logon info",
        "logoninfov",
        "channelconnected",
        "tsg_state_connected",
        "displaycontrolcapspdu",
        "sdl_event_window_shown",
        "successfully connected",
        "postconnect",
        "demand_active",
        "connection_state_active",
        "handleshow",
        "activated",
    ]
    .iter()
    .any(|m| l.contains(m))
}

pub fn is_reconnecting_marker(line: &str) -> bool {
    let l = line.to_ascii_lowercase();
    ["auto-reconnect in progress", "connection state: reconnecting", "reconnecting to ", "client_reconnect_"].iter().any(|m| l.contains(m))
}

/// Masks codes and tokens before a line reaches the session log.
pub fn redact(line: &str) -> String {
    let mut out = line.to_string();
    for key in ["code=", "access_token=", "refresh_token=", "id_token=", "token=", "/p:"] {
        let mut from = 0;
        while let Some(pos) = out.get(from..).and_then(|s| s.find(key)).map(|p| p + from) {
            let start = pos + key.len();
            let end = out.get(start..).map(|s| s.find(['&', ' ', '"', '\'']).map_or(out.len(), |e| start + e)).unwrap_or(out.len());
            out.replace_range(start..end, "[REDACTED]");
            from = start + "[REDACTED]".len();
        }
    }
    if let Some(pos) = out.find("Bearer ") {
        let start = pos + "Bearer ".len();
        let end = out.get(start..).map(|s| s.find([' ', ',', '"']).map_or(out.len(), |e| start + e)).unwrap_or(out.len());
        out.replace_range(start..end, "[REDACTED]");
    }
    out
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum SessionStatus {
    Idle,
    Starting,
    Connecting,
    Reconnecting,
    Connected,
    Disconnecting,
    Disconnected,
    Failed,
}

impl SessionStatus {
    pub fn label(self) -> &'static str {
        match self {
            Self::Idle | Self::Disconnected => "Idle",
            Self::Starting => "Starting",
            Self::Connecting => "Connecting",
            Self::Reconnecting => "Reconnecting",
            Self::Connected => "Connected",
            Self::Disconnecting => "Disconnecting",
            Self::Failed => "Failed",
        }
    }

    /// A process exists for this resource.
    pub fn is_active(self) -> bool {
        matches!(self, Self::Starting | Self::Connecting | Self::Reconnecting | Self::Connected | Self::Disconnecting)
    }

    pub fn is_busy(self) -> bool {
        matches!(self, Self::Starting | Self::Connecting | Self::Reconnecting | Self::Disconnecting)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn opts() -> LaunchOptions {
        LaunchOptions {
            fullscreen: false,
            multi_monitor: false,
            scale_percent: 125,
            sound: true,
            microphone: false,
            usb_redirection: false,
            smartcard: false,
            clipboard: true,
            dynamic_resolution: true,
            gfx_progressive: true,
            async_channels: true,
            auto_reconnect: true,
            prevent_session_lock: true,
            ignore_cert: false,
            extra_args: vec![],
        }
    }

    #[test]
    fn args_shape() {
        let a = build_args("/tmp/x.rdp", "me@corp.com", "PC", &opts()).unwrap();
        assert_eq!(a[0], "/tmp/x.rdp");
        assert!(a.contains(&"/sec:aad".to_string()));
        assert!(a.contains(&"/u:me@corp.com".to_string()));
        assert!(a.contains(&"/scale-desktop:125".to_string()));
        assert!(a.contains(&"+dynamic-resolution".to_string()));
        assert!(!a.contains(&"/f".to_string()));
        let mut o = opts();
        o.multi_monitor = true;
        let a = build_args("x", "", "PC", &o).unwrap();
        assert!(a.contains(&"/multimon:force".to_string()));
        assert!(!a.iter().any(|s| s.starts_with("/scale-desktop")));
        assert!(!a.iter().any(|s| s.starts_with("/u:")));
        o.extra_args = vec!["bad\narg".into()];
        assert!(build_args("x", "", "PC", &o).is_err());
    }

    #[test]
    fn profile_rewrite() {
        let out = prepare_profile("redirectsmartcards:i:1\r\n\r\n usbdevicestoredirect:s:* \nfull address:s:gw\n", &opts());
        assert_eq!(out, "redirectsmartcards:i:0\r\nusbdevicestoredirect:s:\r\nfull address:s:gw\r\n");
    }

    #[test]
    fn stdout_interpretation() {
        let line = "\u{1b}[32mBrowse to: https://login.microsoftonline.com/x?a=1 \u{1b}[0m";
        assert_eq!(auth_url_from_line(line).as_deref(), Some("https://login.microsoftonline.com/x?a=1"));
        assert!(auth_url_from_line("nothing here").is_none());
        assert!(is_connected_marker("[INFO] Logon Info V2"));
        assert!(is_reconnecting_marker("auto-reconnect in progress"));
        assert_eq!(redact("https://x/?code=abc&state=1 Bearer tok,"), "https://x/?code=[REDACTED]&state=1 Bearer [REDACTED],");
    }
}
