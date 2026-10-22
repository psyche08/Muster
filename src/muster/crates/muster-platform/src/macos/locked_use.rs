//! macOS facts and effects for the Rust-owned console transaction.
use super::{ax, codesign, input, procs};
use crate::locked_use::{ConsoleGuard, ConsoleSample, GuardSample, LockedUseOs, UnlockCheck};
use async_trait::async_trait;
use muster_core::locked_use::{Console, Screen};
use std::{
    ffi::CString,
    io::{Read, Write},
    os::fd::AsRawFd,
    path::{Path, PathBuf},
    process::{Child, ChildStdin, Command, Stdio},
    time::{Duration, Instant},
};

unsafe extern "C" {
    fn muster_console_post_return() -> bool;
    fn muster_console_reveal_lock_ui(pid: i32) -> bool;
    fn muster_console_wake_begin(assertion: *mut u32) -> bool;
    fn muster_console_wake_end(assertion: u32) -> bool;
    fn muster_console_display_awake_begin(remaining_ms: u64, assertion: *mut u32) -> bool;
    fn muster_console_sample(uid: *mut u32, asid: *mut u32, locked: *mut bool) -> bool;
    fn muster_console_cover() -> bool;
    fn muster_console_cover_healthy(unlocked: bool) -> bool;
    fn muster_console_pump();
    fn muster_console_uncover();
    fn muster_console_lock() -> bool;
    fn muster_locked_peer(fd: i32, requirement: *const std::ffi::c_char, uid: u32) -> bool;
}
pub struct MacLockedUse {
    dir: PathBuf,
}
impl MacLockedUse {
    pub fn new(dir: &Path) -> Self {
        Self { dir: dir.into() }
    }
}
fn sample() -> ConsoleSample {
    let (mut uid, mut asid, mut locked) = (0, 0, false);
    // SAFETY: pointers to live output values.
    let ok = unsafe { muster_console_sample(&mut uid, &mut asid, &mut locked) };
    ConsoleSample {
        at: Instant::now(),
        console: ok.then_some(Console {
            uid,
            audit_session: asid,
        }),
        screen: if !ok {
            Screen::Unknown
        } else if locked {
            Screen::Locked
        } else {
            Screen::Unlocked
        },
        input_generation: ok.then(input::user_input_generation),
    }
}
fn lock() {
    unsafe {
        muster_console_lock();
    }
}
#[async_trait]
impl LockedUseOs for MacLockedUse {
    fn available(&self) -> Result<(), String> {
        if !ax::trusted() {
            return Err("grant Accessibility to musterd first".into());
        }
        self.update_ready()?;
        // Installation is root-owned; a missing plugin cannot be worked around with AX.
        if !Path::new("/Library/Security/SecurityAgentPlugins/MusterLockedUse.bundle").is_dir() {
            return Err("install the Muster Locked Use authorization plugin first".into());
        }
        Ok(())
    }
    fn update_ready(&self) -> Result<(), String> {
        let marker = self.dir.join("locked-use.pending");
        if marker.exists() {
            let previous = std::fs::read_to_string(&marker).unwrap_or_default();
            let fields: Vec<_> = previous.trim().split(':').collect();
            let current = sample().console;
            let different_login = fields.len() == 3
                && current.is_some_and(|c| {
                    fields[0].parse::<u32>() == Ok(c.uid)
                        && fields[1]
                            .parse::<u32>()
                            .is_ok_and(|old| old != c.audit_session)
                });
            if different_login {
                std::fs::remove_file(marker).map_err(|e| e.to_string())?;
            } else {
                return Err(
                    "an earlier Locked Use transaction needs local recovery (log out and log in)"
                        .into(),
                );
            }
        }
        Ok(())
    }
    fn observe(&self) -> ConsoleSample {
        sample()
    }
    fn guard(
        &self,
        nonce: [u8; 32],
        ttl: Duration,
        initial: Screen,
    ) -> Result<Box<dyn ConsoleGuard>, String> {
        let initial = match initial {
            Screen::Locked => "locked",
            Screen::Unlocked => "unlocked",
            Screen::Unknown => return Err("unknown initial console state".into()),
        };
        let nonce: String = nonce.iter().map(|b| format!("{b:02x}")).collect();
        let mut child = Command::new(std::env::current_exe().map_err(|e| e.to_string())?)
            .arg("--locked-use-watchdog")
            .arg(&self.dir)
            .arg(nonce)
            .arg((monotonic_millis().saturating_add(ttl.as_millis() as u64)).to_string())
            .arg(initial)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::inherit())
            .spawn()
            .map_err(|e| e.to_string())?;
        let input = child.stdin.take().unwrap();
        let mut output = child.stdout.take().unwrap();
        // Never backpressure the pipe reader while the owner is sampling the
        // OS. Eight queued 40 ms reports used to strand newer evidence behind
        // a blocked send and make a healthy cover look older than 250 ms.
        // Ready reports stop after heartbeat loss; terminal reports are sent
        // once, so a stalled owner cannot accumulate quarantine heartbeats.
        let (tx, rx) = std::sync::mpsc::channel();
        std::thread::spawn(move || {
            let mut byte = [0];
            while output.read_exact(&mut byte).is_ok() {
                let event = if byte[0] == b'U' {
                    let mut payload = [0; 24];
                    if output.read_exact(&mut payload).is_err() {
                        break;
                    }
                    match decode_unlock_witness(&payload) {
                        Some(sample) => GuardEvent::Unlocked(sample),
                        None => break,
                    }
                } else {
                    GuardEvent::Status(byte[0], Instant::now())
                };
                if tx.send(event).is_err() {
                    break;
                }
            }
        });
        Ok(Box::new(Guard {
            child,
            input,
            rx,
            last: None,
        }))
    }
    async fn request_unlock(&self, check: UnlockCheck) -> Result<(), String> {
        check()?;
        let wake = Wake::begin()?;
        let started = Instant::now();
        let mut revealed = false;
        let deadline = Instant::now() + Duration::from_secs(4);
        while Instant::now() < deadline {
            check()?;
            if let Some(field) = password_field() {
                log::info!(
                    "Locked Use lock UI: authorization field ready after {} ms; reveal={revealed}",
                    started.elapsed().as_millis()
                );
                check()?;
                field
                    .set_bool("AXFocused", true)
                    .map_err(|e| format!("focus: {e}"))?;
                if field.bool("AXFocused") != Some(true) {
                    return Err("lock-screen field did not take focus".into());
                }
                wake.release()?;
                check()?;
                field
                    .set_string("AXValue", "")
                    .map_err(|e| format!("empty authorization field: {e}"))?;
                if field.bool("AXFocused") != Some(true) {
                    return Err("lock-screen field lost focus".into());
                }
                check()?;
                // Session posting does not advance the HID counters used by
                // both the owner and watchdog. Never fall back to HID or retry.
                return if unsafe { muster_console_post_return() } {
                    Ok(())
                } else {
                    Err("OS event posting is unavailable".into())
                };
            }
            // A power assertion can leave the screen saver/clock visible
            // without a password field. Reveal the real lock UI once with
            // a nonprinting key pair addressed to loginwindow, never Return
            // into an unknown view or input routed to a user-session cover.
            if !revealed
                && started.elapsed() >= Duration::from_millis(400)
                && let Some(pid) = login_window_pid()
            {
                check()?;
                revealed = true;
                let posted = unsafe { muster_console_reveal_lock_ui(pid as i32) };
                log::info!(
                    "Locked Use lock UI: reveal route=loginwindow key=left pid={pid} submitted={posted}; input={:?}",
                    input::user_input_generation()
                );
                check()?;
                if !posted {
                    return Err("cannot reveal the Apple lock-screen authorization UI".into());
                }
            }
            tokio::time::sleep(Duration::from_millis(100)).await;
        }
        log::warn!(
            "Locked Use lock UI: field unavailable after {} ms; reveal={revealed}",
            started.elapsed().as_millis()
        );
        Err("the Apple lock-screen authorization field is unavailable".into())
    }
    fn lock(&self) {
        lock();
    }
    fn plugin_peer(&self, stream: &std::os::unix::net::UnixStream) -> bool {
        let requirement = CString::new("anchor apple and (identifier \"com.apple.authorizationhost\" or identifier \"com.apple.authorizationhosthelper.arm64\" or identifier \"com.apple.authorizationhosthelper.x86_64\")").unwrap();
        unsafe { muster_locked_peer(stream.as_raw_fd(), requirement.as_ptr(), 0) }
    }
}
struct Wake(Option<u32>);
impl Wake {
    fn begin() -> Result<Self, String> {
        let mut assertion = 0;
        if unsafe { muster_console_wake_begin(&mut assertion) } {
            Ok(Self(Some(assertion)))
        } else {
            Err("cannot wake the system authorization UI".into())
        }
    }
    fn release(mut self) -> Result<(), String> {
        let assertion = self.0.take().ok_or("wake assertion already released")?;
        if unsafe { muster_console_wake_end(assertion) } {
            Ok(())
        } else {
            Err("cannot release wake assertion before authorization".into())
        }
    }
}
impl Drop for Wake {
    fn drop(&mut self) {
        if let Some(assertion) = self.0.take() {
            unsafe {
                muster_console_wake_end(assertion);
            }
        }
    }
}

