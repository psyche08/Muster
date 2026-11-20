//! The interactive daemon owns QUIC and authorization. The elevated helper
//! owns only the named Wintun adapter and one framed local packet stream.
use super::{Error, PacketReader, PacketWriter, VnetConfig, interface_config, open_tun};
use std::os::windows::{
    ffi::{OsStrExt, OsStringExt},
    io::{AsRawHandle, FromRawHandle, OwnedHandle},
};
use std::{
    io,
    net::Ipv4Addr,
    path::{Path, PathBuf},
    time::Duration,
};
use tokio::io::{AsyncRead, AsyncReadExt, AsyncWrite, AsyncWriteExt};
use windows_sys::Win32::{
    System::{
        Pipes::GetNamedPipeClientProcessId, SystemInformation::GetSystemDirectoryW,
        Threading::GetProcessId,
    },
    UI::Shell::*,
};

fn error(e: impl std::fmt::Display) -> Error {
    Error::Io(format!("Windows network helper: {e}"))
}
// The interactive daemon may live in a per-user release directory. Only
// the administrator-provisioned helper is eligible for a UAC launch.
fn helper_directory() -> io::Result<PathBuf> {
    let mut path = [0u16; 260];
    let result = unsafe {
        SHGetFolderPathW(
            std::ptr::null_mut(),
            CSIDL_PROGRAM_FILES as i32,
            std::ptr::null_mut(),
            SHGFP_TYPE_CURRENT as u32,
            path.as_mut_ptr(),
        )
    };
    if result < 0 {
        return Err(io::Error::other(format!(
            "Program Files lookup failed ({result:#x})"
        )));
    }
    let len = path
        .iter()
        .position(|unit| *unit == 0)
        .unwrap_or(path.len());
    if len == 0 || len == path.len() {
        return Err(io::Error::other("invalid Program Files path"));
    }
    Ok(PathBuf::from(std::ffi::OsString::from_wide(&path[..len])).join("Muster"))
}
fn endpoint(token: u128) -> io::Result<PathBuf> {
    Ok(helper_directory()?.join(format!("MusterTun-{token:032x}")))
}
fn launch(addr: Ipv4Addr, prefix: u8, token: u128, owner: &str) -> io::Result<OwnedHandle> {
    let exe = helper_directory()?.join("muster-tun.exe");
    if !exe.is_file() || !exe.with_file_name("wintun.dll").is_file() {
        return Err(io::Error::new(
            io::ErrorKind::NotFound,
            "Provision muster-tun.exe and wintun.dll under Program Files\\Muster before starting vnet",
        ));
    }
    let wide = |p: &Path| {
        p.as_os_str()
            .encode_wide()
            .chain(Some(0))
            .collect::<Vec<_>>()
    };
    let path = wide(&exe);
    let dir = wide(
        exe.parent()
            .ok_or_else(|| io::Error::other("missing install directory"))?,
    );
    let args = wide(Path::new(&format!("{addr} {prefix} {token:032x} {owner}")));
    unsafe {
        let mut info = SHELLEXECUTEINFOW {
            cbSize: size_of::<SHELLEXECUTEINFOW>() as _,
            fMask: SEE_MASK_NOCLOSEPROCESS,
            lpVerb: windows_sys::core::w!("runas"),
            lpFile: path.as_ptr(),
            lpParameters: args.as_ptr(),
            lpDirectory: dir.as_ptr(),
            ..Default::default()
        };
        if ShellExecuteExW(&mut info) == 0 || info.hProcess.is_null() {
            return Err(io::Error::last_os_error());
        }
        Ok(OwnedHandle::from_raw_handle(info.hProcess))
    }
}
pub(super) async fn open(
    addr: Ipv4Addr,
    prefix: u8,
) -> Result<(PacketWriter, PacketReader), Error> {
    let token = rand::random::<u128>();
    let listener =
        muster_local::Listener::bind_for_elevated_helper(&endpoint(token).map_err(error)?)
            .map_err(error)?;
    let owner = listener.owner_sid().to_owned();
    let handle = tokio::task::spawn_blocking(move || launch(addr, prefix, token, &owner))
        .await
        .map_err(error)?
        .map_err(error)?;
    let expected = unsafe { GetProcessId(handle.as_raw_handle()) };
    let (mut pipe, ()) = tokio::time::timeout(Duration::from_secs(30), listener.accept())
        .await
        .map_err(error)?
        .map_err(error)?;
    let mut pid = 0;
    if unsafe { GetNamedPipeClientProcessId(pipe.as_raw_handle(), &mut pid) } == 0
        || pid != expected
        || expected == 0
    {
        return Err(error("unexpected helper process"));
    }
    let mut ready = [0; 4];
    tokio::time::timeout(Duration::from_secs(30), pipe.read_exact(&mut ready))
        .await
        .map_err(error)?
        .map_err(error)?;
    if &ready != b"MTN1" {
        return Err(error("invalid adapter handshake"));
    }
    let (read, write) = tokio::io::split(pipe);
    Ok((PacketWriter::Pipe(write), PacketReader::Pipe(read)))
}
pub(super) async fn read_packet<R: AsyncRead + Unpin>(
    r: &mut R,
    buf: &mut [u8],
) -> io::Result<usize> {
    let n = r.read_u16().await? as usize;
    if !(20..=crate::tls::VNET_MTU as usize).contains(&n) || n > buf.len() {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            "invalid IPv4 frame size",
        ));
    }
    r.read_exact(&mut buf[..n]).await?;
    Ok(n)
}
pub(super) async fn write_packet<W: AsyncWrite + Unpin>(w: &mut W, buf: &[u8]) -> io::Result<()> {
    if !(20..=crate::tls::VNET_MTU as usize).contains(&buf.len()) {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            "invalid IPv4 frame size",
        ));
    }
    w.write_u16(buf.len() as u16).await?;
    w.write_all(buf).await
}
/// Called on the single main thread before starting the helper runtime.
/// Restrict lookup of netsh in the Wintun dependency to system executables.
pub fn prepare_process() -> io::Result<()> {
    let mut path = [0u16; 32768];
    let n = unsafe { GetSystemDirectoryW(path.as_mut_ptr(), path.len() as _) } as usize;
    if n == 0 || n >= path.len() {
        return Err(io::Error::last_os_error());
    }
    let dir = PathBuf::from(String::from_utf16_lossy(&path[..n]));
    std::env::set_current_dir(&dir)?;
    // SAFETY: main invokes this before starting any other threads.
    unsafe {
        std::env::set_var("PATH", &dir);
        std::env::set_var(
            "SystemRoot",
            dir.parent()
                .ok_or_else(|| io::Error::other("system root missing"))?,
        );
        std::env::set_var("COMSPEC", dir.join("cmd.exe"));
    }
    Ok(())
}
pub async fn run(addr: Ipv4Addr, prefix: u8, token: u128, owner: &str) -> Result<(), Error> {
    if !(1..=32).contains(&prefix)
        || addr.is_unspecified()
        || addr.is_multicast()
        || addr.is_loopback()
    {
        return Err(error("invalid interface address"));
    }
    let mut pipe = muster_local::connect_to_owner(&endpoint(token).map_err(error)?, owner)
        .await
        .map_err(error)?;
    let config = VnetConfig {
        name: Some("MusterVnet".into()),
        ..Default::default()
    };
    let (mut tun_w, mut tun_r) = open_tun(interface_config(addr, prefix, &config), config.mtu)?;
    pipe.write_all(b"MTN1").await.map_err(error)?;
    let (mut read, mut write) = tokio::io::split(pipe);
    let from_adapter = async {
        let mut buf = vec![0; 65536];
        loop {
            let n = tun_r.read(&mut buf).await?;
            if n == 0 {
                return Ok::<_, io::Error>(());
            }
            write_packet(&mut write, &buf[..n]).await?;
        }
    };
    let to_adapter = async {
        let mut buf = vec![0; 65536];
        loop {
            let n = read_packet(&mut read, &mut buf).await?;
            tun_w.write_all(&buf[..n]).await?;
        }
        #[allow(unreachable_code)]
        Ok::<_, io::Error>(())
    };
    tokio::select! { result=from_adapter=>result, result=to_adapter=>result }.map_err(error)
}
