//! Read one provider Copy deeplink result. Keep existing text formats as opaque
//! copies and do not restore over another clipboard owner.
use muster_agent::Error;
use std::{
    ptr, thread,
    time::{Duration, Instant},
};
use windows_sys::Win32::{
    Foundation::{GetLastError, GlobalFree, HANDLE, HWND, SetLastError},
    System::{DataExchange::*, Memory::*, Ole::OleDuplicateData},
    UI::WindowsAndMessaging::*,
};

fn unavailable() -> Error {
    Error::Unavailable("Cannot safely use the clipboard to verify the Codex thread".into())
}

struct Hidden(HWND);
impl Drop for Hidden {
    fn drop(&mut self) {
        unsafe {
            DestroyWindow(self.0);
        }
    }
}
struct Open;
impl Open {
    fn new(hwnd: HWND) -> Result<Self, Error> {
        let deadline = Instant::now() + Duration::from_millis(400);
        loop {
            if unsafe { OpenClipboard(hwnd) } != 0 {
                return Ok(Self);
            }
            if Instant::now() >= deadline {
                return Err(unavailable());
            }
            thread::sleep(Duration::from_millis(10));
        }
    }
}
impl Drop for Open {
    fn drop(&mut self) {
        unsafe {
            CloseClipboard();
        }
    }
}
struct Saved(u32, HANDLE);
impl Drop for Saved {
    fn drop(&mut self) {
        if !self.1.is_null() {
            unsafe {
                GlobalFree(self.1);
            }
        }
    }
}
fn save() -> Result<Vec<Saved>, Error> {
    let mut saved = Vec::new();
    let mut format = 0;
    let mut total = 0usize;
    loop {
        unsafe {
            SetLastError(0);
            format = EnumClipboardFormats(format);
        }
        if format == 0 {
            return if unsafe { GetLastError() } == 0 {
                Ok(saved)
            } else {
                Err(unavailable())
            };
        }
        if !supported(format) {
            return Err(unavailable());
        }
        saved.push(duplicate(
            format,
            unsafe { GetClipboardData(format) },
            &mut total,
        )?);
    }
}
fn supported(format: u32) -> bool {
    // CF_TEXT, CF_OEMTEXT, CF_UNICODETEXT and CF_LOCALE use HGLOBAL.
    matches!(format, 1 | 7 | 13 | 16)
}
fn duplicate(format: u32, original: HANDLE, total: &mut usize) -> Result<Saved, Error> {
    if original.is_null() {
        return Err(unavailable());
    }
    let size = unsafe { GlobalSize(original) };
    *total = total.saturating_add(size);
    if size == 0 || *total > 4 * 1024 * 1024 {
        return Err(unavailable());
    }
    let copy = unsafe { OleDuplicateData(original, format as u16, 0) };
    if copy.is_null() {
        return Err(unavailable());
    }
    Ok(Saved(format, copy))
}
fn link() -> Result<String, Error> {
    let data = unsafe { GetClipboardData(13) };
    if data.is_null() {
        return Err(unavailable());
    }
    let size = unsafe { GlobalSize(data) };
    if !(2..=2048).contains(&size) || size % 2 != 0 {
        return Err(unavailable());
    }
    let ptr = unsafe { GlobalLock(data) } as *const u16;
    if ptr.is_null() {
        return Err(unavailable());
    }
    let units = unsafe { std::slice::from_raw_parts(ptr, size / 2) };
    let result = units
        .iter()
        .position(|v| *v == 0)
        .and_then(|n| String::from_utf16(&units[..n]).ok());
    unsafe {
        GlobalUnlock(data);
    }
    result
        .filter(|v| v.starts_with("codex://threads/"))
        .ok_or_else(unavailable)
}
pub(super) fn copied(
    pid: u32,
    invoke: impl FnOnce() -> Result<(), Error>,
) -> Result<String, Error> {
    let hwnd = unsafe {
        CreateWindowExW(
            0,
            windows_sys::w!("STATIC"),
            ptr::null(),
            0,
            0,
            0,
            0,
            0,
            HWND_MESSAGE,
            ptr::null_mut(),
            ptr::null_mut(),
            ptr::null(),
        )
    };
    if hwnd.is_null() {
        return Err(unavailable());
    }
    let window = Hidden(hwnd);
    let (sequence, mut saved) = {
        let _open = Open::new(window.0)?;
        let saved = save()?;
        (unsafe { GetClipboardSequenceNumber() }, saved)
    };
    if sequence == 0 || sequence != unsafe { GetClipboardSequenceNumber() } {
        return Err(unavailable());
    }
    // The provider may have copied before reporting a UIA error.
    let input = super::chat::target_input_generation(pid);
    let action = invoke();
    let deadline = Instant::now() + Duration::from_secs(2);
    loop {
        if unsafe { GetClipboardSequenceNumber() } != sequence {
            let _open = Open::new(window.0)?;
            let owner = unsafe { GetClipboardOwner() };
            let mut actual = 0;
            unsafe {
                GetWindowThreadProcessId(owner, &mut actual);
            }
            if actual != pid || owner.is_null() {
                return Err(unavailable());
            }
            let value = link();
            if super::chat::target_input_generation(pid) != input {
                return Err(unavailable());
            }
            if unsafe { EmptyClipboard() } == 0 {
                return Err(unavailable());
            }
            let mut restored = true;
            for slot in &mut saved {
                if unsafe { SetClipboardData(slot.0, slot.1) }.is_null() {
                    restored = false;
                } else {
                    slot.1 = ptr::null_mut();
                }
            }
            if !restored {
                return Err(unavailable());
            }
            action?;
            return value;
        }
        if Instant::now() >= deadline {
            return Err(unavailable());
        }
        thread::sleep(Duration::from_millis(15));
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use windows_sys::Win32::System::StationsAndDesktops::*;

    // Run in a child process with an independent clipboard. Never open the
    // interactive window station's clipboard, including when setup fails.
    #[test]
    fn preserves_opaque_text_and_rejects_nontext_formats() {
        assert!(!supported(2) && !supported(0xc000));
        let data = unsafe { GlobalAlloc(GMEM_MOVEABLE, 64) };
        assert!(!data.is_null());
        let original = Saved(13, data);
        let mut total = 0;
        let block = unsafe { GlobalLock(original.1) }.cast::<u16>();
        assert!(!block.is_null());
        unsafe {
            *block = 0x4e00;
            *block.add(1) = 0;
            GlobalUnlock(original.1);
        }
        let backup = duplicate(13, original.1, &mut total).unwrap();
        drop(original);
        let block = unsafe { GlobalLock(backup.1) }.cast::<u16>();
        assert!(!block.is_null());
        unsafe {
            assert_eq!(*block, 0x4e00);
            assert_eq!(*block.add(1), 0);
            GlobalUnlock(backup.1);
        }
        let mut too_large = 4 * 1024 * 1024;
        assert!(duplicate(13, backup.1, &mut too_large).is_err());
    }

    #[test]
    #[ignore = "requires permission to create a private window station"]
    fn restores_text_on_private_window_station() {
        const CHILD: &str = "MUSTER_TEST_PRIVATE_CLIPBOARD";
        if std::env::var_os(CHILD).is_none() {
            let status = std::process::Command::new(std::env::current_exe().unwrap())
                .args([
                    "--exact",
                    "windows::link_clipboard::tests::restores_text_on_private_window_station",
                    "--ignored",
                    "--test-threads=1",
                    "--nocapture",
                ])
                .env(CHILD, "1")
                .status()
                .unwrap();
            assert!(status.success());
            return;
        }
        unsafe {
            let name: Vec<u16> = format!("Muster-clipboard-test-{}", std::process::id())
                .encode_utf16()
                .chain(Some(0))
                .collect();
            let station = CreateWindowStationW(name.as_ptr(), 0, 0x02000000, ptr::null());
            assert!(!station.is_null() && SetProcessWindowStation(station) != 0);
            let desk = CreateDesktopW(
                windows_sys::w!("Fixture"),
                ptr::null(),
                ptr::null(),
                0,
                0x02000000,
                ptr::null(),
            );
            assert!(!desk.is_null() && SetThreadDesktop(desk) != 0);
            assert!(GetProcessWindowStation() == station);
        }
        let hwnd = Hidden(unsafe {
            CreateWindowExW(
                0,
                windows_sys::w!("STATIC"),
                ptr::null(),
                0,
                0,
                0,
                0,
                0,
                HWND_MESSAGE,
                ptr::null_mut(),
                ptr::null_mut(),
                ptr::null(),
            )
        });
        assert!(!hwnd.0.is_null());
        let write = |text: &str| {
            let _open = Open::new(hwnd.0).unwrap();
            let units: Vec<_> = text.encode_utf16().chain(Some(0)).collect();
            unsafe {
                assert_ne!(EmptyClipboard(), 0);
                let data = GlobalAlloc(GMEM_MOVEABLE, units.len() * 2);
                assert!(!data.is_null());
                let target = GlobalLock(data).cast::<u16>();
                assert!(!target.is_null());
                ptr::copy_nonoverlapping(units.as_ptr(), target, units.len());
                GlobalUnlock(data);
                assert!(!SetClipboardData(13, data).is_null());
            }
        };
        let original = "codex://threads/original-fixture";
        let target = "codex://threads/test-fixture";
        write(original);
        let result = copied(std::process::id(), || {
            write(target);
            Ok(())
        })
        .unwrap();
        assert_eq!(result, target);
        {
            let _open = Open::new(hwnd.0).unwrap();
            assert_eq!(link().unwrap(), original);
        }
        assert!(
            copied(std::process::id(), || {
                write("unrecognized provider link");
                Ok(())
            })
            .is_err()
        );
        {
            let _open = Open::new(hwnd.0).unwrap();
            assert_eq!(link().unwrap(), original);
        }
        let other =
            unsafe { RegisterClipboardFormatW(windows_sys::w!("Muster fixture custom data")) };
        {
            let _open = Open::new(hwnd.0).unwrap();
            let h = unsafe { GlobalAlloc(GMEM_MOVEABLE, 8) };
            assert!(!h.is_null() && !unsafe { SetClipboardData(other, h) }.is_null());
        }
        assert!(
            copied(std::process::id(), || panic!(
                "custom data must be preserved before copy"
            ))
            .is_err()
        );
    }
}