// Unlike the brief lock-screen wake, this resource belongs to the bounded
// watchdog transaction. It prevents idle display sleep from relocking midway
// through an AX workflow; explicit locks and every authorization gate remain.
struct DisplayAwake(u32);
impl DisplayAwake {
    fn begin(deadline: u64) -> Result<Self, String> {
        let remaining = deadline.saturating_sub(monotonic_millis());
        if !(1..=15000).contains(&remaining) {
            return Err("display assertion deadline expired".into());
        }
        let mut assertion = 0;
        if !unsafe { muster_console_display_awake_begin(remaining, &mut assertion) } {
            return Err("cannot keep the covered display awake".into());
        }
        log::info!("Locked Use display assertion acquired; remaining_ms={remaining}");
        Ok(Self(assertion))
    }
}
impl Drop for DisplayAwake {
    fn drop(&mut self) {
        // The OS may already have released it at the hard deadline.
        unsafe {
            muster_console_wake_end(self.0);
        }
        log::info!("Locked Use display assertion released");
    }
}

/// The one secure field of the genuine lock screen, only when it can take a
/// value and confirm. Its contents are never read. loginwindow exposes the
/// lock UI to AX only after manual accessibility is enabled on it.
fn password_field() -> Option<ax::Element> {
    for pid in procs::pids_by_executable_suffix("/loginwindow.app/Contents/MacOS/loginwindow") {
        if !codesign::process_satisfies(
            pid,
            "identifier \"com.apple.loginwindow\" and anchor apple",
        ) {
            continue;
        }
        let app = ax::Element::application(pid);
        app.set_timeout(0.25);
        app.enable_manual_accessibility();
        let mut fields = Vec::new();
        app.walk(|el| {
            if el.string("AXIdentifier").as_deref() == Some("UserPasswordTextField") {
                fields.push(el.clone());
            }
            true
        });
        if fields.len() != 1 {
            continue;
        }
        let field = fields.pop().unwrap();
        if field.settable("AXValue") && field.has_action("AXConfirm") {
            return Some(field);
        }
    }
    None
}

