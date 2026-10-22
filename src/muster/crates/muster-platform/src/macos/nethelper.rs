//! The NetHelper client: the bundled root LaunchDaemon `dev.linsheng.muster.helper`
//! creates the utun, assigns the address from its root-owned config, installs
//! the one route, and hands the descriptor over XPC. musterd then owns the
//! data plane (`VnetConfig::existing_fd`).

use std::ffi::{CStr, CString, c_char, c_void};
use std::os::fd::RawFd;

type XpcObject = *mut c_void;
type XpcConnection = *mut c_void;

#[link(name = "System", kind = "dylib")]
unsafe extern "C" {
    fn xpc_connection_create_mach_service(
        name: *const c_char,
        queue: *mut c_void,
        flags: u64,
    ) -> XpcConnection;
    fn xpc_connection_set_event_handler(connection: XpcConnection, handler: *const c_void);
    fn xpc_connection_resume(connection: XpcConnection);
    fn xpc_connection_cancel(connection: XpcConnection);
    fn xpc_connection_send_message_with_reply_sync(
        connection: XpcConnection,
        message: XpcObject,
    ) -> XpcObject;
    fn xpc_dictionary_create(
        keys: *const *const c_char,
        values: *const XpcObject,
        count: usize,
    ) -> XpcObject;
    fn xpc_dictionary_set_string(dict: XpcObject, key: *const c_char, value: *const c_char);
    fn xpc_dictionary_set_int64(dict: XpcObject, key: *const c_char, value: i64);
    fn xpc_dictionary_get_string(dict: XpcObject, key: *const c_char) -> *const c_char;
    fn xpc_dictionary_get_bool(dict: XpcObject, key: *const c_char) -> bool;
    fn xpc_dictionary_dup_fd(dict: XpcObject, key: *const c_char) -> RawFd;
    fn xpc_release(object: XpcObject);
    fn xpc_get_type(object: XpcObject) -> *const c_void;
    static _xpc_type_error: u8;
}

const XPC_CONNECTION_MACH_SERVICE_PRIVILEGED: u64 = 2;
pub const SERVICE: &str = "dev.linsheng.muster.helper";

// Cancellation delivers an asynchronous final event, after NetHelper::drop.
// A global block must actually live in static storage, not a Box tagged global
// (which XPC does not copy). block2 also supplies the correct Blocks ABI.
block2::global_block! {
    static IGNORE_EVENT = |_event: XpcObject| {};
}

pub struct NetHelper {
    connection: XpcConnection,
    pub ifname: Option<String>,
}

// SAFETY: XPC connections are thread-safe; the block never touches memory.
unsafe impl Send for NetHelper {}
unsafe impl Sync for NetHelper {}

fn cstring(s: &str) -> CString {
    CString::new(s.replace('\0', "")).unwrap_or_default()
}

impl NetHelper {
    pub fn connect() -> Result<Self, String> {
        Self::connect_service(SERVICE)
    }

    fn connect_service(service: &str) -> Result<Self, String> {
        // SAFETY: NULL asks XPC to create its private target queue.
        unsafe { Self::connect_on_queue(service, std::ptr::null_mut()) }
    }

    /// `queue` must be NULL or a live dispatch queue.
    unsafe fn connect_on_queue(service: &str, queue: *mut c_void) -> Result<Self, String> {
        let name = cstring(service);
        // SAFETY: valid C string; NULL queue means a private one.
        let connection = unsafe {
            xpc_connection_create_mach_service(
                name.as_ptr(),
                queue,
                XPC_CONNECTION_MACH_SERVICE_PRIVILEGED,
            )
        };
        if connection.is_null() {
            return Err("cannot create the NetHelper connection".into());
        }
        // SAFETY: the block has process lifetime, including cancellation callbacks.
        unsafe {
            xpc_connection_set_event_handler(
                connection,
                (&*IGNORE_EVENT as *const block2::Block<dyn Fn(XpcObject)>).cast(),
            );
            xpc_connection_resume(connection);
        }
        Ok(Self {
            connection,
            ifname: None,
        })
    }

    /// The bundled root helper chooses all installation paths and derives
    /// the target account from our kernel audit token. No writable arguments.
    pub fn install_locked_use() -> Result<(), String> {
        let helper = Self::connect_service("dev.linsheng.muster.helper")?;
        // SAFETY: create a dictionary and set its sole, fixed operation.
        let message = unsafe { xpc_dictionary_create(std::ptr::null(), std::ptr::null(), 0) };
        let (key, value) = (cstring("op"), cstring("install-locked-use"));
        unsafe { xpc_dictionary_set_string(message, key.as_ptr(), value.as_ptr()) };
        let reply = helper.call(message)?;
        let result = Self::reply_ok(reply);
        // SAFETY: release our owned reply.
        unsafe { xpc_release(reply) };
        result
    }

    fn call(&self, message: XpcObject) -> Result<XpcObject, String> {
        // SAFETY: valid connection and message; the reply is +1.
        let reply =
            unsafe { xpc_connection_send_message_with_reply_sync(self.connection, message) };
        // SAFETY: releasing our message.
        unsafe { xpc_release(message) };
        if reply.is_null() {
            return Err("no reply from the NetHelper".into());
        }
        // SAFETY: valid object.
        if unsafe { xpc_get_type(reply) } == std::ptr::addr_of!(_xpc_type_error) as *const c_void {
            // SAFETY: releasing the reply.
            unsafe { xpc_release(reply) };
            return Err("the NetHelper is not running (install its LaunchDaemon)".into());
        }
        Ok(reply)
    }

