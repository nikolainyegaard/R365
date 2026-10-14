//! Pure, testable logic. No network, no filesystem, no process spawning.

pub mod auth;
pub mod config;
pub mod feed;
pub mod rdp;

/// Errors are plain text the user can read in the status bar.
pub type Result<T> = std::result::Result<T, String>;
