# R365

A Rust client for Windows 365 Cloud PCs and Azure Virtual Desktop on Linux. Small, fast, native: one binary, no embedded browser, no runtime.

R365 signs you in to Entra ID through your own browser, lists the Cloud PCs and apps published to you, and runs FreeRDP 3 for the session.

- Multiple Microsoft work accounts, including different companies. Each has its own tokens, cached workspaces and sessions; switch from the header.
- Choose one monitor for fullscreen, several to span, or all/current monitors; the settings dialog scrolls on small screens.
- Audio output and microphone pickers, shared folders, camera toggle, and one-way or two-way clipboard.
- Graphics and network profiles, reduced visual effects for slow links, and a per-resource default display mode.
- Start at login, start minimized, and keep R365 running when the window is closed.
- Sound and microphone redirection through PulseAudio/PipeWire. R365 lets the host negotiate the audio format instead of pinning a rate the Cloud PC may not offer.

## Requirements

- FreeRDP 3 (`freerdp` package, or the `com.freerdp.FreeRDP` Flatpak)
- `secret-tool` (libsecret) for keyring storage; without it tokens go to an owner-only file
- A browser for sign-in

## Build and run

```
cargo run --release
```

## Status

Early. Sign-in happens in your browser; R365 registers itself as the handler for the `ms-appx-web` links Entra ID redirects to, so the browser hands the result back (it asks once). FreeRDP's per-session Entra ID prompt is completed the same way. If the handler cannot be registered, R365 asks you to paste the address the browser ends on. See docs/auth.md.

## Acknowledgements

Inspired by [JW365](https://github.com/alaurie/JW365), the JavaFX client this one borrows its look and FreeRDP know-how from. See NOTICE.

## License

Apache License 2.0. See [LICENSE](LICENSE).
