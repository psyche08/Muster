//! macOS: Core Graphics session state, the HID input counter, code
//! signature checks, Accessibility control, the Secure Enclave signer and
//! the NetHelper XPC client.

pub mod ax;
pub mod codesign;
pub mod ghostty;
pub mod input;
pub mod lock;
pub mod native_app;
pub mod native_chat;
pub mod nethelper;
pub mod procs;
#[cfg(feature = "sep")]
pub mod sep;

pub mod locked_use;

pub mod automation;
