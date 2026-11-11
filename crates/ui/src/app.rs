//! Application state and the background work it starts. All I/O runs on plain threads and
//! reports back through channels; `update` drains them once per frame.

use r365_core::account::AccountInfo;
use r365_core::auth::{self as core_auth, Pkce, TokenSet};
use r365_core::config::ClientConfig;
use r365_core::feed::{Resource, Workspace};
use r365_core::rdp::{ClipboardMode, DisplayMode, GfxMode, SessionStatus, prepare_profile};
use r365_native::freerdp::{FreeRdp, Monitor, Session, SessionEvent};
use r365_native::http::Http;
use r365_native::{accounts as saved_accounts, audio, auth, autostart, browser, feed, freerdp, handler, ipc, paths, secret};
use std::collections::HashMap;
use std::sync::Arc;
use std::sync::mpsc::{Receiver, Sender, channel};
use std::time::{Duration, Instant};

pub const VERSION: &str = env!("CARGO_PKG_VERSION");

pub enum Msg {
    SignedIn { kind: SignInKind, result: r365_core::Result<(TokenSet, AccountInfo)> },
    Tokens { account_id: String, tokens: TokenSet },
    Workspaces { account_id: String, result: r365_core::Result<Vec<Workspace>> },
    Launched { key: String, generation: u64, result: r365_core::Result<Session> },
    Status(String),
}

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum Tone {
    Neutral,
    Active,
    Error,
}

/// One signed-in Microsoft account and everything the UI shows for it.
pub struct Account {
    pub info: AccountInfo,
    pub tokens: TokenSet,
    pub workspaces: Vec<Workspace>,
    pub last_sync: Option<Instant>,
    pub loading: bool,
}

/// Whether a browser sign-in adds a new account or refreshes one already in the list.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SignInKind {
    Add,
    Reauth(String),
}

/// A sign-in started in the browser and not yet handed back.
pub struct SignIn {
    pub pkce: Pkce,
    pub url: String,
    pub paste: String,
    pub exchanging: bool,
    pub started: Instant,
    pub kind: SignInKind,
}

pub struct PendingAuth {
    pub key: String,
    pub title: String,
    pub url: String,
    pub paste: String,
    pub started: Instant,
}

/// How long the browser gets before the paste fallback is offered. A warm browser session
/// hands the link back in under three seconds.
pub const FALLBACK_AFTER: Duration = Duration::from_secs(3);

pub struct App {
    ctx: egui::Context,
    tx: Sender<Msg>,
    rx: Receiver<Msg>,
    session_tx: Sender<(String, SessionEvent)>,
    session_rx: Receiver<(String, SessionEvent)>,
    ipc_rx: Receiver<String>,
    http: Arc<Http>,
    /// R365 owns the ms-appx-web scheme, so redirects arrive on the socket instead of by paste.
    pub handler_ready: bool,
    pub redirect_uri: &'static str,
    pub config: ClientConfig,
    pub freerdp: Option<FreeRdp>,
    /// Monitors as the SDL client sees them; ids are what `/monitors:` expects.
    pub monitors: Vec<Monitor>,
    pub audio_sinks: Vec<String>,
    pub audio_sources: Vec<String>,
    /// Draft path for the "add shared folder" field in settings.
    pub drive_input: String,
    pub accounts: Vec<Account>,
    /// Index into `accounts`; only meaningful while the list is non-empty.
    pub active: usize,
    pub statuses: HashMap<String, SessionStatus>,
    sessions: HashMap<String, Session>,
    /// Which account launched each session, so signing that account out stops only its sessions.
    session_accounts: HashMap<String, String>,
    /// The current launch attempt per resource; older attempts are ignored when they finish.
    launch_generation: HashMap<String, u64>,
    next_launch: u64,
    pub pending_auth: Option<PendingAuth>,
    pub sign_in: Option<SignIn>,
    pub status: String,
    pub tone: Tone,
    pub search: String,
    pub settings_draft: Option<ClientConfig>,
    pub confirm_sign_out: bool,
    pub error_dialog: Option<String>,
    /// A browser launch waiting for an activation token from the window system.
    browser_launch: Option<BrowserLaunch>,
    token_requested: bool,
    /// Set by the account menu's Quit so close-to-tray does not intercept the close.
    force_quit: bool,
}

