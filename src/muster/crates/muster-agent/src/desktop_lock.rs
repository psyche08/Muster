//! One lock per desktop app on a Mac. Agent (Code mode) and Chat operations
//! drive the same app — Claude.app shows Code or Chat, ChatGPT.app Codex or
//! ChatGPT — so each operation takes the app's lock for its whole duration
//! and releases it afterwards, whether it succeeded or failed. The lock is a
//! file, so it outlives nothing but its holder's intent: a holder that
//! crashes or hangs leaves a lock that expires after its TTL (at most five
//! minutes) and may then be taken over.

use std::io::Write as _;
use std::path::{Path, PathBuf};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use serde::{Deserialize, Serialize};

/// No operation may hold an app for longer than this.
pub const MAX_TTL: Duration = Duration::from_secs(5 * 60);

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum LockKind {
    /// An agent operation in the app's Code / Codex mode.
    Agent,
    /// A desktop Chat-mode operation.
    Chat,
}

impl LockKind {
    pub fn as_str(self) -> &'static str {
        match self {
            LockKind::Agent => "agent",
            LockKind::Chat => "chat",
        }
    }
}

/// Who holds an app, as written in its lock file.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Holder {
    pub kind: LockKind,
    /// What the holder does, for diagnosis ("send claude-chat-…").
    pub detail: String,
    pub pid: u32,
    pub token: String,
    pub acquired_unix_ms: u64,
    pub expires_unix_ms: u64,
}

impl Holder {
    pub fn expired(&self, now_ms: u64) -> bool {
        now_ms >= self.expires_unix_ms
    }
}

/// The app is held by someone else.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Busy {
    pub app: String,
    pub holder: Holder,
    /// Until the holder's lock expires at the latest.
    pub remaining_secs: u64,
}

impl std::fmt::Display for Busy {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let what = match self.holder.kind {
            LockKind::Agent => "an agent operation",
            LockKind::Chat => "a chat operation",
        };
        write!(
            f,
            "{} is in use by {what} ({})",
            self.app, self.holder.detail
        )
    }
}

impl std::error::Error for Busy {}

fn now_ms() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_millis() as u64)
        .unwrap_or(0)
}

/// The locks of one user's desktop apps, in one directory; an app is named
/// case-insensitively ("Claude", "ChatGPT").
#[derive(Clone, Debug)]
pub struct DesktopLocks {
    dir: PathBuf,
}

impl DesktopLocks {
    pub fn new(dir: impl Into<PathBuf>) -> Self {
        Self { dir: dir.into() }
    }

    fn path(&self, app: &str) -> PathBuf {
        let name: String = app
            .chars()
            .map(|c| {
                if c.is_ascii_alphanumeric() || c == '.' || c == '-' {
                    c.to_ascii_lowercase()
                } else {
                    '_'
                }
            })
            .collect();
        self.dir.join(format!("{name}.lock"))
    }

    /// The live holder of `app`, if any (expired and unreadable locks are
    /// reported as free).
    pub fn holder(&self, app: &str) -> Option<Holder> {
        read(&self.path(app)).filter(|h| !h.expired(now_ms()))
    }

    /// Takes `app` now or reports who holds it.
    pub fn try_acquire(
        &self,
        app: &str,
        kind: LockKind,
        detail: &str,
        ttl: Duration,
    ) -> Result<LockGuard, Busy> {
        let ttl = ttl.min(MAX_TTL);
        let path = self.path(app);
        let _ = std::fs::create_dir_all(&self.dir);
        for _ in 0..3 {
            let now = now_ms();
            let holder = Holder {
                kind,
                detail: detail.chars().take(120).collect(),
                pid: std::process::id(),
                token: format!("{:032x}", rand::random::<u128>()),
                acquired_unix_ms: now,
                expires_unix_ms: now + ttl.as_millis() as u64,
            };
            match create(&path, &holder) {
                Ok(()) => {
                    return Ok(LockGuard {
                        path,
                        token: holder.token,
                    });
                }
                Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => {}
                // A lock that cannot be written must not silently let two
                // operations drive one app.
                Err(error) => {
                    return Err(Busy {
                        app: app.to_string(),
                        holder: Holder {
                            detail: format!("lock unavailable: {error}"),
                            ..holder
                        },
                        remaining_secs: 0,
                    });
                }
            }
            match read(&path) {
                Some(current) if !current.expired(now) => {
                    return Err(Busy {
                        app: app.to_string(),
                        remaining_secs: current.expires_unix_ms.saturating_sub(now).div_ceil(1000),
                        holder: current,
                    });
                }
                Some(stale) => take_over(&path, Some(&stale.token)),
                None if stale_unreadable(&path) => take_over(&path, None),
                // Being written right now, or unreadable and young.
                None => std::thread::sleep(Duration::from_millis(20)),
            }
        }
        let current = read(&path);
        Err(Busy {
            app: app.to_string(),
            remaining_secs: current
                .as_ref()
                .map(|h| h.expires_unix_ms.saturating_sub(now_ms()).div_ceil(1000))
                .unwrap_or(0),
            holder: current.unwrap_or(Holder {
                kind,
                detail: "lock contended".into(),
                pid: 0,
                token: String::new(),
                acquired_unix_ms: 0,
                expires_unix_ms: 0,
            }),
        })
    }

