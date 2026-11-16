//! The Locked Use cover (ADR 0222). Each display is dimmed to brightness
//! zero where Muster can set and read back its level, and only the others get
//! the black panel. A panel over the whole screen occludes every window under
//! it, and an occluded Electron window (the Claude app) stops updating what
//! Accessibility reads. A dimmed display occludes nothing.
//!
//! The previous levels are written to a record before any level changes, and
//! the watchdog holds an exclusive `flock` on it for as long as it lives.
//! Whatever ends the watchdog, its panels end with it; a brightness level does
//! not, and a display keeps a DDC level even across a restart.
//!
//! DDC requests stay clear of the moments the display link changes: the
//! console locking or unlocking, a display waking. On the Mac tested, DDC
//! requests at those moments made the display connection reset, and a level
//! written during the reset read back correctly and was then undone by the
//! display. So every display starts under the black panel, a display is
//! dimmed only once the console has been unlocked and the displays steady for
//! a while, DDC reads stop as soon as the cover begins to end, and the levels
//! are put back by the panel helper, started by musterd, once the displays
//! are steady again.

use std::{
    ffi::{CStr, CString, c_char, c_void},
    fs::{File, OpenOptions},
    io::{Read, Seek, SeekFrom, Write},
    os::{
        fd::AsRawFd,
        unix::fs::{MetadataExt, OpenOptionsExt},
    },
    path::{Path, PathBuf},
    sync::{
        Arc,
        atomic::{AtomicBool, AtomicU32, Ordering},
    },
    thread::JoinHandle,
    time::{Duration, Instant},
};

unsafe extern "C" {
    fn muster_display_online(displays: *mut u32, capacity: u32) -> u32;
    fn muster_display_mirror_master(display: u32) -> u32;
    fn muster_display_uuid(display: u32, out: *mut c_char, size: usize) -> bool;
    fn muster_display_from_uuid(uuid: *const c_char) -> u32;
    fn muster_native_brightness_get(display: u32, value: *mut f32) -> bool;
    fn muster_native_brightness_set(display: u32, value: f32) -> bool;
    fn muster_ddc_location(display: u32, out: *mut c_char, size: usize) -> bool;
    fn muster_ddc_open(location: *const c_char) -> *mut c_void;
    fn muster_ddc_close(ddc: *mut c_void);
    fn muster_ddc_read(ddc: *mut c_void, code: u8, current: *mut u16, maximum: *mut u16) -> bool;
    fn muster_ddc_write(ddc: *mut c_void, code: u8, value: u16) -> bool;
    fn muster_display_asleep(display: u32) -> bool;
    fn muster_console_sample(uid: *mut u32, asid: *mut u32, locked: *mut bool) -> bool;
    fn muster_console_cover(displays: *const u32, count: u32) -> bool;
    fn muster_console_cover_keep(displays: *const u32, count: u32);
    fn muster_console_wake_begin(assertion: *mut u32) -> bool;
    fn muster_console_wake_end(assertion: u32) -> bool;
    fn muster_console_cover_healthy(unlocked: bool) -> bool;
    fn muster_console_uncover();
}

/// The record of the levels a cover changed, in musterd's state directory.
pub const RECORD: &str = "locked-use.brightness";
/// VCP code 0x10, luminance.
const LUMINANCE: u8 = 0x10;
/// A native level at or below this is dark.
const DARK: f32 = 0.001;
/// How often the watch thread reads each DDC display back.
const WATCH_INTERVAL: Duration = Duration::from_millis(250);
/// A waking DDC display creates its I2C service again, so the old handle
/// stops answering until a new one is opened, about two seconds on the
/// display tested. A display that gives no valid answer for this long, with
/// the handle reopened in between, counts as lost.
const DDC_SETTLE: Duration = Duration::from_secs(5);
/// How long the displays must have stayed as they are, awake, online under
/// the same IDs and UUIDs, and with the console neither locking nor
/// unlocking, before a DDC request is sent. The display tested set its own
/// level again about two seconds after a wake, undoing a level set while it
/// slept or as it woke.
const STEADY: Duration = Duration::from_secs(3);
/// How often a restorer looks at the displays while it waits.
const RESTORE_POLL: Duration = Duration::from_millis(500);
/// Rounds in which a restorer tries a steady display before it gives up.
const RESTORE_TRIES: u32 = 20;
/// The lock that keeps a second restorer from starting.
const RESTORER: &str = "locked-use.restorer";

/// Why a cover no longer hides the screen. The watchdog sends it to musterd
/// in an `L` frame: the code, then a little-endian detail.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum Fault {
    /// The record, and with it the lock, is gone.
    Record,
    /// The online displays changed; how many are online now.
    Displays(u16),
    /// A natively dimmed display reads above zero; its level in thousandths.
    Native(u16),
    /// A DDC display reads above zero; the level it reads.
    Ddc(u16),
    /// A DDC display gave no valid answer for `DDC_SETTLE`.
    DdcSilent,
    /// A black panel failed its check.
    Panel,
}

impl Fault {
    fn code(self) -> (u8, u16) {
        match self {
            Fault::Record => (1, 0),
            Fault::Displays(count) => (2, count),
            Fault::Native(level) => (3, level),
            Fault::Ddc(level) => (4, level),
            Fault::DdcSilent => (5, 0),
            Fault::Panel => (6, 0),
        }
    }

