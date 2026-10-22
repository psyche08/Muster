//! Ghostty through its AppleScript dictionary (list, type, focus, create,
//! close) and accessibility (a terminal's screen text). Ghostty has no API
//! like iTerm2's. Apple Events need the Automation permission for musterd to
//! control Ghostty; it is checked without prompting before every use, so a
//! consent dialog never waits unseen under a Locked Use cover. The screen is
//! readable only while the console is unlocked.
use std::io::Read;
use std::process::{Command, Stdio};
use std::time::{Duration, Instant};

use super::ax::{self, Element};
use super::{automation, procs};

pub const BUNDLE: &str = "com.mitchellh.ghostty";
const EXECUTABLE: &str = "Ghostty.app/Contents/MacOS/ghostty";
const SCRIPT_TIMEOUT: Duration = Duration::from_secs(5);
/// UTF-16 units read from the end of a screen's text: a few hundred rows.
const TAIL: usize = 64 * 1024;
/// What Ghostty's terminal view tells accessibility it is.
const SURFACE_HELP: &str = "Terminal content area";
const AX_INVALID_ELEMENT: i32 = -25202;

/// One terminal: id, window, tab, title, working directory.
pub type Listed = (String, String, String, String, String);

pub fn installed() -> bool {
    automation::installed(BUNDLE) == Some(true)
}

fn pid() -> Option<u32> {
    procs::pids_by_executable_suffix(EXECUTABLE)
        .into_iter()
        .next()
}

pub fn running() -> bool {
    pid().is_some()
}

/// granted | missing | not_running | not_installed | unavailable, for
/// Ghostty's `perform action` event (Ghst/PfAc). `ask` only for a local
/// user's explicit setup action.
pub fn permission(ask: bool) -> &'static str {
    automation::permission(BUNDLE, *b"Ghst", *b"PfAc", ask)
}

/// Runs an AppleScript `run` handler with `args`. Never launches Ghostty:
/// `tell application` would, so a stopped Ghostty is refused first.
fn script(body: &str, args: &[&str]) -> Result<String, String> {
    if !running() {
        return Err("Ghostty is not running on the Mac".into());
    }
    match permission(false) {
        "granted" => {}
        "missing" => {
            return Err("Muster may not control Ghostty yet: on the Mac, open Muster and allow it (System Settings > Privacy & Security > Automation)".into());
        }
        other => return Err(format!("Ghostty automation is {other}")),
    }
    let source = format!(
        "on run argv\nwith timeout of 4 seconds\ntell application id \"{BUNDLE}\"\n{body}\nend tell\nend timeout\nend run"
    );
    let mut child = Command::new("/usr/bin/osascript")
        .arg("-e")
        .arg(source)
        .args(args)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .map_err(|e| format!("osascript: {e}"))?;
    let deadline = Instant::now() + SCRIPT_TIMEOUT;
    let status = loop {
        if let Some(status) = child.try_wait().map_err(|e| e.to_string())? {
            break status;
        }
        if Instant::now() >= deadline {
            let _ = child.kill();
            let _ = child.wait();
            return Err("Ghostty did not answer within 5 s".into());
        }
        std::thread::sleep(Duration::from_millis(10));
    };
    let mut out = String::new();
    let mut err = String::new();
    if let Some(mut s) = child.stdout.take() {
        let _ = s.read_to_string(&mut out);
    }
    if let Some(mut s) = child.stderr.take() {
        let _ = s.read_to_string(&mut err);
    }
    if !status.success() {
        return Err(format!("Ghostty: {}", err.trim()));
    }
    Ok(out.trim_end_matches('\n').to_owned())
}

/// Every terminal of every window and tab.
pub fn terminals() -> Result<Vec<Listed>, String> {
    let out = script(
        "set us to character id 31
set rs to character id 30
set out to \"\"
repeat with w in windows
set wid to id of w
repeat with t in tabs of w
set tid to id of t
repeat with x in terminals of t
set out to out & (id of x) & us & wid & us & tid & us & (name of x) & us & (working directory of x) & rs
end repeat
end repeat
end repeat
return out",
        &[],
    )?;
    Ok(parse_terminals(&out))
}

fn parse_terminals(out: &str) -> Vec<Listed> {
    out.split('\u{1e}')
        .filter_map(|record| {
            let mut f = record
                .split('\u{1f}')
                .map(|v| v.trim_matches('\n').to_owned());
            let id = f.next().filter(|id| !id.is_empty())?;
            Some((
                id,
                f.next().unwrap_or_default(),
                f.next().unwrap_or_default(),
                f.next().unwrap_or_default(),
                f.next().unwrap_or_default(),
            ))
        })
        .collect()
}

