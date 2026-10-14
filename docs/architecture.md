# Architecture

R365 is a Rust desktop client for Windows 365 Cloud PCs and Azure Virtual Desktop on Linux. It signs in to Entra ID, lists what is published to the user, and runs a FreeRDP 3 client for the session. No browser engine is embedded; sign-in happens in the user's own browser.

## Crates

```
crates/core     r365-core     pure logic, no I/O: token handling, feed XML, FreeRDP arguments, settings
crates/native   r365-native   HTTPS (ureq, rustls, OS trust store), keyring, XDG paths, FreeRDP discovery and supervision
crates/ui       r365-ui       egui views, theme tokens, application state and background threads
apps/r365       r365          the eframe window (glow renderer)
```

Layering: `core` depends on nothing in the workspace. `native` uses `core`. `ui` uses both. Nothing below `apps` opens a window.

## Files

```
crates/core/src/auth.rs      client ID, scopes, device code and refresh forms, token response parsing, id_token claims, TokenSet
crates/core/src/feed.rs      feed discovery and workspace XML parsing, Microsoft host allowlist
crates/core/src/rdp.rs       launch options, .rdp rewriting, FreeRDP argv, stdout markers, log redaction, SessionStatus
crates/core/src/config.rs    ClientConfig with defaults, DisplayMode to LaunchOptions
crates/native/src/http.rs    one blocking client; manual redirects that never carry the bearer token across origins
crates/native/src/auth.rs    device code start and poll, refresh, ensure_fresh
crates/native/src/secret.rs  refresh token in the keyring via secret-tool, file fallback
crates/native/src/feed.rs    discovery, per-tenant fetch, .rdp download, on-disk cache
crates/native/src/freerdp.rs locate a client, spawn it under `script`, parse its output into SessionEvents
crates/native/src/paths.rs   XDG dirs under r365/, private file writes, log pruning
crates/native/src/browser.rs xdg-open for https only
crates/ui/src/app.rs         App state, Msg channel, the sign-in, refresh, connect and disconnect flows
crates/ui/src/views.rs       sign-in screen, header, grid, cards, status bar, dialogs
crates/ui/src/theme.rs       colour tokens and egui style
crates/ui/src/widgets.rs     button, badge, pill, monitor glyph
apps/r365/src/main.rs        viewport options and the eframe App impl
```

## Threads

The UI thread never blocks. Every network call and process spawn runs on a `std::thread` started from `App::spawn`, which requests a repaint when the thread ends. Results come back through two `mpsc` channels drained in `App::pump` at the top of each frame: `Msg` for app-level outcomes and `(key, SessionEvent)` for FreeRDP output. There is no async runtime.

## Dependencies

| Crate | Why |
|-------|-----|
| egui, eframe (glow) | the UI; glow over wgpu for a smaller binary and lower idle memory |
| ureq (rustls, platform-verifier) | blocking HTTPS with the OS trust store, no bundled root list |
| roxmltree | read-only XML tree for the RADC feed; namespace-agnostic lookups are a few lines on top |
| serde, serde_json | settings file, token storage, feed cache, Entra ID JSON |

Everything else is the standard library: `secret-tool`, `xdg-open`, `script` and `flatpak` are run as child processes rather than linked.

## Runtime data

```
~/.config/r365/config.json          settings
~/.local/share/r365/workspaces.json last good feed, shown at startup before the refresh lands
~/.local/share/r365/feed/           downloaded .rdp profiles and their .active.rdp rewrites
~/.local/share/r365/logs/           session logs, newest ten kept, tokens redacted
~/.local/share/r365/tokens.json     only when the keyring is unavailable
```