    fn from_code(code: u8, detail: u16) -> Option<Self> {
        Some(match code {
            1 => Fault::Record,
            2 => Fault::Displays(detail),
            3 => Fault::Native(detail),
            4 => Fault::Ddc(detail),
            5 => Fault::DdcSilent,
            6 => Fault::Panel,
            _ => return None,
        })
    }

    pub(super) fn frame(self) -> [u8; 4] {
        let (code, detail) = self.code();
        let [low, high] = detail.to_le_bytes();
        [b'L', code, low, high]
    }

    /// The payload of an `L` frame, for the log.
    pub(super) fn describe(payload: [u8; 3]) -> String {
        let detail = u16::from_le_bytes([payload[1], payload[2]]);
        match Self::from_code(payload[0], detail) {
            Some(Fault::Record) => "the brightness record is gone".into(),
            Some(Fault::Displays(count)) => format!("the online displays changed ({count} now)"),
            Some(Fault::Native(level)) => {
                format!("a natively dimmed display reads {level}/1000")
            }
            Some(Fault::Ddc(level)) => format!("a DDC display reads level {level}"),
            Some(Fault::DdcSilent) => "a DDC display stopped answering".into(),
            Some(Fault::Panel) => "a black panel failed its check".into(),
            None => format!("unknown fault {}", payload[0]),
        }
    }

    fn pack(self) -> u32 {
        let (code, detail) = self.code();
        u32::from(code) << 16 | u32::from(detail)
    }

    fn unpack(packed: u32) -> Option<Self> {
        Self::from_code((packed >> 16) as u8, packed as u16)
    }
}

fn online() -> Vec<u32> {
    let mut displays = [0u32; 32];
    let count = unsafe { muster_display_online(displays.as_mut_ptr(), displays.len() as u32) };
    let mut online = displays[..count as usize].to_vec();
    online.sort_unstable();
    online
}

fn display_uuid(display: u32) -> Option<String> {
    let mut text = [0 as c_char; 64];
    unsafe { muster_display_uuid(display, text.as_mut_ptr(), text.len()) }
        .then(|| {
            unsafe { CStr::from_ptr(text.as_ptr()) }
                .to_str()
                .ok()
                .map(str::to_owned)
        })
        .flatten()
        .filter(|uuid| valid_uuid(uuid))
}

/// The online display with this UUID.
fn display_for(uuid: &str) -> Option<u32> {
    let text = CString::new(uuid).ok()?;
    let display = unsafe { muster_display_from_uuid(text.as_ptr()) };
    (display != 0 && online().contains(&display)).then_some(display)
}

fn native_get(display: u32) -> Option<f32> {
    let mut value = 0.0;
    unsafe { muster_native_brightness_get(display, &mut value) }.then_some(value)
}

fn native_set(display: u32, value: f32) -> bool {
    unsafe { muster_native_brightness_set(display, value) }
}

