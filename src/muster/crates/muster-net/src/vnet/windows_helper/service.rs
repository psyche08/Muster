//! Demand-start Wintun service. SCM receives only the random pipe token.
//! The protected ImagePath supplies the account allowed to own that pipe.
use std::os::windows::io::{AsRawHandle, FromRawHandle, OwnedHandle};
use std::{io, ptr::null, sync::OnceLock, time::Duration};
use tokio::io::AsyncReadExt;
use windows_sys::Win32::{
    Foundation::{
        ERROR_CALL_NOT_IMPLEMENTED, ERROR_INVALID_PARAMETER, ERROR_SERVICE_DOES_NOT_EXIST,
    },
    System::{Pipes::GetNamedPipeServerProcessId, Services::*, Threading::*},
};
use windows_sys::core::w;

const NAME: *const u16 = w!("MusterNetwork");
static STOP: OnceLock<tokio::sync::watch::Sender<bool>> = OnceLock::new();

/// SCM handles are closed with CloseServiceHandle, not CloseHandle.
pub(super) struct Service(SC_HANDLE);
// SCM handles can be used from any thread while the owner keeps them alive.
unsafe impl Send for Service {}
impl Drop for Service {
    fn drop(&mut self) {
        unsafe {
            CloseServiceHandle(self.0);
        }
    }
}
impl Service {
    pub(super) fn pid(&self) -> io::Result<u32> {
        let mut status = SERVICE_STATUS_PROCESS::default();
        let mut needed = 0;
        if unsafe {
            QueryServiceStatusEx(
                self.0,
                SC_STATUS_PROCESS_INFO,
                (&raw mut status).cast(),
                size_of_val(&status) as _,
                &mut needed,
            )
        } == 0
        {
            return Err(io::Error::last_os_error());
        }
        Ok(status.dwProcessId)
    }
}

/// Only missing registration selects the pre-SCM interactive installation.
pub(super) fn start(token: u128) -> io::Result<Option<Service>> {
    unsafe {
        let scm = OpenSCManagerW(null(), null(), SC_MANAGER_CONNECT);
        if scm.is_null() {
            return Err(io::Error::last_os_error());
        }
        let scm = Service(scm);
        let raw = OpenServiceW(scm.0, NAME, SERVICE_START | SERVICE_QUERY_STATUS);
        if raw.is_null() {
            let err = io::Error::last_os_error();
            return if err.raw_os_error() == Some(ERROR_SERVICE_DOES_NOT_EXIST as i32) {
                Ok(None)
            } else {
                Err(err)
            };
        }
        let service = Service(raw);
        let argument: Vec<_> = format!("{token:032x}")
            .encode_utf16()
            .chain(Some(0))
            .collect();
        if StartServiceW(service.0, 1, &argument.as_ptr()) == 0 {
            return Err(io::Error::last_os_error());
        }
        Ok(Some(service))
    }
}

pub fn dispatch() -> io::Result<()> {
    let table = [
        SERVICE_TABLE_ENTRYW {
            lpServiceName: NAME.cast_mut(),
            lpServiceProc: Some(service_main),
        },
        SERVICE_TABLE_ENTRYW::default(),
    ];
    if unsafe { StartServiceCtrlDispatcherW(table.as_ptr()) } == 0 {
        return Err(io::Error::last_os_error());
    }
    Ok(())
}

unsafe extern "system" fn control(
    code: u32,
    _: u32,
    _: *mut std::ffi::c_void,
    _: *mut std::ffi::c_void,
) -> u32 {
    match code {
        SERVICE_CONTROL_STOP | SERVICE_CONTROL_SHUTDOWN => {
            if let Some(stop) = STOP.get() {
                stop.send_replace(true);
            }
            0
        }
        SERVICE_CONTROL_INTERROGATE => 0,
        _ => ERROR_CALL_NOT_IMPLEMENTED,
    }
}

