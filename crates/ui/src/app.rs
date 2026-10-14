//! Application state and the background work it starts. All I/O runs on plain threads and
//! reports back through channels; `update` drains them once per frame.

use r365_core::auth::{DeviceCode, Poll, TokenSet};
use r365_core::config::ClientConfig;
use r365_core::feed::{Resource, Workspace};
use r365_core::rdp::{DisplayMode, SessionStatus, prepare_profile};
use r365_native::freerdp::{FreeRdp, Session, SessionEvent};
use r365_native::http::Http;
use r365_native::{auth, browser, feed, freerdp, paths, secret};
use std::collections::HashMap;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{Receiver, Sender, channel};
use std::time::{Duration, Instant};

pub const VERSION: &str = env!("CARGO_PKG_VERSION");

pub enum Msg {
    DeviceCode(DeviceCode),
    SignedIn(r365_core::Result<TokenSet>),
    Tokens(TokenSet),
    Workspaces(r365_core::Result<Vec<Workspace>>),
    Launched { key: String, result: r365_core::Result<Session> },
    Status(String),
}

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum Tone {
    Neutral,
    Active,
    Error,
}

pub struct PendingAuth {
    pub key: String,
    pub title: String,
    pub url: String,
    pub paste: String,
}

pub struct App {
    ctx: egui::Context,
    tx: Sender<Msg>,
    rx: Receiver<Msg>,
    session_tx: Sender<(String, SessionEvent)>,
    session_rx: Receiver<(String, SessionEvent)>,
    http: Arc<Http>,
    pub config: ClientConfig,
    pub freerdp: Option<FreeRdp>,
    pub tokens: Option<TokenSet>,
    pub workspaces: Vec<Workspace>,
    pub statuses: HashMap<String, SessionStatus>,
    sessions: HashMap<String, Session>,
    pub pending_auth: Option<PendingAuth>,
    pub device_code: Option<DeviceCode>,
    sign_in_cancel: Arc<AtomicBool>,
    pub signing_in: bool,
    pub loading: bool,
    pub status: String,
    pub tone: Tone,
    pub search: String,
    pub settings_draft: Option<ClientConfig>,
    pub confirm_sign_out: bool,
    pub error_dialog: Option<String>,
    pub last_sync: Option<Instant>,
}

impl App {
    pub fn new(ctx: egui::Context) -> Self {
        let (tx, rx) = channel();
        let (session_tx, session_rx) = channel();
        let config = std::fs::read_to_string(paths::config_file()).ok().and_then(|s| ClientConfig::parse(&s).ok()).unwrap_or_default();
        let freerdp = freerdp::locate(&config.freerdp_path);
        let tokens = secret::load();
        let workspaces = if tokens.is_some() { feed::load_cache() } else { Vec::new() };
        let mut app = Self {
            ctx,
            tx,
            rx,
            session_tx,
            session_rx,
            http: Arc::new(Http::new()),
            config,
            freerdp,
            tokens,
            workspaces,
            statuses: HashMap::new(),
            sessions: HashMap::new(),
            pending_auth: None,
            device_code: None,
            sign_in_cancel: Arc::new(AtomicBool::new(false)),
            signing_in: false,
            loading: false,
            status: String::new(),
            tone: Tone::Neutral,
            search: String::new(),
            settings_draft: None,
            confirm_sign_out: false,
            error_dialog: None,
            last_sync: None,
        };
        if app.tokens.is_some() {
            app.refresh_workspaces();
        } else {
            app.set_status("Sign in to see your Cloud PCs", Tone::Neutral);
        }
        app
    }

    pub fn set_status(&mut self, s: impl Into<String>, tone: Tone) {
        self.status = s.into();
        self.tone = tone;
    }

    pub fn signed_in(&self) -> bool {
        self.tokens.is_some()
    }

    pub fn status_of(&self, key: &str) -> SessionStatus {
        self.statuses.get(key).copied().unwrap_or(SessionStatus::Idle)
    }

    pub fn any_session_busy(&self) -> bool {
        self.statuses.values().any(|s| s.is_busy())
    }

    pub fn resource_count(&self) -> usize {
        self.workspaces.iter().map(|w| w.resources.len()).sum()
    }

    /// Called once per frame before drawing.
    pub fn pump(&mut self) {
        while let Ok(msg) = self.rx.try_recv() {
            self.handle(msg);
        }
        while let Ok((key, event)) = self.session_rx.try_recv() {
            self.handle_session(key, event);
        }
        let due = self.last_sync.is_some_and(|t| t.elapsed() > Duration::from_secs(u64::from(self.config.auto_refresh_minutes.max(1)) * 60));
        if due && !self.loading && self.signed_in() {
            self.refresh_workspaces();
        }
    }

