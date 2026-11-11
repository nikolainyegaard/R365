//! Persisted preferences. Every field has a default so an older or hand-edited file still loads.

use crate::rdp::{ClipboardMode, DisplayMode, GfxMode, LaunchOptions, NetworkProfile};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct ClientConfig {
    /// Explicit FreeRDP binary, or "flatpak"; empty means detect.
    pub freerdp_path: String,
    pub scale_percent: u32,
    pub fullscreen: bool,
    pub multi_monitor: bool,
    /// FreeRDP monitor ids for fullscreen. Empty means the primary/current monitor (or every
    /// monitor when `multi_monitor` is set). One id is single-monitor fullscreen; several ids
    /// span those monitors.
    pub monitor_ids: Vec<u32>,
    pub sound: bool,
    pub microphone: bool,
    pub sound_device: String,
    pub microphone_device: String,
    pub camera: bool,
    pub clipboard: bool,
    /// Off, both directions, or one direction. `clipboard` is kept for older config files.
    pub clipboard_mode: ClipboardMode,
    pub drive_paths: Vec<String>,
    pub dynamic_resolution: bool,
    pub gfx_progressive: bool,
    pub gfx_mode: GfxMode,
    pub network_profile: NetworkProfile,
    pub low_bandwidth_visuals: bool,
    pub async_channels: bool,
    pub auto_reconnect: bool,
    pub usb_redirection: bool,
    pub smartcard: bool,
    pub prevent_session_lock: bool,
    pub ignore_cert: bool,
    pub auto_refresh_minutes: u32,
    /// Keep running with the window hidden when it is closed.
    pub close_to_tray: bool,
    /// Autostart entry; `start_minimized` controls whether that entry passes --minimized.
    pub autostart: bool,
    pub start_minimized: bool,
    /// Per-resource display mode override, keyed by `Resource::key()`.
    pub resource_modes: HashMap<String, DisplayMode>,
    /// Account selected on the last run, so a multi-account install reopens where it left off.
    pub last_account_id: String,
    pub extra_args: Vec<String>,
}

impl Default for ClientConfig {
    fn default() -> Self {
        Self {
            freerdp_path: String::new(),
            scale_percent: 0,
            fullscreen: false,
            multi_monitor: false,
            monitor_ids: Vec::new(),
            sound: true,
            microphone: true,
            sound_device: String::new(),
            microphone_device: String::new(),
            camera: true,
            clipboard: true,
            clipboard_mode: ClipboardMode::Both,
            drive_paths: Vec::new(),
            dynamic_resolution: true,
            gfx_progressive: true,
            gfx_mode: GfxMode::Avc420,
            network_profile: NetworkProfile::Auto,
            low_bandwidth_visuals: false,
            async_channels: true,
            auto_reconnect: true,
            usb_redirection: false,
            smartcard: false,
            prevent_session_lock: true,
            ignore_cert: false,
            auto_refresh_minutes: 15,
            close_to_tray: false,
            autostart: false,
            start_minimized: false,
            resource_modes: HashMap::new(),
            last_account_id: String::new(),
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

    pub fn launch_options(&self, mode: DisplayMode, broker_redirect: bool) -> LaunchOptions {
        let multi_monitor = mode == DisplayMode::MultiMonitor || (mode == DisplayMode::Default && self.multi_monitor);
        let windowed = mode == DisplayMode::Windowed;
        let fullscreen = multi_monitor
            || (!windowed && (mode == DisplayMode::Fullscreen || (mode == DisplayMode::Default && self.fullscreen) || !self.monitor_ids.is_empty()));
        LaunchOptions {
            fullscreen,
            multi_monitor,
            monitors: if fullscreen && !multi_monitor { self.monitor_ids.clone() } else { Vec::new() },
            scale_percent: self.scale_percent,
            sound: self.sound,
            microphone: self.microphone,
            sound_device: self.sound_device.clone(),
            microphone_device: self.microphone_device.clone(),
            camera: self.camera,
            usb_redirection: self.usb_redirection,
            smartcard: self.smartcard,
            clipboard_mode: if self.clipboard { self.clipboard_mode } else { ClipboardMode::Off },
            drive_paths: self.drive_paths.clone(),
            dynamic_resolution: self.dynamic_resolution,
            gfx_mode: if !self.gfx_progressive && self.gfx_mode == GfxMode::Avc420 { GfxMode::Off } else { self.gfx_mode },
            network_profile: self.network_profile,
            low_bandwidth_visuals: self.low_bandwidth_visuals,
            async_channels: self.async_channels,
            auto_reconnect: self.auto_reconnect,
            prevent_session_lock: self.prevent_session_lock,
            ignore_cert: self.ignore_cert,
            broker_redirect,
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
        assert!(c.camera);
        assert_eq!(c.clipboard_mode, ClipboardMode::Both);
        assert_eq!(c.gfx_mode, GfxMode::Avc420);
        assert_eq!(c.network_profile, NetworkProfile::Auto);
        assert_eq!(c.auto_refresh_minutes, 15);
        assert!(ClientConfig::parse("{").is_err());
    }

    #[test]
    fn legacy_device_flags_still_apply() {
        let c = ClientConfig::parse(r#"{"clipboard":false,"gfx_progressive":false}"#).unwrap();
        let o = c.launch_options(DisplayMode::Default, true);
        assert_eq!(o.clipboard_mode, ClipboardMode::Off);
        assert_eq!(o.gfx_mode, GfxMode::Off);
    }

    #[test]
    fn resource_modes_round_trip() {
        let mut c = ClientConfig::default();
        c.resource_modes.insert("t1:pc1".into(), DisplayMode::Fullscreen);
        let parsed = ClientConfig::parse(&c.to_json()).unwrap();
        assert_eq!(parsed.resource_modes.get("t1:pc1"), Some(&DisplayMode::Fullscreen));
    }

    #[test]
    fn display_mode_overrides() {
        let c = ClientConfig::default();
        assert!(c.launch_options(DisplayMode::MultiMonitor, true).fullscreen);
        assert!(!c.launch_options(DisplayMode::Windowed, true).fullscreen);
        let f = ClientConfig { fullscreen: true, ..Default::default() };
        assert!(f.launch_options(DisplayMode::Default, true).fullscreen);
        assert!(!f.launch_options(DisplayMode::Windowed, true).fullscreen);
        let m = ClientConfig { monitor_ids: vec![3], ..Default::default() };
        let o = m.launch_options(DisplayMode::Default, true);
        assert!(o.fullscreen);
        assert_eq!(o.monitors, vec![3]);
        assert!(m.launch_options(DisplayMode::Windowed, true).monitors.is_empty());
    }
}