/// A reveal is allowed only for the one known window of a genuine Apple
/// loginwindow process. This function never reads any input field's value.
fn login_window_pid() -> Option<u32> {
    let mut windows = Vec::new();
    for pid in procs::pids_by_executable_suffix("/loginwindow.app/Contents/MacOS/loginwindow") {
        if !codesign::process_satisfies(
            pid,
            "identifier \"com.apple.loginwindow\" and anchor apple",
        ) {
            continue;
        }
        let app = ax::Element::application(pid);
        app.walk(|node| {
            if node.role() == "AXWindow" && node.string("AXIdentifier").as_deref() == Some("login")
            {
                windows.push(pid);
            }
            true
        });
    }
    (windows.len() == 1).then(|| windows[0])
}

/// `muster debug lock`: the same OS lock request the transaction owner uses.
pub fn lock_now() -> bool {
    // SAFETY: plain C call.
    unsafe { muster_console_lock() }
}

/// `muster debug lock-field`: whether the lock screen currently exposes its
/// authorization field. Read-only; no value, no confirm.
pub fn field_diagnostic() -> String {
    let started = Instant::now();
    let pids = procs::pids_by_executable_suffix("/loginwindow.app/Contents/MacOS/loginwindow");
    let field = password_field();
    let mut elements = Vec::new();
    for pid in &pids {
        if !codesign::process_satisfies(
            *pid,
            "identifier \"com.apple.loginwindow\" and anchor apple",
        ) {
            continue;
        }
        let app = ax::Element::application(*pid);
        app.set_timeout(0.25);
        app.walk(|el| {
            if elements.len() >= 40 {
                return false;
            }
            let role = el.string("AXRole").unwrap_or_default();
            if role == "AXButton" || role == "AXTextField" || role == "AXWindow" {
                elements.push(format!(
                    "role={role} id={:?} enabled={:?} press={} confirm={}",
                    el.string("AXIdentifier"),
                    el.bool("AXEnabled"),
                    el.has_action("AXPress"),
                    el.has_action("AXConfirm")
                ));
            }
            true
        });
    }
    format!(
        "loginwindow={pids:?} field={} focused={:?} screen={:?} ({} ms)\n{}",
        if field.is_some() { "present" } else { "absent" },
        field.as_ref().and_then(|f| f.bool("AXFocused")),
        sample().screen,
        started.elapsed().as_millis(),
        elements.join("\n")
    )
}