struct BrowserLaunch {
    url: String,
    since: Instant,
}

/// How long a browser launch waits for an activation token before going without one. The
/// compositor answers within a frame or two; X11 and headless never answer.
const TOKEN_WAIT: Duration = Duration::from_millis(400);

impl App {
    pub fn new(ctx: egui::Context, initial_url: Option<String>) -> Self {
        let (tx, rx) = channel();
        let (session_tx, session_rx) = channel();
        let (ipc_tx, ipc_rx) = channel();
        let listening = ipc::listen(ipc_tx);
        let handler_ready = listening && handler::register().is_ok();
        let config = std::fs::read_to_string(paths::config_file()).ok().and_then(|s| ClientConfig::parse(&s).ok()).unwrap_or_default();
        let freerdp = freerdp::locate(&config.freerdp_path);
        let monitors = freerdp.as_ref().map(|f| f.list_monitors()).unwrap_or_default();
        let accounts = load_accounts();
        let active = accounts.iter().position(|a| a.info.id == config.last_account_id).unwrap_or(0);
        let mut app = Self {
            ctx,
            tx,
            rx,
            session_tx,
            session_rx,
            ipc_rx,
            http: Arc::new(Http::new()),
            handler_ready,
            redirect_uri: if handler_ready { core_auth::REDIRECT_BROKER } else { core_auth::REDIRECT_NATIVE },
            config,
            freerdp,
            monitors,
            audio_sinks: audio::sinks(),
            audio_sources: audio::sources(),
            drive_input: String::new(),
            accounts,
            active,
            statuses: HashMap::new(),
            sessions: HashMap::new(),
            session_accounts: HashMap::new(),
            launch_generation: HashMap::new(),
            next_launch: 1,
            pending_auth: None,
            sign_in: None,
            status: String::new(),
            tone: Tone::Neutral,
            search: String::new(),
            settings_draft: None,
            confirm_sign_out: false,
            error_dialog: None,
            browser_launch: None,
            token_requested: false,
            force_quit: false,
        };
        if !app.monitors.is_empty() {
            let before = app.config.monitor_ids.len();
            app.config.monitor_ids.retain(|id| app.monitors.iter().any(|m| m.id == *id));
            if app.config.monitor_ids.len() != before {
                app.save_config();
            }
        }
        let _ = autostart::apply(app.config.autostart, app.config.start_minimized);
        if app.signed_in() {
            app.remember_active();
            app.refresh_workspaces();
        } else {
            app.set_status("Sign in to see your Cloud PCs", Tone::Neutral);
        }
        if let Some(url) = initial_url {
            app.deliver_redirect(&url);
        }
        app.ctx.request_repaint();
        app
    }

    pub fn signed_in(&self) -> bool {
        !self.accounts.is_empty()
    }

    pub fn signing_in(&self) -> bool {
        self.sign_in.is_some()
    }

    pub fn set_status(&mut self, s: impl Into<String>, tone: Tone) {
        self.status = s.into();
        self.tone = tone;
    }

    pub fn active_account(&self) -> Option<&Account> {
        self.accounts.get(self.active)
    }

    pub fn workspaces(&self) -> &[Workspace] {
        self.active_account().map_or(&[], |a| a.workspaces.as_slice())
    }

    pub fn loading(&self) -> bool {
        self.active_account().is_some_and(|a| a.loading)
    }

    pub fn last_sync(&self) -> Option<Instant> {
        self.active_account().and_then(|a| a.last_sync)
    }

    /// UPN plus the first tenant the account can see, when that is known.
    pub fn account_label(&self, index: usize) -> String {
        let Some(account) = self.accounts.get(index) else { return String::new() };
        let tenant = account.workspaces.iter().map(|w| w.tenant_name.trim()).find(|t| !t.is_empty());
        match tenant {
            Some(t) if !account.info.upn.trim().is_empty() => format!("{} — {t}", account.info.upn),
            _ => account.info.label().to_string(),
        }
    }

    pub fn status_of(&self, key: &str) -> SessionStatus {
        self.statuses.get(key).copied().unwrap_or(SessionStatus::Idle)
    }

    pub fn any_session_busy(&self) -> bool {
        self.statuses.values().any(|s| s.is_busy())
    }

    pub fn resource_count(&self) -> usize {
        self.workspaces().iter().map(|w| w.resources.len()).sum()
    }