    fn handle(&mut self, msg: Msg) {
        match msg {
            Msg::DeviceCode(code) => {
                self.device_code = Some(code);
                self.set_status("Enter the code in your browser to finish signing in", Tone::Active);
            }
            Msg::SignedIn(Ok(tokens)) => {
                self.signing_in = false;
                self.device_code = None;
                self.tokens = Some(tokens);
                self.set_status("Signed in", Tone::Neutral);
                self.refresh_workspaces();
            }
            Msg::SignedIn(Err(e)) => {
                self.signing_in = false;
                self.device_code = None;
                self.set_status(e, Tone::Error);
            }
            Msg::Tokens(t) => self.tokens = Some(t),
            Msg::Workspaces(Ok(ws)) => {
                self.loading = false;
                self.last_sync = Some(Instant::now());
                self.workspaces = ws;
                let n = self.resource_count();
                self.set_status(
                    if n == 0 {
                        "No Cloud PCs or apps are published to you".into()
                    } else {
                        format!("{n} resource{} available", if n == 1 { "" } else { "s" })
                    },
                    Tone::Neutral,
                );
            }
            Msg::Workspaces(Err(e)) => {
                self.loading = false;
                self.last_sync = Some(Instant::now());
                self.set_status(format!("Refresh failed: {e}"), Tone::Error);
            }
            Msg::Launched { key, result: Ok(session) } => {
                self.sessions.insert(key, session);
            }
            Msg::Launched { key, result: Err(e) } => {
                self.statuses.insert(key, SessionStatus::Failed);
                self.set_status(format!("Connection failed: {e}"), Tone::Error);
                self.error_dialog = Some(e);
            }
            Msg::Status(s) => self.set_status(s, Tone::Active),
        }
    }

    fn handle_session(&mut self, key: String, event: SessionEvent) {
        match event {
            SessionEvent::Status(status, message) => {
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
                let _ = browser::open(&url);
                self.pending_auth = Some(PendingAuth { key, title, url, paste: String::new() });
            }
            SessionEvent::Exited(_) => {
                self.sessions.remove(&key);
                if self.pending_auth.as_ref().is_some_and(|p| p.key == key) {
                    self.pending_auth = None;
                }
            }
        }
    }

    fn title_of(&self, key: &str) -> String {
        self.workspaces.iter().flat_map(|w| &w.resources).find(|r| r.key() == key).map(|r| r.title.clone()).unwrap_or_else(|| "Cloud PC".into())
    }