enum GuardEvent {
    Status(u8, Instant),
    Unlocked(ConsoleSample),
}

/// Publish the newest cover status before the historical unlock witness in one
/// pipe write. Otherwise the owner can consume U while a slow cover query is
/// still running and enter Active with only the previous locked-screen R.
fn guard_report(code: u8, witness: Option<(u64, Console, u64)>) -> Vec<u8> {
    let mut frame = vec![code];
    // Preserve unlock history even after a stop: cleanup needs to witness a
    // late unlock before releasing the cover. A failure status grants nothing.
    if let Some((millis, console, input)) = witness {
        frame.push(b'U');
        frame.extend_from_slice(&millis.to_le_bytes());
        frame.extend_from_slice(&console.uid.to_le_bytes());
        frame.extend_from_slice(&console.audit_session.to_le_bytes());
        frame.extend_from_slice(&input.to_le_bytes());
    }
    frame
}
fn decode_unlock_witness(bytes: &[u8; 24]) -> Option<ConsoleSample> {
    let millis = u64::from_le_bytes(bytes[0..8].try_into().ok()?);
    let age = monotonic_millis().checked_sub(millis)?;
    Some(ConsoleSample {
        at: Instant::now().checked_sub(Duration::from_millis(age))?,
        console: Some(Console {
            uid: u32::from_le_bytes(bytes[8..12].try_into().ok()?),
            audit_session: u32::from_le_bytes(bytes[12..16].try_into().ok()?),
        }),
        screen: Screen::Unlocked,
        input_generation: Some(u64::from_le_bytes(bytes[16..24].try_into().ok()?)),
    })
}

