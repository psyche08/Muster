//! The C ABI as a shell would use it: create a core from JSON and
//! callbacks, feed it intents, read the view models back, ask for the SSH
//! public key, free everything. The core runs its own runtime here, as it
//! does inside an app.

use std::ffi::{CStr, c_char};
use std::os::raw::c_void;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use muster_ffi::{
    MusterPlatform, muster_core_free, muster_core_intent, muster_core_lifecycle, muster_core_new,
    muster_core_ssh_public_key, muster_string_free, muster_version,
};
use muster_proto::Message as _;
use muster_proto::app::{self, Screen, intent::It};

#[derive(Default)]
struct Captured {
    views: Vec<app::ViewModel>,
    effects: Vec<(u64, app::Effect)>,
}

unsafe extern "C" fn on_view(ctx: *mut c_void, bytes: *const u8, len: usize) {
    // SAFETY: the test keeps the Arc alive until after muster_core_free.
    let captured = unsafe { &*(ctx as *const Mutex<Captured>) };
    let slice = unsafe { std::slice::from_raw_parts(bytes, len) };
    let vm = app::ViewModel::decode(slice).expect("a ViewModel");
    captured.lock().unwrap().views.push(vm);
}

unsafe extern "C" fn on_effect(ctx: *mut c_void, id: u64, bytes: *const u8, len: usize) {
    // SAFETY: as above.
    let captured = unsafe { &*(ctx as *const Mutex<Captured>) };
    let slice = unsafe { std::slice::from_raw_parts(bytes, len) };
    let effect = app::Effect::decode(slice).expect("an Effect");
    captured.lock().unwrap().effects.push((id, effect));
}

/// No hardware key on this platform: the core keeps a software key.
unsafe extern "C" fn no_key(_ctx: *mut c_void, _out: *mut u8, _cap: usize) -> isize {
    -1
}

unsafe extern "C" fn no_sign(
    _ctx: *mut c_void,
    _msg: *const u8,
    _len: usize,
    _out: *mut u8,
    _cap: usize,
) -> isize {
    -1
}

fn wait_for(
    captured: &Mutex<Captured>,
    what: &str,
    check: impl Fn(&app::ViewModel) -> bool,
) -> app::ViewModel {
    let started = Instant::now();
    loop {
        if let Some(vm) = captured
            .lock()
            .unwrap()
            .views
            .iter()
            .rev()
            .find(|vm| check(vm))
        {
            return vm.clone();
        }
        assert!(
            started.elapsed() < Duration::from_secs(30),
            "timed out waiting for {what}; last view: {:?}",
            captured.lock().unwrap().views.last()
        );
        std::thread::sleep(Duration::from_millis(50));
    }
}

fn intent(core: *mut muster_ffi::MusterCore, it: It) {
    let bytes = app::Intent { it: Some(it) }.encode_to_vec();
    // SAFETY: core is live; the buffer outlives the call.
    unsafe { muster_core_intent(core, bytes.as_ptr(), bytes.len()) };
}

