//! Ghostty's terminals. Ghostty has no API like iTerm2's: no socket, no
//! screen notifications. The platform lists its terminals and types into them
//! through AppleScript, which works whether or not the console is locked, and
//! reads a terminal's screen through accessibility, which the OS offers only
//! while the console is unlocked (on a locked Mac: inside a Locked Use
//! terminal session). The screen is polled; frames are line deltas.
use std::sync::{Arc, Mutex};
use std::time::Duration;

use tokio::sync::mpsc;

use crate::{Error, TerminalFrame, TerminalLine, TerminalSession, line_delta};

/// How often an attached screen is read. Ghostty caches the text it gives
/// accessibility for 500 ms, so faster reads see nothing newer.
const POLL: Duration = Duration::from_millis(300);
/// How often the terminal list is read for a watch.
const LIST_POLL: Duration = Duration::from_secs(2);
/// Rows of a screen sent to a viewer: the newest ones, scrollback included.
pub const MAX_ROWS: usize = 300;

/// One terminal as Ghostty's scripting lists it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Listed {
    /// Ghostty's stable id for the surface (a UUID).
    pub id: String,
    pub title: String,
    pub cwd: String,
    pub window: String,
    pub tab: String,
}

/// A terminal's screen, opened for reading.
pub trait Surface: Send {
    /// The screen text, scrollback included (the newest part when it is
    /// long). `None` when the terminal is gone.
    fn text(&mut self) -> Result<Option<String>, Error>;
}

/// What the platform offers. Every call blocks (Apple Events, accessibility).
pub trait GhosttyOs: Send + Sync {
    /// Ghostty is running (asking never launches it).
    fn running(&self) -> bool;
    fn terminals(&self) -> Result<Vec<Listed>, Error>;
    /// A Ghostty binding action on one terminal, such as `text:…`.
    fn perform(&self, id: &str, action: &str) -> Result<(), Error>;
    /// Opens `id`'s screen. It may bring that terminal to the front: the
    /// OS gives accessibility the focused terminal, which proves which one
    /// it is.
    fn open(&self, id: &str) -> Result<Box<dyn Surface>, Error>;
    /// A new window; its terminal's id.
    fn create(&self) -> Result<String, Error>;
    fn create_command(&self, _command: &str) -> Result<String, Error> {
        Err(Error::Unavailable(
            "This Ghostty adapter does not support initial commands".into(),
        ))
    }
    fn close(&self, id: &str) -> Result<(), Error>;
}

pub struct GhosttyService {
    os: Arc<dyn GhosttyOs>,
}

impl GhosttyService {
    pub fn new(os: Arc<dyn GhosttyOs>) -> Arc<Self> {
        Arc::new(Self { os })
    }

    pub fn available(&self) -> bool {
        self.os.running()
    }