pub fn perform(id: &str, action: &str) -> Result<(), String> {
    let done = script(
        "return (perform action (item 2 of argv) on terminal id (item 1 of argv)) as text",
        &[id, action],
    )?;
    if done == "true" {
        Ok(())
    } else {
        Err("Ghostty did not perform the input".into())
    }
}

fn focused() -> Result<String, String> {
    script(
        "return id of focused terminal of selected tab of front window",
        &[],
    )
}

pub fn create() -> Result<String, String> {
    script(
        "set w to new window\nreturn id of focused terminal of selected tab of w",
        &[],
    )
}

pub fn close(id: &str) -> Result<(), String> {
    script("close terminal id (item 1 of argv)", &[id]).map(drop)
}

/// A terminal's screen as accessibility shows it.
pub struct Surface {
    element: Element,
}

/// Opens `id`'s screen. Accessibility identifies a terminal view only as the
/// focused one, so the terminal is brought to the front unless it already
/// is, and Ghostty's own focused-terminal id must match before and after the
/// view is taken.
pub fn open(id: &str) -> Result<Surface, String> {
    let pid = pid().ok_or("Ghostty is not running on the Mac")?;
    if !ax::trusted() {
        return Err("Muster has no Accessibility permission on the Mac".into());
    }
    if focused().ok().as_deref() != Some(id) {
        script("focus terminal id (item 1 of argv)", &[id])?;
    }
    let deadline = Instant::now() + Duration::from_secs(2);
    loop {
        if focused()? == id {
            let view = Element::application(pid)
                .element("AXFocusedUIElement")
                .filter(|e| {
                    e.role() == "AXTextArea" && e.string("AXHelp").as_deref() == Some(SURFACE_HELP)
                });
            if let Some(element) = view
                && focused()? == id
            {
                return Ok(Surface { element });
            }
        }
        if Instant::now() >= deadline {
            return Err(
                "Ghostty's terminal screen is not readable; the Mac must be unlocked".into(),
            );
        }
        std::thread::sleep(Duration::from_millis(100));
    }
}

impl Surface {
    /// The newest part of the screen text; `None` once the terminal is gone.
    pub fn text(&self) -> Result<Option<String>, String> {
        let Some(count) = self.element.number("AXNumberOfCharacters") else {
            return if self.element.status("AXRole") == AX_INVALID_ELEMENT {
                Ok(None)
            } else {
                Err("Ghostty's terminal screen is not readable; the Mac must be unlocked".into())
            };
        };
        let count = count.max(0) as usize;
        let start = count.saturating_sub(TAIL);
        // Ghostty counts characters but reads ranges in UTF-16 units: a range
        // may start inside a surrogate pair, and text may lie past `count`.
        let mut text = self
            .element
            .string_for_range(start, count - start)
            .or_else(|| {
                (start > 0)
                    .then(|| self.element.string_for_range(start - 1, count - start + 1))
                    .flatten()
            })
            .ok_or("Ghostty's terminal screen could not be read")?;
        let mut end = count;
        for chunk in [4096, 256, 16, 1] {
            while let Some(more) = self
                .element
                .string_for_range(end, chunk)
                .filter(|s| !s.is_empty())
            {
                end += chunk;
                text.push_str(&more);
            }
        }
        if start > 0 {
            // The first line is cut: drop it.
            text = text
                .split_once('\n')
                .map(|(_, rest)| rest.to_owned())
                .unwrap_or_default();
        }
        Ok(Some(text))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn listed_terminals_parse_from_separated_records() {
        let out = "A\u{1f}w1\u{1f}t1\u{1f}vim x\u{1f}/tmp\u{1e}B\u{1f}w1\u{1f}t2\u{1f}\u{1f}/Users/me\u{1e}";
        assert_eq!(
            parse_terminals(out),
            [
                (
                    "A".into(),
                    "w1".into(),
                    "t1".into(),
                    "vim x".into(),
                    "/tmp".into()
                ),
                (
                    "B".into(),
                    "w1".into(),
                    "t2".into(),
                    String::new(),
                    "/Users/me".into()
                ),
            ]
        );
        assert!(parse_terminals("").is_empty());
    }
}