/// A display's DDC channel.
/// A display's DDC channel and where to find it again: the framebuffer's
/// registry location stays when a waking display replaces its I2C service.
struct Ddc {
    handle: *mut c_void,
    location: CString,
}
// SAFETY: an IOAVService handle has no thread affinity, and each one is used
// by one thread at a time: the watch thread takes it and gives it back.
unsafe impl Send for Ddc {}
impl Ddc {
    fn open(display: u32) -> Option<Self> {
        let mut text = [0 as c_char; 512];
        if !unsafe { muster_ddc_location(display, text.as_mut_ptr(), text.len()) } {
            return None;
        }
        let location = unsafe { CStr::from_ptr(text.as_ptr()) }.to_owned();
        let handle = unsafe { muster_ddc_open(location.as_ptr()) };
        (!handle.is_null()).then_some(Self { handle, location })
    }
    /// Only IOKit, so the watch thread may call it. The handle may be null
    /// until the display's new service exists; reads then fail.
    fn reopen(&mut self) {
        unsafe {
            muster_ddc_close(self.handle);
            self.handle = muster_ddc_open(self.location.as_ptr());
        }
    }
    fn read_once(&self) -> Option<u16> {
        let (mut current, mut maximum) = (0, 0);
        unsafe { muster_ddc_read(self.handle, LUMINANCE, &mut current, &mut maximum) }
            .then_some(current)
    }
    /// A display sometimes answers a request with nothing; ask up to three times.
    fn read(&self) -> Option<u16> {
        (0..3).find_map(|_| self.read_once())
    }
    fn write(&self, level: u16) -> bool {
        unsafe { muster_ddc_write(self.handle, LUMINANCE, level) }
    }
}
impl Drop for Ddc {
    fn drop(&mut self) {
        unsafe { muster_ddc_close(self.handle) }
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
enum Level {
    Native(f32),
    Ddc(u16),
}

/// One display's level before a cover changed it.
#[derive(Clone, Debug, PartialEq)]
struct Saved {
    uuid: String,
    level: Level,
}

fn valid_uuid(uuid: &str) -> bool {
    uuid.len() == 36 && uuid.bytes().all(|b| b.is_ascii_hexdigit() || b == b'-')
}

fn format(entries: &[Saved]) -> String {
    entries
        .iter()
        .map(|entry| match entry.level {
            Level::Native(value) => format!("native {} {value}\n", entry.uuid),
            Level::Ddc(value) => format!("ddc {} {value}\n", entry.uuid),
        })
        .collect()
}

/// Lines that do not parse are dropped: a level that cannot be read back
/// cannot be put back either.
fn parse(text: &str) -> Vec<Saved> {
    text.lines()
        .filter_map(|line| {
            let mut fields = line.split(' ');
            let (kind, uuid, value) = (fields.next()?, fields.next()?, fields.next()?);
            if fields.next().is_some() || !valid_uuid(uuid) {
                return None;
            }
            let level = match kind {
                "native" => Level::Native(
                    value
                        .parse::<f32>()
                        .ok()
                        .filter(|v| (0.0..=1.0).contains(v))?,
                ),
                "ddc" => Level::Ddc(value.parse().ok()?),
                _ => return None,
            };
            Some(Saved {
                uuid: uuid.to_owned(),
                level,
            })
        })
        .collect()
}

enum Restore {
    Done,
    Later,
}

/// Puts one display back if it is still dark. A level someone changed since
/// is theirs and stays. For a DDC display, call it only once the display is
/// steady: see `Steady`.
fn put_back(display: u32, level: Level) -> Restore {
    match level {
        Level::Native(previous) => match native_get(display) {
            None => Restore::Later,
            Some(now) if now > DARK => Restore::Done,
            Some(_) if native_set(display, previous) => Restore::Done,
            Some(_) => Restore::Later,
        },
        Level::Ddc(previous) => ddc_back(display, previous),
    }
}

/// Through a new handle: one from before a display reconfigured can answer
/// with an old reply. A level written back must still read back a second
/// later.
fn ddc_back(display: u32, previous: u16) -> Restore {
    let Some(ddc) = Ddc::open(display) else {
        return Restore::Later;
    };
    match ddc.read() {
        None => Restore::Later,
        Some(now) if now == previous => Restore::Done,
        Some(0) => {
            if !(ddc.write(previous) && ddc.read() == Some(previous)) {
                return Restore::Later;
            }
            std::thread::sleep(Duration::from_secs(1));
            if ddc.read() == Some(previous) {
                Restore::Done
            } else {
                Restore::Later
            }
        }
        Some(_) => Restore::Done,
    }
}

fn restore(entry: &Saved) -> Restore {
    match display_for(&entry.uuid) {
        Some(display) => put_back(display, entry.level),
        None => Restore::Later,
    }
}

/// The displays online, each with its UUID and whether it is asleep, and
/// whether the console is locked.
#[derive(Clone, Debug, PartialEq)]
struct Config {
    displays: Vec<(u32, Option<String>, bool)>,
    locked: bool,
}

/// Whether the displays have stayed as they are for `STEADY`.
struct Steady {
    config: Option<Config>,
    since: Instant,
}
impl Steady {
    fn new() -> Self {
        Self {
            config: None,
            since: Instant::now(),
        }
    }

    fn sample(&mut self, locked: bool) {
        let displays = online()
            .into_iter()
            .map(|display| {
                (display, display_uuid(display), unsafe {
                    muster_display_asleep(display)
                })
            })
            .collect();
        self.observe(Config { displays, locked }, Instant::now());
    }

    fn observe(&mut self, config: Config, at: Instant) {
        if self.config.as_ref() != Some(&config) {
            self.config = Some(config);
            self.since = at;
        }
    }

    fn held(&self, now: Instant) -> Option<&Config> {
        (now.duration_since(self.since) >= STEADY)
            .then_some(self.config.as_ref())
            .flatten()
    }

    /// A display that is awake, once the displays have been steady.
    fn ready(&self, display: u32, now: Instant) -> bool {
        self.held(now).is_some_and(|config| {
            config
                .displays
                .iter()
                .any(|(id, _, asleep)| *id == display && !asleep)
        })
    }

    /// Every one of `online`, and only those, awake under an unlocked
    /// console, once the displays have been steady.
    fn all_ready(&self, online: &[u32], now: Instant) -> bool {
        self.held(now).is_some_and(|config| {
            !config.locked
                && config.displays.len() == online.len()
                && config
                    .displays
                    .iter()
                    .all(|(id, _, asleep)| online.contains(id) && !asleep)
        })
    }
}

/// Whether the console is locked; an unknown console counts as locked.
fn console_locked() -> bool {
    let (mut uid, mut asid, mut locked) = (0, 0, false);
    !unsafe { muster_console_sample(&mut uid, &mut asid, &mut locked) } || locked
}

/// The record file under its exclusive lock.
struct Record(File);
impl Record {
    fn try_lock(file: &File) -> std::io::Result<bool> {
        if unsafe { libc::flock(file.as_raw_fd(), libc::LOCK_EX | libc::LOCK_NB) } == 0 {
            return Ok(true);
        }
        let error = std::io::Error::last_os_error();
        if error.raw_os_error() == Some(libc::EWOULDBLOCK) {
            Ok(false)
        } else {
            Err(error)
        }
    }

    /// Whether the locked file is still the one at `path`: a holder that
    /// finished may have removed it while this one waited.
    fn current(file: &File, path: &Path) -> bool {
        match (file.metadata(), std::fs::metadata(path)) {
            (Ok(held), Ok(named)) => held.ino() == named.ino() && held.dev() == named.dev(),
            _ => false,
        }
    }

    /// For the watchdog: creates the record if needed and waits for a
    /// restorer to finish with it. Another live cover holds it for good, so
    /// the wait is bounded.
    fn hold(path: &Path) -> Result<Self, String> {
        let deadline = Instant::now() + Duration::from_secs(2);
        loop {
            let file = OpenOptions::new()
                .read(true)
                .write(true)
                .create(true)
                .truncate(false)
                .mode(0o600)
                .open(path)
                .map_err(|e| format!("brightness record: {e}"))?;
            if Self::try_lock(&file).map_err(|e| format!("brightness record lock: {e}"))?
                && Self::current(&file, path)
            {
                return Ok(Self(file));
            }
            if Instant::now() >= deadline {
                return Err("another cover holds the brightness record".into());
            }
            std::thread::sleep(Duration::from_millis(20));
        }
    }

    /// For a restorer: the record only if it exists and no watchdog holds it.
    fn take(path: &Path) -> Result<Option<Self>, String> {
        let file = match OpenOptions::new().read(true).write(true).open(path) {
            Ok(file) => file,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(None),
            Err(e) => return Err(format!("brightness record: {e}")),
        };
        if !Self::try_lock(&file).map_err(|e| format!("brightness record lock: {e}"))?
            || !Self::current(&file, path)
        {
            return Ok(None);
        }
        Ok(Some(Self(file)))
    }

    fn entries(&mut self) -> Vec<Saved> {
        let mut text = String::new();
        let _ = self.0.seek(SeekFrom::Start(0));
        let _ = self.0.read_to_string(&mut text);
        parse(&text)
    }

    fn store(&mut self, entries: &[Saved]) -> Result<(), String> {
        let text = format(entries);
        self.0
            .set_len(0)
            .and_then(|_| self.0.seek(SeekFrom::Start(0)))
            .and_then(|_| self.0.write_all(text.as_bytes()))
            .and_then(|_| self.0.sync_all())
            .map_err(|e| format!("brightness record: {e}"))
    }

    /// Empties and removes the record; the lock goes with the file.
    fn clear(self, path: &Path) {
        let _ = self.0.set_len(0);
        let _ = std::fs::remove_file(path);
    }

    /// Keeps what is still to be put back, or removes the record.
    fn finish(mut self, path: &Path, left: &[Saved]) {
        if left.is_empty() {
            self.clear(path);
        } else if let Err(e) = self.store(left) {
            log::warn!("Locked Use: {e}");
        }
    }
}

/// Puts back, and confirms, the levels in the record: those a cover wrote
/// back as it ended and those of a cover that was killed. Waits for each
/// display to be steady, and for a display that is asleep to wake; writes the
/// record only while no watchdog holds it. Returns once nothing is left, no
/// recorded display is online, or a steady display kept failing. One
/// restorer runs at a time; another returns at once.
///
/// Display calls need the process's main thread: musterd runs this in the
/// panel helper (`muster-panel --locked-use-restore`), never in itself.
/// `report` gets a line for musterd's log whenever something changed.
pub fn restore_saved(dir: &Path, mut report: impl FnMut(String)) {
    let Ok(restorer) = OpenOptions::new()
        .read(true)
        .write(true)
        .create(true)
        .truncate(false)
        .mode(0o600)
        .open(dir.join(RESTORER))
    else {
        return;
    };
    if !Record::try_lock(&restorer).unwrap_or(false) {
        return;
    }
    let path = dir.join(RECORD);
    let mut steady = Steady::new();
    let mut tries = 0;
    while path.exists() {
        steady.sample(console_locked());
        let now = Instant::now();
        let mut tried = false;
        let pass = restore_saved_with(dir, |entry| match display_for(&entry.uuid) {
            Some(display) if steady.ready(display, now) => {
                tried = true;
                put_back(display, entry.level)
            }
            _ => Restore::Later,
        });
        if let Some(pass) = pass {
            if pass.left.len() < pass.entries {
                report(format!(
                    "put back {} display level(s) a cover left; {} still wait",
                    pass.entries - pass.left.len(),
                    pass.left.len()
                ));
            }
            if pass.left.is_empty() {
                return;
            }
            if !pass
                .left
                .iter()
                .any(|entry| display_for(&entry.uuid).is_some())
            {
                report(format!(
                    "{} display level(s) wait for a display that is not online",
                    pass.left.len()
                ));
                return;
            }
        }
        tries += u32::from(tried);
        if tries >= RESTORE_TRIES {
            report("a steady display did not take its level back; it stays in the record".into());
            return;
        }
        std::thread::sleep(RESTORE_POLL);
    }
}

/// One pass of a restorer: how many entries the record had and which are
/// left. `None` when there is no record or a watchdog holds it. The record is
/// rewritten only when an entry was put back.
struct Pass {
    entries: usize,
    left: Vec<Saved>,
}

fn restore_saved_with(dir: &Path, mut restore: impl FnMut(&Saved) -> Restore) -> Option<Pass> {
    let path = dir.join(RECORD);
    let mut record = match Record::take(&path) {
        Ok(Some(record)) => record,
        Ok(None) => return None,
        Err(e) => {
            log::warn!("Locked Use: {e}");
            return None;
        }
    };
    let entries = record.entries();
    let left: Vec<Saved> = entries
        .iter()
        .filter(|entry| matches!(restore(entry), Restore::Later))
        .cloned()
        .collect();
    if left.len() < entries.len() || entries.is_empty() {
        record.finish(&path, &left);
    }
    Some(Pass {
        entries: entries.len(),
        left,
    })
}

/// A display this cover dimmed.
struct Dimmed<T> {
    display: u32,
    saved: Saved,
    control: T,
}

/// Reads every DDC display back while the cover is up. A level that is not
/// zero, or no answer for three rounds, is a lost cover.
struct Watch {
    stop: Arc<AtomicBool>,
    /// The first fault, packed; zero while there is none.
    fault: Arc<AtomicU32>,
    thread: JoinHandle<()>,
}
impl Watch {
    fn start(displays: Vec<Dimmed<Ddc>>) -> Self {
        let stop = Arc::new(AtomicBool::new(false));
        let fault = Arc::new(AtomicU32::new(0));
        let (halt, first) = (stop.clone(), fault.clone());
        let thread = std::thread::spawn(move || {
            let mut displays = displays;
            let mut silent: Vec<Option<Instant>> = vec![None; displays.len()];
            let lose = |fault: Fault| {
                let _ =
                    first.compare_exchange(0, fault.pack(), Ordering::AcqRel, Ordering::Acquire);
            };
            while !halt.load(Ordering::Acquire) {
                for (dimmed, since) in displays.iter_mut().zip(&mut silent) {
                    match dimmed.control.read_once() {
                        Some(0) => *since = None,
                        Some(level) => lose(Fault::Ddc(level)),
                        None => {
                            if since.get_or_insert_with(Instant::now).elapsed() >= DDC_SETTLE {
                                lose(Fault::DdcSilent);
                            }
                            dimmed.control.reopen();
                        }
                    }
                }
                let next = Instant::now() + WATCH_INTERVAL;
                while !halt.load(Ordering::Acquire) && Instant::now() < next {
                    std::thread::sleep(Duration::from_millis(25));
                }
            }
        });
        Self {
            stop,
            fault,
            thread,
        }
    }

    fn fault(&self) -> Option<Fault> {
        Fault::unpack(self.fault.load(Ordering::Acquire))
    }

    /// Stops reading; the handles close with the thread.
    fn stop(self) {
        self.stop.store(true, Ordering::Release);
        let _ = self.thread.join();
    }
}

/// The cover over every online display: dimmed, or under the black panel.
pub(super) struct Shield {
    path: PathBuf,
    online: Vec<u32>,
    native: Vec<Dimmed<()>>,
    ddc: Option<Watch>,
    /// The DDC entries. Their levels are put back by display UUID: a display
    /// that reconfigured has a new ID and a new I2C service.
    ddc_saved: Vec<Saved>,
    panels: Vec<u32>,
    /// Entries in the record that are not confirmed back, and this cover
    /// does not dim: from an earlier cover, or a display it could not dim.
    leftover: Vec<Saved>,
    record: Option<Record>,
    waking: Option<Waking>,
    steady: Steady,
    /// Set once the displays were dimmed, or will no longer be.
    settled: bool,
    /// Why displays kept the panel, for the owner's log; sent once.
    note: Option<Note>,
}

/// Why displays kept the black panel.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum Note {
    /// The displays could not be woken.
    Unwoken = 1,
    /// The levels could not be recorded.
    Unrecorded = 3,
}

impl Note {
    /// An `S` frame: the code.
    pub(super) fn frame(self) -> [u8; 2] {
        [b'S', self as u8]
    }

