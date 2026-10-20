//! Finding a FreeRDP 3 client and running a session under a pseudo-terminal.
//!
//! FreeRDP only prompts for Entra ID session auth ("Browse to: ...") when stdout is a terminal,
//! so the client is run through `script`, which gives it one and copies its output to us.

use r365_core::rdp::{self, LaunchOptions, SessionStatus};
use std::io::{BufRead, BufReader, Write};
use std::path::{Path, PathBuf};
use std::process::{Child, ChildStdin, Command, Stdio};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::Sender;
use std::sync::{Arc, Mutex};

const CANDIDATES: [&str; 6] = ["sdl-freerdp3", "sdl-freerdp", "xfreerdp3", "xfreerdp", "wlfreerdp3", "wlfreerdp"];
const FLATPAK_ID: &str = "com.freerdp.FreeRDP";
const MAX_LOG_BYTES: u64 = 2 * 1024 * 1024;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FreeRdp {
    /// None means the Flatpak.
    pub path: Option<PathBuf>,
    pub version: String,
}

impl FreeRdp {
    pub fn display_name(&self) -> String {
        match &self.path {
            Some(p) => format!("{} {}", p.file_name().and_then(|n| n.to_str()).unwrap_or("FreeRDP"), self.version),
            None => format!("Flatpak FreeRDP {}", self.version),
        }
    }

    /// The full argv for one session.
    fn argv(&self, args: Vec<String>) -> Vec<String> {
        match &self.path {
            Some(p) => std::iter::once(p.to_string_lossy().into_owned()).chain(args).collect(),
            None => {
                let mut v: Vec<String> = ["flatpak", "run", "--file-forwarding", FLATPAK_ID, "@@"].iter().map(|s| s.to_string()).collect();
                let mut it = args.into_iter();
                if let Some(file) = it.next() {
                    v.push(file);
                }
                v.push("@@".into());
                v.extend(it);
                v
            }
        }
    }
}

/// `preference` is a binary path, "flatpak", or empty for detection (PATH first, Flatpak second).
pub fn locate(preference: &str) -> Option<FreeRdp> {
    let pref = preference.trim();
    if pref.eq_ignore_ascii_case("flatpak") || pref.contains(FLATPAK_ID) {
        return flatpak();
    }
    if !pref.is_empty() {
        let p = PathBuf::from(pref);
        return is_executable(&p).then(|| inspect(p));
    }
    if let Some(env) = std::env::var_os("R365_FREERDP").map(PathBuf::from).filter(|p| is_executable(p)) {
        return Some(inspect(env));
    }
    CANDIDATES.iter().find_map(|c| find_on_path(c)).map(inspect).or_else(flatpak)
}

fn is_executable(p: &Path) -> bool {
    use std::os::unix::fs::PermissionsExt;
    p.is_file() && p.metadata().is_ok_and(|m| m.permissions().mode() & 0o111 != 0)
}

fn find_on_path(name: &str) -> Option<PathBuf> {
    std::env::var_os("PATH").and_then(|paths| std::env::split_paths(&paths).map(|d| d.join(name)).find(|p| is_executable(p)))
}

fn inspect(path: PathBuf) -> FreeRdp {
    let version =
        Command::new(&path).arg("/version").output().ok().and_then(|o| extract_version(&String::from_utf8_lossy(&o.stdout))).unwrap_or_default();
    FreeRdp { path: Some(path), version }
}

fn flatpak() -> Option<FreeRdp> {
    let out = Command::new("flatpak").args(["info", FLATPAK_ID]).output().ok().filter(|o| o.status.success())?;
    Some(FreeRdp { path: None, version: extract_version(&String::from_utf8_lossy(&out.stdout)).unwrap_or_default() })
}

/// "3.32.1" out of "This is FreeRDP version 3.32.1 (n/a)" or a Flatpak "Version: 3.32.1" line.
fn extract_version(text: &str) -> Option<String> {
    text.split(|c: char| c.is_whitespace() || c == '(' || c == ')')
        .find(|w| {
            let w = w.trim_start_matches('v');
            w.split('.').count() >= 2 && w.split('.').all(|p| !p.is_empty() && p.chars().all(|c| c.is_ascii_digit()))
        })
        .map(|w| w.trim_start_matches('v').to_string())
}

