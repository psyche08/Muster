//! The C ABI (ARCHITECTURE §3.7): create a core with the platform's
//! callbacks, feed it intents and effect results, free it. Everything else
//! is protobuf in and out.

use std::ffi::{CStr, CString, c_char};
use std::os::raw::c_void;
use std::path::PathBuf;
use std::sync::Arc;

use muster_app_core::{AppConfig, AppCore, Mode, Shell};

mod log_file;

#[repr(C)]
pub struct MusterPlatform {
    pub ctx: *mut c_void,
    pub on_view: Option<unsafe extern "C" fn(ctx: *mut c_void, view_model: *const u8, len: usize)>,
    pub on_effect:
        Option<unsafe extern "C" fn(ctx: *mut c_void, id: u64, effect: *const u8, len: usize)>,
    pub hw_public_key:
        Option<unsafe extern "C" fn(ctx: *mut c_void, out: *mut u8, cap: usize) -> isize>,
    pub hw_sign: Option<
        unsafe extern "C" fn(
            ctx: *mut c_void,
            msg: *const u8,
            msg_len: usize,
            out: *mut u8,
            cap: usize,
        ) -> isize,
    >,
    pub presence_public_key:
        Option<unsafe extern "C" fn(ctx: *mut c_void, out: *mut u8, cap: usize) -> isize>,
}

/// The platform's callbacks as a Shell. The platform promises its `ctx`
/// may be used from any thread.
type VideoCallback = unsafe extern "C" fn(*mut c_void, u64, *const u8, usize);
struct CShell {
    video: std::sync::Mutex<Option<VideoCallback>>,
    platform: MusterPlatform,
}

unsafe impl Send for CShell {}
unsafe impl Sync for CShell {}

impl CShell {
    fn key(
        &self,
        f: Option<unsafe extern "C" fn(*mut c_void, *mut u8, usize) -> isize>,
    ) -> Option<Vec<u8>> {
        let f = f?;
        let mut out = vec![0u8; 256];
        // SAFETY: the platform writes at most `cap` bytes into `out`.
        let n = unsafe { f(self.platform.ctx, out.as_mut_ptr(), out.len()) };
        if n < 0 || n as usize > out.len() {
            return None;
        }
        out.truncate(n as usize);
        Some(out)
    }
}

impl Shell for CShell {
    fn on_desktop_video(&self, id: u64, data: &[u8]) {
        if let Some(f) = *self.video.lock().unwrap() {
            unsafe { f(self.platform.ctx, id, data.as_ptr(), data.len()) };
        }
    }
    fn on_view(&self, view_model: &[u8]) {
        if let Some(f) = self.platform.on_view {
            // SAFETY: the slice outlives the call.
            unsafe { f(self.platform.ctx, view_model.as_ptr(), view_model.len()) }
        }
    }
    fn on_effect(&self, id: u64, effect: &[u8]) {
        if let Some(f) = self.platform.on_effect {
            // SAFETY: the slice outlives the call.
            unsafe { f(self.platform.ctx, id, effect.as_ptr(), effect.len()) }
        }
    }
    fn hw_public_key(&self) -> Option<Vec<u8>> {
        self.key(self.platform.hw_public_key)
            .filter(|k| k.len() == 65)
    }
    fn hw_sign(&self, message: &[u8]) -> Option<Vec<u8>> {
        let f = self.platform.hw_sign?;
        let mut out = vec![0u8; 128];
        // SAFETY: both buffers are valid for the given lengths for the call.
        let n = unsafe {
            f(
                self.platform.ctx,
                message.as_ptr(),
                message.len(),
                out.as_mut_ptr(),
                out.len(),
            )
        };
        if n < 0 || n as usize > out.len() {
            return None;
        }
        out.truncate(n as usize);
        Some(out)
    }
    fn presence_public_key(&self) -> Option<Vec<u8>> {
        self.key(self.platform.presence_public_key)
            .filter(|k| k.len() == 65)
    }
}

#[derive(serde::Deserialize)]
struct ConfigJson {
    dir: PathBuf,
    #[serde(default = "default_mode")]
    mode: String,
    #[serde(default)]
    relay: Option<String>,
    #[serde(default)]
    name: String,
    #[serde(default = "default_kind")]
    kind: String,
    #[serde(default)]
    socket: Option<PathBuf>,
    #[serde(default = "default_bind")]
    bind: String,
    #[serde(default)]
    allow_loopback: bool,
    #[serde(default)]
    version: String,
}

