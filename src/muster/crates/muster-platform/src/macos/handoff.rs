//! Launch requests that reach the daemon, handed to the app (ADR 0183).
//!
//! `musterd` holds a Launch Services record under the app's bundle id
//! (ADR 0164). When Muster is opened from the Dock, Finder, Spotlight or
//! `open`, Launch Services can resolve the request to that record: always
//! when no app process is running, and also while one runs that registered
//! after the daemon. It then makes the daemon's record a foreground
//! application and sends the daemon the reopen event, or a `muster:` link's
//! URL. The daemon has no window, so the request used to end there, and
//! Muster appeared to open with no window.
//!
//! The daemon now hands the request to the app. A running app process gets
//! the same event, addressed to its pid, and is activated; a URL for an app
//! that is still starting waits until it can take Apple Events. With none
//! running, `open -n` starts a new instance and passes on the URL; a plain
//! `open` would resolve to the daemon's record again. Requests that arrive
//! while that `open` runs start no second instance: a reopen is dropped,
//! since the new app opens its window anyway, and a URL is handed to the app
//! once its process is there. Either way the daemon then sets its own
//! policy back to prohibited, so its record is `BackgroundOnly` again and
//! has no Dock tile.
//!
//! The open-application event a process receives when it registers is left
//! unhandled, so a daemon that launchd starts at login opens no window.

use std::ffi::{OsString, c_void};
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

use super::automation::{AECreateDesc, AEDisposeDesc, Descriptor as Desc};

type OsErr = i16;

/// `AEEventHandlerProcPtr`.
type Handler = unsafe extern "C" fn(*const Desc, *mut Desc, *mut c_void) -> OsErr;

#[link(name = "CoreServices", kind = "framework")]
unsafe extern "C" {
    fn AEInstallEventHandler(
        class: u32,
        id: u32,
        handler: Handler,
        refcon: *mut c_void,
        system: u8,
    ) -> OsErr;
    fn AEGetParamPtr(
        event: *const Desc,
        key: u32,
        desired: u32,
        actual: *mut u32,
        data: *mut c_void,
        max: isize,
        size: *mut isize,
    ) -> OsErr;
    fn AECreateAppleEvent(
        class: u32,
        id: u32,
        target: *const Desc,
        return_id: i16,
        transaction: i32,
        out: *mut Desc,
    ) -> OsErr;
    fn AEPutParamPtr(
        event: *mut Desc,
        key: u32,
        kind: u32,
        data: *const c_void,
        len: isize,
    ) -> OsErr;
    fn AESendMessage(
        event: *const Desc,
        reply: *mut Desc,
        mode: i32,
        timeout: std::ffi::c_long,
    ) -> i32;
}

const fn code(c: &[u8; 4]) -> u32 {
    u32::from_be_bytes(*c)
}

const CORE_EVENT_CLASS: u32 = code(b"aevt");
const REOPEN_APPLICATION: u32 = code(b"rapp");
const INTERNET_EVENT_CLASS: u32 = code(b"GURL");
const GET_URL: u32 = code(b"GURL");
const DIRECT_OBJECT: u32 = code(b"----");
const UTF8_TEXT: u32 = code(b"utf8");
const KERNEL_PROCESS_ID: u32 = code(b"kpid");
const AUTO_GENERATE_RETURN_ID: i16 = -1;
const ANY_TRANSACTION_ID: i32 = 0;
/// `kAENoReply | kAENeverInteract`: nothing waits on the app, and nothing
/// may ask the user anything on the daemon's behalf.
const SEND_MODE: i32 = 0x1 | 0x10;
const DEFAULT_TIMEOUT: std::ffi::c_long = -1;
const NO_ERR: OsErr = 0;
/// `errAEEventNotHandled`.
const NOT_HANDLED: OsErr = -1708;
/// The longest URL handed on. A longer one is refused, not truncated.
const URL_LIMIT: usize = 16 * 1024;

/// What Launch Services asked the daemon's record to do.
#[derive(Debug, PartialEq, Eq)]
enum Request {
    Reopen,
    Url(String),
}

impl Request {
    /// The Apple Event class and id that carry this request.
    fn event(&self) -> (u32, u32) {
        match self {
            Request::Reopen => (CORE_EVENT_CLASS, REOPEN_APPLICATION),
            Request::Url(_) => (INTERNET_EVENT_CLASS, GET_URL),
        }
    }

    fn url(&self) -> Option<&str> {
        match self {
            Request::Reopen => None,
            Request::Url(url) => Some(url),
        }
    }

    /// For the log, which never records the URL itself.
    fn describe(&self) -> &'static str {
        match self {
            Request::Reopen => "a reopen request",
            Request::Url(_) => "a URL",
        }
    }
}

