//! FreeRDP command line, .rdp profile adjustments and stdout interpretation. The argument list
//! follows JW365's, which spent a year finding what works against the AVD gateway (see NOTICE).

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum DisplayMode {
    Default,
    Fullscreen,
    Windowed,
    MultiMonitor,
}

impl DisplayMode {
    pub fn label(self) -> &'static str {
        match self {
            Self::Default => "Use settings",
            Self::Fullscreen => "Fullscreen",
            Self::Windowed => "Windowed",
            Self::MultiMonitor => "All monitors",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
pub enum ClipboardMode {
    #[default]
    Both,
    Off,
    LocalToRemote,
    RemoteToLocal,
}

impl ClipboardMode {
    pub fn label(self) -> &'static str {
        match self {
            Self::Both => "Both directions",
            Self::Off => "Off",
            Self::LocalToRemote => "Local → remote",
            Self::RemoteToLocal => "Remote → local",
        }
    }

    pub fn enabled(self) -> bool {
        self != Self::Off
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
pub enum GfxMode {
    #[default]
    Avc420,
    Avc420Standard,
    Avc444,
    RemoteFx,
    Off,
}

impl GfxMode {
    pub fn label(self) -> &'static str {
        match self {
            Self::Avc420 => "AVC420 progressive",
            Self::Avc420Standard => "AVC420 standard",
            Self::Avc444 => "AVC444 (best colors)",
            Self::RemoteFx => "RemoteFX",
            Self::Off => "Off (server bitmap)",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
pub enum NetworkProfile {
    #[default]
    Auto,
    Lan,
    Wan,
    LowBandwidth,
}

impl NetworkProfile {
    pub fn label(self) -> &'static str {
        match self {
            Self::Auto => "Auto",
            Self::Lan => "LAN / fast",
            Self::Wan => "WAN",
            Self::LowBandwidth => "Low bandwidth",
        }
    }

    pub fn arg(self) -> &'static str {
        match self {
            Self::Auto => "auto",
            Self::Lan => "lan",
            Self::Wan => "wan",
            Self::LowBandwidth => "broadband-low",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LaunchOptions {
    pub fullscreen: bool,
    pub multi_monitor: bool,
    /// FreeRDP monitor ids selected for fullscreen. Empty means the current/primary monitor (or
    /// every monitor when `multi_monitor` is set).
    pub monitors: Vec<u32>,
    pub scale_percent: u32,
    pub sound: bool,
    pub microphone: bool,
    pub sound_device: String,
    pub microphone_device: String,
    pub camera: bool,
    pub usb_redirection: bool,
    pub smartcard: bool,
    pub clipboard_mode: ClipboardMode,
    pub drive_paths: Vec<String>,
    pub dynamic_resolution: bool,
    pub gfx_mode: GfxMode,
    pub network_profile: NetworkProfile,
    pub low_bandwidth_visuals: bool,
    pub async_channels: bool,
    pub auto_reconnect: bool,
    pub prevent_session_lock: bool,
    pub ignore_cert: bool,
    /// Ask Entra to redirect session auth to the ms-appx-web broker scheme that R365 handles,
    /// instead of the nativeclient page the user would have to copy from.
    pub broker_redirect: bool,
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
            "camerastoredirect" if !opts.camera => Some("camerastoredirect:s:"),
            "redirectclipboard" if !opts.clipboard_mode.enabled() => Some("redirectclipboard:i:0"),
            _ => None,
        };
        out.push_str(replacement.unwrap_or(line));
        out.push_str("\r\n");
    }
    out
}

fn audio_arg(kind: &str, device: &str) -> String {
    let device = device.trim();
    if device.is_empty() || device.contains([',', '\0', '\n', '\r']) {
        format!("/{kind}:sys:pulse")
    } else {
        format!("/{kind}:sys:pulse,dev:{device}")
    }
}

/// Arguments after the FreeRDP binary. The .rdp file comes first, as FreeRDP requires.
///
/// `gateway_token` is the access token for the AVD gateway scope. Given, FreeRDP uses it as
/// the ARM gateway bearer and skips its own browser round trip for it, leaving only the
/// session token prompt. It ends up on the command line, readable by other local users for
/// the hour it is valid.
pub fn build_args(rdp_file: &str, username: &str, title: &str, opts: &LaunchOptions, gateway_token: Option<&str>) -> crate::Result<Vec<String>> {
    let mut a: Vec<String> = vec![rdp_file.into(), "/gateway:type:arm".into(), "/sec:aad".into()];
    if let Some(token) = gateway_token.map(str::trim).filter(|t| !t.is_empty() && !t.contains([',', ' ', '\n'])) {
        a.push(format!("/gateway:bearer:{token}"));
    }
    if opts.broker_redirect {
        // FreeRDP's CLI builds its authorize URL from this printf format (the default is the
        // nativeclient page); a format without placeholders is valid C and ignores the arguments.
        // A literal percent must be doubled, since the value is a printf format.
        a.push(format!("/azure:avd-access:{}", crate::auth::percent_encode(crate::auth::REDIRECT_BROKER).replace('%', "%%")));
    }
    if !username.trim().is_empty() {
        a.push(format!("/u:{username}"));
    }
    // Do not pin rate/channel. Windows 365 hosts commonly publish a single 44.1 kHz PCM
    // format; forcing 48 kHz leaves FreeRDP with zero matching formats, so the microphone
    // never opens (FreeRDP #14137 / #14146). Let the server negotiate.
    if opts.sound {
        a.push(audio_arg("sound", &opts.sound_device));
    }
    if opts.microphone {
        a.push(audio_arg("microphone", &opts.microphone_device));
    }
    for (index, path) in opts.drive_paths.iter().enumerate() {
        let path = path.trim();
        if path.is_empty() {
            continue;
        }
        if path.contains([',', '\0', '\n', '\r']) {
            return Err(format!("Shared folder path cannot contain a comma or control character: {path}"));
        }
        a.push(format!("/drive:r365_{},{path}", index + 1));
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
    if whole_screen && !opts.monitors.is_empty() {
        if opts.monitors.len() > 1 && !opts.multi_monitor {
            a.push("/multimon:force".into());
        }
        let ids = opts.monitors.iter().map(u32::to_string).collect::<Vec<_>>().join(",");
        a.push(format!("/monitors:{ids}"));
    }
    if opts.scale_percent > 0 && !whole_screen {
        a.push(format!("/scale-desktop:{}", opts.scale_percent));
    }
    if opts.ignore_cert {
        a.push("/cert:ignore".into());
    }
    match opts.clipboard_mode {
        ClipboardMode::Off => a.push("-clipboard".into()),
        ClipboardMode::Both => a.push("+clipboard".into()),
        ClipboardMode::LocalToRemote => a.push("/clipboard:direction-to:remote,files-to:remote".into()),
        ClipboardMode::RemoteToLocal => a.push("/clipboard:direction-to:local,files-to:local".into()),
    }
    if opts.dynamic_resolution && !whole_screen {
        a.push("+dynamic-resolution".into());
    }
    a.push(format!("/network:{}", opts.network_profile.arg()));
    a.push("+compression".into());
    if opts.low_bandwidth_visuals {
        a.push("-fonts".into());
        a.push("-aero".into());
        a.push("-wallpaper".into());
        a.push("-themes".into());
        a.push("-window-drag".into());
        a.push("-menu-anims".into());
    } else {
        a.push("+fonts".into());
        a.push("+aero".into());
    }
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
    match opts.gfx_mode {
        GfxMode::Avc420 => {
            a.push("/gfx:AVC420,progressive".into());
            a.push("+rfx".into());
        }
        GfxMode::Avc420Standard => {
            a.push("/gfx:AVC420".into());
            a.push("+rfx".into());
        }
        GfxMode::Avc444 => {
            a.push("/gfx:AVC444".into());
            a.push("+rfx".into());
        }
        GfxMode::RemoteFx => {
            a.push("/gfx:RFX".into());
            a.push("+rfx".into());
        }
        GfxMode::Off => a.push("-gfx".into()),
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
    for key in ["code=", "access_token=", "refresh_token=", "id_token=", "token=", "/p:", "bearer:"] {
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
            monitors: vec![],
            scale_percent: 125,
            sound: true,
            microphone: false,
            sound_device: String::new(),
            microphone_device: String::new(),
            camera: true,
            usb_redirection: false,
            smartcard: false,
            clipboard_mode: ClipboardMode::Both,
            drive_paths: vec![],
            dynamic_resolution: true,
            gfx_mode: GfxMode::Avc420,
            network_profile: NetworkProfile::Auto,
            low_bandwidth_visuals: false,
            async_channels: true,
            auto_reconnect: true,
            prevent_session_lock: true,
            ignore_cert: false,
            broker_redirect: true,
            extra_args: vec![],
        }
    }

    #[test]
    fn args_shape() {
        let a = build_args("/tmp/x.rdp", "me@corp.com", "PC", &opts(), Some("tok.en")).unwrap();
        assert!(a.contains(&"/gateway:bearer:tok.en".to_string()));
        assert_eq!(a[0], "/tmp/x.rdp");
        assert!(a.contains(&"/sec:aad".to_string()));
        assert!(a.contains(&"/u:me@corp.com".to_string()));
        assert!(a.iter().any(|x| x.starts_with("/azure:avd-access:ms-appx-web%%3A%%2F%%2FMicrosoft.AAD.BrokerPlugin%%2Fa85cf173")));
        assert!(a.contains(&"/scale-desktop:125".to_string()));
        assert!(a.contains(&"+dynamic-resolution".to_string()));
        assert!(!a.contains(&"/f".to_string()));
        let mut o = opts();
        o.multi_monitor = true;
        let a = build_args("x", "", "PC", &o, None).unwrap();
        assert!(!a.iter().any(|x| x.starts_with("/gateway:bearer:")));
        assert!(a.contains(&"/multimon:force".to_string()));
        assert!(!a.iter().any(|s| s.starts_with("/scale-desktop")));
        assert!(!a.iter().any(|s| s.starts_with("/u:")));
        o.extra_args = vec!["bad\narg".into()];
        assert!(build_args("x", "", "PC", &o, None).is_err());
    }

    #[test]
    fn audio_args_negotiate_server_formats() {
        let mut o = opts();
        o.microphone = true;
        let a = build_args("x", "", "PC", &o, None).unwrap();
        assert!(a.contains(&"/sound:sys:pulse".to_string()));
        assert!(a.contains(&"/microphone:sys:pulse".to_string()));
        assert!(!a.iter().any(|s| s.starts_with("/sound:") && s.contains("rate:")));
        assert!(!a.iter().any(|s| s.starts_with("/microphone:") && s.contains("rate:")));
    }

    #[test]
    fn monitor_selection_is_passed_through() {
        let mut o = opts();
        o.fullscreen = true;
        o.monitors = vec![3];
        let a = build_args("x", "", "PC", &o, None).unwrap();
        assert!(a.contains(&"/f".to_string()));
        assert!(a.contains(&"/monitors:3".to_string()));
        assert!(!a.contains(&"/multimon:force".to_string()));
        o.monitors = vec![3, 4];
        let a = build_args("x", "", "PC", &o, None).unwrap();
        assert!(a.contains(&"/monitors:3,4".to_string()));
        assert!(a.contains(&"/multimon:force".to_string()));
        o.fullscreen = false;
        o.monitors = vec![3];
        let a = build_args("x", "", "PC", &o, None).unwrap();
        assert!(!a.iter().any(|s| s.starts_with("/monitors:")));
    }

    #[test]
    fn device_and_quality_args() {
        let mut o = opts();
        o.microphone = true;
        o.sound_device = "sink.one".into();
        o.microphone_device = "source.one".into();
        o.clipboard_mode = ClipboardMode::LocalToRemote;
        o.drive_paths = vec!["/home/me/Documents".into()];
        o.gfx_mode = GfxMode::Avc444;
        o.network_profile = NetworkProfile::LowBandwidth;
        o.low_bandwidth_visuals = true;
        let a = build_args("x", "", "PC", &o, None).unwrap();
        assert!(a.contains(&"/sound:sys:pulse,dev:sink.one".to_string()));
        assert!(a.contains(&"/microphone:sys:pulse,dev:source.one".to_string()));
        assert!(a.contains(&"/clipboard:direction-to:remote,files-to:remote".to_string()));
        assert!(a.contains(&"/drive:r365_1,/home/me/Documents".to_string()));
        assert!(a.contains(&"/gfx:AVC444".to_string()));
        assert!(a.contains(&"/network:broadband-low".to_string()));
        assert!(a.contains(&"-wallpaper".to_string()));
        assert!(!a.contains(&"+aero".to_string()));
        o.drive_paths = vec!["/tmp/a,b".into()];
        assert!(build_args("x", "", "PC", &o, None).is_err());
    }

    #[test]
    fn alternative_graphics_modes() {
        let mut o = opts();
        o.gfx_mode = GfxMode::Avc420Standard;
        let a = build_args("x", "", "PC", &o, None).unwrap();
        assert!(a.contains(&"/gfx:AVC420".to_string()));
        assert!(!a.contains(&"/gfx:AVC420,progressive".to_string()));
        o.gfx_mode = GfxMode::RemoteFx;
        let a = build_args("x", "", "PC", &o, None).unwrap();
        assert!(a.contains(&"/gfx:RFX".to_string()));
        o.gfx_mode = GfxMode::Off;
        let a = build_args("x", "", "PC", &o, None).unwrap();
        assert!(a.contains(&"-gfx".to_string()));
    }

    #[test]
    fn devices_switch_off_in_the_profile() {
        let mut o = opts();
        o.camera = false;
        o.clipboard_mode = ClipboardMode::Off;
        let out = prepare_profile("camerastoredirect:s:*\r\nredirectclipboard:i:1\r\n", &o);
        assert_eq!(out, "camerastoredirect:s:\r\nredirectclipboard:i:0\r\n");
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
        assert_eq!(redact("/gateway:bearer:abc.def /f"), "/gateway:bearer:[REDACTED] /f");
    }
}
