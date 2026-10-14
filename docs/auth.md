# Sign-in and tokens

## Facts that shaped this

Established 2026-10-12 against the live Entra ID endpoints.

- The client ID is Microsoft's own Remote Desktop app, `a85cf173-4192-42f8-81fa-777a763e6e2c`. FreeRDP hardcodes the same ID in `libfreerdp/core/settings.c`. There is no R365 app registration: the AVD and RDS broker scopes would need admin consent in every tenant.
- Loopback redirects are rejected for that client ID (AADSTS50011 for `http://localhost:8400`, tested in a real browser). The authorize endpoint still serves the sign-in page for any redirect URI, so curl cannot test this; only a real sign-in can.
- The device code grant is enabled for the client ID (`/organizations/oauth2/v2.0/devicecode` returns a user code with the AVD scope).
- A registered redirect a system browser can deliver is the broker scheme `ms-appx-web://Microsoft.AAD.BrokerPlugin/<client_id>`, FreeRDP's default non-CLI redirect format. Not used yet; see Session auth below.

## First sign-in: device code

`App::start_sign_in` runs on a thread:

1. POST the device code form (`core::auth::device_code_form`). The response carries `user_code`, `device_code`, `verification_uri` and a poll `interval`.
2. Show the user code on the sign-in screen and open `verification_uri` with xdg-open. Passkeys, FIDO keys and conditional access all work because it is the user's real browser.
3. Poll the token endpoint every `interval` seconds with the device code grant. `authorization_pending` keeps going, `slow_down` adds five seconds, anything else ends the attempt with a readable message.
4. On success build a `TokenSet` (access token, refresh token, expiry, id_token claims) and store it.

The authority is `organizations`, same as JW365, so personal accounts are refused up front.

## Everything after: refresh token

`native::auth::ensure_fresh` is called before any feed request or session launch. If the access token expires within five minutes it POSTs the refresh token grant, stores the new set, and the caller adopts it through `Msg::Tokens`. If Microsoft does not return a new refresh token the old one is kept.

## Storage

`native::secret` stores the whole `TokenSet` as one JSON secret in the user keyring through `secret-tool` (attributes `service=r365 account=default`). If `secret-tool` is missing or fails, the set goes to `~/.local/share/r365/tokens.json` with mode 0600, and that file is deleted again the next time the keyring works. Sign out clears both.

## Session auth for FreeRDP

With `/sec:aad` FreeRDP prints `Browse to: <authorize URL>` and waits on its terminal for the URL the browser lands on. `native::freerdp::watch` turns that line into `SessionEvent::AuthRequired`, the app opens the URL in the browser, and a dialog asks the user to paste the final address (the nativeclient redirect with a `code` parameter). `Session::submit_redirect` writes it to FreeRDP's stdin. The browser's SSO cookie usually makes the redirect immediate.

Planned, not built: register `x-scheme-handler/ms-appx-web` in the .desktop file and pass `/azure:avd-token:ms-appx-web%3a%2f%2fMicrosoft.AAD.BrokerPlugin%2f%s` to FreeRDP, so the browser hands the redirect to R365 and the paste step disappears. Further out: a token request in FreeRDP's auth helper protocol (`client/common/aad-helper-protocol.md`) answered from the refresh token, so no browser tab opens per session at all.
