//! Persisted preferences. Every field has a default so an older or hand-edited file still loads.

use crate::rdp::{DisplayMode, LaunchOptions};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct ClientConfig {
    /// Explicit FreeRDP binary, or "flatpak"; empty means detect.
    pub freerdp_path: String,
    pub scale_percent: u32,
    pub fullscreen: bool,
    pub multi_monitor: bool,
    pub sound: bool,
    pub microphone: bool,
    pub clipboard: bool,
    pub dynamic_resolution: bool,
    pub gfx_progressive: bool,
    pub async_channels: bool,
    pub auto_reconnect: bool,
    pub usb_redirection: bool,
    pub smartcard: bool,
    pub prevent_session_lock: bool,
    pub ignore_cert: bool,
    pub auto_refresh_minutes: u32,
    pub extra_args: Vec<String>,
}

impl Default for ClientConfig {
    fn default() -> Self {
        Self {
            freerdp_path: String::new(),
            scale_percent: 0,
            fullscreen: false,
            multi_monitor: false,
            sound: true,
            microphone: true,
            clipboard: true,
            dynamic_resolution: true,
            gfx_progressive: true,
            async_channels: true,
            auto_reconnect: true,
            usb_redirection: false,
            smartcard: false,
            prevent_session_lock: true,
            ignore_cert: false,
            auto_refresh_minutes: 15,
            extra_args: Vec::new(),
        }
    }
}

impl ClientConfig {
    pub fn parse(json: &str) -> crate::Result<Self> {
        serde_json::from_str(json).map_err(|e| format!("Settings file could not be read: {e}"))
    }

    pub fn to_json(&self) -> String {
        serde_json::to_string_pretty(self).unwrap_or_else(|_| "{}".into())
    }

    pub fn launch_options(&self, mode: DisplayMode) -> LaunchOptions {
        let multi_monitor = mode == DisplayMode::MultiMonitor || (mode == DisplayMode::Default && self.multi_monitor);
        let fullscreen = multi_monitor || mode == DisplayMode::Fullscreen || (mode == DisplayMode::Default && self.fullscreen);
        LaunchOptions {
            fullscreen,
            multi_monitor,
            scale_percent: self.scale_percent,
            sound: self.sound,
            microphone: self.microphone,
            usb_redirection: self.usb_redirection,
            smartcard: self.smartcard,
            clipboard: self.clipboard,
            dynamic_resolution: self.dynamic_resolution,
            gfx_progressive: self.gfx_progressive,
            async_channels: self.async_channels,
            auto_reconnect: self.auto_reconnect,
            prevent_session_lock: self.prevent_session_lock,
            ignore_cert: self.ignore_cert,
            extra_args: self.extra_args.clone(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn partial_file_fills_defaults() {
        let c = ClientConfig::parse(r#"{"fullscreen":true,"unknown":1}"#).unwrap();
        assert!(c.fullscreen);
        assert!(c.sound);
        assert_eq!(c.auto_refresh_minutes, 15);
        assert!(ClientConfig::parse("{").is_err());
    }

    #[test]
    fn display_mode_overrides() {
        let c = ClientConfig::default();
        assert!(c.launch_options(DisplayMode::MultiMonitor).fullscreen);
        assert!(!c.launch_options(DisplayMode::Windowed).fullscreen);
        let mut f = ClientConfig::default();
        f.fullscreen = true;
        assert!(f.launch_options(DisplayMode::Default).fullscreen);
        assert!(!f.launch_options(DisplayMode::Windowed).fullscreen);
    }
}