    /// Called once per frame before drawing.
    pub fn pump(&mut self) {
        while let Ok(msg) = self.rx.try_recv() {
            self.handle(msg);
        }
        while let Ok((key, event)) = self.session_rx.try_recv() {
            self.handle_session(key, event);
        }
        while let Ok(line) = self.ipc_rx.try_recv() {
            if line == "@show" {
                self.show_window();
            } else {
                self.deliver_redirect(&line);
            }
        }
        if self.config.close_to_tray && !self.force_quit && self.ctx.input(|i| i.viewport().close_requested()) {
            self.ctx.send_viewport_cmd(egui::ViewportCommand::CancelClose);
            self.ctx.send_viewport_cmd(egui::ViewportCommand::Visible(false));
        }
        if self.browser_launch.as_ref().is_some_and(|b| b.since.elapsed() >= TOKEN_WAIT) {
            self.launch_browser(None);
        }
        let due = self
            .active_account()
            .and_then(|a| a.last_sync)
            .is_some_and(|t| t.elapsed() > Duration::from_secs(u64::from(self.config.auto_refresh_minutes.max(1)) * 60));
        if due && !self.loading() && self.signed_in() {
            self.refresh_workspaces();
        }
        // A hidden window still has to wake up to receive "@show" from a second launch.
        if self.config.close_to_tray || self.config.start_minimized {
            self.ctx.request_repaint_after(Duration::from_millis(500));
        }
    }

    fn handle(&mut self, msg: Msg) {
        match msg {
            Msg::SignedIn { kind, result } => self.handle_signed_in(kind, result),
            Msg::Tokens { account_id, tokens } => {
                if let Some(account) = self.accounts.iter_mut().find(|a| a.info.id == account_id) {
                    account.tokens = tokens;
                }
            }
            Msg::Workspaces { account_id, result } => self.handle_workspaces(&account_id, result),
            Msg::Launched { key, generation, result } => self.handle_launched(key, generation, result),
            Msg::Status(s) => self.set_status(s, Tone::Active),
        }
    }

    fn handle_launched(&mut self, key: String, generation: u64, result: r365_core::Result<Session>) {
        if self.launch_generation.get(&key) != Some(&generation) {
            // A newer attempt, a disconnect or a sign-out superseded this one.
            if let Ok(session) = result {
                session.stop();
            }
            return;
        }
        self.launch_generation.remove(&key);
        match result {
            Ok(session) => {
                self.sessions.insert(key, session);
            }
            Err(e) => {
                self.session_accounts.remove(&key);
                self.statuses.insert(key, SessionStatus::Failed);
                self.set_status(format!("Connection failed: {e}"), Tone::Error);
                self.error_dialog = Some(e);
            }
        }
    }

    fn handle_signed_in(&mut self, kind: SignInKind, result: r365_core::Result<(TokenSet, AccountInfo)>) {
        self.sign_in = None;
        let (tokens, info) = match result {
            Ok(v) => v,
            Err(e) => {
                self.set_status(e, Tone::Error);
                return;
            }
        };
        self.attention();
        let index = match kind {
            SignInKind::Reauth(id) => {
                if let Some(index) = self.accounts.iter().position(|a| a.info.id == id) {
                    if let Some(account) = self.accounts.get_mut(index) {
                        account.info = info.clone();
                        account.tokens = tokens.clone();
                    }
                    index
                } else {
                    self.add_or_update(info.clone(), tokens.clone())
                }
            }
            SignInKind::Add => self.add_or_update(info.clone(), tokens.clone()),
        };
        self.persist_accounts();
        self.active = index;
        self.remember_active();
        let label = self.accounts.get(index).map(|a| a.info.label().to_string()).unwrap_or_default();
        self.set_status(format!("Signed in as {label}"), Tone::Neutral);
        let id = self.accounts.get(index).map(|a| a.info.id.clone()).unwrap_or_default();
        self.refresh_account(&id);
    }

    fn add_or_update(&mut self, info: AccountInfo, tokens: TokenSet) -> usize {
        if let Some(index) = self.accounts.iter().position(|a| a.info.id == info.id) {
            if let Some(account) = self.accounts.get_mut(index) {
                account.info = info;
                account.tokens = tokens;
            }
            index
        } else {
            let workspaces = feed::load_cache(&info.id);
            self.accounts.push(Account { info, tokens, workspaces, last_sync: None, loading: false });
            self.accounts.len().saturating_sub(1)
        }
    }

