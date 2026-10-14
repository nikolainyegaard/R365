# FreeRDP

## Discovery

`native::freerdp::locate(preference)`:

- `preference` is a path to a binary, the word `flatpak`, or empty.
- Empty: `R365_FREERDP` from the environment, then `sdl-freerdp3`, `sdl-freerdp`, `xfreerdp3`, `xfreerdp`, `wlfreerdp3`, `wlfreerdp` on PATH, then the `com.freerdp.FreeRDP` Flatpak.
- The version comes from `/version`; a missing version is shown as an empty string, never a failure.

## Launch

`core::rdp::build_args` produces the argument list after the binary. It is JW365's list (see NOTICE): `/gateway:type:arm /sec:aad`, PulseAudio at 48 kHz, `/gdi:sw /bpp:32`, H.264 progressive with RemoteFX, auto-reconnect, `/prevent-session-lock:120`, `/wm-class:r365` so the session window groups with the app. Fullscreen and multi-monitor drop `/scale-desktop` and `+dynamic-resolution`, which FreeRDP rejects in combination.

The downloaded `.rdp` is rewritten by `core::rdp::prepare_profile` into `<name>.active.rdp`: smartcard, USB and clipboard lines are zeroed when the setting is off, which stops FreeRDP polling an absent smartcard service (`SCARD_E_NO_SERVICE` loops).

For the Flatpak, `native::freerdp::FreeRdp::argv` wraps the profile path in `--file-forwarding ... @@ file @@` so the sandbox can read it.

## Pseudo-terminal

FreeRDP only prints the `Browse to:` prompt when stdout is a terminal, so the client runs as `/usr/bin/script -q -c '<argv>' /dev/null`. `script` is in util-linux and present on every mainstream distribution; without it the client is spawned directly and session auth will not prompt.

Environment set for the child: `SDL_RENDER_VSYNC=1`, `SDL_VIDEO_DOUBLE_BUFFER=1`; under Wayland with an X display available `SDL_VIDEODRIVER=x11`, because the SDL client on XWayland avoids buffer-swap tearing and workspace-switch freezes seen on native Wayland; `PULSE_SERVER` pointed at `$XDG_RUNTIME_DIR/pulse/native` when unset.

The SDL client's key bindings file `~/.config/freerdp/sdl-freerdp.json` is written once if absent: F12 disconnect, F11 minimise, F10 fullscreen, with right Ctrl as the modifier.

## Output parsing

`native::freerdp::watch` reads the merged output line by line:

- `Browse to: <url>` becomes `SessionEvent::AuthRequired` (ANSI escapes stripped, `core::rdp::auth_url_from_line`).
- Markers in `core::rdp::is_connected_marker` flip the status to Connected once; reconnect markers flip it to Reconnecting.
- Exit codes 0, 129, 130 and 143, or a user-initiated stop, end as Disconnected; anything else is Failed with the code.

Every line is written to `~/.local/share/r365/logs/session_<key>_<unix>.log` after `core::rdp::redact`, capped at 2 MB per log, ten logs kept.