/// Install the reopen and URL handlers. Main thread, before it serves events.
pub(super) fn install() -> bool {
    let refcon = std::ptr::null_mut();
    // SAFETY: both handlers have the `AEEventHandlerProcPtr` signature and
    // live for the whole process; the refcon is unused.
    let (reopen, url) = unsafe {
        (
            AEInstallEventHandler(CORE_EVENT_CLASS, REOPEN_APPLICATION, on_reopen, refcon, 0),
            AEInstallEventHandler(INTERNET_EVENT_CLASS, GET_URL, on_url, refcon, 0),
        )
    };
    reopen == NO_ERR && url == NO_ERR
}

unsafe extern "C" fn on_reopen(_: *const Desc, _: *mut Desc, _: *mut c_void) -> OsErr {
    answer(|| Some(Request::Reopen))
}

unsafe extern "C" fn on_url(event: *const Desc, _: *mut Desc, _: *mut c_void) -> OsErr {
    // SAFETY: the Apple Event Manager passes the event it is dispatching.
    answer(|| unsafe { url_of(event) }.map(Request::Url))
}

/// Hand the request on, then leave the Dock again. Runs on the main thread,
/// inside the event loop's autorelease pool, and never unwinds into the
/// Apple Event Manager.
fn answer(read: impl FnOnce() -> Option<Request>) -> OsErr {
    let status = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| match read() {
        Some(request) => {
            hand_off(&request);
            NO_ERR
        }
        None => {
            log::warn!("a URL reached the daemon without a usable URL; ignored");
            NOT_HANDLED
        }
    }))
    .unwrap_or(NOT_HANDLED);
    if !super::events::leave_the_dock_again() {
        log::warn!("could not set the daemon's activation policy back to prohibited");
    }
    status
}

/// The event's URL, if it is UTF-8 text no longer than [`URL_LIMIT`] and
/// [`url_argument`] accepts it.
///
/// # Safety
/// `event` is a live Apple Event.
unsafe fn url_of(event: *const Desc) -> Option<String> {
    let mut buf = vec![0u8; URL_LIMIT];
    let mut kind = 0u32;
    let mut size = 0isize;
    // SAFETY: the buffer is valid for its length; `size` reports the full
    // length even when the data did not fit.
    let status = unsafe {
        AEGetParamPtr(
            event,
            DIRECT_OBJECT,
            UTF8_TEXT,
            &mut kind,
            buf.as_mut_ptr().cast(),
            buf.len() as isize,
            &mut size,
        )
    };
    let size = usize::try_from(size).ok()?;
    if status != NO_ERR || size > buf.len() {
        return None;
    }
    buf.truncate(size);
    let text = String::from_utf8(buf).ok()?;
    url_argument(&text).map(str::to_owned)
}

/// `text` if it is an absolute URL, `scheme ":" rest` with an RFC 3986
/// scheme and no control characters. That also keeps it from being read as
/// an option when it is passed to `open`.
fn url_argument(text: &str) -> Option<&str> {
    let (scheme, _) = text.split_once(':')?;
    let mut chars = scheme.chars();
    let first = chars.next()?;
    let scheme_ok = first.is_ascii_alphabetic()
        && chars.all(|c| c.is_ascii_alphanumeric() || matches!(c, '+' | '-' | '.'));
    (scheme_ok && !text.chars().any(char::is_control)).then_some(text)
}

/// The app bundle this process belongs to, and the app's own executable.
struct App {
    bundle: PathBuf,
    executable: PathBuf,
}

/// `None` unless this process runs from an `.app` whose declared executable
/// is a different file.
fn this_app() -> Option<App> {
    let bundle = super::procs::process_bundle(std::process::id())?;
    let own = std::env::current_exe().ok()?;
    app_in(bundle, own.file_name()?)
}

/// The app in `bundle`, the executable its `Info.plist` names, unless that
/// is `own`. Read from the plist, not `CFBundleCopyExecutableURL`: for the
/// main bundle, that answers with the running executable — the daemon.
fn app_in(bundle: PathBuf, own: &std::ffi::OsStr) -> Option<App> {
    use core_foundation::bundle::CFBundle;
    use core_foundation::string::CFString;
    use core_foundation::url::CFURL;

    let info = CFBundle::new(CFURL::from_path(&bundle, true)?)?.info_dictionary();
    let name = info
        .find(CFString::from_static_string("CFBundleExecutable"))?
        .downcast::<CFString>()?
        .to_string();
    // A file name, not a path out of `Contents/MacOS`.
    if name.is_empty() || name.contains('/') || name.as_str() == own {
        return None;
    }
    let executable = bundle.join("Contents/MacOS").join(name);
    Some(App { bundle, executable })
}

