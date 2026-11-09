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

/// What a trusted process sees of an application's windows, asked attribute by
/// attribute. `ax-dump` walks `AXChildren`, which for an application yields the
/// menu bar and not the windows, so a dump that shows no `AXWindow` says
/// nothing about windows; this asks `AXWindows` and its neighbours by name.
///
/// Read only, and deliberately so: unlike `ax-dump` this does not call
/// `enable_manual_accessibility`, because setting `AXManualAccessibility` or
/// `AXEnhancedUserInterface` changes the app being measured and would confound
/// the reading (ADR 0129).
fn windows_diagnostic(pid: u32, point: Option<(f64, f64)>) -> String {
    use super::ax::Element;
    let app = Element::application(pid);
    let mut out = String::new();
    let line = |out: &mut String, text: String| {
        out.push_str(&text);
        out.push('\n');
    };
    let describe = |element: &Element| -> String {
        format!(
            "role={} subrole={:?} title={:?}",
            element.role(),
            element.string("AXSubrole").unwrap_or_default(),
            element.string("AXTitle").unwrap_or_default()
        )
    };
    line(&mut out, format!("pid={pid} {}", describe(&app)));

    let children = app.children();
    line(
        &mut out,
        format!(
            "AXChildren: count={} roles=[{}]",
            children.len(),
            children
                .iter()
                .map(Element::role)
                .collect::<Vec<_>>()
                .join(", ")
        ),
    );

    let status = app.status("AXWindows");
    let windows = app
        .attribute("AXWindows")
        .map(|value| Element::elements_of(&value))
        .unwrap_or_default();
    line(
        &mut out,
        format!("AXWindows: status={status} count={}", windows.len()),
    );
    for (index, window) in windows.iter().enumerate() {
        line(&mut out, format!("  [{index}] {}", describe(window)));
    }

    for name in ["AXMainWindow", "AXFocusedWindow", "AXMenuBar"] {
        let found = app.element(name);
        line(
            &mut out,
            match found {
                Some(element) => {
                    format!("{name}: status={} {}", app.status(name), describe(&element))
                }
                None => format!("{name}: status={} none", app.status(name)),
            },
        );
    }

    let names = app.attribute_names();
    line(
        &mut out,
        format!(
            "attribute names: count={} advertises AXWindows={} [{}]",
            names.len(),
            names.iter().any(|n| n == "AXWindows"),
            names.join(", ")
        ),
    );

    // A window the application does not list may still answer at its own
    // position, which is a different route into the same tree. This hit-tests
    // whatever is frontmost at the point, so it says something about this app
    // only when this app's window is in front there.
    if let Some((x, y)) = point {
        let hit = Element::system_wide().element_at(x, y);
        line(
            &mut out,
            match hit {
                Some(element) => format!(
                    "elementAtPosition({x},{y}): pid={:?} {}",
                    element.pid(),
                    describe(&element)
                ),
                None => format!("elementAtPosition({x},{y}): none"),
            },
        );
    }
    out
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
    fn signed_app_child(&self, pid: u32, start: u64, requirement: &str) -> bool {
        super::codesign::signed_gui_child(pid, start, requirement)
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
            // Ask a window question in an unlocked session. While the screen
            // is locked -- including under a Locked Use cover -- the
            // Accessibility API hands back the application element in place of
            // window elements, for every app, so a walk reports no `AXWindow`
            // and the attribute reads mislabel (ADR 0131 corrects ADR 0129,
            // which took that artifact for a blind spot in this action).
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
            "ax-windows" => {
                if !super::ax::trusted() {
                    return Err("Accessibility not granted".into());
                }
                // `<pid|App> [x,y]`: the point is optional and is asked of the
                // system-wide element, not of this application.
                let mut fields = arg.split_whitespace();
                let who = fields.next().unwrap_or_default();
                let point = match fields.next() {
                    None => None,
                    Some(pair) => match pair.split_once(',') {
                        Some((x, y)) => match (x.trim().parse::<f64>(), y.trim().parse::<f64>()) {
                            (Ok(x), Ok(y)) => Some((x, y)),
                            _ => return Err("usage: ax-windows <pid|App> [x,y]".into()),
                        },
                        None => return Err("usage: ax-windows <pid|App> [x,y]".into()),
                    },
                };
                Ok(windows_diagnostic(pid(who)?, point))
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
