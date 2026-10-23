//! The PTY terminal source (§3.6.3, P6): shells musterd runs itself, for
//! Linux servers and any machine without iTerm2. Output goes through the VT
//! grid so clients get the same line-delta frames as from iTerm2.

use std::collections::HashMap;
use std::io::{Read, Write};
use std::os::fd::{AsRawFd, FromRawFd};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use muster_proto::v1::{TerminalFrame, TerminalLine, TerminalSession};
use tokio::sync::{broadcast, mpsc};

use crate::Error;
use crate::grid::Grid;

#[derive(Clone, Debug)]
pub struct PtyConfig {
    pub shell: PathBuf,
    pub cols: u16,
    pub rows: u16,
    pub max_sessions: usize,
}

impl Default for PtyConfig {
    fn default() -> Self {
        Self {
            shell: std::env::var_os("SHELL")
                .map(PathBuf::from)
                .filter(|p| p.is_absolute())
                .unwrap_or_else(|| PathBuf::from("/bin/sh")),
            cols: 120,
            rows: 40,
            max_sessions: 16,
        }
    }
}

struct Session {
    id: String,
    command: String,
    grid: Mutex<Grid>,
    master: std::fs::File,
    child: Mutex<Option<std::process::Child>>,
    writes: Mutex<()>,
    frames: broadcast::Sender<TerminalFrame>,
    ended: AtomicBool,
}

impl Session {
    fn summary(&self) -> TerminalSession {
        let grid = self.grid.lock().unwrap();
        let title = if grid.title.is_empty() {
            self.command.clone()
        } else {
            grid.title.clone()
        };
        TerminalSession {
            id: self.id.clone(),
            title,
            cols: grid.cols() as u32,
            rows: grid.rows() as u32,
            window_id: String::new(),
            tab_id: String::new(),
            source: if self.id.starts_with("tmux-") {
                "tmux"
            } else {
                "pty"
            }
            .into(),
            device_id: String::new(),
            device_name: String::new(),
        }
    }
}

pub struct PtyService {
    config: PtyConfig,
    sessions: Mutex<HashMap<String, Arc<Session>>>,
    next_id: AtomicU64,
    list_changed: broadcast::Sender<()>,
}

#[repr(C)]
struct Winsize {
    ws_row: u16,
    ws_col: u16,
    ws_xpixel: u16,
    ws_ypixel: u16,
}

fn set_winsize(fd: i32, cols: u16, rows: u16) -> std::io::Result<()> {
    let size = Winsize {
        ws_row: rows,
        ws_col: cols,
        ws_xpixel: 0,
        ws_ypixel: 0,
    };
    // SAFETY: TIOCSWINSZ takes a pointer to a winsize struct.
    if unsafe { libc::ioctl(fd, libc::TIOCSWINSZ, &size as *const Winsize) } < 0 {
        return Err(std::io::Error::last_os_error());
    }
    Ok(())
}

impl PtyService {
    pub fn new(config: PtyConfig) -> Arc<Self> {
        let (list_changed, _) = broadcast::channel(16);
        Arc::new(Self {
            config,
            sessions: Mutex::new(HashMap::new()),
            next_id: AtomicU64::new(1),
            list_changed,
        })
    }

    pub fn available(&self) -> bool {
        self.config.shell.exists()
    }

    fn get(&self, id: &str) -> Result<Arc<Session>, Error> {
        self.sessions
            .lock()
            .unwrap()
            .get(id)
            .cloned()
            .ok_or_else(|| Error::NotFound(format!("session {id}")))
    }

    pub(crate) fn contains(&self, id: &str) -> bool {
        self.sessions
            .lock()
            .unwrap()
            .get(id)
            .is_some_and(|s| !s.ended.load(Ordering::Acquire))
    }

    pub async fn sessions(&self) -> Result<Vec<TerminalSession>, Error> {
        let mut list: Vec<TerminalSession> = self
            .sessions
            .lock()
            .unwrap()
            .values()
            .map(|s| s.summary())
            .collect();
        list.sort_by(|a, b| a.id.cmp(&b.id));
        Ok(list)
    }