fn hand_off(request: &Request) {
    let what = request.describe();
    let Some(app) = this_app() else {
        log::warn!("{what} reached the daemon outside an app bundle; ignored");
        return;
    };
    match app_pid(&app.executable) {
        Some(pid) => {
            let sent = forward(pid, request);
            let activated = super::events::activate(pid as i32);
            match sent {
                Ok(()) => log::info!(
                    "{what} reached the daemon; handed it to the app (pid {pid}), activated: {activated}"
                ),
                // The app's process is there but not ready for Apple Events.
                // Starting, it opens its window anyway; a URL follows.
                Err(PROCESS_NOT_READY) => {
                    if let Some(url) = request.url() {
                        follow_up(app.executable, vec![url.to_owned()]);
                    }
                    log::info!("{what} reached the daemon while the app (pid {pid}) is starting");
                }
                Err(status) => log::warn!(
                    "{what} reached the daemon; forwarding it to the app (pid {pid}) failed \
                     with {status}, activated: {activated}"
                ),
            }
        }
        None => {
            {
                let mut starting = lock_starting();
                if let Some(queued) = starting.as_mut() {
                    // A second Dock click must not start a second instance.
                    if let Some(url) = request.url() {
                        queued.push(url.to_owned());
                    }
                    log::info!("{what} reached the daemon while the app it started is starting");
                    return;
                }
                *starting = Some(Vec::new());
            }
            match launch(app, request.url()) {
                Ok(()) => {
                    log::info!("{what} reached the daemon with no app running; started the app")
                }
                Err(e) => {
                    *lock_starting() = None;
                    log::warn!(
                        "{what} reached the daemon with no app running; could not start the app: {e}"
                    )
                }
            }
        }
    }
}

/// `Some` while an `open` this daemon ran is still running, with the URLs
/// that arrived meanwhile.
static STARTING: std::sync::Mutex<Option<Vec<String>>> = std::sync::Mutex::new(None);

fn lock_starting() -> std::sync::MutexGuard<'static, Option<Vec<String>>> {
    STARTING
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner)
}

/// Send `request` again, addressed to the app process `pid`.
fn forward(pid: u32, request: &Request) -> Result<(), i32> {
    let pid = pid as i32;
    let (class, id) = request.event();
    let mut target = Desc::null();
    let mut event = Desc::null();
    // SAFETY: every descriptor is initialized before use and disposed once;
    // the data pointers are valid for the lengths given.
    unsafe {
        let status = AECreateDesc(
            KERNEL_PROCESS_ID,
            (&raw const pid).cast(),
            std::mem::size_of::<i32>() as isize,
            &mut target,
        );
        if status != NO_ERR {
            return Err(status.into());
        }
        let status = AECreateAppleEvent(
            class,
            id,
            &target,
            AUTO_GENERATE_RETURN_ID,
            ANY_TRANSACTION_ID,
            &mut event,
        );
        AEDisposeDesc(&mut target);
        if status != NO_ERR {
            return Err(status.into());
        }
        if let Some(url) = request.url() {
            let status = AEPutParamPtr(
                &mut event,
                DIRECT_OBJECT,
                UTF8_TEXT,
                url.as_ptr().cast(),
                url.len() as isize,
            );
            if status != NO_ERR {
                AEDisposeDesc(&mut event);
                return Err(status.into());
            }
        }
        let status = AESendMessage(&event, std::ptr::null_mut(), SEND_MODE, DEFAULT_TIMEOUT);
        AEDisposeDesc(&mut event);
        if status != 0 {
            return Err(status);
        }
    }
    Ok(())
}

/// `open`'s arguments for a new instance of `bundle`, given `url` if any.
fn open_arguments(bundle: &Path, url: Option<&str>) -> Vec<OsString> {
    let mut args = vec![OsString::from("-n")];
    if let Some(url) = url {
        args.extend([OsString::from("-a"), bundle.into(), url.into()]);
    } else {
        args.push(bundle.into());
    }
    args
}

/// `procNotFound`: the app's process cannot take Apple Events yet.
const PROCESS_NOT_READY: i32 = -600;
/// How long a URL waits for a starting app to take it.
const STARTED_WAIT: Duration = Duration::from_secs(10);
const POLL: Duration = Duration::from_millis(250);

/// Start a new instance of the app through Launch Services. Off the main
/// thread, wait for `open`, then hand the app the URLs that arrived while it
/// ran.
fn launch(app: App, url: Option<&str>) -> std::io::Result<()> {
    use std::process::{Command, Stdio};
    let mut child = Command::new("/usr/bin/open")
        .args(open_arguments(&app.bundle, url))
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()?;
    let waiter = std::thread::Builder::new()
        .name("musterd-open".into())
        .spawn(move || {
            let _ = child.wait();
            let queued = lock_starting().take().unwrap_or_default();
            if !queued.is_empty() {
                deliver_when_ready(&app.executable, queued);
            }
        });
    if let Err(e) = waiter {
        // The child is reaped when the daemon exits; the launch itself is
        // under way, so only the queue is lost.
        *lock_starting() = None;
        log::warn!("could not wait for the app to start: {e}");
    }
    Ok(())
}

