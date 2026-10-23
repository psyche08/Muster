//! muster-terminal (ARCHITECTURE §3.6.3): the terminal service behind musterd.
//! Sources are iTerm2, Ghostty and the daemon's own PTYs; frames are line
//! deltas.

#[allow(clippy::all, dead_code)]
pub mod proto {
    include!(concat!(env!("OUT_DIR"), "/iterm2.rs"));
}

pub mod commands;
pub mod ghostty;
pub mod grid;
pub mod iterm2;
#[cfg(unix)]
pub mod pty;
#[cfg(unix)]
pub mod runs;
pub mod ssh;
#[cfg(unix)]
pub mod tmux;

use std::path::PathBuf;
use std::sync::Arc;
use std::time::Duration;

use tokio::sync::{Mutex, mpsc};

pub use muster_proto::v1::{TerminalFrame, TerminalLine, TerminalSession};

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("terminal unavailable: {0}")]
    Unavailable(String),
    #[error("not found: {0}")]
    NotFound(String),
    #[error("protocol: {0}")]
    Protocol(String),
    #[error("conflict: {0}")]
    Conflict(String),
}

#[derive(Clone, Debug)]
pub struct TerminalConfig {
    pub socket: PathBuf,
    pub app_name: String,
    /// Apple Events target for the cookie request.
    pub iterm_app: String,
}

impl Default for TerminalConfig {
    fn default() -> Self {
        Self {
            socket: iterm2::default_socket(),
            app_name: "musterd".into(),
            iterm_app: "iTerm2".into(),
        }
    }
}

/// The service: connects to iTerm2 lazily, reconnects when it goes away.
pub struct TerminalService {
    config: TerminalConfig,
    connection: Mutex<Option<Arc<iterm2::Iterm2>>>,
}

impl TerminalService {
    pub fn new(config: TerminalConfig) -> Arc<Self> {
        Arc::new(Self {
            config,
            connection: Mutex::new(None),
        })
    }

    pub fn available(&self) -> bool {
        self.config.socket.exists()
    }

    async fn connection(&self) -> Result<Arc<iterm2::Iterm2>, Error> {
        let mut slot = self.connection.lock().await;
        if let Some(existing) = slot.as_ref()
            && existing.sessions().await.is_ok()
        {
            return Ok(existing.clone());
        }
        if !self.available() {
            return Err(Error::Unavailable(
                "iTerm2 is not running or its API is disabled".into(),
            ));
        }
        let cookie = match iterm2::Cookie::from_env() {
            Some(cookie) => cookie,
            None => iterm2::Cookie::request(&self.config.iterm_app, &self.config.app_name).await?,
        };
        let connection = tokio::time::timeout(
            Duration::from_secs(10),
            iterm2::Iterm2::connect(&self.config.socket, &cookie, &self.config.app_name),
        )
        .await
        .map_err(|_| Error::Unavailable("iTerm2 handshake timed out".into()))??;
        *slot = Some(connection.clone());
        Ok(connection)
    }

    pub async fn sessions(&self) -> Result<Vec<TerminalSession>, Error> {
        self.connection().await?.sessions().await
    }

    /// Pushes session-list changes until the receiver is dropped.
    pub async fn watch_sessions(
        &self,
        out: mpsc::Sender<Vec<TerminalSession>>,
    ) -> Result<(), Error> {
        let connection = self.connection().await?;
        connection
            .subscribe(proto::NotificationType::NotifyOnNewSession, None)
            .await?;
        connection
            .subscribe(proto::NotificationType::NotifyOnTerminateSession, None)
            .await?;
        connection
            .subscribe(proto::NotificationType::NotifyOnLayoutChange, None)
            .await?;
        let mut notifications = connection.subscribe_notifications();
        if out.send(connection.sessions().await?).await.is_err() {
            return Ok(());
        }
        loop {
            match notifications.recv().await {
                Ok(n) => {
                    if n.new_session_notification.is_some()
                        || n.terminate_session_notification.is_some()
                        || n.layout_changed_notification.is_some()
                    {
                        let sessions = connection.sessions().await?;
                        if out.send(sessions).await.is_err() {
                            return Ok(());
                        }
                    }
                }
                Err(tokio::sync::broadcast::error::RecvError::Lagged(_)) => continue,
                Err(_) => return Ok(()),
            }
        }
    }

