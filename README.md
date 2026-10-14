# R365

A Rust client for Windows 365 Cloud PCs and Azure Virtual Desktop on Linux. Small, fast, native: one binary, no embedded browser, no runtime.

R365 signs you in to Entra ID through your own browser, lists the Cloud PCs and apps published to you, and runs FreeRDP 3 for the session.

## Requirements

- FreeRDP 3 (`freerdp` package, or the `com.freerdp.FreeRDP` Flatpak)
- `secret-tool` (libsecret) for keyring storage; without it tokens go to an owner-only file
- A browser for sign-in

## Build and run

```
cargo run --release
```

## Status

Early. Sign-in, the resource grid and session launch work. FreeRDP's per-session Entra ID prompt is answered by pasting the browser's final address into R365; a scheme handler that removes that step is planned (see docs/auth.md).

## Acknowledgements

Inspired by [JW365](https://github.com/alaurie/JW365), the JavaFX client this one borrows its look and FreeRDP know-how from. See NOTICE.

## License

Apache License 2.0. See [LICENSE](LICENSE).