struct Guard {
    child: Child,
    input: ChildStdin,
    rx: std::sync::mpsc::Receiver<GuardEvent>,
    last: Option<(u8, Instant)>,
}
fn drain_guard_events(
    rx: &std::sync::mpsc::Receiver<GuardEvent>,
    last: &mut Option<(u8, Instant)>,
) -> Option<ConsoleSample> {
    let mut observed_unlock = None;
    while let Ok(event) = rx.try_recv() {
        let value = match event {
            GuardEvent::Unlocked(sample) => {
                observed_unlock = Some(sample);
                continue;
            }
            GuardEvent::Status(code, at) => (code, at),
        };
        if value.0 != b'R' && value.0 != b'F' && last.map(|v| v.0) != Some(value.0) {
            let reason = match value.0 {
                b'E' => "deadline expired",
                b'H' => "owner heartbeat lost",
                b'C' => "console changed",
                b'I' => "local input changed",
                b'V' => "display cover lost",
                _ => "owner stopped or disconnected",
            };
            log::warn!("Locked Use watchdog: {reason}");
        }
        *last = Some(value);
    }
    observed_unlock
}
impl ConsoleGuard for Guard {
    fn pulse(&mut self) -> GuardSample {
        let alive =
            self.child.try_wait().ok().flatten().is_none() && self.input.write_all(b"h").is_ok();
        let observed_unlock = fresh_guard_events(&self.rx, &mut self.last, alive);
        let covered = self.last.is_some_and(|(b, at)| {
            b == b'R' && at.elapsed() < muster_core::locked_use::MAX_OBSERVATION_AGE
        });
        GuardSample {
            observed_unlock,
            status_age: self.last.map(|(_, at)| at.elapsed()),
            alive: alive && covered,
            covered,
            failed: !alive
                || self
                    .last
                    .is_some_and(|(byte, _)| !matches!(byte, b'R' | b'F')),
        }
    }
    fn lock(&mut self) {
        let _ = self.input.write_all(b"k");
    }
    fn extend(&mut self, ttl: Duration) -> Result<(), String> {
        if ttl > Duration::from_millis(MAX_EXTENSION_MS) {
            return Err("extension beyond the Locked Use maximum".into());
        }
        let mut frame = vec![b'x'];
        frame.extend_from_slice(
            &monotonic_millis()
                .saturating_add(ttl.as_millis() as u64)
                .to_le_bytes(),
        );
        self.input.write_all(&frame).map_err(|e| e.to_string())
    }
    fn release(&mut self) -> Result<(), String> {
        self.input.write_all(b"f").map_err(|e| e.to_string())?;
        let deadline = Instant::now() + Duration::from_secs(2);
        loop {
            let remaining = deadline.saturating_duration_since(Instant::now());
            match self.rx.recv_timeout(remaining) {
                Ok(GuardEvent::Status(b'F', _)) => return Ok(()),
                Ok(_) if Instant::now() < deadline => continue,
                _ => return Err("watchdog did not confirm cover removal".into()),
            }
        }
    }
}

/// A slow WindowServer sample at unlock can put the next report just behind
/// the owner's poll. Request a heartbeat first and briefly await *new* evidence
/// instead of treating an old ready report as proof that the cover disappeared.
/// The original freshness limit remains unchanged; neither a timeout nor a
/// terminal report grants authority.
fn fresh_guard_events(
    rx: &std::sync::mpsc::Receiver<GuardEvent>,
    last: &mut Option<(u8, Instant)>,
    alive: bool,
) -> Option<ConsoleSample> {
    let mut witness = drain_guard_events(rx, last);
    if !alive
        || !last.is_some_and(|(code, at)| {
            code == b'R' && at.elapsed() >= muster_core::locked_use::MAX_OBSERVATION_AGE
        })
    {
        return witness;
    }
    let deadline = Instant::now() + Duration::from_millis(100);
    while let Ok(event) = rx.recv_timeout(deadline.saturating_duration_since(Instant::now())) {
        match event {
            GuardEvent::Unlocked(sample) => witness = Some(sample),
            GuardEvent::Status(code, at) => {
                *last = Some((code, at));
                if code != b'R' || at.elapsed() < muster_core::locked_use::MAX_OBSERVATION_AGE {
                    break;
                }
            }
        }
    }
    drain_guard_events(rx, last).or(witness)
}
// Dropping a Child does not kill it: EOF makes the independent watchdog
// quarantine the display until a different login audit session is observed.

