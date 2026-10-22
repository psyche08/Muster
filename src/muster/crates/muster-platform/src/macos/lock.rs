//! Console session state from Core Graphics (the same dictionary the old
//! `macos_claude_native_input.rs` read): on console, login done, and the
//! screen lock flag.

use core_foundation::base::{CFType, TCFType};
use core_foundation::boolean::CFBoolean;
use core_foundation::dictionary::CFDictionary;
use core_foundation::string::CFString;
use core_foundation_sys::dictionary::CFDictionaryRef;

use crate::Platform;

unsafe extern "C" {
    fn CGSessionCopyCurrentDictionary() -> CFDictionaryRef;
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct SessionState {
    pub on_console: bool,
    pub login_done: bool,
    pub screen_locked: bool,
}

/// `None` when the dictionary cannot be read: callers treat that as locked.
pub fn session_state() -> Option<SessionState> {
    // SAFETY: returns a +1 CFDictionary or NULL.
    let raw = unsafe { CGSessionCopyCurrentDictionary() };
    if raw.is_null() {
        return None;
    }
    // SAFETY: raw is a valid +1 reference we now own.
    let dict: CFDictionary<CFString, CFType> = unsafe { CFDictionary::wrap_under_create_rule(raw) };
    let flag = |key: &str| -> Option<bool> {
        let value = dict.find(CFString::new(key))?;
        value.downcast::<CFBoolean>().map(|b| b.into())
    };
    Some(SessionState {
        on_console: flag("kCGSSessionOnConsoleKey").unwrap_or(false),
        login_done: flag("kCGSessionLoginDoneKey").unwrap_or(false),
        screen_locked: flag("CGSSessionScreenIsLocked").unwrap_or(false),
    })
}

pub struct MacPlatform;

impl Platform for MacPlatform {
    fn locked_use(
        &self,
        dir: &std::path::Path,
    ) -> Option<std::sync::Arc<dyn crate::locked_use::LockedUseOs>> {
        Some(std::sync::Arc::new(super::locked_use::MacLockedUse::new(
            dir,
        )))
    }

    fn locked(&self) -> bool {
        match session_state() {
            Some(state) => state.screen_locked || !state.login_done,
            None => true,
        }
    }
    fn on_console(&self) -> bool {
        session_state().is_some_and(|s| s.on_console)
    }
    fn accessibility(&self) -> Option<bool> {
        Some(super::ax::trusted())
    }
    fn request_accessibility(&self) -> Option<bool> {
        Some(super::ax::request_trust())
    }
    fn debug(&self, what: &str, arg: &str) -> Result<String, String> {
        let pid = |arg: &str| -> Result<u32, String> {
            if let Ok(pid) = arg.parse::<u32>() {
                return Ok(pid);
            }
            let name = if arg.is_empty() { "Claude" } else { arg };
            let suffix = format!("{name}.app/Contents/MacOS/{name}");
            // A profiled app's GUI process, not a helper running its binary.
            match super::native_app::profiles()
                .into_iter()
                .find(|p| p.executable == suffix)
            {
                Some(profile) => super::native_app::app_pids(profile),
                None => super::procs::pids_by_executable_suffix(&suffix),
            }
            .into_iter()
            .next()
            .ok_or_else(|| format!("{name}.app is not running"))
        };
        match what {
            "locked-use" => Ok(super::locked_use::diagnostic()),
            "lock" => {
                if super::locked_use::lock_now() {
                    Ok("lock requested; confirm with `debug locked-use`".into())
                } else {
                    Err("the OS lock request is unavailable".into())
                }
            }
            "lock-field" => Ok(super::locked_use::field_diagnostic()),
            "ax-dump" => {
                if !super::ax::trusted() {
                    return Err("Accessibility not granted".into());
                }
                let app = super::ax::Element::application(pid(arg)?);
                app.enable_manual_accessibility();
                // A long conversation in the ChatGPT app runs to thousands of
                // nodes; recordings need the whole window (read only).
                Ok(app.dump(20_000, std::time::Duration::from_secs(30)))
            }
            "ax-chat" => super::native_chat::diagnostic(arg),
            "ax-sessions" => Ok(super::native_app::web_areas(pid(arg)?)
                .iter()
                .map(|k| {
                    format!(
                        "title={:?} url={:?}",
                        k.title.as_deref().unwrap_or(""),
                        k.url.as_deref().unwrap_or("")
                    )
                })
                .collect::<Vec<_>>()
                .join("\n")),
            "procs" => Ok(super::procs::pids_by_executable_suffix(arg)
                .iter()
                .map(|p| p.to_string())
                .collect::<Vec<_>>()
                .join(" ")),
            other => Err(format!("unknown diagnostic {other:?}")),
        }
    }
}