/// Writes the SDL client key bindings once (F12 disconnect, F11 minimise, F10 fullscreen) so
/// the help text is true. Never overwrites a file the user already has.
pub fn ensure_sdl_config() {
    let content = "{\n  \"SDL_KeyModMask\": [\"KMOD_RCTRL\"],\n  \"SDL_Disconnect\": [\"SDL_SCANCODE_F12\"],\n  \"SDL_Minimize\": [\"SDL_SCANCODE_F11\"],\n  \"SDL_Fullscreen\": [\"SDL_SCANCODE_F10\"]\n}\n";
    let home = std::env::var_os("HOME").map(PathBuf::from);
    let mut bases: Vec<PathBuf> = std::env::var_os("XDG_CONFIG_HOME").map(PathBuf::from).into_iter().collect();
    if let Some(h) = home {
        bases.push(h.join(".config"));
        bases.push(h.join(".var/app").join(FLATPAK_ID).join("config"));
    }
    for base in bases.into_iter().filter(|b| b.is_dir()) {
        let dir = base.join("freerdp");
        let file = dir.join("sdl-freerdp.json");
        if !file.exists() && std::fs::create_dir_all(&dir).is_ok() {
            let _ = std::fs::write(&file, content);
        }
    }
}

/// What a running session tells the app.
#[derive(Debug, Clone)]
pub enum SessionEvent {
    Status(SessionStatus, String),
    /// FreeRDP wants an Entra ID redirect URL for this authorize URL.
    AuthRequired(String),
    Exited(i32),
}

/// Handle to a running FreeRDP process. Cheap to clone.
#[derive(Clone)]
pub struct Session {
    child: Arc<Mutex<Child>>,
    stdin: Arc<Mutex<Option<ChildStdin>>>,
    user_stopped: Arc<AtomicBool>,
    pub log_file: PathBuf,
}

impl Session {
    /// Answers a "Browse to" prompt with the URL the browser landed on.
    pub fn submit_redirect(&self, url: &str) -> crate::Result<()> {
        let mut guard = self.stdin.lock().map_err(|_| "Session input is poisoned".to_string())?;
        let stdin = guard.as_mut().ok_or_else(|| "Session is no longer accepting input".to_string())?;
        stdin
            .write_all(format!("{}\n", url.trim()).as_bytes())
            .and_then(|()| stdin.flush())
            .map_err(|e| format!("Could not pass the sign-in result to FreeRDP: {e}"))
    }

    pub fn stop(&self) {
        self.user_stopped.store(true, Ordering::SeqCst);
        if let Ok(mut child) = self.child.lock() {
            let _ = child.kill();
        }
    }
}

/// What one session launch needs besides the options.
pub struct LaunchRequest<'a> {
    pub rdp_file: &'a Path,
    pub username: &'a str,
    pub title: &'a str,
    /// Access token for the AVD gateway scope; see `core::rdp::build_args`.
    pub gateway_token: Option<&'a str>,
    /// The resource key every event is tagged with.
    pub key: String,
}

/// Starts FreeRDP for one resource. Events arrive on `tx` tagged with the request's key from a
/// reader thread until the process exits.
pub fn launch(freerdp: &FreeRdp, req: LaunchRequest<'_>, opts: &LaunchOptions, tx: Sender<(String, SessionEvent)>) -> crate::Result<Session> {
    let LaunchRequest { rdp_file, username, title, gateway_token, key } = req;
    ensure_sdl_config();
    let args = rdp::build_args(&rdp_file.to_string_lossy(), username, title, opts, gateway_token)?;
    let argv = freerdp.argv(args);
    let mut cmd = if is_executable(Path::new("/usr/bin/script")) {
        let joined = argv.iter().map(|a| format!("'{}'", a.replace('\'', "'\\''"))).collect::<Vec<_>>().join(" ");
        let mut c = Command::new("/usr/bin/script");
        c.args(["-q", "-c", &joined, "/dev/null"]);
        c
    } else {
        let mut c = Command::new(argv.first().cloned().unwrap_or_default());
        c.args(argv.iter().skip(1));
        c
    };
    cmd.stdin(Stdio::piped()).stdout(Stdio::piped()).stderr(Stdio::piped());
    cmd.env("SDL_RENDER_VSYNC", "1").env("SDL_VIDEO_DOUBLE_BUFFER", "1");
    if std::env::var_os("WAYLAND_DISPLAY").is_some() {
        // Under Wayland the SDL client is steadier on XWayland; see docs/freerdp.md.
        cmd.env("SDL_VIDEODRIVER", if std::env::var_os("DISPLAY").is_some() { "x11" } else { "wayland,x11" });
    }
    if std::env::var_os("PULSE_SERVER").is_none()
        && let Some(runtime) = std::env::var_os("XDG_RUNTIME_DIR")
        && Path::new(&runtime).join("pulse/native").exists()
    {
        cmd.env("PULSE_SERVER", format!("unix:{}/pulse/native", runtime.to_string_lossy()));
    }
    let mut child = cmd.spawn().map_err(|e| format!("FreeRDP could not start: {e}"))?;
    let stdout = child.stdout.take().ok_or_else(|| "FreeRDP output is not readable".to_string())?;
    let stderr = child.stderr.take();
    let stdin = child.stdin.take();
    let log_file = crate::paths::logs_dir().join(format!("session_{}_{}.log", sanitize(&key), crate::now_unix()));
    let session = Session {
        child: Arc::new(Mutex::new(child)),
        stdin: Arc::new(Mutex::new(stdin)),
        user_stopped: Arc::new(AtomicBool::new(false)),
        log_file: log_file.clone(),
    };
    let _ = tx.send((key.clone(), SessionEvent::Status(SessionStatus::Connecting, format!("Connecting to {title}"))));
    if let Some(err) = stderr {
        std::thread::spawn(move || for _ in BufReader::new(err).lines() {});
    }
    let watcher = session.clone();
    let title = title.to_string();
    std::thread::Builder::new()
        .name(format!("rdp-{}", sanitize(&key)))
        .spawn(move || watch(watcher, stdout, &log_file, &title, key, tx))
        .map_err(|e| format!("Could not start the session watcher: {e}"))?;
    Ok(session)
}