fn default_mode() -> String {
    "embedded".into()
}
fn default_kind() -> String {
    "phone".into()
}
fn default_bind() -> String {
    "0.0.0.0:0".into()
}

pub struct MusterCore {
    core: AppCore,
    shell: Arc<CShell>,
}

fn set_error(error_out: *mut *mut c_char, message: String) {
    if error_out.is_null() {
        return;
    }
    let c = CString::new(message.replace('\0', " ")).unwrap_or_default();
    // SAFETY: error_out is a valid out-pointer per the header contract.
    unsafe { *error_out = c.into_raw() };
}

/// # Safety
/// `config_json` points to `len` readable bytes; `platform` is a valid struct
/// whose callbacks stay valid until `muster_core_free`.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn muster_core_new(
    config_json: *const u8,
    len: usize,
    platform: *const MusterPlatform,
    error_out: *mut *mut c_char,
) -> *mut MusterCore {
    if config_json.is_null() || platform.is_null() {
        set_error(error_out, "null argument".into());
        return std::ptr::null_mut();
    }
    // SAFETY: per the function contract.
    let bytes = unsafe { std::slice::from_raw_parts(config_json, len) };
    let config: ConfigJson = match serde_json::from_slice(bytes) {
        Ok(c) => c,
        Err(e) => {
            set_error(error_out, format!("config: {e}"));
            return std::ptr::null_mut();
        }
    };
    let mode = match config.mode.as_str() {
        "socket" => Mode::Socket {
            path: config
                .socket
                .unwrap_or_else(|| config.dir.join("musterd.sock")),
        },
        "embedded" => Mode::Embedded {
            relay: config.relay,
            name: config.name,
            kind: config.kind,
            version: config.version,
            bind: match config.bind.parse() {
                Ok(b) => b,
                Err(e) => {
                    set_error(error_out, format!("bind: {e}"));
                    return std::ptr::null_mut();
                }
            },
        },
        other => {
            set_error(error_out, format!("mode {other:?}: embedded or socket"));
            return std::ptr::null_mut();
        }
    };
    // SAFETY: platform is valid per the contract; we copy the struct.
    let platform = unsafe { std::ptr::read(platform) };
    let shell = Arc::new(CShell {
        platform,
        video: Default::default(),
    });
    let app_config = AppConfig {
        dir: config.dir,
        mode,
        allow_loopback: config.allow_loopback,
    };
    match AppCore::new(app_config, shell.clone()) {
        Ok(core) => Box::into_raw(Box::new(MusterCore { core, shell })),
        Err(e) => {
            set_error(error_out, e.to_string());
            std::ptr::null_mut()
        }
    }
}

/// # Safety
/// `core` came from `muster_core_new`; `intent` points to `len` readable bytes.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn muster_core_intent(core: *mut MusterCore, intent: *const u8, len: usize) {
    if core.is_null() || intent.is_null() {
        return;
    }
    // SAFETY: per the contract.
    let (core, bytes) = unsafe { (&*core, std::slice::from_raw_parts(intent, len)) };
    if let Err(e) = core.core.intent_bytes(bytes) {
        log::warn!("intent: {e}");
    }
}

/// # Safety
/// `core` came from `muster_core_new`; `result` points to `len` readable bytes.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn muster_core_effect_result(
    core: *mut MusterCore,
    effect_id: u64,
    result: *const u8,
    len: usize,
) {
    if core.is_null() || result.is_null() {
        return;
    }
    // SAFETY: per the contract.
    let (core, bytes) = unsafe { (&*core, std::slice::from_raw_parts(result, len)) };
    if let Err(e) = core.core.effect_result_bytes(effect_id, bytes) {
        log::warn!("effect result: {e}");
    }
}

/// # Safety
/// `core` came from `muster_core_new`.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn muster_core_lifecycle(core: *mut MusterCore, event: u32) {
    if core.is_null() {
        return;
    }
    // SAFETY: per the contract.
    unsafe { &*core }.core.lifecycle_raw(event);
}