    async fn blocking<T: Send + 'static>(
        &self,
        work: impl FnOnce(&dyn GhosttyOs) -> Result<T, Error> + Send + 'static,
    ) -> Result<T, Error> {
        let os = self.os.clone();
        tokio::task::spawn_blocking(move || work(&*os))
            .await
            .map_err(|e| Error::Unavailable(format!("Ghostty worker: {e}")))?
    }

    pub async fn sessions(&self) -> Result<Vec<TerminalSession>, Error> {
        if !self.os.running() {
            return Err(Error::Unavailable("Ghostty is not running".into()));
        }
        Ok(self
            .blocking(|os| os.terminals())
            .await?
            .into_iter()
            .map(session)
            .collect())
    }

    /// Ghostty sends no list changes: the list is read every two seconds and
    /// sent when it differs.
    pub async fn watch_sessions(
        &self,
        out: mpsc::Sender<Vec<TerminalSession>>,
    ) -> Result<(), Error> {
        let mut last = None;
        loop {
            let sessions = self.sessions().await?;
            if last.as_ref() != Some(&sessions) {
                if out.send(sessions.clone()).await.is_err() {
                    return Ok(());
                }
                last = Some(sessions);
            }
            tokio::select! {
                _ = out.closed() => return Ok(()),
                _ = tokio::time::sleep(LIST_POLL) => {}
            }
        }
    }

    pub async fn attach(
        &self,
        id: &str,
        since_rev: u64,
        frames: mpsc::Sender<TerminalFrame>,
    ) -> Result<(), Error> {
        let target = id.to_owned();
        let surface = Arc::new(Mutex::new(self.blocking(move |os| os.open(&target)).await?));
        let mut rev = since_rev;
        let mut previous: Option<Vec<String>> = None;
        loop {
            let reader = surface.clone();
            let text = tokio::task::spawn_blocking(move || reader.lock().unwrap().text())
                .await
                .map_err(|e| Error::Unavailable(format!("Ghostty worker: {e}")))??;
            let Some(text) = text else {
                rev += 1;
                let _ = frames.send(ended(id, rev)).await;
                return Ok(());
            };
            let lines = screen_lines(&text, MAX_ROWS);
            let frame = match &previous {
                None => Some(frame(id, rev + 1, true, &lines, all(&lines))),
                Some(before) => line_delta(before, &lines)
                    .map(|delta| frame(id, rev + 1, delta.full, &lines, delta.lines)),
            };
            if let Some(frame) = frame {
                rev = frame.rev;
                if frames.send(frame).await.is_err() {
                    return Ok(());
                }
                previous = Some(lines);
            }
            tokio::select! {
                _ = frames.closed() => return Ok(()),
                _ = tokio::time::sleep(POLL) => {}
            }
        }
    }

    pub async fn input(&self, id: &str, bytes: &[u8]) -> Result<(), Error> {
        let (id, action) = (id.to_owned(), text_action(bytes));
        self.blocking(move |os| os.perform(&id, &action)).await
    }

    pub async fn create(&self) -> Result<TerminalSession, Error> {
        let id = self.blocking(|os| os.create()).await?;
        Ok(TerminalSession {
            id,
            source: "ghostty".into(),
            ..Default::default()
        })
    }

    pub async fn close(&self, id: &str) -> Result<(), Error> {
        let id = id.to_owned();
        self.blocking(move |os| os.close(&id)).await
    }

    pub async fn create_command(&self, command: &str) -> Result<TerminalSession, Error> {
        let command = command.to_owned();
        let id = self.blocking(move |os| os.create_command(&command)).await?;
        // Creation already succeeded. Do not risk retrying it if a subsequent
        // inventory fails or the short-lived command has already exited.
        Ok(TerminalSession {
            id,
            source: "ghostty".into(),
            ..Default::default()
        })
    }

    pub async fn history(&self, id: &str, lines: u32) -> Result<Vec<TerminalLine>, Error> {
        let id = id.to_owned();
        let text = self
            .blocking(move |os| os.open(&id)?.text())
            .await?
            .ok_or_else(|| Error::NotFound("the Ghostty terminal is gone".into()))?;
        Ok(all(&screen_lines(&text, lines.max(1) as usize)))
    }
}

fn session(t: Listed) -> TerminalSession {
    TerminalSession {
        title: if t.title.trim().is_empty() {
            t.cwd
        } else {
            t.title
        },
        id: t.id,
        window_id: t.window,
        tab_id: t.tab,
        source: "ghostty".into(),
        ..Default::default()
    }
}

/// The newest `max` rows of a screen's text, without the blank rows below the
/// last output (the empty part of the active screen).
pub fn screen_lines(text: &str, max: usize) -> Vec<String> {
    let mut lines: Vec<String> = text
        .split('\n')
        .map(|l| l.strip_suffix('\r').unwrap_or(l).trim_end().to_owned())
        .collect();
    while lines.last().is_some_and(String::is_empty) {
        lines.pop();
    }
    let skip = lines.len().saturating_sub(max);
    lines.drain(..skip);
    lines
}

fn all(lines: &[String]) -> Vec<TerminalLine> {
    lines
        .iter()
        .enumerate()
        .map(|(i, text)| TerminalLine {
            index: i as u32,
            text: text.clone(),
            styles: Vec::new(),
        })
        .collect()
}

/// Ghostty gives no cursor: it is put after the last row's text.
fn frame(
    id: &str,
    rev: u64,
    full: bool,
    rows: &[String],
    lines: Vec<TerminalLine>,
) -> TerminalFrame {
    TerminalFrame {
        session: id.to_owned(),
        rev,
        full,
        cols: rows.iter().map(|l| l.chars().count()).max().unwrap_or(0) as u32,
        rows: rows.len() as u32,
        lines,
        cursor_x: rows.last().map_or(0, |l| l.chars().count()) as u32,
        cursor_y: rows.len().saturating_sub(1) as u32,
        ended: false,
        ..Default::default()
    }
}

fn ended(id: &str, rev: u64) -> TerminalFrame {
    TerminalFrame {
        session: id.to_owned(),
        rev,
        ended: true,
        ..Default::default()
    }
}

