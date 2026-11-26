//! Lock state of the interactive session that owns this daemon, including RDP.
use windows_sys::Win32::System::RemoteDesktop::*;

pub(crate) mod chat;
pub(crate) mod code;
mod link_clipboard;
mod provider;
pub struct Windows;

/// An elevated batch logon must never offer the user's full Terminal or Agent services.
pub fn require_ordinary_user() -> std::io::Result<()> {
    use std::os::windows::io::{AsRawHandle, FromRawHandle, OwnedHandle};
    use windows_sys::Win32::{Security::*, System::Threading::*};
    unsafe {
        let mut raw = std::ptr::null_mut();
        if OpenProcessToken(GetCurrentProcess(), TOKEN_QUERY, &mut raw) == 0 {
            return Err(std::io::Error::last_os_error());
        }
        let token = OwnedHandle::from_raw_handle(raw);
        let mut elevation = TOKEN_ELEVATION::default();
        let mut bytes = 0;
        if GetTokenInformation(
            token.as_raw_handle(),
            TokenElevation,
            (&raw mut elevation).cast(),
            size_of_val(&elevation) as u32,
            &mut bytes,
        ) == 0
        {
            return Err(std::io::Error::last_os_error());
        }
        let mut sid = [0u32; SECURITY_MAX_SID_SIZE as usize / 4];
        let mut size = size_of_val(&sid) as u32;
        let mut admin = 0;
        if CreateWellKnownSid(
            WinBuiltinAdministratorsSid,
            std::ptr::null_mut(),
            sid.as_mut_ptr().cast(),
            &mut size,
        ) == 0
            || CheckTokenMembership(
                std::ptr::null_mut(),
                sid.as_ptr().cast_mut().cast(),
                &mut admin,
            ) == 0
        {
            return Err(std::io::Error::last_os_error());
        }
        if elevation.TokenIsElevated != 0 || admin != 0 {
            return Err(std::io::Error::new(
                std::io::ErrorKind::PermissionDenied,
                "enrolled musterd requires a non-administrative user token",
            ));
        }
        Ok(())
    }
}
fn session() -> Option<WTSINFOEX_LEVEL1_W> {
    let mut buffer = std::ptr::null_mut();
    let mut size = 0;
    unsafe {
        if WTSQuerySessionInformationW(
            std::ptr::null_mut(),
            WTS_CURRENT_SESSION,
            WTSSessionInfoEx,
            &mut buffer,
            &mut size,
        ) == 0
        {
            return None;
        }
        let result = if size as usize >= size_of::<WTSINFOEXW>() {
            let info = &*buffer.cast::<WTSINFOEXW>();
            (info.Level == 1).then(|| info.Data.WTSInfoExLevel1)
        } else {
            None
        };
        WTSFreeMemory(buffer.cast());
        result
    }
}

impl crate::Platform for Windows {
    fn accessibility(&self) -> Option<bool> {
        Some(!crate::Platform::locked(self))
    }
    fn debug(&self, what: &str, arg: &str) -> Result<String, String> {
        match what {
            "ax-chat" => crate::native_chat::diagnostic(arg),
            _ => Err(format!("no Windows diagnostic {what:?}")),
        }
    }
    fn locked(&self) -> bool {
        !session().is_some_and(|s| {
            s.SessionState == WTSActive && s.SessionFlags as u32 == WTS_SESSIONSTATE_UNLOCK
        })
    }
    fn on_console(&self) -> bool {
        session().is_some_and(|s| s.SessionState == WTSActive)
    }
}

pub fn desktop_pixels() -> Option<(u32, u32)> {
    use windows_sys::Win32::Graphics::Gdi::{
        DEVMODEW, ENUM_CURRENT_SETTINGS, EnumDisplaySettingsW,
    };
    let mut mode: DEVMODEW = unsafe { std::mem::zeroed() };
    mode.dmSize = size_of::<DEVMODEW>() as u16;
    (unsafe { EnumDisplaySettingsW(std::ptr::null(), ENUM_CURRENT_SETTINGS, &mut mode) } != 0)
        .then_some((mode.dmPelsWidth, mode.dmPelsHeight))
}