    pub async fn watch_sessions(
        &self,
        out: mpsc::Sender<Vec<TerminalSession>>,
    ) -> Result<(), Error> {
        let mut changes = self.list_changed.subscribe();
        if out.send(self.sessions().await?).await.is_err() {
            return Ok(());
        }
        loop {
            match changes.recv().await {
                Ok(()) | Err(broadcast::error::RecvError::Lagged(_)) => {
                    if out.send(self.sessions().await?).await.is_err() {
                        return Ok(());
                    }
                }
                Err(_) => return Ok(()),
            }
        }
    }

    /// Starts a shell (or `profile` as a command name) on a new PTY.
    pub async fn create(
        self: &Arc<Self>,
        profile: Option<&str>,
        _background: bool,
    ) -> Result<TerminalSession, Error> {
        if self.sessions.lock().unwrap().len() >= self.config.max_sessions {
            return Err(Error::Unavailable("too many terminal sessions".into()));
        }
        let command = match profile.map(str::trim).filter(|p| !p.is_empty()) {
            Some(p)
                if p.chars()
                    .all(|c| c.is_ascii_alphanumeric() || matches!(c, '-' | '_' | '.' | '/')) =>
            {
                p.to_string()
            }
            Some(_) => return Err(Error::Protocol("profile must be a command name".into())),
            None => self.config.shell.to_string_lossy().into_owned(),
        };
        let args = if command.ends_with("sh") {
            vec!["-i".into()]
        } else {
            vec![]
        };
        self.create_command(Path::new(&command), &args, None, "pty-")
            .await
    }

    /// Arguments and cwd are passed directly to exec, never through a shell.
    pub(crate) async fn create_command(
        self: &Arc<Self>,
        command: &Path,
        args: &[String],
        cwd: Option<&Path>,
        prefix: &str,
    ) -> Result<TerminalSession, Error> {
        if self.sessions.lock().unwrap().len() >= self.config.max_sessions {
            return Err(Error::Unavailable("too many terminal sessions".into()));
        }
        let (cols, rows) = (self.config.cols, self.config.rows);
        let mut master = -1i32;
        let mut slave = -1i32;
        let mut size = Winsize {
            ws_row: rows,
            ws_col: cols,
            ws_xpixel: 0,
            ws_ypixel: 0,
        };
        // SAFETY: openpty fills the two descriptors; the name buffer is
        // optional. The termios and winsize pointers are `*mut` in the macOS
        // binding and `*const` on Linux; a `*mut` coerces to both.
        if unsafe {
            libc::openpty(
                &mut master,
                &mut slave,
                std::ptr::null_mut(),
                std::ptr::null_mut(),
                &mut size as *mut Winsize as *mut libc::winsize,
            )
        } < 0
        {
            return Err(Error::Unavailable(format!(
                "openpty: {}",
                std::io::Error::last_os_error()
            )));
        }
        // SAFETY: fresh descriptors we own.
        let master_file = unsafe { std::fs::File::from_raw_fd(master) };
        // SAFETY: as above.
        let slave_file = unsafe { std::fs::File::from_raw_fd(slave) };
        // SAFETY: setting close-on-exec on the master so the child does not inherit it.
        unsafe { libc::fcntl(master, libc::F_SETFD, libc::FD_CLOEXEC) };
        let mut cmd = std::process::Command::new(command);
        cmd.args(args).env_remove("TMUX");
        if let Some(cwd) = cwd {
            cmd.current_dir(cwd);
        }
        cmd.env("TERM", "xterm-256color")
            .env("COLORTERM", "truecolor")
            .stdin(
                slave_file
                    .try_clone()
                    .map_err(|e| Error::Unavailable(e.to_string()))?,
            )
            .stdout(
                slave_file
                    .try_clone()
                    .map_err(|e| Error::Unavailable(e.to_string()))?,
            )
            .stderr(slave_file);
        // SAFETY: only async-signal-safe calls between fork and exec.
        unsafe {
            use std::os::unix::process::CommandExt;
            cmd.pre_exec(|| {
                if libc::setsid() < 0 {
                    return Err(std::io::Error::last_os_error());
                }
                if libc::ioctl(0, libc::TIOCSCTTY as _, 0) < 0 {
                    return Err(std::io::Error::last_os_error());
                }
                Ok(())
            });
        }
        let child = cmd
            .spawn()
            .map_err(|e| Error::Unavailable(format!("{}: {e}", command.display())))?;
        let id = format!("{prefix}{}", self.next_id.fetch_add(1, Ordering::Relaxed));
        let (frames, _) = broadcast::channel(256);
        let session = Arc::new(Session {
            id: id.clone(),
            command: command.to_string_lossy().into_owned(),
            grid: Mutex::new(Grid::new(cols, rows)),
            master: master_file,
            child: Mutex::new(Some(child)),
            writes: Mutex::new(()),
            frames,
            ended: AtomicBool::new(false),
        });
        self.sessions
            .lock()
            .unwrap()
            .insert(id.clone(), session.clone());
        let _ = self.list_changed.send(());
        let service = self.clone();
        let reader = session
            .master
            .try_clone()
            .map_err(|e| Error::Unavailable(e.to_string()))?;
        tokio::task::spawn_blocking(move || service.read_loop(session, reader));
        Ok(self.get(&id)?.summary())
    }

