//! Everything that touches the operating system. Nothing here opens a window.

pub mod auth;
pub mod browser;
pub mod feed;
pub mod freerdp;
pub mod handler;
pub mod http;
pub mod ipc;
pub mod paths;
pub mod secret;

pub use r365_core::Result;

/// Unix seconds now; zero if the clock is before 1970, which only a broken RTC produces.
pub fn now_unix() -> u64 {
    std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_secs()).unwrap_or(0)
}