    fn handle_workspaces(&mut self, account_id: &str, result: r365_core::Result<Vec<Workspace>>) {
        let Some(index) = self.accounts.iter().position(|a| a.info.id == account_id) else { return };
        let active = index == self.active;
        match result {
            Ok(ws) => {
                let n: usize = ws.iter().map(|w| w.resources.len()).sum();
                if let Some(account) = self.accounts.get_mut(index) {
                    account.loading = false;
                    account.last_sync = Some(Instant::now());
                    account.workspaces = ws;
                }
                if active {
                    self.set_status(
                        if n == 0 {
                            "No Cloud PCs or apps are published to you".into()
                        } else {
                            format!("{n} resource{} available", if n == 1 { "" } else { "s" })
                        },
                        Tone::Neutral,
                    );
                }
            }
            Err(e) => {
                if let Some(account) = self.accounts.get_mut(index) {
                    account.loading = false;
                    account.last_sync = Some(Instant::now());
                }
                if active {
                    self.set_status(format!("Refresh failed: {e}"), Tone::Error);
                }
            }
        }
    }

    fn handle_session(&mut self, key: String, event: SessionEvent) {
        match event {
            SessionEvent::Status(status, message) => {
                if status == SessionStatus::Failed {
                    self.attention();
                }
                let tone = match status {
                    SessionStatus::Failed => Tone::Error,
                    s if s.is_busy() => Tone::Active,
                    _ => Tone::Neutral,
                };
                self.set_status(message, tone);
                self.statuses.insert(key, status);
            }
            SessionEvent::AuthRequired(url) => {
                let title = self.title_of(&key);
                self.open_browser(&url);
                self.pending_auth = Some(PendingAuth { key, title, url, paste: String::new(), started: Instant::now() });
            }
            SessionEvent::Exited(_) => {
                self.sessions.remove(&key);
                self.session_accounts.remove(&key);
                if self.pending_auth.as_ref().is_some_and(|p| p.key == key) {
                    self.pending_auth = None;
                }
            }
        }
    }

    fn title_of(&self, key: &str) -> String {
        self.accounts
            .iter()
            .flat_map(|a| &a.workspaces)
            .flat_map(|w| &w.resources)
            .find(|r| r.key() == key)
            .map(|r| r.title.clone())
            .unwrap_or_else(|| "Cloud PC".into())
    }

    /// Opens `url` in the browser once the shell has had a chance to fetch an activation token.
    fn open_browser(&mut self, url: &str) {
        self.browser_launch = Some(BrowserLaunch { url: url.to_string(), since: Instant::now() });
        self.token_requested = true;
        self.ctx.request_repaint_after(TOKEN_WAIT);
    }

    /// The shell asks this every frame and calls eframe's token request when it is true.
    pub fn take_token_request(&mut self) -> bool {
        std::mem::take(&mut self.token_requested)
    }

    /// The shell hands over the token the window system answered with.
    pub fn deliver_activation_token(&mut self, token: String) {
        if self.browser_launch.is_some() {
            self.launch_browser(Some(&token));
        }
    }

    fn launch_browser(&mut self, token: Option<&str>) {
        if let Some(launch) = self.browser_launch.take()
            && let Err(e) = browser::open(&launch.url, token)
        {
            self.set_status(e, Tone::Error);
        }
    }

    /// Brings a window hidden by close-to-tray back. Used by a second `r365` launch.
    fn show_window(&self) {
        self.ctx.send_viewport_cmd(egui::ViewportCommand::Visible(true));
        self.ctx.send_viewport_cmd(egui::ViewportCommand::Minimized(false));
        self.ctx.send_viewport_cmd(egui::ViewportCommand::Focus);
    }

    pub fn quit(&mut self) {
        self.force_quit = true;
        self.ctx.send_viewport_cmd(egui::ViewportCommand::Close);
    }

    /// Highlights R365 in the task bar: after a browser hand-off the window is behind the browser
    /// and Wayland will not let it raise itself.
    fn attention(&self) {
        self.ctx.send_viewport_cmd(egui::ViewportCommand::RequestUserAttention(egui::UserAttentionType::Informational));
    }