    fn read_loop(&self, session: Arc<Session>, mut reader: std::fs::File) {
        let mut buf = vec![0u8; 16384];
        let mut deadline = None;
        loop {
            if let Some(until) = deadline {
                let ready = Instant::now() < until && readable_before(reader.as_raw_fd(), until);
                if !ready {
                    let mut grid = session.grid.lock().unwrap();
                    if let Some(frame) = grid.frame(&session.id) {
                        let _ = session.frames.send(frame);
                    }
                    deadline = None;
                }
            }
            let n = match reader.read(&mut buf) {
                Ok(0) => break,
                Err(error) if error.kind() == std::io::ErrorKind::Interrupted => continue,
                Err(_) => break,
                Ok(n) => n,
            };
            deadline.get_or_insert_with(|| Instant::now() + Duration::from_millis(8));
            // Parse and answer queries immediately, even within a frame window.
            let replies = session.grid.lock().unwrap().feed(&buf[..n]);
            if !replies.is_empty() {
                let _guard = session.writes.lock().unwrap();
                let _ = (&session.master).write_all(&replies);
            }
        }
        {
            let mut grid = session.grid.lock().unwrap();
            if let Some(frame) = grid.frame(&session.id) {
                let _ = session.frames.send(frame);
            }
            session.ended.store(true, Ordering::Release);
            let _ = session.frames.send(TerminalFrame {
                session: session.id.clone(),
                rev: grid.revision() + 1,
                ended: true,
                ..Default::default()
            });
        }
        if let Some(mut child) = session.child.lock().unwrap().take() {
            let _ = child.wait();
        }
        self.sessions.lock().unwrap().remove(&session.id);
        let _ = self.list_changed.send(());
    }

    pub async fn attach(
        &self,
        id: &str,
        _since_rev: u64,
        out: mpsc::Sender<TerminalFrame>,
    ) -> Result<(), Error> {
        let session = self.get(id)?;
        let mut frames = session.frames.subscribe();
        let snapshot = || {
            let grid = session.grid.lock().unwrap();
            let mut full = grid.snapshot(id);
            if session.ended.load(Ordering::Acquire) {
                full.ended = true;
                full.rev += 1;
            }
            full
        };
        let full = snapshot();
        let mut revision = full.rev;
        let ended = full.ended;
        if out.send(full).await.is_err() || ended {
            return Ok(());
        }
        loop {
            tokio::select! {
                _ = out.closed() => return Ok(()),
                next = frames.recv() => match next {
                    Ok(frame) => {
                        // A snapshot may already include queued frames.
                        if frame.rev <= revision { continue; }
                        revision = frame.rev;
                        let ended = frame.ended;
                        if out.send(frame).await.is_err() || ended { return Ok(()); }
                    }
                    Err(broadcast::error::RecvError::Lagged(_)) => {
                        // Resubscribe before capturing, so no update can fall
                        // between recovery and observing subsequent deltas.
                        frames = session.frames.subscribe();
                        let full = snapshot();
                        revision = full.rev;
                        let ended = full.ended;
                        if out.send(full).await.is_err() || ended { return Ok(()); }
                    }
                    Err(_) => return Ok(()),
                }
            }
        }
    }

