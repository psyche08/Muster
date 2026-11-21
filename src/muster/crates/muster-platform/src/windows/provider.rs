//! Bind provider control to the Windows process that owns a unique Electron window.
use muster_agent::desktop_chat::ChatApp;
use std::os::windows::io::{AsRawHandle, FromRawHandle, OwnedHandle};
use std::path::Path;
use windows_sys::Win32::{
    Foundation::*,
    Security::{Cryptography::*, WinTrust::*},
    Storage::Packaging::Appx::*,
    System::Threading::*,
    UI::WindowsAndMessaging::*,
};

fn handle(pid: u32) -> std::io::Result<OwnedHandle> {
    let raw = unsafe { OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, 0, pid) };
    if raw.is_null() {
        Err(std::io::Error::last_os_error())
    } else {
        Ok(unsafe { OwnedHandle::from_raw_handle(raw) })
    }
}
fn start(h: HANDLE) -> std::io::Result<u64> {
    let mut time = [FILETIME::default(); 4];
    let [a, b, c, d] = &mut time;
    if unsafe { GetProcessTimes(h, a, b, c, d) } == 0 {
        return Err(std::io::Error::last_os_error());
    }
    Ok((u64::from(a.dwHighDateTime) << 32) | u64::from(a.dwLowDateTime))
}
pub(crate) fn observe_process_start(pid: u32) -> std::io::Result<Option<u64>> {
    match handle(pid) {
        Ok(h) => start(h.as_raw_handle()).map(Some),
        Err(e) if e.raw_os_error() == Some(ERROR_INVALID_PARAMETER as i32) => Ok(None),
        Err(e) => Err(e),
    }
}
pub(crate) fn process_start(pid: u32) -> Option<u64> {
    observe_process_start(pid).ok().flatten()
}

unsafe extern "system" fn collect(hwnd: HWND, context: LPARAM) -> i32 {
    let mut class = [0; 64];
    unsafe {
        if IsWindowVisible(hwnd) != 0 && GetWindow(hwnd, GW_OWNER).is_null() {
            let len = GetClassNameW(hwnd, class.as_mut_ptr(), class.len() as i32);
            if class[..len.max(0) as usize]
                .iter()
                .copied()
                .eq("Chrome_WidgetWin_1".encode_utf16())
            {
                let mut pid = 0;
                GetWindowThreadProcessId(hwnd, &mut pid);
                (*(context as *mut Vec<(u32, isize)>)).push((pid, hwnd as isize));
            }
        }
    }
    1
}
fn windows() -> Vec<(u32, isize)> {
    let mut result: Vec<(u32, isize)> = Vec::new();
    unsafe {
        EnumWindows(Some(collect), &mut result as *mut _ as isize);
    }
    result
}
pub(super) fn window(pid: u32) -> Option<isize> {
    let list = windows();
    let mut matches = list.iter().filter(|(p, _)| *p == pid);
    let first = matches.next()?.1;
    matches.next().is_none().then_some(first)
}

// Verify the executable signature and obtain the organization from the very
// certificate WinVerifyTrust chose for that signature.
fn signed_by(path: &[u16], expected: &str) -> bool {
    unsafe {
        let mut file = WINTRUST_FILE_INFO {
            cbStruct: size_of::<WINTRUST_FILE_INFO>() as _,
            pcwszFilePath: path.as_ptr(),
            ..Default::default()
        };
        let mut data = WINTRUST_DATA {
            cbStruct: size_of::<WINTRUST_DATA>() as _,
            dwUIChoice: WTD_UI_NONE,
            dwUnionChoice: WTD_CHOICE_FILE,
            Anonymous: WINTRUST_DATA_0 { pFile: &mut file },
            dwStateAction: WTD_STATEACTION_VERIFY,
            dwProvFlags: WTD_CACHE_ONLY_URL_RETRIEVAL,
            ..Default::default()
        };
        let mut policy = WINTRUST_ACTION_GENERIC_VERIFY_V2;
        let ok = WinVerifyTrust(
            std::ptr::null_mut(),
            &mut policy,
            (&mut data as *mut WINTRUST_DATA).cast(),
        ) == 0
            && (|| {
                let state = WTHelperProvDataFromStateData(data.hWVTStateData);
                if state.is_null() {
                    return None;
                }
                let signer = WTHelperGetProvSignerFromChain(state, 0, 0, 0).as_ref()?;
                if signer.csCertChain == 0 {
                    return None;
                }
                let cert = signer.pasCertChain.as_ref()?.pCert;
                let mut org = [0; 256];
                let len = CertGetNameStringW(
                    cert,
                    CERT_NAME_ATTR_TYPE,
                    0,
                    c"2.5.4.10".as_ptr().cast_mut().cast(),
                    org.as_mut_ptr(),
                    org.len() as _,
                );
                (len > 1).then(|| String::from_utf16_lossy(&org[..(len - 1) as usize]))
            })()
            .is_some_and(|org| org == expected);
        data.dwStateAction = WTD_STATEACTION_CLOSE;
        WinVerifyTrust(
            std::ptr::null_mut(),
            &mut policy,
            (&mut data as *mut WINTRUST_DATA).cast(),
        );
        ok
    }
}