fn watch(session: Session, stdout: std::process::ChildStdout, log_file: &Path, title: &str, key: String, tx: Sender<(String, SessionEvent)>) {
    let mut log = std::fs::File::create(log_file).ok();
    let mut written: u64 = 0;
    let mut log_line = |line: &str| {
        if let Some(f) = log.as_mut() {
            let redacted = rdp::redact(line);
            if written + (redacted.len() as u64) < MAX_LOG_BYTES {
                let _ = writeln!(f, "{redacted}");
                written += redacted.len() as u64 + 1;
            }
        }
    };
    log_line(&format!("=== R365 session log for {title} ==="));
    let mut connected = false;
    for line in BufReader::new(stdout).lines().map_while(Result::ok) {
        log_line(&line);
        if let Some(url) = rdp::auth_url_from_line(&line) {
            let _ = tx.send((key.clone(), SessionEvent::Status(SessionStatus::Connecting, "Waiting for Entra ID sign-in".into())));
            let _ = tx.send((key.clone(), SessionEvent::AuthRequired(url)));
        } else if rdp::is_reconnecting_marker(&line) {
            connected = false;
            let _ = tx.send((key.clone(), SessionEvent::Status(SessionStatus::Reconnecting, "Reconnecting".into())));
        } else if !connected && rdp::is_connected_marker(&line) {
            connected = true;
            let _ = tx.send((key.clone(), SessionEvent::Status(SessionStatus::Connected, "Connected".into())));
        }
    }
    let code = session.child.lock().ok().and_then(|mut c| c.wait().ok()).and_then(|s| s.code()).unwrap_or(-1);
    log_line(&format!("=== exited with code {code} ==="));
    let clean = session.user_stopped.load(Ordering::SeqCst) || matches!(code, 0 | 129 | 130 | 143);
    let status = if clean {
        (SessionStatus::Disconnected, "Session disconnected".to_string())
    } else {
        (SessionStatus::Failed, format!("Session exited with code {code}"))
    };
    let _ = tx.send((key.clone(), SessionEvent::Status(status.0, status.1)));
    let _ = tx.send((key, SessionEvent::Exited(code)));
    crate::paths::prune_logs(10);
}

fn sanitize(s: &str) -> String {
    s.chars().map(|c| if c.is_ascii_alphanumeric() || matches!(c, '.' | '_' | '-') { c } else { '_' }).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn version_parsing() {
        assert_eq!(extract_version("This is FreeRDP version 3.32.1 (n/a)").as_deref(), Some("3.32.1"));
        assert_eq!(extract_version("Version: 3.31\nBranch: stable").as_deref(), Some("3.31"));
        assert_eq!(extract_version("nothing"), None);
    }

    #[test]
    fn flatpak_argv_wraps_the_file() {
        let f = FreeRdp { path: None, version: String::new() };
        let v = f.argv(vec!["/x.rdp".into(), "/f".into()]);
        assert_eq!(v, ["flatpak", "run", "--file-forwarding", FLATPAK_ID, "@@", "/x.rdp", "@@", "/f"]);
    }
}