    pub async fn attach(
        &self,
        session: &str,
        since_rev: u64,
        frames: mpsc::Sender<TerminalFrame>,
    ) -> Result<(), Error> {
        let connection = self.connection().await?;
        connection.attach(session, since_rev, frames).await
    }

    pub async fn input(&self, session: &str, bytes: &[u8]) -> Result<(), Error> {
        let text = String::from_utf8_lossy(bytes);
        self.connection().await?.send_text(session, &text).await
    }

    pub async fn create(
        &self,
        profile: Option<&str>,
        background: bool,
    ) -> Result<TerminalSession, Error> {
        self.connection()
            .await?
            .create_tab(profile, background)
            .await
    }

    pub async fn resize(&self, session: &str, cols: u32, rows: u32) -> Result<(), Error> {
        self.connection().await?.resize(session, cols, rows).await
    }

    pub async fn close(&self, session: &str) -> Result<(), Error> {
        self.connection().await?.close(session).await
    }

    pub async fn history(&self, session: &str, lines: u32) -> Result<Vec<TerminalLine>, Error> {
        let (lines, _) = self
            .connection()
            .await?
            .buffer(session, Some(lines.max(1) as i32))
            .await?;
        Ok(lines
            .into_iter()
            .enumerate()
            .map(|(i, text)| TerminalLine {
                index: i as u32,
                text,
                styles: Vec::new(),
            })
            .collect())
    }
}

/// What changed between two screen snapshots: `None` when nothing did,
/// the changed rows when rows were only rewritten or appended, or a full
/// frame when the screen shrank (a client cannot drop rows from a delta).
pub struct LineDelta {
    pub full: bool,
    pub lines: Vec<TerminalLine>,
}

pub fn line_delta(previous: &[String], current: &[String]) -> Option<LineDelta> {
    let line = |(i, text): (usize, &String)| TerminalLine {
        index: i as u32,
        text: text.clone(),
        styles: Vec::new(),
    };
    if current.len() < previous.len() {
        return Some(LineDelta {
            full: true,
            lines: current.iter().enumerate().map(line).collect(),
        });
    }
    let changed: Vec<TerminalLine> = current
        .iter()
        .enumerate()
        .filter(|(i, text)| previous.get(*i) != Some(text))
        .map(line)
        .collect();
    if changed.is_empty() {
        return None;
    }
    Some(LineDelta {
        full: false,
        lines: changed,
    })
}

/// Which terminal source a daemon offers.
#[derive(Clone, Debug)]
pub enum TerminalKind {
    Iterm2(TerminalConfig),
    Ghostty(GhosttyPlatform),
    #[cfg(unix)]
    Pty(pty::PtyConfig),
}

/// The platform's Ghostty adapter (macOS).
#[derive(Clone)]
pub struct GhosttyPlatform(pub Arc<dyn ghostty::GhosttyOs>);

impl std::fmt::Debug for GhosttyPlatform {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("GhosttyPlatform")
    }
}

/// The terminal service behind musterd, whichever source backs it.
pub enum Terminal {
    Iterm2(Arc<TerminalService>),
    Ghostty(Arc<ghostty::GhosttyService>),
    #[cfg(unix)]
    Pty(Arc<pty::PtyService>),
    #[cfg(unix)]
    Tmux(Arc<tmux::TmuxService>),
}

impl Terminal {
    pub fn new(kind: TerminalKind) -> Arc<Terminal> {
        Arc::new(match kind {
            TerminalKind::Iterm2(config) => Terminal::Iterm2(TerminalService::new(config)),
            TerminalKind::Ghostty(os) => Terminal::Ghostty(ghostty::GhosttyService::new(os.0)),
            #[cfg(unix)]
            TerminalKind::Pty(config) => Terminal::Pty(pty::PtyService::new(config)),
        })
    }

