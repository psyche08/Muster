use sha2::{Digest, Sha256};
use std::os::windows::{
    ffi::OsStrExt,
    io::{AsRawHandle, FromRawHandle, OwnedHandle},
};
use std::{ffi::c_void, io, path::Path, ptr::null_mut};
use tokio::net::windows::named_pipe::{
    ClientOptions, NamedPipeClient, NamedPipeServer, ServerOptions,
};
use windows_sys::Win32::{
    Foundation::LocalFree, Security::Authorization::*, Security::*, System::Threading::*,
};

struct Local(*mut c_void);
impl Drop for Local {
    fn drop(&mut self) {
        unsafe {
            LocalFree(self.0);
        }
    }
}

// Win32 allocates a NUL-terminated SID string on success.
fn sid_string(sid: PSID) -> io::Result<String> {
    unsafe {
        let mut wide = null_mut();
        if ConvertSidToStringSidW(sid, &mut wide) == 0 {
            return Err(io::Error::last_os_error());
        }
        let _allocation = Local(wide.cast());
        let mut n = 0;
        while *wide.add(n) != 0 {
            n += 1;
        }
        Ok(String::from_utf16_lossy(std::slice::from_raw_parts(
            wide, n,
        )))
    }
}

fn current_sid() -> io::Result<String> {
    unsafe {
        let mut token = null_mut();
        if OpenProcessToken(GetCurrentProcess(), TOKEN_QUERY, &mut token) == 0 {
            return Err(io::Error::last_os_error());
        }
        let token = OwnedHandle::from_raw_handle(token);
        let mut bytes = 0;
        GetTokenInformation(token.as_raw_handle(), TokenUser, null_mut(), 0, &mut bytes);
        if bytes == 0 {
            return Err(io::Error::last_os_error());
        }
        let mut storage = vec![0usize; (bytes as usize).div_ceil(size_of::<usize>())];
        let info = storage.as_mut_ptr().cast::<TOKEN_USER>();
        if GetTokenInformation(
            token.as_raw_handle(),
            TokenUser,
            info.cast(),
            bytes,
            &mut bytes,
        ) == 0
        {
            return Err(io::Error::last_os_error());
        }
        sid_string((*info).User.Sid)
    }
}

fn pipe_name(path: &Path, sid: &str) -> io::Result<String> {
    let path = std::path::absolute(path)?;
    let mut hash = Sha256::new();
    for unit in path.as_os_str().encode_wide() {
        hash.update(unit.to_le_bytes());
    }
    Ok(format!(
        r"\\.\pipe\Muster-{sid}-{}",
        hex::encode(hash.finalize())
    ))
}

pub struct Listener {
    name: String,
    sid: String,
    pending: tokio::sync::Mutex<NamedPipeServer>,
}

fn instance(name: &str, sid: &str, first: bool) -> io::Result<NamedPipeServer> {
    let sddl: Vec<u16> = format!("O:{sid}D:P(A;;GA;;;{sid})")
        .encode_utf16()
        .chain(Some(0))
        .collect();
    unsafe {
        let mut descriptor = null_mut();
        if ConvertStringSecurityDescriptorToSecurityDescriptorW(
            sddl.as_ptr(),
            1,
            &mut descriptor,
            null_mut(),
        ) == 0
        {
            return Err(io::Error::last_os_error());
        }
        let allocation = Local(descriptor);
        let mut sa = SECURITY_ATTRIBUTES {
            nLength: size_of::<SECURITY_ATTRIBUTES>() as _,
            lpSecurityDescriptor: allocation.0,
            bInheritHandle: 0,
        };
        ServerOptions::new()
            .first_pipe_instance(first)
            .reject_remote_clients(true)
            .create_with_security_attributes_raw(name, (&mut sa as *mut SECURITY_ATTRIBUTES).cast())
    }
}

impl Listener {
    pub fn bind(path: impl AsRef<Path>) -> io::Result<Self> {
        let sid = current_sid()?;
        let name = pipe_name(path.as_ref(), &sid)?;
        let pending = instance(&name, &sid, true)?.into();
        Ok(Self { name, sid, pending })
    }
    pub async fn accept(&self) -> io::Result<(NamedPipeServer, ())> {
        let mut pending = self.pending.lock().await;
        pending.connect().await?;
        let next = instance(&self.name, &self.sid, false)?;
        Ok((std::mem::replace(&mut *pending, next), ()))
    }
}

pub async fn connect(path: &Path) -> io::Result<NamedPipeClient> {
    let sid = current_sid()?;
    let name = pipe_name(path, &sid)?;
    let deadline = tokio::time::Instant::now() + std::time::Duration::from_secs(2);
    let pipe = loop {
        match ClientOptions::new().open(&name) {
            Ok(pipe) => break pipe,
            Err(e)
                if e.raw_os_error()
                    == Some(windows_sys::Win32::Foundation::ERROR_PIPE_BUSY as i32)
                    && tokio::time::Instant::now() < deadline =>
            {
                tokio::time::sleep(std::time::Duration::from_millis(10)).await;
            }
            Err(e) => return Err(e),
        }
    };
    // Check the opened object before sending anything, even if another account
    // pre-created the predictable pipe name before the daemon started.
    unsafe {
        let mut owner = null_mut();
        let mut descriptor = null_mut();
        let status = GetSecurityInfo(
            pipe.as_raw_handle(),
            SE_KERNEL_OBJECT,
            OWNER_SECURITY_INFORMATION,
            &mut owner,
            null_mut(),
            null_mut(),
            null_mut(),
            &mut descriptor,
        );
        if status != 0 {
            return Err(io::Error::from_raw_os_error(status as _));
        }
        let _allocation = Local(descriptor);
        if sid_string(owner)? != sid {
            return Err(io::Error::new(
                io::ErrorKind::PermissionDenied,
                "pipe owner differs from current user",
            ));
        }
    }
    Ok(pipe)
}

#[cfg(test)]
mod tests {
    use super::*;
    use tokio::io::{AsyncReadExt, AsyncWriteExt};

    #[tokio::test]
    async fn local_owner_round_trip_and_exclusive_server() {
        let name = std::env::temp_dir().join(format!("muster-pipe-test-{}", std::process::id()));
        let listener = Listener::bind(&name).unwrap();
        assert!(Listener::bind(&name).is_err());
        let mut client = connect(&name).await.unwrap();
        let (mut server, ()) = listener.accept().await.unwrap();
        client.write_all(b"hello").await.unwrap();
        let mut data = [0; 5];
        server.read_exact(&mut data).await.unwrap();
        assert_eq!(&data, b"hello");
    }
}
