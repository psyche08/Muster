//! A notification-style banner at the top right of the main display while
//! Muster works in a desktop app the person may be using (ADR 0087). AppKit
//! windows need a main thread the daemon does not own, so the banner lives in
//! a short helper process (`muster-panel --muster-notice`, from the bundle's
//! `Contents/Helpers`: an AppKit process in `Contents/MacOS` would take the
//! app's Launch Services identity, ADR 0128). Dropping the handle closes it;
//! so does the daemon exiting, or [`LIFETIME`].

use std::{
    ffi::CString,
    io::{BufRead, Write},
    process::{Child, Command, Stdio},
    time::{Duration, Instant},
};

unsafe extern "C" {
    fn muster_notice_show(title: *const std::ffi::c_char, body: *const std::ffi::c_char) -> bool;
    fn muster_console_pump();
}

/// The helper's first argument.
pub const FLAG: &str = "--muster-notice";
/// The helper closes itself after this even if nobody closes it.
const LIFETIME: Duration = Duration::from_secs(10 * 60);

pub struct Notice {
    child: Child,
}

impl Notice {
    pub fn show(title: &str, body: &str) -> Result<Self, String> {
        let mut child = Command::new(super::bundle::panel()?)
            .arg(FLAG)
            .stdin(Stdio::piped())
            .stdout(Stdio::null())
            .stderr(Stdio::inherit())
            .spawn()
            .map_err(|e| e.to_string())?;
        let written = child
            .stdin
            .as_mut()
            .ok_or("no helper input")?
            .write_all(format!("{}\n{}\n", line(title), line(body)).as_bytes());
        let notice = Self { child };
        written.map_err(|e| e.to_string())?;
        Ok(notice)
    }
}

impl Drop for Notice {
    fn drop(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}

/// One line of text without NULs.
fn line(text: &str) -> String {
    text.chars()
        .map(|c| if c.is_control() { ' ' } else { c })
        .collect()
}

/// The helper: read the title and body, show the banner and keep it until
/// its input closes. Must run on the process main thread.
pub fn run() -> Result<(), String> {
    let mut lines = std::io::stdin().lock().lines();
    let mut next = || -> Result<CString, String> {
        let text = lines
            .next()
            .ok_or("missing notice text")?
            .map_err(|e| e.to_string())?;
        CString::new(line(&text)).map_err(|e| e.to_string())
    };
    let (title, body) = (next()?, next()?);
    // SAFETY: both pointers are live NUL-terminated strings.
    if !unsafe { muster_notice_show(title.as_ptr(), body.as_ptr()) } {
        return Err("cannot show the notice".into());
    }
    // SAFETY: plain fcntl on this process's standard input.
    unsafe {
        libc::fcntl(0, libc::F_SETFL, libc::O_NONBLOCK);
    }
    let started = Instant::now();
    while started.elapsed() < LIFETIME {
        // SAFETY: AppKit was set up on this thread by `muster_notice_show`.
        unsafe { muster_console_pump() };
        let mut byte = [0u8; 16];
        // SAFETY: reads into a live local buffer.
        let count = unsafe { libc::read(0, byte.as_mut_ptr().cast(), byte.len()) };
        if count == 0
            || (count < 0
                && std::io::Error::last_os_error().kind() != std::io::ErrorKind::WouldBlock)
        {
            break;
        }
    }
    Ok(())
}
