# Sign-in and tokens

## Facts that shaped this

Established 2026-10-12 against the live Entra ID endpoints.

- The client ID is Microsoft's own Remote Desktop app, `a85cf173-4192-42f8-81fa-777a763e6e2c`. FreeRDP hardcodes the same ID in `libfreerdp/core/settings.c`. There is no R365 app registration: the AVD and RDS broker scopes would need admin consent in every tenant.
- Loopback redirects are rejected for that client ID (AADSTS50011 for `http://localhost:8400`, tested in a real browser). The authorize endpoint still serves the sign-in page for any redirect URI, so curl cannot test this; only a real sign-in can.
- The device code grant is enabled for the client ID (`/organizations/oauth2/v2.0/devicecode` returns a user code with the AVD scope).
- A registered redirect a system browser can deliver is the broker scheme `ms-appx-web://Microsoft.AAD.BrokerPlugin/<client_id>`, FreeRDP's default non-CLI redirect format. Not used yet; see Session auth below.

## First sign-in: authorization code with PKCE

`App::start_sign_in`:

1. `native::auth::new_pkce` reads 48 bytes from `/dev/urandom`; `core::auth::Pkce::new` makes the verifier, its S256 challenge and the state.
2. `core::auth::authorize_url` builds the `/authorize` URL with `response_mode=query` and the redirect URI below; the app opens it with xdg-open. Passkeys, FIDO keys and conditional access all work because it is the user's real browser.
3. Entra redirects the browser to `ms-appx-web://Microsoft.AAD.BrokerPlugin/<client_id>?code=...&state=...`. The browser hands that to the desktop's handler for the `ms-appx-web` scheme, which is R365 (see Scheme handler). The second R365 process forwards the URL over the socket and exits.
4. `App::deliver_redirect` checks the state, then `native::auth::exchange_code` POSTs the code and verifier to the token endpoint. The result is a `TokenSet` (access token, refresh token, expiry, id_token claims), stored in the keyring.

The authority is `organizations`, same as JW365, so personal accounts are refused up front.

### Why not the device code grant

It was the first choice: no redirect URI at all. Starting it works and polling reports `authorization_pending`, but the moment the user completes the browser step the token endpoint answers AADSTS7000218, "client_assertion or client_secret required": the Remote Desktop app registration does not allow public client flows for that grant. Tested 2026-10-14 with a real account. The authorization code grant with PKCE is exempt because its redirect URI is registered as a native (public) one.

## Scheme handler

`native::handler::register` runs at every start: it writes `~/.local/share/applications/r365.desktop` with `Exec=<this executable> %u` and `MimeType=x-scheme-handler/ms-appx-web;`, then `xdg-mime default r365.desktop x-scheme-handler/ms-appx-web`. Firefox asks once whether to open the link with R365; Chromium asks "Open R365?".

`native::ipc` keeps one instance: the first binds `$XDG_RUNTIME_DIR/r365.sock`; `r365 <url>` connects, writes the URL, and exits. If binding fails (another instance is running) or the handler cannot be registered, `App::handler_ready` is false and the app falls back to the nativeclient redirect: the browser ends on a blank Microsoft page and the user pastes its address into the sign-in card. The paste box is shown in both modes as a safety net.

## Everything after: refresh token

`native::auth::ensure_fresh` is called before any feed request or session launch. If the access token expires within five minutes it POSTs the refresh token grant, stores the new set, and the caller adopts it through `Msg::Tokens`. If Microsoft does not return a new refresh token the old one is kept.

## Storage

`native::secret` stores the whole `TokenSet` as one JSON secret in the user keyring through `secret-tool` (attributes `service=r365 account=default`). If `secret-tool` is missing or fails, the set goes to `~/.local/share/r365/tokens.json` with mode 0600, and that file is deleted again the next time the keyring works. Sign out clears both.

## Session auth for FreeRDP

With `/sec:aad` FreeRDP prints `Browse to: <authorize URL>` and waits on its terminal for the URL the browser lands on. `native::freerdp::watch` turns that line into `SessionEvent::AuthRequired`, the app opens the URL in the browser and shows a dialog. With the scheme handler active, `core::rdp::build_args` also passes `/azure:avd-access:<broker redirect>` so FreeRDP's authorize URL redirects to `ms-appx-web://...`, the browser hands it to R365, and `App::deliver_redirect` writes it to FreeRDP's stdin through `Session::submit_redirect`; the dialog closes by itself. Without the handler the user pastes the nativeclient address. FreeRDP only looks for `code=` and the mirrored `state=` in what it is given, so the redirect's scheme does not matter to it.

The `/azure:avd-access` value is used by FreeRDP as a printf format, so every `%` in it is doubled.

Further out: a token request in FreeRDP's auth helper protocol (`client/common/aad-helper-protocol.md`) answered from the refresh token, so no browser tab opens per session at all.
