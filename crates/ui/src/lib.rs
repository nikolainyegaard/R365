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
