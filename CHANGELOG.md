# Changelog

All notable changes to this project are documented here. The format follows Keep a Changelog and the project follows Semantic Versioning.

## [Unreleased]

### Added
- Entra ID sign-in in the system browser (authorization code with PKCE, handed back through the ms-appx-web scheme handler R365 registers), refresh token kept in the user keyring
- Multiple Microsoft accounts, including different companies: each has its own tokens, feed cache, sync state and sessions. Switch accounts from the header; adding one leaves the others signed in
- Workspace feed discovery and resource grid grouped by tenant, with search and a cached last feed
- FreeRDP 3 launch for Cloud PCs and RemoteApps with session status on each card, display mode choices in the context menu, and session logs
- Session auth for FreeRDP's Entra ID prompt, completed automatically through the scheme handler, with a paste fallback; the gateway token is passed in so only the session token prompts
- Settings for display, devices, connection and the FreeRDP binary
- Audio output and microphone pickers backed by PulseAudio/PipeWire
- Shared local folders as redirected drives
- Camera on/off, clipboard direction (both, off, or one way)
- Graphics profile (AVC420 progressive/standard, AVC444, RemoteFX, or off), network profile (auto/LAN/WAN/low bandwidth) and a reduce-visual-effects switch
- Start at login, start minimized, and keep running when the window is closed
- Per-resource default display mode from the card context menu
- The settings dialog scrolls, so every section stays reachable on smaller screens
- Monitor selection: pick one monitor for single-monitor fullscreen or several monitors to span, or keep using all/current monitors
- The browser comes to the front for sign-in on Wayland, via an activation token from the R365 window

### Fixed
- Audio formats are negotiated with the host instead of being pinned to 48 kHz stereo. Windows 365 commonly offers only 44.1 kHz PCM, which left FreeRDP with zero matching formats and silently disabled the microphone (`audin_process_open: invalid format index 0`)

### Changed
- Existing single-account tokens and feed cache are migrated automatically to the new per-account storage on first launch
- Stored access tokens are reused until they expire or the service rejects them; R365 refreshes only then, and retries feed and profile requests once after a 401/403 instead of refreshing five minutes ahead

[Unreleased]: https://github.com/nikolainyegaard/R365/commits/main