    /// Takes `app`, waiting while `patience` (of the current holder) has
    /// not run out since the first try.
    pub async fn acquire(
        &self,
        app: &str,
        kind: LockKind,
        detail: &str,
        ttl: Duration,
        patience: impl Fn(&Holder) -> Duration,
    ) -> Result<LockGuard, Busy> {
        let started = tokio::time::Instant::now();
        loop {
            match self.try_acquire(app, kind, detail, ttl) {
                Ok(guard) => return Ok(guard),
                Err(busy) if started.elapsed() >= patience(&busy.holder) => return Err(busy),
                Err(_) => tokio::time::sleep(Duration::from_millis(100)).await,
            }
        }
    }
}

/// How long an operation of `kind` waits for `holder`. An agent operation
/// waits for a chat (or another agent) operation to finish: they are short
/// and the agent's app cannot be swapped. A chat operation does not wait for
/// an agent: the client picks another Mac instead, since every Mac shows
/// the same conversations. Chat operations queue behind each other briefly.
pub fn patience(kind: LockKind, holder: &Holder) -> Duration {
    match (kind, holder.kind) {
        (LockKind::Agent, _) => Duration::from_secs(30),
        (LockKind::Chat, LockKind::Chat) => Duration::from_secs(20),
        (LockKind::Chat, LockKind::Agent) => Duration::ZERO,
    }
}

/// Holds an app until dropped; releases only its own lock.
#[derive(Debug)]
pub struct LockGuard {
    path: PathBuf,
    token: String,
}

impl Drop for LockGuard {
    fn drop(&mut self) {
        if read(&self.path).is_some_and(|h| h.token == self.token) {
            let _ = std::fs::remove_file(&self.path);
        }
    }
}

fn read(path: &Path) -> Option<Holder> {
    let text = std::fs::read_to_string(path).ok()?;
    serde_json::from_str(&text).ok()
}

/// Writes the whole record to a private file, then links it into place:
/// the lock appears complete or not at all.
fn create(path: &Path, holder: &Holder) -> std::io::Result<()> {
    let tmp = path.with_extension(format!("tmp-{}", holder.token));
    let result = (|| {
        let mut file = std::fs::File::create(&tmp)?;
        file.write_all(serde_json::to_string(holder)?.as_bytes())?;
        file.sync_all()?;
        std::fs::hard_link(&tmp, path)
    })();
    let _ = std::fs::remove_file(&tmp);
    result
}

fn stale_unreadable(path: &Path) -> bool {
    std::fs::metadata(path)
        .and_then(|m| m.modified())
        .ok()
        .and_then(|t| t.elapsed().ok())
        .is_some_and(|age| age >= MAX_TTL)
}