fn initial_screen_matches(expected: &str, observed: Screen) -> bool {
    matches!(
        (expected, observed),
        ("locked", Screen::Locked) | ("unlocked", Screen::Unlocked)
    )
}

/// Must run on the process main thread, before starting Tokio/AppKit elsewhere.
pub fn watchdog(args: &[String]) -> Result<(), String> {
    use std::os::unix::fs::OpenOptionsExt;
    if args.len() != 4 || args[1].len() != 64 || !args[1].bytes().all(|b| b.is_ascii_hexdigit()) {
        return Err("invalid watchdog arguments".into());
    }
    let mut deadline: u64 = args[2].parse().map_err(|_| "invalid deadline")?;
    let remaining = deadline.saturating_sub(monotonic_millis());
    if !(1..=15000).contains(&remaining) {
        return Err("expired or invalid watchdog deadline".into());
    }
    let original = sample();
    let console = original.console.ok_or("no console")?;
    if !initial_screen_matches(&args[3], original.screen) {
        return Err("console changed before cover startup".into());
    }
    let mut display_awake = Some(DisplayAwake::begin(deadline)?);
    let marker = Path::new(&args[0]).join("locked-use.pending");
    let mut file = std::fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .mode(0o600)
        .open(&marker)
        .map_err(|e| e.to_string())?;
    writeln!(
        file,
        "{}:{}:{}",
        console.uid, console.audit_session, args[1]
    )
    .map_err(|e| e.to_string())?;
    file.sync_all().map_err(|e| e.to_string())?;
    if !unsafe { muster_console_cover() } {
        return Err("cannot cover every display".into());
    }
    unsafe {
        libc::fcntl(0, libc::F_SETFL, libc::O_NONBLOCK);
        libc::fcntl(1, libc::F_SETFL, libc::O_NONBLOCK);
    }
    let started = Instant::now();
    let mut heartbeat = started;
    let mut unlock_reported = false;
    let mut stopping = false;
    let mut stop_code = b'Q';
    let mut last_lock = started - Duration::from_secs(1);
    let mut last_report = last_lock;
    let mut terminal_reported = false;
    // An extension frame: `x`, then 8 little-endian bytes of a new absolute
    // deadline, possibly split across reads.
    let mut extension: Option<Vec<u8>> = None;
    loop {
        unsafe {
            muster_console_pump();
        }
        let mut bytes = [0; 64];
        let count = unsafe { libc::read(0, bytes.as_mut_ptr().cast(), bytes.len()) };
        let read_at = Instant::now();
        if count == 0 {
            stopping = true;
        }
        if count > 0 {
            for byte in &bytes[..count as usize] {
                if let Some(frame) = extension.as_mut() {
                    frame.push(*byte);
                    if frame.len() == 8 {
                        let requested = u64::from_le_bytes(frame[..].try_into().unwrap());
                        extension = None;
                        // Only forward, only while healthy, never more than
                        // the maximum from now: a renewal the owner verified.
                        if !stopping
                            && requested > deadline
                            && requested <= monotonic_millis().saturating_add(MAX_EXTENSION_MS)
                        {
                            deadline = requested;
                            display_awake = Some(DisplayAwake::begin(deadline)?);
                        } else {
                            stopping = true;
                        }
                    }
                    continue;
                }
                match byte {
                    b'x' => extension = Some(Vec::with_capacity(8)),
                    b'h' => heartbeat = read_at,
                    b'k' => stopping = true,
                    b'f' => {
                        // Only the live Rust transaction owner can send this;
                        // the pipe is inherited, never a public IPC endpoint.
                        std::fs::remove_file(&marker).map_err(|e| e.to_string())?;
                        unsafe {
                            muster_console_uncover();
                            libc::write(1, b"F".as_ptr().cast(), 1);
                        }
                        return Ok(());
                    }
                    _ => stopping = true,
                }
            }
        }
        let observed_millis = monotonic_millis();
        let now = sample();
        let witness = if !unlock_reported
            && now.screen == Screen::Unlocked
            && now.console == Some(console)
            && now.input_generation == original.input_generation
            && let Some(input) = now.input_generation
        {
            Some((observed_millis, console, input))
        } else {
            None
        };
        let cover_started = Instant::now();
        let healthy = unsafe { muster_console_cover_healthy(now.screen == Screen::Unlocked) };
        if cover_started.elapsed() > Duration::from_millis(100) {
            log::warn!(
                "Locked Use watchdog: display cover query took {} ms",
                cover_started.elapsed().as_millis()
            );
        }
        let expired = monotonic_millis() >= deadline;
        let heartbeat_lost = heartbeat_expired(heartbeat, read_at);
        let console_changed = now.console != Some(console);
        let input_changed = now.input_generation != original.input_generation;
        if !stopping && (expired || heartbeat_lost || console_changed || input_changed || !healthy)
        {
            stop_code = if expired {
                b'E'
            } else if heartbeat_lost {
                b'H'
            } else if console_changed {
                b'C'
            } else if input_changed {
                b'I'
            } else {
                b'V'
            };
            stopping = true;
        }
        if stopping {
            // Quarantine may outlive an interrupted operation. It must not
            // keep the display awake after authority has ended.
            display_awake.take();
        }
        if now.console.is_some_and(|c| c != console) {
            // Never lock a different user's console. Logout destroys the old
            // session's possible authorization; a new login may clear marker.
            unsafe {
                muster_console_uncover();
            }
            return Ok(());
        }
        if stopping
            && now.console == Some(console)
            && last_lock.elapsed() > Duration::from_millis(200)
        {
            lock();
            last_lock = Instant::now();
        }
        if witness.is_some()
            || (!terminal_reported && last_report.elapsed() >= Duration::from_millis(40))
        {
            let b = if healthy && !stopping {
                b'R'
            } else {
                stop_code
            };
            let frame = guard_report(b, witness);
            // Below PIPE_BUF: either the entire status+witness arrives, or no
            // bytes do. A failed write is retried with fresh observations.
            let written = unsafe { libc::write(1, frame.as_ptr().cast(), frame.len()) }
                == frame.len() as isize;
            unlock_reported |= written && frame.len() > 1;
            terminal_reported = stopping && written;
            last_report = Instant::now();
        }
    }
}