unsafe extern "system" fn service_main(argc: u32, argv: *mut *mut u16) {
    let (stop, mut stopped) = tokio::sync::watch::channel(false);
    if STOP.set(stop).is_err() {
        return;
    }
    let handle = unsafe { RegisterServiceCtrlHandlerExW(NAME, Some(control), null()) };
    if handle.is_null() {
        return;
    }
    let status = |state, accepts, code| {
        let status = SERVICE_STATUS {
            dwServiceType: SERVICE_WIN32_OWN_PROCESS,
            dwCurrentState: state,
            dwControlsAccepted: accepts,
            dwWin32ExitCode: code,
            ..Default::default()
        };
        unsafe { SetServiceStatus(handle, &status) != 0 }
    };
    let mut action = || -> io::Result<()> {
        if argc != 2 || argv.is_null() {
            return Err(io::Error::from_raw_os_error(87));
        }
        // SCM supplies a terminated UTF-16 argument. Accept exactly 32 lowercase hex digits.
        let arg = unsafe { *argv.add(1) };
        if arg.is_null() {
            return Err(io::Error::from_raw_os_error(87));
        }
        let mut token = 0u128;
        for pos in 0..32 {
            let unit = unsafe { *arg.add(pos) };
            let digit = match unit {
                48..=57 => unit - 48,
                97..=102 => unit - 87,
                _ => return Err(io::Error::from_raw_os_error(87)),
            };
            token = (token << 4) | u128::from(digit);
        }
        if unsafe { *arg.add(32) } != 0 {
            return Err(io::Error::from_raw_os_error(87));
        }
        let owner = std::env::args()
            .nth(2)
            .ok_or_else(|| io::Error::from_raw_os_error(87))?;
        let rt = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()?;
        if !status(
            SERVICE_RUNNING,
            SERVICE_ACCEPT_STOP | SERVICE_ACCEPT_SHUTDOWN,
            0,
        ) {
            return Err(io::Error::last_os_error());
        }
        rt.block_on(async {
            tokio::select! {
                result = run(token, &owner) => result,
                _ = stopped.changed() => Ok(()),
            }
        })
    };
    let code = action()
        .err()
        .map(|e| {
            e.raw_os_error()
                .map(|v| v as u32)
                .unwrap_or(ERROR_INVALID_PARAMETER)
        })
        .unwrap_or(0);
    status(SERVICE_STOPPED, 0, code);
}

async fn run(token: u128, owner: &str) -> io::Result<()> {
    let mut pipe = muster_local::connect_to_owner(&super::endpoint(token)?, owner).await?;
    let mut pid = 0;
    if unsafe { GetNamedPipeServerProcessId(pipe.as_raw_handle(), &mut pid) } == 0 {
        return Err(io::Error::last_os_error());
    }
    let raw = unsafe { OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, 0, pid) };
    if raw.is_null() {
        return Err(io::Error::last_os_error());
    }
    let process = unsafe { OwnedHandle::from_raw_handle(raw) };
    let mut image = vec![0u16; 32768];
    let mut len = image.len() as u32;
    if unsafe {
        QueryFullProcessImageNameW(process.as_raw_handle(), 0, image.as_mut_ptr(), &mut len)
    } == 0
    {
        return Err(io::Error::last_os_error());
    }
    let image = std::path::PathBuf::from(
        String::from_utf16(&image[..len as usize]).map_err(io::Error::other)?,
    );
    let installed_daemon = std::env::current_exe()?.with_file_name("musterd.exe");
    if image != installed_daemon {
        return Err(io::Error::new(
            io::ErrorKind::PermissionDenied,
            "expected the installed sibling daemon",
        ));
    }
    let mut plan = [0; 5];
    tokio::time::timeout(Duration::from_secs(10), pipe.read_exact(&mut plan)).await??;
    let [a, b, c, d, prefix] = plan;
    super::serve_adapter(pipe, [a, b, c, d].into(), prefix)
        .await
        .map_err(io::Error::other)
}

#[cfg(test)]
mod tests {
    #[tokio::test]
    async fn service_rejects_same_user_pipe_from_an_uninstalled_process() {
        let token = rand::random::<u128>();
        let listener =
            muster_local::Listener::bind(super::super::endpoint(token).unwrap()).unwrap();
        let refusal = super::run(token, listener.owner_sid()).await.unwrap_err();
        assert_eq!(refusal.kind(), std::io::ErrorKind::PermissionDenied);
        assert!(refusal.to_string().contains("installed sibling daemon"));
    }
}