#[test]
fn create_a_network_through_the_c_abi() {
    let dir = tempfile::tempdir().unwrap();
    let captured = Arc::new(Mutex::new(Captured::default()));
    let platform = MusterPlatform {
        ctx: Arc::as_ptr(&captured) as *mut c_void,
        on_view: Some(on_view),
        on_effect: Some(on_effect),
        hw_public_key: Some(no_key),
        hw_sign: Some(no_sign),
        presence_public_key: Some(no_key),
    };
    let config = serde_json::json!({
        "dir": dir.path(),
        "mode": "embedded",
        "name": "ffi phone",
        "kind": "phone",
        "bind": "127.0.0.1:0",
        "allow_loopback": true,
        "version": "1.0 (16)",
    })
    .to_string();

    // SAFETY: valid pointers for the duration of each call.
    let version = unsafe { CStr::from_ptr(muster_version()) };
    assert!(!version.to_str().unwrap().is_empty());

    let mut error: *mut c_char = std::ptr::null_mut();
    let core = unsafe {
        muster_core_new(
            config.as_ptr(),
            config.len(),
            &platform,
            &mut error as *mut *mut c_char,
        )
    };
    assert!(
        !core.is_null(),
        "core: {}",
        unsafe { CStr::from_ptr(error) }.to_string_lossy()
    );
    assert!(error.is_null());

    let vm = wait_for(&captured, "onboarding", |vm| {
        vm.self_.is_some() && vm.screen == Screen::Onboarding as i32
    });
    assert!(vm.embedded);
    let me = vm.self_.unwrap();
    assert_eq!(me.name, "ffi phone");
    assert_eq!(me.kind, "phone");
    assert_eq!(me.device_id.len(), 64);
    assert_eq!(me.version, "1.0 (16)");
    assert!(!me.commit.is_empty());

    intent(
        core,
        It::CreateNetwork(app::CreateNetwork {
            name: "home".into(),
            vnet_cidr: String::new(),
        }),
    );
    // A new network opens the Agents workspace across all devices.
    let vm = wait_for(&captured, "agents", |vm| {
        vm.screen == Screen::Agents as i32 && vm.network.as_ref().is_some_and(|n| n.has_network)
    });
    assert_eq!(vm.agents.as_ref().map(|a| a.device_id.as_str()), Some("*"));
    let network = vm.network.unwrap();
    assert_eq!(network.name, "home");
    assert_eq!(network.members.len(), 1);
    assert_eq!(network.members[0].version, "1.0 (16)");

    // Intents that need an open chat are refused with a toast, not a crash.
    intent(
        core,
        It::OpenFiles(app::OpenFiles {
            path: String::new(),
        }),
    );
    wait_for(&captured, "toast", |vm| {
        vm.toasts
            .iter()
            .any(|t| t.text.contains("open a session first"))
    });

    // A push token and a network change are accepted without a relay.
    intent(
        core,
        It::SetPushToken(app::SetPushToken {
            kind: 1,
            token: vec![1, 2, 3],
        }),
    );
    unsafe { muster_core_lifecycle(core, app::Lifecycle::NetworkChanged as u32) };
    unsafe { muster_core_lifecycle(core, app::Lifecycle::Foreground as u32) };
    unsafe { muster_core_lifecycle(core, 99) }; // unknown events are ignored

    let line = unsafe { muster_core_ssh_public_key(core) };
    assert!(!line.is_null());
    let text = unsafe { CStr::from_ptr(line) }
        .to_str()
        .unwrap()
        .to_string();
    assert!(
        text.starts_with(
            "ecdsa-sha2-nistp256 AAAAE2VjZHNhLXNoYTItbmlzdHAyNTYAAAAIbmlzdHAyNTYAAABB"
        ),
        "{text}"
    );
    assert!(text.ends_with(" muster-soft"));
    unsafe { muster_string_free(line) };

    // Null and garbage never crash.
    unsafe {
        muster_core_intent(core, std::ptr::null(), 0);
        muster_core_intent(core, b"\xff\xff".as_ptr(), 2);
        muster_core_ssh_public_key(std::ptr::null_mut());
        muster_core_free(std::ptr::null_mut());
        muster_string_free(std::ptr::null_mut());
    }

    unsafe { muster_core_free(core) };
    assert!(captured.lock().unwrap().views.len() >= 3);
}

#[test]
fn bad_config_reports_an_error() {
    let captured = Mutex::new(Captured::default());
    let platform = MusterPlatform {
        ctx: &captured as *const _ as *mut c_void,
        on_view: Some(on_view),
        on_effect: Some(on_effect),
        hw_public_key: None,
        hw_sign: None,
        presence_public_key: None,
    };
    for config in ["not json", r#"{"dir": "/tmp/x", "mode": "teleport"}"#] {
        let mut error: *mut c_char = std::ptr::null_mut();
        let core = unsafe {
            muster_core_new(
                config.as_ptr(),
                config.len(),
                &platform,
                &mut error as *mut *mut c_char,
            )
        };
        assert!(core.is_null());
        assert!(!error.is_null());
        let text = unsafe { CStr::from_ptr(error) }
            .to_string_lossy()
            .into_owned();
        assert!(!text.is_empty());
        unsafe { muster_string_free(error) };
    }
    let mut error: *mut c_char = std::ptr::null_mut();
    let core = unsafe { muster_core_new(std::ptr::null(), 0, &platform, &mut error) };
    assert!(core.is_null());
    unsafe { muster_string_free(error) };
}

#[cfg(unix)]
#[test]
fn desktop_preparation_validates_abi_and_preserves_healthy_service() {
    use muster_ffi::muster_desktop_prepare;
    let mut error = std::ptr::null_mut();
    // SAFETY: null input is explicitly handled; out-pointer is writable.
    assert_eq!(
        unsafe { muster_desktop_prepare(std::ptr::null(), 0, None, &mut error) },
        -1
    );
    assert!(!error.is_null());
    unsafe { muster_string_free(error) };
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("s");
    let _listener = std::os::unix::net::UnixListener::bind(&path).unwrap();
    let path = std::ffi::CString::new(path.to_str().unwrap()).unwrap();
    assert_eq!(
        unsafe { muster_desktop_prepare(path.as_ptr(), 1, None, &mut error) },
        -1
    );
    unsafe { muster_string_free(error) };
    assert_eq!(
        unsafe { muster_desktop_prepare(path.as_ptr(), 0, None, &mut error) },
        0
    );
    assert!(error.is_null());
    unsafe extern "C" fn approval(_: u32) -> i32 {
        1
    }
    let missing = std::ffi::CString::new(dir.path().join("missing").to_str().unwrap()).unwrap();
    assert_eq!(
        unsafe { muster_desktop_prepare(missing.as_ptr(), 1, Some(approval), &mut error) },
        -1
    );
    assert!(
        unsafe { CStr::from_ptr(error) }
            .to_str()
            .unwrap()
            .contains("Login Items")
    );
    unsafe { muster_string_free(error) };
}