    pub fn source(&self) -> &'static str {
        match self {
            Terminal::Iterm2(_) => "iterm2",
            Terminal::Ghostty(_) => "ghostty",
            #[cfg(unix)]
            Terminal::Pty(_) => "pty",
            #[cfg(unix)]
            Terminal::Tmux(_) => "tmux",
        }
    }

    pub fn available(&self) -> bool {
        match self {
            Terminal::Iterm2(t) => t.available(),
            Terminal::Ghostty(t) => t.available(),
            #[cfg(unix)]
            Terminal::Pty(t) => t.available(),
            #[cfg(unix)]
            Terminal::Tmux(t) => t.available(),
        }
    }

    pub async fn sessions(&self) -> Result<Vec<TerminalSession>, Error> {
        match self {
            Terminal::Iterm2(t) => t.sessions().await,
            Terminal::Ghostty(t) => t.sessions().await,
            #[cfg(unix)]
            Terminal::Pty(t) => t.sessions().await,
            #[cfg(unix)]
            Terminal::Tmux(t) => t.sessions().await,
        }
    }

    pub async fn watch_sessions(
        &self,
        out: mpsc::Sender<Vec<TerminalSession>>,
    ) -> Result<(), Error> {
        match self {
            Terminal::Iterm2(t) => t.watch_sessions(out).await,
            Terminal::Ghostty(t) => t.watch_sessions(out).await,
            #[cfg(unix)]
            Terminal::Pty(t) => t.watch_sessions(out).await,
            #[cfg(unix)]
            Terminal::Tmux(t) => t.watch_sessions(out).await,
        }
    }

    pub async fn attach(
        &self,
        session: &str,
        since_rev: u64,
        frames: mpsc::Sender<TerminalFrame>,
    ) -> Result<(), Error> {
        match self {
            Terminal::Iterm2(t) => t.attach(session, since_rev, frames).await,
            Terminal::Ghostty(t) => t.attach(session, since_rev, frames).await,
            #[cfg(unix)]
            Terminal::Pty(t) => t.attach(session, since_rev, frames).await,
            #[cfg(unix)]
            Terminal::Tmux(t) => t.attach(session, since_rev, frames).await,
        }
    }

    pub async fn input(&self, session: &str, bytes: &[u8]) -> Result<(), Error> {
        match self {
            Terminal::Iterm2(t) => t.input(session, bytes).await,
            Terminal::Ghostty(t) => t.input(session, bytes).await,
            #[cfg(unix)]
            Terminal::Pty(t) => t.input(session, bytes).await,
            #[cfg(unix)]
            Terminal::Tmux(t) => t.input(session, bytes).await,
        }
    }

    pub async fn create(
        &self,
        profile: Option<&str>,
        background: bool,
    ) -> Result<TerminalSession, Error> {
        match self {
            Terminal::Iterm2(t) => t.create(profile, background).await,
            Terminal::Ghostty(t) => t.create().await,
            #[cfg(unix)]
            Terminal::Pty(t) => t.create(profile, background).await,
            #[cfg(unix)]
            Terminal::Tmux(_) => Err(Error::Protocol("Open tmux from an agent session".into())),
        }
    }

    pub async fn resize(&self, session: &str, cols: u32, rows: u32) -> Result<(), Error> {
        match self {
            Terminal::Iterm2(t) => t.resize(session, cols, rows).await,
            // A Ghostty window keeps the size it has on the Mac.
            Terminal::Ghostty(_) => Ok(()),
            #[cfg(unix)]
            Terminal::Pty(t) => t.resize(session, cols, rows).await,
            #[cfg(unix)]
            Terminal::Tmux(t) => t.resize(session, cols, rows).await,
        }
    }

    pub async fn close(&self, session: &str) -> Result<(), Error> {
        match self {
            Terminal::Iterm2(t) => t.close(session).await,
            Terminal::Ghostty(t) => t.close(session).await,
            #[cfg(unix)]
            Terminal::Pty(t) => t.close(session).await,
            #[cfg(unix)]
            Terminal::Tmux(t) => t.close(session).await,
        }
    }

    pub async fn history(&self, session: &str, lines: u32) -> Result<Vec<TerminalLine>, Error> {
        match self {
            Terminal::Iterm2(t) => t.history(session, lines).await,
            Terminal::Ghostty(t) => t.history(session, lines).await,
            #[cfg(unix)]
            Terminal::Pty(t) => t.history(session, lines).await,
            #[cfg(unix)]
            Terminal::Tmux(t) => t.history(session, lines).await,
        }
    }
}