/// The owner must write a heartbeat at least this often.
const HEARTBEAT_LIMIT: Duration = Duration::from_millis(350);
/// The furthest a renewal may move the watchdog's deadline from now.
const MAX_EXTENSION_MS: u64 = 15_000;

/// Ages the newest heartbeat as of the last pipe read, not after this
/// watchdog's own OS sampling. A slow sample or cover check cannot age a
/// heartbeat that already arrived, while a stalled owner writes nothing and
/// its age keeps growing on every read.
fn heartbeat_expired(heartbeat: Instant, read_at: Instant) -> bool {
    read_at.saturating_duration_since(heartbeat) > HEARTBEAT_LIMIT
}

/// Read-only OS diagnostic: no AX tree or credential contents.
pub fn diagnostic() -> String {
    let s = sample();
    format!(
        "console={:?} screen={:?} input={:?} accessibility={}",
        s.console,
        s.screen,
        s.input_generation,
        ax::trusted()
    )
}

// An absolute cross-process deadline prevents process startup from renewing TTL.
fn monotonic_millis() -> u64 {
    let mut ts = libc::timespec {
        tv_sec: 0,
        tv_nsec: 0,
    };
    if unsafe { libc::clock_gettime(libc::CLOCK_MONOTONIC, &mut ts) } != 0 {
        return u64::MAX;
    }
    (ts.tv_sec as u64)
        .saturating_mul(1000)
        .saturating_add(ts.tv_nsec as u64 / 1_000_000)
}