    pub(super) fn describe(code: u8) -> &'static str {
        match code {
            1 => "the displays could not be woken; every display keeps the black panel",
            3 => "the levels could not be recorded; every display keeps the black panel",
            _ => "unknown note",
        }
    }
}

/// Keeps the displays that were asleep waking until they are dimmed.
struct Waking {
    assertion: u32,
}
impl Drop for Waking {
    fn drop(&mut self) {
        unsafe { muster_console_wake_end(self.assertion) };
    }
}

enum Plan {
    Native(Dimmed<()>),
    Ddc(Dimmed<Ddc>),
}

/// How a display can be dimmed, with its current level; `None` for a panel.
/// A display without a UUID gets the panel: its level could not be found
/// again if this process were killed.
fn plan(display: u32) -> Option<Plan> {
    let uuid = display_uuid(display)?;
    if let Some(value) = native_get(display) {
        return Some(Plan::Native(Dimmed {
            display,
            saved: Saved {
                uuid,
                level: Level::Native(value),
            },
            control: (),
        }));
    }
    let ddc = Ddc::open(display)?;
    let value = ddc.read()?;
    Some(Plan::Ddc(Dimmed {
        display,
        saved: Saved {
            uuid,
            level: Level::Ddc(value),
        },
        control: ddc,
    }))
}