    fn spawn<F: FnOnce() + Send + 'static>(&self, f: F) {
        let ctx = self.ctx.clone();
        std::thread::spawn(move || {
            f();
            ctx.request_repaint();
        });
    }

    pub fn start_sign_in(&mut self) {
        self.start_sign_in_kind(SignInKind::Add);
    }

    /// Adds another company account without dropping the one currently in use.
    pub fn start_add_account(&mut self) {
        self.start_sign_in_kind(SignInKind::Add);
    }

    fn start_sign_in_kind(&mut self, kind: SignInKind) {
        if self.sign_in.as_ref().is_some_and(|s| s.exchanging) {
            return;
        }
        let pkce = match auth::new_pkce() {
            Ok(p) => p,
            Err(e) => {
                self.set_status(e, Tone::Error);
                return;
            }
        };
        let url = core_auth::authorize_url(&pkce, self.redirect_uri);
        self.open_browser(&url);
        self.sign_in = Some(SignIn { pkce, url, paste: String::new(), exchanging: false, started: Instant::now(), kind });
        self.set_status("Finish signing in in your browser", Tone::Active);
    }

    pub fn reopen_sign_in(&mut self) {
        if let Some(url) = self.sign_in.as_ref().map(|s| s.url.clone()) {
            self.open_browser(&url);
        }
    }

    pub fn cancel_sign_in(&mut self) {
        self.sign_in = None;
        if self.signed_in() {
            self.set_status("Sign-in cancelled", Tone::Neutral);
        } else {
            self.set_status("Sign in to see your Cloud PCs", Tone::Neutral);
        }
    }

    /// A redirect URL from the browser, the socket, or the paste box. Whichever flow is waiting
    /// gets it: the app's own sign-in, or a FreeRDP session prompt.
    pub fn deliver_redirect(&mut self, url: &str) {
        let url = url.trim();
        if let Some(sign_in) = self.sign_in.as_mut().filter(|s| !s.exchanging) {
            match core_auth::parse_redirect(url) {
                Ok((code, state)) if state == sign_in.pkce.state => {
                    sign_in.exchanging = true;
                    let kind = sign_in.kind.clone();
                    let (http, tx, pkce, redirect) = (self.http.clone(), self.tx.clone(), sign_in.pkce.clone(), self.redirect_uri);
                    self.set_status("Completing sign-in", Tone::Active);
                    self.spawn(move || {
                        let result = auth::exchange_code(&http, &code, &pkce, redirect).and_then(|tokens| {
                            let info = AccountInfo::from_claims(&tokens.claims);
                            secret::store(&info.id, &tokens)?;
                            Ok((tokens, info))
                        });
                        let _ = tx.send(Msg::SignedIn { kind, result });
                    });
                }
                Ok(_) => self.set_status("That sign-in link belongs to an older attempt. Start the sign-in again.", Tone::Error),
                Err(e) => self.set_status(e, Tone::Error),
            }
            return;
        }
        if let Some(pending) = self.pending_auth.take() {
            match self.sessions.get(&pending.key) {
                Some(session) if url.contains("code=") => match session.submit_redirect(url) {
                    Ok(()) => self.set_status(format!("Completing sign-in for {}", pending.title), Tone::Active),
                    Err(e) => self.error_dialog = Some(e),
                },
                Some(_) => {
                    self.set_status("That address has no sign-in code in it", Tone::Error);
                    self.pending_auth = Some(pending);
                }
                None => self.set_status("The session ended before sign-in finished", Tone::Neutral),
            }
            return;
        }
        self.set_status("Received a sign-in link, but nothing was waiting for it", Tone::Neutral);
    }

    /// Refreshes the account currently shown. The header and the auto-refresh timer use this.
    pub fn refresh_workspaces(&mut self) {
        let Some(id) = self.active_account().map(|a| a.info.id.clone()) else { return };
        self.refresh_account(&id);
    }

    fn refresh_account(&mut self, account_id: &str) {
        let Some(index) = self.accounts.iter().position(|a| a.info.id == account_id) else { return };
        let Some(account) = self.accounts.get(index) else { return };
        if account.loading {
            return;
        }
        let tokens = account.tokens.clone();
        if let Some(account) = self.accounts.get_mut(index) {
            account.loading = true;
        }
        self.set_status("Refreshing workspaces", Tone::Active);
        let (http, tx) = (self.http.clone(), self.tx.clone());
        let id = account_id.to_string();
        self.spawn(move || {
            let result = (|| {
                let (fresh, changed) = auth::ensure_fresh(&http, &tokens, &id)?;
                if changed {
                    let _ = tx.send(Msg::Tokens { account_id: id.clone(), tokens: fresh.clone() });
                }
                match feed::fetch_all(&http, &fresh.access_token) {
                    Ok(ws) => Ok(ws),
                    Err(e) if feed::is_auth_error(&e) => {
                        let refreshed = refresh_tokens(&http, &fresh, &id, &tx)?;
                        feed::fetch_all(&http, &refreshed.access_token)
                    }
                    Err(e) => Err(e),
                }
            })();
            if let Ok(ws) = &result {
                feed::save_cache(&id, ws);
            }
            let _ = tx.send(Msg::Workspaces { account_id: id, result });
        });
    }

    /// Switches the visible account. Sessions already running from other accounts keep running.
    pub fn set_active(&mut self, index: usize) {
        if index >= self.accounts.len() {
            return;
        }
        self.active = index;
        self.remember_active();
        let label = self.accounts.get(index).map(|a| a.info.label().to_string()).unwrap_or_default();
        self.set_status(format!("Signed in as {label}"), Tone::Neutral);
        let id = self.accounts.get(index).map(|a| a.info.id.clone()).unwrap_or_default();
        self.refresh_account(&id);
    }

    pub fn resource_mode(&self, key: &str) -> Option<DisplayMode> {
        self.config.resource_modes.get(key).copied()
    }

    pub fn set_resource_mode(&mut self, key: &str, mode: Option<DisplayMode>) {
        match mode {
            Some(mode) => {
                self.config.resource_modes.insert(key.to_string(), mode);
            }
            None => {
                self.config.resource_modes.remove(key);
            }
        }
        self.save_config();
        self.set_status("Default display mode saved", Tone::Neutral);
    }

    fn account_for_resource(&self, resource: &Resource) -> Option<usize> {
        if self.active_account().is_some_and(|a| a.workspaces.iter().any(|w| w.tenant_id == resource.tenant_id)) {
            return Some(self.active);
        }
        self.accounts
            .iter()
            .position(|a| a.workspaces.iter().any(|w| w.tenant_id == resource.tenant_id))
            .or_else(|| self.signed_in().then_some(self.active))
    }

    pub fn connect(&mut self, resource: &Resource, mode: DisplayMode) {
        let key = resource.key();
        let mode = if mode == DisplayMode::Default { self.config.resource_modes.get(&key).copied().unwrap_or(DisplayMode::Default) } else { mode };
        let Some(freerdp) = self.freerdp.clone() else {
            self.statuses.insert(key, SessionStatus::Failed);
            self.error_dialog =
                Some("FreeRDP was not found. Install the freerdp package (or the com.freerdp.FreeRDP Flatpak), or set its path in Settings.".into());
            return;
        };
        let Some(index) = self.account_for_resource(resource) else { return };
        let Some(account) = self.accounts.get(index) else { return };
        let account_id = account.info.id.clone();
        let tokens = account.tokens.clone();
        let username = tokens.claims.rdp_username();
        if let Some(existing) = self.sessions.get(&key) {
            existing.stop();
        }
        let generation = self.next_launch;
        self.next_launch = self.next_launch.wrapping_add(1);
        self.launch_generation.insert(key.clone(), generation);
        self.session_accounts.insert(key.clone(), account_id.clone());
        self.statuses.insert(key.clone(), SessionStatus::Starting);
        self.set_status(format!("Starting {}", resource.title), Tone::Active);
        let mut launch_config = self.config.clone();
        if !self.monitors.is_empty() {
            launch_config.monitor_ids.retain(|id| self.monitors.iter().any(|m| m.id == *id));
        }
        let opts = launch_config.launch_options(mode, self.handler_ready);
        let (http, tx, stx, resource) = (self.http.clone(), self.tx.clone(), self.session_tx.clone(), resource.clone());
        self.spawn(move || {
            let result = (|| {
                let (mut fresh, changed) = auth::ensure_fresh(&http, &tokens, &account_id)?;
                if changed {
                    let _ = tx.send(Msg::Tokens { account_id: account_id.clone(), tokens: fresh.clone() });
                }
                let url = resource.rdp_url.as_deref().ok_or_else(|| "This resource has no remote desktop profile to download".to_string())?;
                let _ = tx.send(Msg::Status("Downloading remote desktop profile".into()));
                let path = match feed::download_rdp(&http, &fresh.access_token, url, &resource.file_stem()) {
                    Ok(path) => path,
                    Err(e) if feed::is_auth_error(&e) => {
                        fresh = refresh_tokens(&http, &fresh, &account_id, &tx)?;
                        feed::download_rdp(&http, &fresh.access_token, url, &resource.file_stem())?
                    }
                    Err(e) => return Err(e),
                };
                let contents = std::fs::read_to_string(&path).map_err(|e| format!("Could not read the downloaded profile: {e}"))?;
                let active = path.with_extension("active.rdp");
                paths::write_private(&active, prepare_profile(&contents, &opts).as_bytes())?;
                let _ = tx.send(Msg::Status(format!("Starting FreeRDP for {}", resource.title)));
                freerdp::launch(
                    &freerdp,
                    freerdp::LaunchRequest {
                        rdp_file: &active,
                        username: &username,
                        title: &resource.title,
                        gateway_token: Some(&fresh.access_token),
                        key: resource.key(),
                    },
                    &opts,
                    stx,
                )
            })();
            let _ = tx.send(Msg::Launched { key: resource.key(), generation, result });
        });
    }

    pub fn disconnect(&mut self, key: &str) {
        self.launch_generation.remove(key);
        if let Some(s) = self.sessions.get(key) {
            s.stop();
            self.statuses.insert(key.to_string(), SessionStatus::Disconnecting);
        }
        if self.pending_auth.as_ref().is_some_and(|p| p.key == key) {
            self.pending_auth = None;
        }
    }

    pub fn restart(&mut self, resource: &Resource) {
        self.disconnect(&resource.key());
        self.connect(resource, DisplayMode::Default);
    }

    pub fn submit_pasted(&mut self) {
        let pasted =
            self.sign_in.as_ref().map(|s| s.paste.clone()).or_else(|| self.pending_auth.as_ref().map(|p| p.paste.clone())).unwrap_or_default();
        if !pasted.trim().is_empty() {
            self.deliver_redirect(&pasted);
        }
    }

    pub fn reopen_auth_url(&mut self) {
        if let Some(url) = self.pending_auth.as_ref().map(|p| p.url.clone()) {
            self.open_browser(&url);
        }
    }

    pub fn latest_log(&self, key: &str) -> Option<std::path::PathBuf> {
        self.sessions.get(key).map(|s| s.log_file.clone()).filter(|p| p.exists())
    }

    /// Signs out the account currently shown. Other accounts and their sessions stay.
    pub fn sign_out(&mut self) {
        let Some(account) = self.accounts.get(self.active) else { return };
        let id = account.info.id.clone();
        let keys: Vec<String> = self.session_accounts.iter().filter(|(_, a)| a.as_str() == id.as_str()).map(|(k, _)| k.clone()).collect();
        for key in &keys {
            if let Some(session) = self.sessions.remove(key) {
                session.stop();
            }
            self.launch_generation.remove(key);
            self.statuses.remove(key);
            self.session_accounts.remove(key);
        }
        if self.pending_auth.as_ref().is_some_and(|p| keys.iter().any(|k| k == &p.key)) {
            self.pending_auth = None;
        }
        secret::clear(&id);
        feed::clear_cache(&id);
        self.accounts.remove(self.active);
        self.persist_accounts();
        if self.accounts.is_empty() {
            self.active = 0;
            self.config.last_account_id.clear();
            self.save_config();
            self.set_status("Signed out", Tone::Neutral);
        } else {
            self.active = self.active.min(self.accounts.len().saturating_sub(1));
            self.remember_active();
            let label = self.accounts.get(self.active).map(|a| a.info.label().to_string()).unwrap_or_default();
            self.set_status(format!("Signed out; now using {label}"), Tone::Neutral);
            let next = self.accounts.get(self.active).map(|a| a.info.id.clone()).unwrap_or_default();
            self.refresh_account(&next);
        }
    }

    pub fn open_settings(&mut self) {
        self.refresh_monitors();
        self.audio_sinks = audio::sinks();
        self.audio_sources = audio::sources();
        let mut draft = self.config.clone();
        // Bring the old bool fields in line with the newer enum settings once, so the dialog
        // shows what will actually be used.
        if !draft.clipboard {
            draft.clipboard_mode = ClipboardMode::Off;
        }
        if !draft.gfx_progressive && draft.gfx_mode == GfxMode::Avc420 {
            draft.gfx_mode = GfxMode::Off;
        }
        self.settings_draft = Some(draft);
    }

    pub fn refresh_monitors(&mut self) {
        self.monitors = self.freerdp.as_ref().map(|f| f.list_monitors()).unwrap_or_default();
    }

    pub fn save_settings(&mut self, mut draft: ClientConfig) {
        // The settings dialog does not edit account selection; keep the live value.
        draft.last_account_id = self.config.last_account_id.clone();
        if !self.monitors.is_empty() {
            draft.monitor_ids.retain(|id| self.monitors.iter().any(|m| m.id == *id));
        }
        draft.clipboard = draft.clipboard_mode.enabled();
        draft.gfx_progressive = draft.gfx_mode != GfxMode::Off;
        if let Err(e) = autostart::apply(draft.autostart, draft.start_minimized) {
            self.error_dialog = Some(e);
        }
        if let Err(e) = paths::write_private(&paths::config_file(), draft.to_json().as_bytes()) {
            self.error_dialog = Some(e);
        }
        if draft.freerdp_path != self.config.freerdp_path {
            self.freerdp = freerdp::locate(&draft.freerdp_path);
            self.refresh_monitors();
            if !self.monitors.is_empty() {
                draft.monitor_ids.retain(|id| self.monitors.iter().any(|m| m.id == *id));
            }
        }
        self.config = draft;
    }

    pub fn redetect_freerdp(&mut self) {
        self.freerdp = freerdp::locate(&self.config.freerdp_path);
        self.refresh_monitors();
        if !self.monitors.is_empty() {
            self.config.monitor_ids.retain(|id| self.monitors.iter().any(|m| m.id == *id));
        }
    }

    pub fn shutdown(&self) {
        for s in self.sessions.values() {
            s.stop();
        }
    }

    fn persist_accounts(&mut self) {
        let list: Vec<AccountInfo> = self.accounts.iter().map(|a| a.info.clone()).collect();
        if let Err(e) = saved_accounts::save(&list) {
            self.set_status(e, Tone::Error);
        }
    }

    fn remember_active(&mut self) {
        if let Some(account) = self.accounts.get(self.active) {
            self.config.last_account_id = account.info.id.clone();
        }
        self.save_config();
    }

    fn save_config(&self) {
        let _ = paths::write_private(&paths::config_file(), self.config.to_json().as_bytes());
    }
}