/// Removes an expired lock without removing a fresh one another process
/// placed in the meantime: move it aside, check it is the stale holder, and
/// put anything else back.
fn take_over(path: &Path, stale_token: Option<&str>) {
    let aside = path.with_extension(format!("stale-{:032x}", rand::random::<u128>()));
    if std::fs::rename(path, &aside).is_err() {
        return;
    }
    let moved = read(&aside);
    let is_stale = match (stale_token, &moved) {
        (Some(token), Some(h)) => h.token == token,
        (None, None) => true,
        _ => false,
    };
    if !is_stale {
        // Not ours to remove: restore it unless a new lock already exists.
        let _ = std::fs::hard_link(&aside, path);
    } else {
        log::warn!("desktop lock {} expired; taken over", path.display());
    }
    let _ = std::fs::remove_file(&aside);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn one_holder_at_a_time_and_release_on_drop() {
        let dir = tempfile::tempdir().unwrap();
        let locks = DesktopLocks::new(dir.path());
        let guard = locks
            .try_acquire(
                "claude",
                LockKind::Agent,
                "send s1",
                Duration::from_secs(60),
            )
            .unwrap();
        let busy = locks
            .try_acquire("claude", LockKind::Chat, "open", Duration::from_secs(60))
            .unwrap_err();
        assert_eq!(busy.holder.kind, LockKind::Agent);
        assert!(busy.remaining_secs > 0 && busy.remaining_secs <= 60);
        assert_eq!(locks.holder("claude").unwrap().detail, "send s1");
        // Another app is independent.
        let other = locks
            .try_acquire("codex", LockKind::Chat, "open", Duration::from_secs(60))
            .unwrap();
        drop(guard);
        assert!(locks.holder("claude").is_none());
        let again = locks
            .try_acquire("claude", LockKind::Chat, "open", Duration::from_secs(60))
            .unwrap();
        drop((again, other));
        assert_eq!(std::fs::read_dir(dir.path()).unwrap().count(), 0);
    }

    #[test]
    fn ttl_is_capped_and_an_expired_lock_is_taken_over() {
        let dir = tempfile::tempdir().unwrap();
        let locks = DesktopLocks::new(dir.path());
        let long = locks
            .try_acquire("claude", LockKind::Agent, "x", Duration::from_secs(3600))
            .unwrap();
        let holder = locks.holder("claude").unwrap();
        assert!(holder.expires_unix_ms - holder.acquired_unix_ms <= MAX_TTL.as_millis() as u64);
        drop(long);

        let stale = locks
            .try_acquire("claude", LockKind::Agent, "hung", Duration::from_millis(1))
            .unwrap();
        std::thread::sleep(Duration::from_millis(5));
        assert!(locks.holder("claude").is_none());
        let fresh = locks
            .try_acquire("claude", LockKind::Chat, "open", Duration::from_secs(60))
            .unwrap();
        // The stale holder's release must not remove the new lock.
        drop(stale);
        assert_eq!(locks.holder("claude").unwrap().kind, LockKind::Chat);
        drop(fresh);
        assert!(locks.holder("claude").is_none());
    }

    #[test]
    fn unreadable_young_lock_is_busy() {
        let dir = tempfile::tempdir().unwrap();
        let locks = DesktopLocks::new(dir.path());
        std::fs::write(dir.path().join("claude.lock"), b"not json").unwrap();
        assert!(
            locks
                .try_acquire("claude", LockKind::Chat, "open", Duration::from_secs(60))
                .is_err()
        );
    }

    #[tokio::test]
    async fn agents_wait_for_chats_but_chats_give_way_to_agents() {
        let dir = tempfile::tempdir().unwrap();
        let locks = DesktopLocks::new(dir.path());
        let chat = locks
            .try_acquire("codex", LockKind::Chat, "send", Duration::from_secs(60))
            .unwrap();
        let release = tokio::spawn(async move {
            tokio::time::sleep(Duration::from_millis(250)).await;
            drop(chat);
        });
        let agent = locks
            .acquire(
                "codex",
                LockKind::Agent,
                "send",
                Duration::from_secs(60),
                |h| patience(LockKind::Agent, h),
            )
            .await
            .unwrap();
        release.await.unwrap();
        let started = std::time::Instant::now();
        let busy = locks
            .acquire(
                "codex",
                LockKind::Chat,
                "open",
                Duration::from_secs(60),
                |h| patience(LockKind::Chat, h),
            )
            .await
            .unwrap_err();
        assert_eq!(busy.holder.kind, LockKind::Agent);
        assert!(started.elapsed() < Duration::from_secs(1));
        assert_eq!(
            busy.to_string(),
            "codex is in use by an agent operation (send)"
        );
        drop(agent);
    }
}