/// Takes a display's level from an earlier cover's entry of the same kind,
/// if the record has one: the display may still be at that cover's zero.
fn adopt(leftover: &mut Vec<Saved>, saved: &mut Saved) {
    let same = |entry: &Saved| {
        entry.uuid == saved.uuid
            && std::mem::discriminant(&entry.level) == std::mem::discriminant(&saved.level)
    };
    if let Some(i) = leftover.iter().position(same) {
        *saved = leftover.remove(i);
    }
}

fn wait_dark(display: u32) -> bool {
    let deadline = Instant::now() + Duration::from_millis(300);
    loop {
        if native_get(display).is_some_and(|value| value <= DARK) {
            return true;
        }
        if Instant::now() >= deadline {
            return false;
        }
        std::thread::sleep(Duration::from_millis(20));
    }
}

/// A mirror set shows on one display; its panel covers the set.
fn panel_targets(displays: &[u32]) -> Vec<u32> {
    let mut targets: Vec<u32> = displays
        .iter()
        .map(|&display| unsafe { muster_display_mirror_master(display) })
        .collect();
    targets.sort_unstable();
    targets.dedup();
    targets
}

fn any_asleep(displays: &[u32]) -> bool {
    displays
        .iter()
        .any(|&display| unsafe { muster_display_asleep(display) })
}