/// # Safety
/// `core` came from `muster_core_new`.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn muster_core_ssh_public_key(core: *mut MusterCore) -> *mut c_char {
    if core.is_null() {
        return std::ptr::null_mut();
    }
    // SAFETY: per the contract.
    let core = unsafe { &*core };
    let Some(signer) = core.core.backend().signer() else {
        return std::ptr::null_mut();
    };
    let name = core
        .shell
        .hw_public_key()
        .map(|_| "muster-device")
        .unwrap_or("muster-soft");
    match muster_terminal::ssh::authorized_key_line(&signer.public_key(), name) {
        Ok(line) => CString::new(line)
            .map(|c| c.into_raw())
            .unwrap_or(std::ptr::null_mut()),
        Err(_) => std::ptr::null_mut(),
    }
}

/// # Safety
/// `core` came from `muster_core_new` and is not used afterwards.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn muster_core_free(core: *mut MusterCore) {
    if core.is_null() {
        return;
    }
    // SAFETY: per the contract.
    let core = unsafe { Box::from_raw(core) };
    core.core.shutdown();
}

/// # Safety
/// `s` came from this library.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn muster_string_free(s: *mut c_char) {
    if !s.is_null() {
        // SAFETY: per the contract.
        drop(unsafe { CString::from_raw(s) });
    }
}

fn logger(level: u32) -> env_logger::Builder {
    let filter = match level {
        0 => "off",
        1 => "error",
        2 => "warn",
        3 => "info",
        _ => "debug",
    };
    env_logger::Builder::from_env(env_logger::Env::default().default_filter_or(filter))
}

#[unsafe(no_mangle)]
pub extern "C" fn muster_log_init(level: u32) {
    let _ = logger(level).try_init();
}

/// Like `muster_log_init`, and also to `app.log` in the platform's log
/// directory (`muster_paths::log_dir`), for a desktop app whose stderr goes
/// nowhere. Stderr alone when that file cannot be opened.
#[unsafe(no_mangle)]
pub extern "C" fn muster_log_init_file(level: u32) {
    let mut logger = logger(level);
    let file = muster_paths::log_dir()
        .ok_or_else(|| std::io::Error::other("no log directory"))
        .and_then(|dir| log_file::LogFile::open(&dir, log_file::LIMIT));
    let opened = file.map(|file| {
        logger.target(env_logger::Target::Pipe(Box::new(file)));
    });
    if logger.try_init().is_ok() {
        match opened {
            Ok(()) => log::info!("app log opened, pid {}", std::process::id()),
            Err(error) => log::warn!("app log file unavailable, stderr only: {error}"),
        }
    }
}

#[unsafe(no_mangle)]
pub extern "C" fn muster_version() -> *const c_char {
    static VERSION: &CStr = c"0.1.0";
    VERSION.as_ptr()
}

/// Run desktop installer lifecycle hooks before creating the app runtime.
#[unsafe(no_mangle)]
pub extern "C" fn muster_desktop_update_startup() {
    #[cfg(any(target_os = "macos", target_os = "windows"))]
    muster_app_core::desktop_update::startup();
}

/// The one desktop startup coordinator: the first look and the Start button
/// share its admission, so neither runs while the other is in flight.
static DESKTOP_STARTUP: std::sync::LazyLock<muster_app_core::desktop_startup::Coordinator> =
    std::sync::LazyLock::new(muster_app_core::desktop_startup::Coordinator::default);

/// The arguments both desktop startup calls share, checked.
fn desktop_arguments<'a>(
    socket: *const c_char,
    managed: u8,
    action: Option<unsafe extern "C" fn(u32) -> i32>,
    error_out: *mut *mut c_char,
) -> Option<&'a str> {
    if !error_out.is_null() {
        // SAFETY: writable out-pointer required by the contract.
        unsafe { *error_out = std::ptr::null_mut() };
    }
    if socket.is_null() || managed > 1 || (managed == 1 && action.is_none()) {
        set_error(error_out, "invalid desktop startup arguments".into());
        return None;
    }
    // SAFETY: valid string required by the contract.
    match unsafe { CStr::from_ptr(socket) }.to_str() {
        Ok(path) if !path.is_empty() => Some(path),
        _ => {
            set_error(error_out, "invalid desktop socket path".into());
            None
        }
    }
}