/// One captured screen line.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CapturedLine {
    pub text: String,
    /// The line ends here (not wrapped into the next one).
    pub hard_eol: bool,
}

/// A screen capture with the cursor and the job behind the session, taken
/// in one round trip; what the CLI controller compares before writing.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Capture {
    pub lines: Vec<CapturedLine>,
    /// Cursor column, and row relative to `lines[0]`.
    pub cursor: (usize, usize),
    pub job_pid: u32,
    pub tty: String,
}

impl TerminalService {
    pub async fn capture(&self, session: &str) -> Result<Capture, Error> {
        self.connection().await?.capture(session).await
    }

    pub async fn send_if_unchanged(
        &self,
        session: &str,
        expected: &Capture,
        text: &str,
    ) -> Result<(), Error> {
        self.connection()
            .await?
            .send_if_unchanged(session, expected, text)
            .await
    }

    /// Session variables such as "session.jobPid".
    pub async fn variables(
        &self,
        session: &str,
        names: &[&str],
    ) -> Result<std::collections::HashMap<String, serde_json::Value>, Error> {
        self.connection().await?.variables(session, names).await
    }
}

/// The largest paste sent as terminal input, in bytes of text.
pub const MAX_PASTE: usize = 64 * 1024;

/// Pasted text to the bytes a terminal receives. Line breaks (CRLF, LF, CR)
/// become Enter (CR); tab stays; every other control character (C0, DEL,
/// C1, including ESC) is dropped, so a paste cannot carry escape sequences,
/// end a bracketed paste early or press keys. With `bracketed` (the program
/// enabled DECSET 2004) the text is wrapped in `ESC[200~` … `ESC[201~`.
pub fn paste_bytes(text: &str, bracketed: bool) -> Vec<u8> {
    let mut out = String::with_capacity(text.len() + 12);
    if bracketed {
        out.push_str("\x1b[200~");
    }
    let mut chars = text.chars().peekable();
    while let Some(c) = chars.next() {
        match c {
            '\r' => {
                chars.next_if_eq(&'\n');
                out.push('\r');
            }
            '\n' => out.push('\r'),
            '\t' => out.push('\t'),
            c if c.is_control() => {}
            c => out.push(c),
        }
    }
    if bracketed {
        out.push_str("\x1b[201~");
    }
    out.into_bytes()
}