impl Shield {
    /// Covers every display with the black panel and wakes those that are
    /// asleep; `settle` dims them later. Fails, with nothing changed, unless
    /// every online display is covered.
    pub(super) fn raise(dir: &Path) -> Result<Self, String> {
        let online = online();
        if online.is_empty() {
            return Err("no display to cover".into());
        }
        let path = dir.join(RECORD);
        let mut record = Record::hold(&path)?;
        // An earlier cover's levels that are not confirmed back yet. `dim`
        // takes a display's level from here rather than from the display,
        // which may still be at that cover's zero.
        let leftover = record.entries();
        let mut shield = Self {
            path,
            online,
            native: Vec::new(),
            ddc: None,
            ddc_saved: Vec::new(),
            panels: Vec::new(),
            leftover,
            record: Some(record),
            waking: None,
            steady: Steady::new(),
            settled: false,
            note: None,
        };
        if any_asleep(&shield.online) {
            // A display that cannot be woken keeps its panel.
            let mut assertion = 0;
            if unsafe { muster_console_wake_begin(&mut assertion) } {
                shield.waking = Some(Waking { assertion });
            } else {
                shield.note = Some(Note::Unwoken);
                shield.settled = true;
            }
        }
        let targets = panel_targets(&shield.online);
        // Always called: it also starts AppKit for the watchdog.
        let covered = unsafe { muster_console_cover(targets.as_ptr(), targets.len() as u32) };
        shield.panels = targets;
        if !covered {
            shield.release();
            return Err("cannot cover every display".into());
        }
        Ok(shield)
    }

    /// Reads and records every level, then dims each display it can. The
    /// displays it could not dim are returned for the panel.
    fn dim(&mut self) -> Result<Vec<u32>, String> {
        // Every level is read and recorded before any changes.
        let mut plans = Vec::new();
        let mut panels = Vec::new();
        for &display in &self.online {
            match plan(display) {
                Some(mut plan) => {
                    let saved = match &mut plan {
                        Plan::Native(dimmed) => &mut dimmed.saved,
                        Plan::Ddc(dimmed) => &mut dimmed.saved,
                    };
                    adopt(&mut self.leftover, saved);
                    plans.push(plan);
                }
                None => panels.push(display),
            }
        }
        let mut saved = self.leftover.clone();
        saved.extend(plans.iter().map(|plan| match plan {
            Plan::Native(dimmed) => dimmed.saved.clone(),
            Plan::Ddc(dimmed) => dimmed.saved.clone(),
        }));
        self.record
            .as_mut()
            .ok_or("the brightness record is gone")?
            .store(&saved)?;
        let mut ddc = Vec::new();
        for plan in plans {
            match plan {
                Plan::Native(dimmed) => {
                    if native_set(dimmed.display, 0.0) && wait_dark(dimmed.display) {
                        self.native.push(dimmed);
                    } else {
                        if let Restore::Later = put_back(dimmed.display, dimmed.saved.level) {
                            self.leftover.push(dimmed.saved);
                        }
                        panels.push(dimmed.display);
                    }
                }
                Plan::Ddc(dimmed) => {
                    if dimmed.control.write(0) && dimmed.control.read() == Some(0) {
                        self.ddc_saved.push(dimmed.saved.clone());
                        ddc.push(dimmed);
                    } else {
                        // Its level may have changed; a restorer confirms it.
                        if let Level::Ddc(previous) = dimmed.saved.level {
                            dimmed.control.write(previous);
                        }
                        self.leftover.push(dimmed.saved);
                        panels.push(dimmed.display);
                    }
                }
            }
        }
        if !ddc.is_empty() {
            self.ddc = Some(Watch::start(ddc));
        }
        Ok(panels)
    }

    /// Once the console has been unlocked and every display awake and
    /// unchanged for `STEADY`, dims what it can and takes the panel off each
    /// display it dimmed. True when the cover changed. A level that cannot be
    /// recorded leaves every panel where it is.
    pub(super) fn settle(&mut self, unlocked: bool) -> bool {
        if self.settled || self.record.is_none() {
            return false;
        }
        self.steady.sample(!unlocked);
        if !self.steady.all_ready(&self.online, Instant::now()) {
            return false;
        }
        self.settled = true;
        // From here the watchdog's display assertion keeps them awake.
        self.waking = None;
        let Ok(panels) = self.dim() else {
            self.note = Some(Note::Unrecorded);
            return false;
        };
        let targets = panel_targets(&panels);
        unsafe { muster_console_cover_keep(targets.as_ptr(), targets.len() as u32) };
        self.panels = targets;
        true
    }

