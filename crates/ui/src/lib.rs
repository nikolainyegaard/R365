//! egui views, theme and application state. Nothing here spawns a window; apps/r365 does that.

pub mod app;
pub mod theme;
pub mod views;
pub mod widgets;

pub use app::App;

/// Hands a redirect URL to an already running R365; false when none is running.
pub fn deliver_to_running(url: &str) -> bool {
    r365_native::ipc::send_to_running(url)
}

/// Brings a hidden/close-to-tray instance back; false when none is running.
pub fn show_running() -> bool {
    r365_native::ipc::send_to_running("@show")
}

pub fn is_running() -> bool {
    r365_native::ipc::is_running()
}
