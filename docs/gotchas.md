# Gotchas

- **egui 0.36 panels take a `Ui`, not a `Context`.** `eframe::App::ui(&mut self, ui, frame)` replaced `update(ctx, frame)`; `egui::Panel::top(..).show(ui, ..)` replaced `TopBottomPanel::top(..).show(ctx, ..)`. Windows still take the context (`ui.ctx()`).
- **Card buttons sit on top of a clickable card.** The card rect is allocated with `Sense::click` first and the button is added later inside a child `Ui`; egui gives the later, smaller widget the click, so the button works and the card still gets double-clicks and the context menu.
- **The feed service answers unknown user agents with HTML.** `core::feed::USER_AGENT` is the web client's string and is sent as both `User-Agent` and `X-MS-User-Agent`.
- **Entra returns HTTP 200 for a rejected redirect URI.** The AADSTS50011 error is rendered in the page. Any test of redirect registration has to be a real sign-in.
- **`secret-tool lookup` returns the secret without a trailing newline;** `store` reads the secret from stdin until EOF. Both are relied on in `native::secret`.
- **`script` merges stderr into stdout.** The direct-spawn fallback pipes stderr separately and drains it so FreeRDP never blocks on a full pipe.
- **Never-crash lints are workspace-wide** (`[workspace.lints.clippy]` in the root Cargo.toml), with unwrap, expect, panic and indexing allowed in tests through `clippy.toml`. A new crate gets them by adding `[lints] workspace = true`.
- **The device code grant dies at the finish line for this client ID.** Start and polling look fine; completion returns AADSTS7000218. Do not try it again; see docs/auth.md.
- **FreeRDP's `/azure:avd-access` value is a printf format.** `%2F` would be parsed as a conversion. Double every percent sign.
- **The desktop file points at the running binary's path.** A `cargo run` build registers `target/debug/r365`, a release build `target/release/r365`; whichever ran last owns the scheme. Harmless in development, and packaging will install a fixed path.