    pub async fn input(&self, id: &str, bytes: &[u8]) -> Result<(), Error> {
        let session = self.get(id)?;
        if session.ended.load(Ordering::Relaxed) {
            return Err(Error::NotFound(format!("session {id} ended")));
        }
        let bytes = bytes.to_vec();
        tokio::task::spawn_blocking(move || {
            let _guard = session.writes.lock().unwrap();
            (&session.master).write_all(&bytes)
        })
        .await
        .map_err(|e| Error::Unavailable(e.to_string()))?
        .map_err(|e| Error::Unavailable(format!("write: {e}")))
    }

    pub async fn resize(&self, id: &str, cols: u32, rows: u32) -> Result<(), Error> {
        let session = self.get(id)?;
        let (cols, rows) = (cols.clamp(2, 500) as u16, rows.clamp(1, 300) as u16);
        {
            let mut grid = session.grid.lock().unwrap();
            if grid.cols() == cols && grid.rows() == rows {
                return Ok(());
            }
            set_winsize(session.master.as_raw_fd(), cols, rows)
                .map_err(|e| Error::Unavailable(format!("resize: {e}")))?;
            grid.resize(cols, rows);
            let _ = session.frames.send(grid.full_frame(id));
        }
        let _ = self.list_changed.send(());
        Ok(())
    }

    pub async fn close(&self, id: &str) -> Result<(), Error> {
        let session = self.get(id)?;
        if let Some(child) = session.child.lock().unwrap().as_mut() {
            // SAFETY: signalling our own child.
            unsafe { libc::kill(child.id() as i32, libc::SIGHUP) };
        }
        Ok(())
    }

    pub async fn history(&self, id: &str, lines: u32) -> Result<Vec<TerminalLine>, Error> {
        let session = self.get(id)?;
        let grid = session.grid.lock().unwrap();
        let mut text = grid.scrollback(lines.max(1) as usize);
        text.extend((0..grid.rows()).map(|r| grid.row_text(r)));
        let skip = text.len().saturating_sub(lines.max(1) as usize);
        Ok(text
            .into_iter()
            .skip(skip)
            .enumerate()
            .map(|(i, text)| TerminalLine {
                index: i as u32,
                text,
                styles: Vec::new(),
            })
            .collect())
    }
}