    /// Why displays kept the panel, once.
    pub(super) fn take_note(&mut self) -> Option<Note> {
        self.note.take()
    }

    /// How many displays were dimmed natively, over DDC, and covered.
    pub(super) fn counts(&self) -> [u8; 3] {
        let count = |n: usize| n.min(u8::MAX as usize) as u8;
        [
            count(self.native.len()),
            count(self.ddc_saved.len()),
            count(self.panels.len()),
        ]
    }

    /// What, if anything, no longer holds: the same displays online, each
    /// one still dark, each panel still up.
    pub(super) fn fault(&self, unlocked: bool) -> Option<Fault> {
        if self.record.is_none() {
            return Some(Fault::Record);
        }
        let now = online();
        if now != self.online {
            return Some(Fault::Displays(now.len() as u16));
        }
        for dimmed in &self.native {
            match native_get(dimmed.display) {
                Some(value) if value <= DARK => {}
                value => {
                    let thousandths = value.map_or(u16::MAX, |v| (v * 1000.0).round() as u16);
                    return Some(Fault::Native(thousandths));
                }
            }
        }
        if let Some(fault) = self.ddc.as_ref().and_then(Watch::fault) {
            return Some(fault);
        }
        if !self.panels.is_empty() && !unsafe { muster_console_cover_healthy(unlocked) } {
            return Some(Fault::Panel);
        }
        None
    }

    /// The cover is ending: no more DDC requests, and no dimming, while the
    /// console locks again. Idempotent.
    pub(super) fn quiet(&mut self) {
        self.settled = true;
        self.waking = None;
        if let Some(watch) = self.ddc.take() {
            watch.stop();
        }
    }

    /// Puts the native levels back and removes the panels. A DDC level stays
    /// in the record for the panel helper to put back once the displays are
    /// steady. Idempotent.
    pub(super) fn release(&mut self) {
        self.quiet();
        let Some(record) = self.record.take() else {
            return;
        };
        let mut left = std::mem::take(&mut self.leftover);
        for dimmed in self.native.drain(..) {
            if let Restore::Later = restore(&dimmed.saved) {
                left.push(dimmed.saved);
            }
        }
        left.append(&mut self.ddc_saved);
        unsafe { muster_console_uncover() };
        record.finish(&self.path, &left);
    }
}

