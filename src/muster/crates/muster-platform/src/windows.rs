//! Lock state of the interactive session that owns this daemon, including RDP.
use windows_sys::Win32::System::RemoteDesktop::*;

pub(crate) mod chat;
mod provider;
pub struct Windows;
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