fn refresh_tokens(http: &Http, current: &TokenSet, account_id: &str, tx: &Sender<Msg>) -> r365_core::Result<TokenSet> {
    let fresh = auth::refresh(http, current)?;
    secret::store(account_id, &fresh)?;
    let _ = tx.send(Msg::Tokens { account_id: account_id.to_string(), tokens: fresh.clone() });
    Ok(fresh)
}

/// Loads every saved account whose token still exists, migrating the single-account files from
/// earlier versions into the first account's per-account slots.
fn load_accounts() -> Vec<Account> {
    let mut accounts = Vec::new();
    for info in saved_accounts::load() {
        if let Some(tokens) = secret::load(&info.id) {
            let workspaces = feed::load_cache(&info.id);
            accounts.push(Account { info, tokens, workspaces, last_sync: None, loading: false });
        }
    }
    if accounts.is_empty()
        && let Some(tokens) = secret::load_legacy()
    {
        let info = AccountInfo::from_claims(&tokens.claims);
        let workspaces = feed::load_cache_legacy();
        // Only remove the old copy after the new one is safely stored. If the keyring and the
        // fallback file both fail, keep the legacy tokens for the next launch to retry.
        if secret::store(&info.id, &tokens).is_ok() {
            secret::clear_legacy();
        }
        feed::save_cache(&info.id, &workspaces);
        feed::clear_cache_legacy();
        let _ = saved_accounts::save(std::slice::from_ref(&info));
        accounts.push(Account { info, tokens, workspaces, last_sync: None, loading: false });
    }
    accounts
}