/// The platform's service adapter as Rust sees it. Inspect answers with the
/// facts' bits (negative is a failure); every other action answers 0 done,
/// 1 macOS wants the owner's approval, 3 the owner cancelled the password
/// prompt, anything else a native failure.
fn desktop_service(
    action: Option<unsafe extern "C" fn(u32) -> i32>,
) -> impl Fn(muster_app_core::desktop_startup::ServiceAction) -> Result<u32, muster_app_core::Error>
{
    use muster_app_core::desktop_startup::ServiceAction;
    move |a| {
        // The managed argument validation guarantees this callback exists.
        let callback = action
            .ok_or_else(|| muster_app_core::Error::Invalid("missing service callback".into()))?;
        // SAFETY: callback validity required by the contract.
        let code = unsafe { callback(a as u32) };
        match (a, code) {
            (ServiceAction::Inspect, code) if code >= 0 => Ok(code as u32),
            (_, 0) => Ok(0),
            (_, 1) => Err(muster_app_core::Error::Unavailable(
                "Allow Muster in System Settings > General > Login Items & Extensions, then try again"
                    .into(),
            )),
            (_, 3) => Err(muster_app_core::Error::Unavailable(
                "安装 musterd 登录代理需要管理员密码，已取消。".into(),
            )),
            (_, code) => Err(muster_app_core::Error::Unavailable(format!(
                "Native service preparation failed ({code}); see desktop startup logs"
            ))),
        }
    }
}

/// What the app shows before its main page. Call on a worker thread
/// outside a Rust runtime; it may wait a few seconds for a daemon launchd
/// is still starting. Changes nothing.
///
/// # Safety
/// `socket` must be a valid NUL-terminated UTF-8 string. `error_out`, when
/// non-null, must be writable. The callback must remain valid for this call.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn muster_desktop_check(
    socket: *const c_char,
    managed: u8,
    resumed: u8,
    action: Option<unsafe extern "C" fn(u32) -> i32>,
    error_out: *mut *mut c_char,
) -> i32 {
    let Some(path) = desktop_arguments(socket, managed, action, error_out) else {
        return -1;
    };
    if resumed > 1 {
        set_error(error_out, "invalid desktop startup arguments".into());
        return -1;
    }
    let result = DESKTOP_STARTUP
        .check_blocking(
            std::path::Path::new(path),
            managed == 1,
            resumed == 1,
            desktop_service(action),
        )
        .map(|startup| startup.code());
    match result {
        Ok(code) => code,
        Err(error) => {
            set_error(error_out, error.to_string());
            -1
        }
    }
}

/// Prepare the desktop socket. Call on a worker thread outside a Rust runtime.
///
/// # Safety
/// `socket` must be a valid NUL-terminated UTF-8 string. `error_out`, when
/// non-null, must be writable. The callback must remain valid for this call.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn muster_desktop_prepare(
    socket: *const c_char,
    managed: u8,
    action: Option<unsafe extern "C" fn(u32) -> i32>,
    error_out: *mut *mut c_char,
) -> i32 {
    let Some(path) = desktop_arguments(socket, managed, action, error_out) else {
        return -1;
    };
    let result = DESKTOP_STARTUP.prepare_blocking(
        std::path::Path::new(path),
        managed == 1,
        desktop_service(action),
    );
    match result {
        Ok(()) => 0,
        Err(error) => {
            set_error(error_out, error.to_string());
            -1
        }
    }
}

/// Validate an app-core presence challenge before opening the Windows TPM key.
///
/// # Safety
/// A non-null pointer must span the indicated number of readable bytes.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn muster_is_presence_challenge(bytes: *const u8, length: usize) -> u8 {
    if bytes.is_null() || length > isize::MAX as usize {
        return 0;
    }
    muster_app_core::is_presence_challenge(unsafe { std::slice::from_raw_parts(bytes, length) })
        as u8
}

/// # Safety
/// The core and callback context must remain valid until core shutdown.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn muster_core_set_video_callback(
    core: *mut MusterCore,
    cb: Option<VideoCallback>,
) {
    if let Some(core) = unsafe { core.as_ref() } {
        *core.shell.video.lock().unwrap() = cb;
    }
}