#[cfg(test)]
mod heartbeat_tests {
    #[test]
    fn stale_ready_waits_for_fresh_evidence_without_refreshing_old_timestamp() {
        use super::*;
        let (tx, rx) = std::sync::mpsc::channel();
        let old = Instant::now() - Duration::from_secs(1);
        let mut last = Some((b'R', old));
        let worker = std::thread::spawn(move || {
            std::thread::sleep(Duration::from_millis(10));
            tx.send(GuardEvent::Status(b'R', Instant::now())).unwrap();
        });
        fresh_guard_events(&rx, &mut last, true);
        worker.join().unwrap();
        assert!(last.unwrap().1 > old);
        assert!(last.unwrap().1.elapsed() < muster_core::locked_use::MAX_OBSERVATION_AGE);

        let (_tx, rx) = std::sync::mpsc::channel();
        let mut last = Some((b'R', old));
        fresh_guard_events(&rx, &mut last, true);
        assert_eq!(last, Some((b'R', old)));
    }

    #[test]
    fn terminal_status_is_never_replaced_by_waiting_for_recovery() {
        use super::*;
        let (tx, rx) = std::sync::mpsc::channel();
        let old = Instant::now() - Duration::from_secs(1);
        let mut last = Some((b'R', old));
        tx.send(GuardEvent::Status(b'V', Instant::now())).unwrap();
        fresh_guard_events(&rx, &mut last, true);
        assert_eq!(last.unwrap().0, b'V');
    }
    use super::*;

    #[test]
    fn unlock_witness_follows_status_and_preserves_late_unlock_after_failure() {
        let witness = Some((
            1234,
            Console {
                uid: 501,
                audit_session: 7,
            },
            99,
        ));
        let ready = guard_report(b'R', witness);
        assert_eq!(&ready[..2], b"RU");
        assert_eq!(ready.len(), 26);
        assert_eq!(u64::from_le_bytes(ready[2..10].try_into().unwrap()), 1234);
        for failure in *b"VHICEQ" {
            let stopped = guard_report(failure, witness);
            assert_eq!(stopped[0], failure);
            assert_eq!(&stopped[1..], &ready[1..]);
        }
        assert_eq!(guard_report(b'R', None), b"R");
    }

    #[test]
    fn slow_owner_drains_latest_status_and_preserves_unlock_witness() {
        let (tx, rx) = std::sync::mpsc::channel();
        let start = Instant::now();
        for n in 0..20 {
            tx.send(GuardEvent::Status(
                b'R',
                start + Duration::from_millis(n * 40),
            ))
            .unwrap();
        }
        let sample = ConsoleSample {
            at: start,
            console: None,
            screen: Screen::Unlocked,
            input_generation: Some(7),
        };
        tx.send(GuardEvent::Unlocked(sample)).unwrap();
        let mut last = None;
        let unlock = drain_guard_events(&rx, &mut last).unwrap();
        assert_eq!(unlock.input_generation, Some(7));
        assert_eq!(last, Some((b'R', start + Duration::from_millis(760))));
        tx.send(GuardEvent::Status(b'V', start + Duration::from_millis(800)))
            .unwrap();
        assert!(drain_guard_events(&rx, &mut last).is_none());
        assert_eq!(last.unwrap().0, b'V');
    }

    #[test]
    fn startup_requires_the_exact_admitted_screen_state() {
        for expected in ["locked", "unlocked", "", "unknown"] {
            for observed in [Screen::Locked, Screen::Unlocked, Screen::Unknown] {
                assert_eq!(
                    initial_screen_matches(expected, observed),
                    (expected == "locked" && observed == Screen::Locked)
                        || (expected == "unlocked" && observed == Screen::Unlocked)
                );
            }
        }
    }

    #[test]
    fn heartbeat_age_is_measured_at_the_pipe_read() {
        let heartbeat = Instant::now();
        assert!(!heartbeat_expired(heartbeat, heartbeat));
        assert!(!heartbeat_expired(heartbeat, heartbeat + HEARTBEAT_LIMIT));
        assert!(heartbeat_expired(
            heartbeat,
            heartbeat + HEARTBEAT_LIMIT + Duration::from_millis(1)
        ));
        // A heartbeat read after a slow sample is fresh, never negative.
        assert!(!heartbeat_expired(
            heartbeat + Duration::from_millis(900),
            heartbeat
        ));
    }
}