impl Drop for Shield {
    fn drop(&mut self) {
        self.release();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const A: &str = "0F1E2D3C-4B5A-6978-8796-A5B4C3D2E1F0";
    const B: &str = "00000000-1111-2222-3333-444455556666";

    #[test]
    fn a_fault_frame_names_what_failed() {
        for (fault, text) in [
            (Fault::Ddc(37), "a DDC display reads level 37"),
            (Fault::DdcSilent, "a DDC display stopped answering"),
            (Fault::Displays(2), "the online displays changed (2 now)"),
            (
                Fault::Native(400),
                "a natively dimmed display reads 400/1000",
            ),
        ] {
            let frame = fault.frame();
            assert_eq!(frame[0], b'L');
            assert_eq!(Fault::describe([frame[1], frame[2], frame[3]]), text);
            assert_eq!(Fault::unpack(fault.pack()), Some(fault));
        }
        assert_eq!(Fault::unpack(0), None);
    }

    fn dir() -> PathBuf {
        let dir = std::env::temp_dir().join(format!(
            "muster-dimming-{}-{:?}",
            std::process::id(),
            std::thread::current().id()
        ));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    #[test]
    fn the_record_round_trips_and_drops_what_it_cannot_put_back() {
        let entries = vec![
            Saved {
                uuid: A.into(),
                level: Level::Native(0.6875),
            },
            Saved {
                uuid: B.into(),
                level: Level::Ddc(50),
            },
        ];
        assert_eq!(parse(&format(&entries)), entries);
        let noisy = format!(
            "{}native {A} 1.5\nnative {A} NaN\nddc {B} 70000\nddc {B} 5 extra\nddc short 5\nother {A} 1\n\n",
            format(&entries)
        );
        assert_eq!(parse(&noisy), entries);
    }

    #[test]
    fn a_held_record_is_left_to_its_holder_and_found_once_released() {
        let dir = dir();
        let path = dir.join(RECORD);
        assert!(Record::take(&path).unwrap().is_none());
        let mut held = Record::hold(&path).unwrap();
        held.store(&[Saved {
            uuid: A.into(),
            level: Level::Ddc(40),
        }])
        .unwrap();
        assert!(Record::take(&path).unwrap().is_none());
        // A restorer that finds it locked changes nothing.
        assert!(restore_saved_with(&dir, |_| Restore::Done).is_none());
        assert_eq!(held.entries().len(), 1);
        drop(held);
        let mut taken = Record::take(&path).unwrap().unwrap();
        assert_eq!(taken.entries().len(), 1);
        taken.clear(&path);
        assert!(!path.exists());
        std::fs::remove_dir_all(dir).unwrap();
    }

    // Display calls abort off the main thread, so these tests decide by UUID.
    #[test]
    fn a_level_that_cannot_be_put_back_yet_waits_in_the_record() {
        let dir = dir();
        let path = dir.join(RECORD);
        let back = |entry: &Saved| {
            if entry.uuid == A {
                Restore::Done
            } else {
                Restore::Later
            }
        };
        let entries = vec![
            Saved {
                uuid: A.into(),
                level: Level::Native(0.25),
            },
            Saved {
                uuid: B.into(),
                level: Level::Ddc(30),
            },
        ];
        Record::hold(&path).unwrap().store(&entries).unwrap();
        let pass = restore_saved_with(&dir, back).unwrap();
        assert_eq!((pass.entries, &pass.left[..]), (2, &entries[1..]));
        assert_eq!(
            Record::take(&path).unwrap().unwrap().entries(),
            entries[1..]
        );
        // A pass that puts nothing back leaves the record as it was.
        let modified = std::fs::metadata(&path).unwrap().modified().unwrap();
        std::thread::sleep(Duration::from_millis(20));
        let pass = restore_saved_with(&dir, |_| Restore::Later).unwrap();
        assert_eq!(pass.left, entries[1..]);
        assert_eq!(
            std::fs::metadata(&path).unwrap().modified().unwrap(),
            modified
        );
        // Once everything is back, the record is removed.
        restore_saved_with(&dir, |_| Restore::Done);
        assert!(!path.exists());
        std::fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn displays_are_steady_only_after_staying_unchanged_and_unlocked() {
        let at = Instant::now();
        let config = |locked, asleep| Config {
            displays: vec![(3, Some(A.to_owned()), asleep)],
            locked,
        };
        let mut steady = Steady::new();
        steady.observe(config(true, false), at);
        // Locked: a restorer may go ahead, a cover may not dim.
        assert!(!steady.ready(3, at + STEADY - Duration::from_millis(1)));
        assert!(steady.ready(3, at + STEADY));
        assert!(!steady.all_ready(&[3], at + STEADY));
        // Unlocking starts the wait again.
        let unlocked = at + STEADY;
        steady.observe(config(false, false), unlocked);
        assert!(!steady.all_ready(&[3], unlocked + Duration::from_secs(2)));
        assert!(steady.all_ready(&[3], unlocked + STEADY));
        // A display that went or came, or is asleep, is not ready.
        assert!(!steady.all_ready(&[3, 4], unlocked + STEADY));
        assert!(!steady.ready(4, unlocked + STEADY));
        let asleep = unlocked + STEADY;
        steady.observe(config(false, true), asleep);
        assert!(!steady.ready(3, asleep + STEADY));
        // The same sample again does not start the wait again.
        let woke = asleep + STEADY;
        steady.observe(config(false, false), woke);
        steady.observe(config(false, false), woke + STEADY);
        assert!(steady.all_ready(&[3], woke + STEADY));
    }

    #[test]
    fn a_display_keeps_the_level_an_earlier_cover_recorded() {
        let mut leftover = vec![
            Saved {
                uuid: A.into(),
                level: Level::Ddc(50),
            },
            Saved {
                uuid: B.into(),
                level: Level::Native(0.5),
            },
        ];
        // Read at that cover's zero; the recorded level is the display's own.
        let mut saved = Saved {
            uuid: A.into(),
            level: Level::Ddc(0),
        };
        adopt(&mut leftover, &mut saved);
        assert_eq!(saved.level, Level::Ddc(50));
        assert_eq!(leftover.len(), 1);
        // Another display, or another kind of level, is not taken.
        let mut other = Saved {
            uuid: B.into(),
            level: Level::Ddc(0),
        };
        adopt(&mut leftover, &mut other);
        assert_eq!(other.level, Level::Ddc(0));
        assert_eq!(leftover.len(), 1);
    }

    #[test]
    fn a_second_restorer_leaves_the_record_to_the_first() {
        let dir = dir();
        let path = dir.join(RECORD);
        Record::hold(&path)
            .unwrap()
            .store(&[Saved {
                uuid: A.into(),
                level: Level::Ddc(40),
            }])
            .unwrap();
        let first = File::create(dir.join(RESTORER)).unwrap();
        assert!(Record::try_lock(&first).unwrap());
        // It returns before any display call, which would abort off the
        // main thread.
        let mut lines = Vec::new();
        restore_saved(&dir, |line| lines.push(line));
        assert!(lines.is_empty());
        assert_eq!(Record::take(&path).unwrap().unwrap().entries().len(), 1);
        std::fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn a_holder_that_waited_for_a_removed_record_takes_the_new_one() {
        let dir = dir();
        let path = dir.join(RECORD);
        let first = Record::hold(&path).unwrap();
        let waiting = {
            let path = path.clone();
            std::thread::spawn(move || {
                let mut record = Record::hold(&path).unwrap();
                record
                    .store(&[Saved {
                        uuid: A.into(),
                        level: Level::Native(0.5),
                    }])
                    .unwrap();
            })
        };
        std::thread::sleep(Duration::from_millis(50));
        first.clear(&path);
        waiting.join().unwrap();
        // The waiter wrote to the file at the path, not to the removed one.
        assert_eq!(Record::take(&path).unwrap().unwrap().entries().len(), 1);
        std::fs::remove_dir_all(dir).unwrap();
    }
}