/// The app's process at `executable`, other than this one.
fn app_pid(executable: &Path) -> Option<u32> {
    let own = std::process::id();
    super::procs::pids_by_executable(executable)
        .into_iter()
        .find(|&pid| pid != own)
}

/// [`deliver_when_ready`], on a thread of its own.
fn follow_up(executable: PathBuf, urls: Vec<String>) {
    let spawned = std::thread::Builder::new()
        .name("musterd-url".into())
        .spawn(move || deliver_when_ready(&executable, urls));
    if let Err(e) = spawned {
        log::warn!("could not wait to hand a URL to the starting app: {e}");
    }
}

/// Forward `urls` to the app at `executable` once its process can take
/// them, for at most [`STARTED_WAIT`]. Blocks; never on the main thread.
fn deliver_when_ready(executable: &Path, urls: Vec<String>) {
    let deadline = Instant::now() + STARTED_WAIT;
    let pid = loop {
        if let Some(pid) = app_pid(executable) {
            break pid;
        }
        if Instant::now() >= deadline {
            log::warn!(
                "{} URL(s) waited for the app, which did not start; dropped",
                urls.len()
            );
            return;
        }
        std::thread::sleep(POLL);
    };
    for url in urls {
        let request = Request::Url(url);
        let sent = loop {
            match forward(pid, &request) {
                Err(PROCESS_NOT_READY) if Instant::now() < deadline => std::thread::sleep(POLL),
                sent => break sent,
            }
        };
        if let Err(status) = sent {
            log::warn!("forwarding a URL to the starting app (pid {pid}) failed with {status}");
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn accepts_absolute_urls_only() {
        for url in [
            "muster://browser",
            "muster://browser?device=M4Mini&draft=%7B%7D",
            "x-muster+dev.1:opaque",
        ] {
            assert_eq!(url_argument(url), Some(url));
        }
        for text in [
            "",
            "muster",
            "-a",
            "--args:x",
            ":muster",
            "1muster://browser",
            "mu ster://browser",
            "muster://browser\n-a",
            "muster://\u{7}",
        ] {
            assert_eq!(url_argument(text), None, "{text:?}");
        }
    }

    #[test]
    fn opens_a_new_instance_with_or_without_a_url() {
        let bundle = Path::new("/Applications/Muster.app");
        assert_eq!(
            open_arguments(bundle, None),
            ["-n", "/Applications/Muster.app"]
        );
        assert_eq!(
            open_arguments(bundle, Some("muster://browser")),
            ["-n", "-a", "/Applications/Muster.app", "muster://browser"]
        );
    }

    #[test]
    fn forwards_each_request_as_the_event_it_arrived_as() {
        assert_eq!(Request::Reopen.event(), (code(b"aevt"), code(b"rapp")));
        assert_eq!(
            Request::Url("muster://browser".into()).event(),
            (code(b"GURL"), code(b"GURL"))
        );
        assert_eq!(Request::Reopen.url(), None);
    }

    #[test]
    fn a_build_tree_has_no_app_to_hand_to() {
        // A cargo test binary does not run from an `.app`.
        assert!(this_app().is_none());
    }

    #[test]
    fn hands_to_the_executable_the_info_plist_names() {
        let root = std::env::temp_dir().join(format!("muster-handoff-{}", std::process::id()));
        let bundle = root.join("Muster.app");
        let contents = bundle.join("Contents");
        std::fs::create_dir_all(contents.join("MacOS")).unwrap();
        let plist = |executable: &str| {
            format!(
                "<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n<plist version=\"1.0\"><dict>\
                 <key>CFBundleIdentifier</key><string>dev.example.handoff-{}</string>\
                 <key>CFBundleExecutable</key><string>{executable}</string>\
                 <key>CFBundlePackageType</key><string>APPL</string></dict></plist>\n",
                std::process::id()
            )
        };
        std::fs::write(contents.join("Info.plist"), plist("Muster")).unwrap();

        let app = app_in(bundle.clone(), "musterd".as_ref()).expect("an app");
        assert_eq!(app.bundle, bundle);
        assert_eq!(app.executable, contents.join("MacOS/Muster"));
        // The daemon is never its own app.
        assert!(app_in(bundle.clone(), "Muster".as_ref()).is_none());

        std::fs::remove_dir_all(&root).unwrap();
    }
}