/// Key names from the shells to the bytes a terminal expects (§3.6.3: the
/// mapping lives here so no platform re-derives it).
///
/// A name is a key, optionally after `ctrl-`, `alt-` and `shift-` prefixes
/// in any order: a named key (`enter`, `up`, `f5`, …) or one character
/// (`ctrl-c`, `alt-b`, `ctrl-alt-x`). Ctrl folds a letter or `@[\]^_?` and
/// space to its control byte; alt prefixes ESC; modified cursor, editing and
/// function keys use xterm's `CSI 1;m` / `CSI n;m~` forms.
pub fn key_bytes(key: &str) -> Option<Vec<u8>> {
    let (mut ctrl, mut alt, mut shift) = (false, false, false);
    let mut rest = key;
    loop {
        if let Some(r) = rest.strip_prefix("ctrl-").filter(|r| !r.is_empty()) {
            ctrl = true;
            rest = r;
        } else if let Some(r) = rest.strip_prefix("alt-").filter(|r| !r.is_empty()) {
            alt = true;
            rest = r;
        } else if let Some(r) = rest.strip_prefix("shift-").filter(|r| !r.is_empty()) {
            shift = true;
            rest = r;
        } else {
            break;
        }
    }
    let modifier = 1 + u8::from(shift) + 2 * u8::from(alt) + 4 * u8::from(ctrl);
    let csi = |final_byte: char| -> Vec<u8> {
        if modifier == 1 {
            format!("\x1b[{final_byte}").into_bytes()
        } else {
            format!("\x1b[1;{modifier}{final_byte}").into_bytes()
        }
    };
    let tilde = |n: u8| -> Vec<u8> {
        if modifier == 1 {
            format!("\x1b[{n}~").into_bytes()
        } else {
            format!("\x1b[{n};{modifier}~").into_bytes()
        }
    };
    let ss3 = |final_byte: char| -> Vec<u8> {
        if modifier == 1 {
            format!("\x1bO{final_byte}").into_bytes()
        } else {
            format!("\x1b[1;{modifier}{final_byte}").into_bytes()
        }
    };
    let esc = |bytes: &[u8]| -> Vec<u8> {
        let mut out = Vec::with_capacity(bytes.len() + 1);
        if alt {
            out.push(0x1b);
        }
        out.extend_from_slice(bytes);
        out
    };
    Some(match rest {
        "up" => csi('A'),
        "down" => csi('B'),
        "right" => csi('C'),
        "left" => csi('D'),
        "home" => csi('H'),
        "end" => csi('F'),
        "insert" => tilde(2),
        "delete" => tilde(3),
        "pageup" => tilde(5),
        "pagedown" => tilde(6),
        "f1" => ss3('P'),
        "f2" => ss3('Q'),
        "f3" => ss3('R'),
        "f4" => ss3('S'),
        "f5" => tilde(15),
        "f6" => tilde(17),
        "f7" => tilde(18),
        "f8" => tilde(19),
        "f9" => tilde(20),
        "f10" => tilde(21),
        "f11" => tilde(23),
        "f12" => tilde(24),
        "tab" if shift && !ctrl => esc(b"\x1b[Z"),
        "tab" => esc(b"\t"),
        "enter" => esc(b"\r"),
        "escape" => esc(b"\x1b"),
        "backspace" if ctrl => esc(b"\x08"),
        "backspace" => esc(b"\x7f"),
        "space" if ctrl => esc(b"\x00"),
        "space" => esc(b" "),
        _ => {
            let mut chars = rest.chars();
            let c = chars.next()?;
            if chars.next().is_some() {
                return None;
            }
            if ctrl {
                let byte = match c.to_ascii_lowercase() {
                    l @ 'a'..='z' => l as u8 & 0x1f,
                    '@' | ' ' | '2' => 0x00,
                    '[' | '3' => 0x1b,
                    '\\' | '4' => 0x1c,
                    ']' | '5' => 0x1d,
                    '^' | '6' => 0x1e,
                    '_' | '/' | '7' => 0x1f,
                    '?' | '8' => 0x7f,
                    _ => return None,
                };
                esc(&[byte])
            } else {
                let mut buf = [0u8; 4];
                esc(c.encode_utf8(&mut buf).as_bytes())
            }
        }
    })
}

#[cfg(test)]
mod tests {
    #[test]
    fn pasted_text_becomes_input_without_control_characters() {
        assert_eq!(paste_bytes("ls -la\n", false), b"ls -la\r");
        assert_eq!(paste_bytes("a\r\nb\rc\nd", false), b"a\rb\rc\rd");
        assert_eq!(paste_bytes("x\ty", false), b"x\ty");
        // ESC, other C0 controls, DEL and C1 never reach the terminal: a
        // paste cannot close the bracket or press keys.
        assert_eq!(
            paste_bytes("rm -rf /\x1b[201~\x03\x7f\u{9b}2J", true),
            b"\x1b[200~rm -rf /[201~2J\x1b[201~"
        );
        assert_eq!(paste_bytes("日本語 ✓", false), "日本語 ✓".as_bytes());
        assert_eq!(paste_bytes("", true), b"\x1b[200~\x1b[201~");
    }