fn is_provider(h: HANDLE, app: ChatApp) -> bool {
    let mut path = vec![0u16; 32768];
    let mut len = path.len() as u32;
    if unsafe { QueryFullProcessImageNameW(h, 0, path.as_mut_ptr(), &mut len) } == 0 {
        return false;
    }
    path.truncate(len as usize);
    let image = String::from_utf16_lossy(&path);
    path.push(0);
    let (name, org) = match app {
        ChatApp::Claude => ("Claude.exe", "Anthropic, PBC"),
        ChatApp::ChatGpt => ("ChatGPT.exe", "OpenAI, L.L.C."),
    };
    if !Path::new(&image)
        .file_name()
        .is_some_and(|n| n.eq_ignore_ascii_case(name))
    {
        return false;
    }
    if app == ChatApp::ChatGpt {
        // Store releases may use a catalog signature. Verify both the package
        // identity and that the executable resides in its registered directory.
        let mut family = [0; 256];
        let mut n = family.len() as u32;
        if unsafe { GetPackageFamilyName(h, &mut n, family.as_mut_ptr()) } == 0
            && n > 1
            && matches!(
                String::from_utf16_lossy(&family[..n as usize - 1]).as_str(),
                "OpenAI.Codex_2p2nqsd0c76g0" | "OpenAI.ChatGPT-Desktop_2p2nqsd0c76g0"
            )
        {
            let mut full = [0; 256];
            let mut n = full.len() as u32;
            let mut dir = vec![0; 32768];
            let mut d = dir.len() as u32;
            if unsafe { GetPackageFullName(h, &mut n, full.as_mut_ptr()) } == 0
                && unsafe { GetPackagePathByFullName(full.as_ptr(), &mut d, dir.as_mut_ptr()) } == 0
                && d > 1
            {
                return Path::new(&image.to_lowercase())
                    .starts_with(String::from_utf16_lossy(&dir[..d as usize - 1]).to_lowercase());
            }
        }
    }
    signed_by(&path, org)
}
pub(crate) fn process(app: ChatApp) -> Option<(u32, u64)> {
    let mut found = None;
    for (pid, _) in windows() {
        let Ok(h) = handle(pid) else { continue };
        if is_provider(h.as_raw_handle(), app) {
            if found.is_some() {
                return None;
            }
            found = Some((pid, start(h.as_raw_handle()).ok()?));
        }
    }
    found
}

pub(super) const CODE_PACKAGE: &str = "OpenAI.Codex_2p2nqsd0c76g0";

pub(super) fn code_process() -> Option<(u32, u64)> {
    let mut matches = windows().into_iter().filter_map(|(pid, _)| {
        let process = handle(pid).ok()?;
        let h = process.as_raw_handle();
        let mut name = [0u16; 256];
        let mut len = name.len() as u32;
        if unsafe { GetPackageFamilyName(h, &mut len, name.as_mut_ptr()) } != 0
            || len < 2
            || String::from_utf16_lossy(&name[..len as usize - 1]) != CODE_PACKAGE
            || !is_provider(h, ChatApp::ChatGpt)
        {
            return None;
        }
        Some((pid, start(h).ok()?))
    });
    let one = matches.next()?;
    matches.next().is_none().then_some(one)
}