/// Wait only until this burst's frame deadline, never once per output chunk.
fn readable_before(fd: i32, deadline: Instant) -> bool {
    loop {
        let remaining = deadline.saturating_duration_since(Instant::now());
        if remaining.is_zero() {
            return false;
        }
        let mut event = libc::pollfd {
            fd,
            events: libc::POLLIN,
            revents: 0,
        };
        // SAFETY: one valid pollfd, borrowed from the live PTY reader.
        let result = unsafe { libc::poll(&mut event, 1, remaining.as_millis().max(1) as i32) };
        if result >= 0 {
            return result > 0;
        }
        if std::io::Error::last_os_error().kind() != std::io::ErrorKind::Interrupted {
            return false;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Duration;

    #[tokio::test]
    async fn lagged_viewer_recovers_without_replaying_older_deltas() {
        let service = PtyService::new(PtyConfig::default());
        let created = service
            .create_command(Path::new("/bin/sleep"), &["30".into()], None, "pty-")
            .await
            .unwrap();
        let session = service.get(&created.id).unwrap();
        let (tx, mut rx) = mpsc::channel(1);
        let task_service = service.clone();
        let id = created.id.clone();
        let task = tokio::spawn(async move { task_service.attach(&id, 0, tx).await });
        assert!(rx.recv().await.unwrap().full);
        // Overflow the broadcast queue while the subscriber cannot run.
        for i in 0..400 {
            let mut grid = session.grid.lock().unwrap();
            grid.feed(format!("\rframe-{i:03}").as_bytes());
            let frame = grid.frame(&created.id).unwrap();
            session.frames.send(frame).unwrap();
        }
        let full = tokio::time::timeout(Duration::from_secs(2), rx.recv())
            .await
            .unwrap()
            .unwrap();
        assert!(full.full);
        assert!(full.lines[0].text.contains("frame-399"));
        let expected_rev = {
            let mut grid = session.grid.lock().unwrap();
            grid.feed(b"\rFINAL");
            let frame = grid.frame(&created.id).unwrap();
            let revision = frame.rev;
            session.frames.send(frame).unwrap();
            revision
        };
        let next = tokio::time::timeout(Duration::from_secs(2), rx.recv())
            .await
            .unwrap()
            .unwrap();
        assert_eq!(next.rev, expected_rev);
        assert!(next.rev > full.rev);
        assert!(next.lines[0].text.starts_with("FINAL"));
        drop(rx);
        tokio::time::timeout(Duration::from_secs(2), task)
            .await
            .unwrap()
            .unwrap()
            .unwrap();
        service.close(&created.id).await.unwrap();
    }

    #[tokio::test]
    async fn burst_output_is_coalesced_and_end_revision_follows_last_frame() {
        let service = PtyService::new(PtyConfig::default());
        let created = service.create_command(Path::new("/bin/sh"), &[
            "-c".into(), "read start; i=0; while [ $i -lt 2000 ]; do printf 'line-%s\\n' $i; i=$((i+1)); done; printf 'BURST-DONE\\n'".into()
        ], None, "pty-").await.unwrap();
        let (tx, mut rx) = mpsc::channel(256);
        let svc = service.clone();
        let id = created.id.clone();
        let task = tokio::spawn(async move { svc.attach(&id, 0, tx).await });
        assert!(rx.recv().await.unwrap().full);
        service.input(&created.id, b"go\n").await.unwrap();
        let mut previous = 0;
        let mut count = 0;
        let mut done = false;
        loop {
            let frame = tokio::time::timeout(Duration::from_secs(10), rx.recv())
                .await
                .unwrap()
                .unwrap();
            assert!(frame.rev > previous);
            previous = frame.rev;
            done |= frame
                .lines
                .iter()
                .any(|line| line.text.contains("BURST-DONE"));
            if frame.ended {
                break;
            }
            count += 1;
        }
        assert!(done, "final output must precede end");
        assert!(
            count < 200,
            "2000 lines should not produce per-read frames: {count}"
        );
        eprintln!("2000 output lines produced {count} frames");
        task.await.unwrap().unwrap();
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn shell_echo_resize_and_close() {
        let service = PtyService::new(PtyConfig {
            shell: PathBuf::from("/bin/sh"),
            cols: 40,
            rows: 6,
            ..PtyConfig::default()
        });
        let session = service.create(None, false).await.unwrap();
        assert_eq!(session.source, "pty");
        let (tx, mut rx) = mpsc::channel(64);
        let id = session.id.clone();
        let svc = service.clone();
        tokio::spawn(async move { svc.attach(&id, 0, tx).await });
        let first = tokio::time::timeout(Duration::from_secs(5), rx.recv())
            .await
            .unwrap()
            .unwrap();
        assert!(first.full);
        service
            .input(&session.id, b"echo muster-$((6*7))\r")
            .await
            .unwrap();
        let mut seen = String::new();
        let deadline = tokio::time::Instant::now() + Duration::from_secs(10);
        while !seen.contains("muster-42") {
            let frame = tokio::time::timeout_at(deadline, rx.recv())
                .await
                .expect("output in time")
                .unwrap();
            seen = frame
                .lines
                .iter()
                .map(|l| l.text.clone())
                .collect::<Vec<_>>()
                .join("\n");
            if frame.ended {
                panic!("session ended early: {seen}");
            }
        }
        service.resize(&session.id, 80, 10).await.unwrap();
        let sessions = service.sessions().await.unwrap();
        assert_eq!(sessions[0].cols, 80);
        let history = service.history(&session.id, 20).await.unwrap();
        assert!(history.iter().any(|l| l.text.contains("muster-42")));
        service.input(&session.id, b"exit\r").await.unwrap();
        let ended = loop {
            let frame = tokio::time::timeout(Duration::from_secs(5), rx.recv())
                .await
                .expect("end in time");
            match frame {
                Some(f) if f.ended => break true,
                Some(_) => continue,
                None => break true,
            }
        };
        assert!(ended);
        tokio::time::sleep(Duration::from_millis(100)).await;
        assert!(service.sessions().await.unwrap().is_empty());
    }
}