    fn reply_string(reply: XpcObject, key: &str) -> String {
        let key = cstring(key);
        // SAFETY: valid reply; the pointer is owned by the reply.
        let ptr = unsafe { xpc_dictionary_get_string(reply, key.as_ptr()) };
        if ptr.is_null() {
            return String::new();
        }
        // SAFETY: NUL-terminated string owned by the reply.
        unsafe { CStr::from_ptr(ptr) }
            .to_string_lossy()
            .into_owned()
    }

    fn reply_ok(reply: XpcObject) -> Result<(), String> {
        let key = cstring("ok");
        // SAFETY: valid reply.
        if unsafe { xpc_dictionary_get_bool(reply, key.as_ptr()) } {
            Ok(())
        } else {
            Err(Self::reply_string(reply, "reason"))
        }
    }

    pub fn ping(&self) -> Result<String, String> {
        // SAFETY: creating an empty dictionary.
        let message = unsafe { xpc_dictionary_create(std::ptr::null(), std::ptr::null(), 0) };
        let (op, ping, nonce_key, nonce) = (
            cstring("op"),
            cstring("ping"),
            cstring("nonce"),
            cstring("musterd"),
        );
        // SAFETY: valid dictionary and strings.
        unsafe {
            xpc_dictionary_set_string(message, op.as_ptr(), ping.as_ptr());
            xpc_dictionary_set_string(message, nonce_key.as_ptr(), nonce.as_ptr());
        }
        let reply = self.call(message)?;
        let result = Self::reply_ok(reply).map(|()| Self::reply_string(reply, "version"));
        // SAFETY: releasing the reply.
        unsafe { xpc_release(reply) };
        result
    }

    /// Opens the utun with `address` ("a.b.c.d", inside the helper's
    /// configured prefix) and `mtu`; returns the descriptor musterd now owns.
    pub fn open(&mut self, address: &str, mtu: i64) -> Result<RawFd, String> {
        // SAFETY: creating an empty dictionary.
        let message = unsafe { xpc_dictionary_create(std::ptr::null(), std::ptr::null(), 0) };
        let (op, open, address_key, address_value, mtu_key) = (
            cstring("op"),
            cstring("open"),
            cstring("address"),
            cstring(address),
            cstring("mtu"),
        );
        // SAFETY: valid dictionary and strings.
        unsafe {
            xpc_dictionary_set_string(message, op.as_ptr(), open.as_ptr());
            xpc_dictionary_set_string(message, address_key.as_ptr(), address_value.as_ptr());
            xpc_dictionary_set_int64(message, mtu_key.as_ptr(), mtu);
        }
        let reply = self.call(message)?;
        let result = Self::reply_ok(reply).and_then(|()| {
            let key = cstring("fd");
            // SAFETY: valid reply; dup_fd returns -1 when absent.
            let fd = unsafe { xpc_dictionary_dup_fd(reply, key.as_ptr()) };
            if fd < 0 {
                return Err("the helper sent no descriptor".into());
            }
            self.ifname = Some(Self::reply_string(reply, "ifname"));
            Ok(fd)
        });
        // SAFETY: releasing the reply.
        unsafe { xpc_release(reply) };
        result
    }

    pub fn close(&mut self) {
        // SAFETY: creating an empty dictionary.
        let message = unsafe { xpc_dictionary_create(std::ptr::null(), std::ptr::null(), 0) };
        let (op, close) = (cstring("op"), cstring("close"));
        // SAFETY: valid dictionary and strings.
        unsafe { xpc_dictionary_set_string(message, op.as_ptr(), close.as_ptr()) };
        if let Ok(reply) = self.call(message) {
            // SAFETY: releasing the reply.
            unsafe { xpc_release(reply) };
        }
        self.ifname = None;
    }
}

impl Drop for NetHelper {
    fn drop(&mut self) {
        // SAFETY: release our +1 connection. XPC retains it for pending events;
        // their handler remains valid independently of this Rust owner.
        unsafe {
            xpc_connection_cancel(self.connection);
            xpc_release(self.connection);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    unsafe extern "C" {
        fn dispatch_queue_create(label: *const c_char, attr: *const c_void) -> *mut c_void;
        fn dispatch_suspend(queue: *mut c_void);
        fn dispatch_resume(queue: *mut c_void);
        fn dispatch_release(queue: *mut c_void);
    }

    // Isolate the asynchronous XPC callbacks: the old heap block tagged global
    // can crash after Drop returns. Scribbling makes that lifetime bug visible.
    #[test]
    fn cancellation_callbacks_outlive_the_rust_owner() {
        const CHILD: &str = "MUSTER_TEST_XPC_CANCEL_CHILD";
        if std::env::var_os(CHILD).is_some() {
            let service = format!("dev.linsheng.muster.test.missing.{}", std::process::id());
            // Hold all events until every Rust owner has been destroyed.
            // This forces the cancellation/Drop race rather than relying on luck.
            unsafe {
                let queue =
                    dispatch_queue_create(c"muster-xpc-cancel-test".as_ptr(), std::ptr::null());
                assert!(!queue.is_null());
                dispatch_suspend(queue);
                for _ in 0..200 {
                    let helper = NetHelper::connect_on_queue(&service, queue).unwrap();
                    drop(helper);
                }
                dispatch_resume(queue);
                std::thread::sleep(std::time::Duration::from_millis(500));
                dispatch_release(queue);
            }
            return;
        }
        let status = std::process::Command::new(std::env::current_exe().unwrap())
            .args([
                "--exact",
                "macos::nethelper::tests::cancellation_callbacks_outlive_the_rust_owner",
                "--nocapture",
            ])
            .env(CHILD, "1")
            .env("MallocScribble", "1")
            .status()
            .unwrap();
        assert!(status.success(), "XPC cancellation child failed: {status}");
    }
}