/// Bytes for the terminal as Ghostty's `text:` binding action, which writes
/// them to the terminal's program as typed (not as a paste). Its argument is
/// a Zig string literal whose `\xNN` escape is a code point, not a byte, so
/// only ASCII controls and the backslash are escaped; UTF-8 passes as is.
pub fn text_action(bytes: &[u8]) -> String {
    let mut out = String::with_capacity(5 + bytes.len());
    out.push_str("text:");
    let text = String::from_utf8_lossy(bytes);
    for c in text.chars() {
        match c {
            '\\' => out.push_str("\\\\"),
            c if (c as u32) < 0x20 || c == '\x7f' => out.push_str(&format!("\\x{:02x}", c as u32)),
            c => out.push(c),
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn input_becomes_a_text_action_with_escaped_controls_only() {
        assert_eq!(text_action(b"ls -la\r"), "text:ls -la\\x0d");
        assert_eq!(text_action(b"\x1b[A\x03"), "text:\\x1b[A\\x03");
        assert_eq!(
            text_action("a\\b \"q\" 中文\t".as_bytes()),
            "text:a\\\\b \"q\" 中文\\x09"
        );
        assert_eq!(text_action(b"\x7f"), "text:\\x7f");
    }

    #[test]
    fn screen_text_keeps_the_newest_rows_without_trailing_blanks() {
        let text = "one\r\ntwo  \n\nthree\n\n\n";
        assert_eq!(screen_lines(text, 10), ["one", "two", "", "three"]);
        assert_eq!(screen_lines(text, 2), ["", "three"]);
        assert!(screen_lines("\n\n", 5).is_empty());
    }

    struct Fake {
        screens: Mutex<Vec<Option<String>>>,
        actions: Mutex<Vec<(String, String)>>,
    }
    struct FakeSurface(Arc<Fake>);
    impl Surface for FakeSurface {
        fn text(&mut self) -> Result<Option<String>, Error> {
            let mut screens = self.0.screens.lock().unwrap();
            Ok(if screens.len() > 1 {
                screens.remove(0)
            } else {
                screens[0].clone()
            })
        }
    }
    struct Os(Arc<Fake>);
    impl GhosttyOs for Os {
        fn running(&self) -> bool {
            true
        }
        fn terminals(&self) -> Result<Vec<Listed>, Error> {
            Ok(vec![Listed {
                id: "A".into(),
                title: String::new(),
                cwd: "/tmp".into(),
                window: "w".into(),
                tab: "t".into(),
            }])
        }
        fn perform(&self, id: &str, action: &str) -> Result<(), Error> {
            self.0
                .actions
                .lock()
                .unwrap()
                .push((id.into(), action.into()));
            Ok(())
        }
        fn open(&self, _: &str) -> Result<Box<dyn Surface>, Error> {
            Ok(Box::new(FakeSurface(self.0.clone())))
        }
        fn create(&self) -> Result<String, Error> {
            Ok("A".into())
        }
        fn close(&self, _: &str) -> Result<(), Error> {
            Ok(())
        }
    }

    #[tokio::test]
    async fn a_polled_screen_streams_a_snapshot_then_deltas_then_the_end() {
        let fake = Arc::new(Fake {
            screens: Mutex::new(vec![
                Some("$ ls\n".into()),
                Some("$ ls\n".into()),
                Some("$ ls\na b\n$\n".into()),
                None,
            ]),
            actions: Mutex::new(Vec::new()),
        });
        let service = GhosttyService::new(Arc::new(Os(fake.clone())));
        let listed = service.sessions().await.unwrap();
        assert_eq!(
            listed[0].title, "/tmp",
            "a terminal without a title shows its directory"
        );
        assert_eq!(listed[0].source, "ghostty");
        let (tx, mut rx) = mpsc::channel(8);
        service.attach("A", 0, tx).await.unwrap();
        let first = rx.recv().await.unwrap();
        assert!(first.full && first.rev == 1 && first.lines.len() == 1);
        let delta = rx.recv().await.unwrap();
        assert!(!delta.full && delta.rev == 2);
        assert_eq!(
            delta
                .lines
                .iter()
                .map(|l| (l.index, l.text.as_str()))
                .collect::<Vec<_>>(),
            [(1, "a b"), (2, "$")]
        );
        assert_eq!((delta.cursor_x, delta.cursor_y), (1, 2));
        assert!(rx.recv().await.unwrap().ended);
        service.input("A", b"q\r").await.unwrap();
        assert_eq!(
            fake.actions.lock().unwrap().as_slice(),
            [("A".to_owned(), "text:q\\x0d".to_owned())]
        );
    }
}