    use super::*;

    fn rows(v: &[&str]) -> Vec<String> {
        v.iter().map(|s| s.to_string()).collect()
    }

    #[test]
    fn delta_reports_only_changed_rows() {
        let a = rows(&["$ ls", "a b c", ""]);
        let b = rows(&["$ ls", "a b c", "$ "]);
        let d = line_delta(&a, &b).unwrap();
        assert!(!d.full);
        assert_eq!(d.lines.len(), 1);
        assert_eq!(d.lines[0].index, 2);
        assert_eq!(d.lines[0].text, "$ ");
        assert!(line_delta(&b, &b).is_none());
    }

    #[test]
    fn delta_is_full_when_the_screen_shrinks_and_partial_when_it_grows() {
        let a = rows(&["1", "2", "3"]);
        let b = rows(&["1", "2"]);
        let d = line_delta(&a, &b).unwrap();
        assert!(d.full);
        assert_eq!(d.lines.len(), 2);
        let c = rows(&["1", "2", "3", "4"]);
        let d = line_delta(&a, &c).unwrap();
        assert!(!d.full);
        assert_eq!(d.lines.iter().map(|l| l.index).collect::<Vec<_>>(), vec![3]);
    }

    #[test]
    fn key_names_map_to_bytes() {
        let k = |name: &str| key_bytes(name).map(|b| String::from_utf8_lossy(&b).into_owned());
        assert_eq!(key_bytes("enter").as_deref(), Some(&b"\r"[..]));
        assert_eq!(key_bytes("ctrl-c").as_deref(), Some(&b"\x03"[..]));
        assert_eq!(key_bytes("nope"), None);
        assert_eq!(key_bytes("ctrl-"), None);
        assert_eq!(key_bytes("ctrl-é"), None);
        // Every control letter, not only the ones the old table listed.
        assert_eq!(key_bytes("ctrl-b").as_deref(), Some(&b"\x02"[..]));
        assert_eq!(key_bytes("ctrl-X").as_deref(), Some(&b"\x18"[..]));
        assert_eq!(key_bytes("ctrl-[").as_deref(), Some(&b"\x1b"[..]));
        assert_eq!(key_bytes("ctrl-space").as_deref(), Some(&b"\x00"[..]));
        // Alt is an ESC prefix, also combined with ctrl.
        assert_eq!(key_bytes("alt-b").as_deref(), Some(&b"\x1bb"[..]));
        assert_eq!(key_bytes("ctrl-alt-x").as_deref(), Some(&b"\x1b\x18"[..]));
        assert_eq!(
            key_bytes("alt-backspace").as_deref(),
            Some(&b"\x1b\x7f"[..])
        );
        assert_eq!(key_bytes("alt-中").as_deref(), Some("\x1b中".as_bytes()));
        // Function keys, plain and modified (xterm).
        assert_eq!(k("f1").as_deref(), Some("\x1bOP"));
        assert_eq!(k("f5").as_deref(), Some("\x1b[15~"));
        assert_eq!(k("f12").as_deref(), Some("\x1b[24~"));
        assert_eq!(k("ctrl-f1").as_deref(), Some("\x1b[1;5P"));
        assert_eq!(k("shift-f5").as_deref(), Some("\x1b[15;2~"));
        assert_eq!(k("ctrl-left").as_deref(), Some("\x1b[1;5D"));
        assert_eq!(k("alt-up").as_deref(), Some("\x1b[1;3A"));
        assert_eq!(k("ctrl-shift-end").as_deref(), Some("\x1b[1;6F"));
        assert_eq!(k("shift-tab").as_deref(), Some("\x1b[Z"));
        assert_eq!(k("escape").as_deref(), Some("\x1b"));
    }
}