    fn spawn<F: FnOnce() + Send + 'static>(&self, f: F) {
        let ctx = self.ctx.clone();
        std::thread::spawn(move || {
            f();
            ctx.request_repaint();
        });
    }

    pub fn start_sign_in(&mut self) {
        if self.signing_in {
            return;
        }
        self.signing_in = true;
        self.sign_in_cancel = Arc::new(AtomicBool::new(false));
        self.set_status("Contacting Microsoft", Tone::Active);
        let (http, tx, ctx, cancel) = (self.http.clone(), self.tx.clone(), self.ctx.clone(), self.sign_in_cancel.clone());
        self.spawn(move || {
            let code = match auth::start_device_code(&http) {
                Ok(c) => c,
                Err(e) => {
                    let _ = tx.send(Msg::SignedIn(Err(e)));
                    return;
                }
            };
            let _ = tx.send(Msg::DeviceCode(code.clone()));
            ctx.request_repaint();
            let _ = browser::open(&code.verification_uri);
            let deadline = Instant::now() + Duration::from_secs(code.expires_in);
            let mut interval = code.interval.max(1);
            while Instant::now() < deadline {
                std::thread::sleep(Duration::from_secs(interval));
                if cancel.load(Ordering::SeqCst) {
                    return;
                }
                match auth::poll_device_code(&http, &code) {
                    Poll::Pending => {}
                    Poll::SlowDown => interval += 5,
                    Poll::Done(resp) => {
                        let result = TokenSet::from_response(resp, None, r365_native::now_unix()).and_then(|t| secret::store(&t).map(|()| t));
                        let _ = tx.send(Msg::SignedIn(result));
                        return;
                    }
                    Poll::Failed(e) => {
                        let _ = tx.send(Msg::SignedIn(Err(e)));
                        return;
                    }
                }
            }
            let _ = tx.send(Msg::SignedIn(Err("The sign-in code expired. Start again.".into())));
        });
    }

    pub fn cancel_sign_in(&mut self) {
        self.sign_in_cancel.store(true, Ordering::SeqCst);
        self.signing_in = false;
        self.device_code = None;
        self.set_status("Sign-in cancelled", Tone::Neutral);
    }

    pub fn refresh_workspaces(&mut self) {
        let Some(tokens) = self.tokens.clone() else { return };
        if self.loading {
            return;
        }
        self.loading = true;
        self.set_status("Refreshing workspaces", Tone::Active);
        let (http, tx) = (self.http.clone(), self.tx.clone());
        self.spawn(move || {
            let result = auth::ensure_fresh(&http, &tokens).and_then(|(fresh, changed)| {
                if changed {
                    let _ = tx.send(Msg::Tokens(fresh.clone()));
                }
                feed::fetch_all(&http, &fresh.access_token)
            });
            if let Ok(ws) = &result {
                feed::save_cache(ws);
            }
            let _ = tx.send(Msg::Workspaces(result));
        });
    }

    pub fn connect(&mut self, resource: &Resource, mode: DisplayMode) {
        let key = resource.key();
        let Some(freerdp) = self.freerdp.clone() else {
            self.statuses.insert(key, SessionStatus::Failed);
            self.error_dialog =
                Some("FreeRDP was not found. Install the freerdp package (or the com.freerdp.FreeRDP Flatpak), or set its path in Settings.".into());
            return;
        };
        let Some(tokens) = self.tokens.clone() else { return };
        if let Some(existing) = self.sessions.get(&key) {
            existing.stop();
        }
        self.statuses.insert(key.clone(), SessionStatus::Starting);
        self.set_status(format!("Starting {}", resource.title), Tone::Active);
        let opts = self.config.launch_options(mode);
        let username = tokens.claims.rdp_username();
        let (http, tx, stx, resource) = (self.http.clone(), self.tx.clone(), self.session_tx.clone(), resource.clone());
        self.spawn(move || {
            let result = (|| {
                let (fresh, changed) = auth::ensure_fresh(&http, &tokens)?;
                if changed {
                    let _ = tx.send(Msg::Tokens(fresh.clone()));
                }
                let url = resource.rdp_url.as_deref().ok_or_else(|| "This resource has no remote desktop profile to download".to_string())?;
                let _ = tx.send(Msg::Status("Downloading remote desktop profile".into()));
                let path = feed::download_rdp(&http, &fresh.access_token, url, &resource.file_stem())?;
                let contents = std::fs::read_to_string(&path).map_err(|e| format!("Could not read the downloaded profile: {e}"))?;
                let active = path.with_extension("active.rdp");
                paths::write_private(&active, prepare_profile(&contents, &opts).as_bytes())?;
                let _ = tx.send(Msg::Status(format!("Starting FreeRDP for {}", resource.title)));
                freerdp::launch(&freerdp, &active, &username, &resource.title, &opts, resource.key(), stx)
            })();
            let _ = tx.send(Msg::Launched { key: resource.key(), result });
        });
    }

    pub fn disconnect(&mut self, key: &str) {
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

    pub fn submit_redirect(&mut self) {
        let Some(pending) = self.pending_auth.take() else { return };
        let Some(session) = self.sessions.get(&pending.key) else { return };
        if let Err(e) = session.submit_redirect(&pending.paste) {
            self.error_dialog = Some(e);
        } else {
            self.set_status(format!("Completing sign-in for {}", pending.title), Tone::Active);
        }
    }

    pub fn reopen_auth_url(&self) {
        if let Some(p) = &self.pending_auth {
            let _ = browser::open(&p.url);
        }
    }

    pub fn latest_log(&self, key: &str) -> Option<std::path::PathBuf> {
        self.sessions.get(key).map(|s| s.log_file.clone()).filter(|p| p.exists())
    }

    pub fn sign_out(&mut self) {
        for s in self.sessions.values() {
            s.stop();
        }
        self.sessions.clear();
        self.statuses.clear();
        self.pending_auth = None;
        secret::clear();
        feed::clear_cache();
        self.tokens = None;
        self.workspaces.clear();
        self.last_sync = None;
        self.set_status("Signed out", Tone::Neutral);
    }

    pub fn save_settings(&mut self, draft: ClientConfig) {
        if let Err(e) = paths::write_private(&paths::config_file(), draft.to_json().as_bytes()) {
            self.error_dialog = Some(e);
        }
        if draft.freerdp_path != self.config.freerdp_path {
            self.freerdp = freerdp::locate(&draft.freerdp_path);
        }
        self.config = draft;
    }

    pub fn redetect_freerdp(&mut self) {
        self.freerdp = freerdp::locate(&self.config.freerdp_path);
    }

    pub fn shutdown(&self) {
        for s in self.sessions.values() {
            s.stop();
        }
    }
}
