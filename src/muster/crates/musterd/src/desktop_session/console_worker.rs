//! An account-local lease for the ordinary physical console. The pipe carries a
//! bounded launch profile, never a program name, command line, file path or input.
use super::{engine::*, host};
use crate::Error;
use muster_net::remote_desktop::gamestream::{EnginePorts, LocalEngine};
use muster_proto::v1::DesktopStreamSettings;
use std::{
    ffi::OsString,
    io,
    net::SocketAddrV4,
    os::windows::{
        ffi::OsStringExt,
        io::{AsHandle, AsRawHandle, BorrowedHandle, FromRawHandle, OwnedHandle},
    },
    path::{Path, PathBuf},
    sync::Arc,
    time::Duration,
};
use tokio::{
    io::{AsyncReadExt, AsyncWriteExt},
    net::windows::named_pipe::{NamedPipeClient, NamedPipeServer},
    time::{sleep, timeout},
};
use windows_sys::Win32::{
    Foundation::WAIT_TIMEOUT,
    System::{JobObjects::*, Pipes::*, RemoteDesktop::*, StationsAndDesktops::*, Threading::*},
};

const WORKER: &str = "muster-stream-worker.exe";
const READY: u8 = 1;
const NO_DESKTOP: u8 = 2;
const START_FAILED: u8 = 3;

fn no_console() -> Error {
    unavailable(CONSOLE_MESSAGE)
}
pub(super) fn physical_id() -> Option<u32> {
    let id = unsafe { WTSGetActiveConsoleSessionId() };
    (id != u32::MAX && id != 0).then_some(id)
}
async fn physical_session_changed(session: u32) {
    while physical_id() == Some(session) {
        sleep(Duration::from_millis(50)).await;
    }
}
fn process_session(pid: u32) -> io::Result<u32> {
    let mut session = 0;
    if unsafe { ProcessIdToSessionId(pid, &mut session) } == 0 {
        return Err(io::Error::last_os_error());
    }
    Ok(session)
}
fn endpoint(session: u32) -> io::Result<PathBuf> {
    Ok(std::env::current_exe()?.with_file_name(format!("MusterStreamWorker-v1-{session}")))
}

fn own_sid() -> io::Result<String> {
    muster_local::process_user_sid(unsafe { BorrowedHandle::borrow_raw(GetCurrentProcess()) })
}

fn console_account_field(session: u32, field: WTS_INFO_CLASS) -> io::Result<String> {
    let mut data = std::ptr::null_mut();
    let mut bytes = 0;
    unsafe {
        if WTSQuerySessionInformationW(std::ptr::null_mut(), session, field, &mut data, &mut bytes)
            == 0
        {
            return Err(io::Error::last_os_error());
        }
        if data.is_null() {
            return Err(io::Error::other("missing console account"));
        }
        let result = if (4..=2048).contains(&bytes) && bytes % 2 == 0 {
            std::slice::from_raw_parts(data, bytes as usize / 2)
                .strip_suffix(&[0])
                .and_then(|s| String::from_utf16(s).ok())
        } else {
            None
        };
        WTSFreeMemory(data.cast());
        result
            .filter(|s| !s.is_empty() && !s.contains(['\0', '\\']))
            .ok_or_else(|| io::Error::other("unknown console account"))
    }
}

fn owns_console_account(session: u32, sid: &str) -> io::Result<bool> {
    let domain = console_account_field(session, WTSDomainName)?;
    let user = console_account_field(session, WTSUserName)?;
    Ok(muster_local::account_user_sid(&format!("{domain}\\{user}"))? == sid)
}

fn named_object(handle: *mut std::ffi::c_void, expected: &str) -> bool {
    let mut buf = [0u16; 64];
    let mut bytes = 0;
    !handle.is_null()
        && unsafe {
            GetUserObjectInformationW(
                handle,
                UOI_NAME,
                buf.as_mut_ptr().cast(),
                size_of_val(&buf) as _,
                &mut bytes,
            ) != 0
        }
        && buf
            .iter()
            .position(|&c| c == 0)
            .is_some_and(|n| String::from_utf16_lossy(&buf[..n]).eq_ignore_ascii_case(expected))
}

fn ordinary_desktop() -> bool {
    if !muster_platform::on_unlocked_physical_console()
        || !named_object(unsafe { GetProcessWindowStation() }, "WinSta0")
    {
        return false;
    }
    let input = unsafe { OpenInputDesktop(0, 0, DESKTOP_READOBJECTS) };
    if input.is_null() {
        return false;
    }
    let ordinary = named_object(input, "Default");
    unsafe {
        CloseDesktop(input);
    }
    ordinary
}

// Pin the kernel process object during the pipe exchange or socket checks.
pub(super) struct InstalledProcess {
    handle: OwnedHandle,
    pid: u32,
}
impl InstalledProcess {
    fn open(pid: u32, expected: &Path, session: Option<u32>) -> io::Result<Self> {
        let raw = unsafe {
            OpenProcess(
                PROCESS_QUERY_LIMITED_INFORMATION | PROCESS_SYNCHRONIZE,
                0,
                pid,
            )
        };
        if raw.is_null() {
            return Err(io::Error::last_os_error());
        }
        let handle = unsafe { OwnedHandle::from_raw_handle(raw) };
        let mut path = vec![0u16; 32768];
        let mut n = path.len() as u32;
        if unsafe { QueryFullProcessImageNameW(raw, 0, path.as_mut_ptr(), &mut n) } == 0 {
            return Err(io::Error::last_os_error());
        }
        let image = PathBuf::from(OsString::from_wide(&path[..n as usize]));
        let process = Self { handle, pid };
        if std::fs::canonicalize(image)? != std::fs::canonicalize(expected)?
            || muster_local::process_user_sid(process.handle.as_handle())? != own_sid()?
            || session.is_some_and(|s| process_session(pid).ok() != Some(s))
            || !process.alive()
        {
            return Err(io::Error::new(
                io::ErrorKind::PermissionDenied,
                "unexpected stream process",
            ));
        }
        Ok(process)
    }
    fn alive(&self) -> bool {
        unsafe { WaitForSingleObject(self.handle.as_raw_handle(), 0) == WAIT_TIMEOUT }
    }
    fn owns(&self, addr: SocketAddrV4, tcp: bool) -> bool {
        self.alive() && muster_platform::engine_socket::owned(self.pid, addr, tcp) && self.alive()
    }
    pub(super) async fn exited(&self) -> Error {
        while self.alive() {
            sleep(Duration::from_millis(100)).await;
        }
        unavailable("console stream engine exited")
    }
}
impl LocalEngine for InstalledProcess {
    fn owns_tcp(&self, addr: SocketAddrV4) -> bool {
        self.owns(addr, true)
    }
    fn owns_udp(&self, addr: SocketAddrV4) -> bool {
        self.owns(addr, false)
    }
}

pub(super) struct Lease {
    pub(super) engine: Arc<InstalledProcess>,
    pub(super) ports: EnginePorts,
    pipe: NamedPipeClient,
    _worker: InstalledProcess,
    pub(super) session: u32,
}
impl Lease {
    pub(super) async fn closed(&mut self) -> Result<(), Error> {
        tokio::select! {
            _ = self.pipe.read_u8() => {},
            _ = physical_session_changed(self.session) => {},
        }
        Err(no_console())
    }
}

pub(super) async fn request(
    launch: &Launch,
    video: Option<&DesktopStreamSettings>,
) -> Result<(Lease, (u16, u16)), Error> {
    let session = physical_id().ok_or_else(no_console)?;
    let mut pipe = muster_local::connect(&endpoint(session).map_err(|_| no_console())?)
        .await
        .map_err(|_| no_console())?;
    let mut pid = 0;
    if unsafe { GetNamedPipeServerProcessId(pipe.as_raw_handle(), &mut pid) } == 0 {
        return Err(no_console());
    }
    let worker = InstalledProcess::open(
        pid,
        &program(WORKER).map_err(|_| no_console())?,
        Some(session),
    )
    .map_err(|_| no_console())?;
    let hello = video
        .map(DesktopStreamSettings::video_hello)
        .unwrap_or_default();
    let exchange = async {
        pipe.write_all(&launch.encode()).await?;
        pipe.write_u8(hello.len() as u8).await?;
        pipe.write_all(&hello).await?;
        let status = pipe.read_u8().await?;
        if status != READY {
            return Ok((status, None));
        }
        Ok::<_, io::Error>((
            status,
            Some((
                pipe.read_u32().await?,
                pipe.read_u16().await?,
                (pipe.read_u16().await?, pipe.read_u16().await?),
            )),
        ))
    };
    let (status, info) = timeout(Duration::from_secs(46), exchange)
        .await
        .map_err(|_| unavailable("console media engine startup timed out"))?
        .map_err(|_| no_console())?;
    if status == START_FAILED {
        return Err(unavailable("console media engine could not start"));
    }
    let (pid, base, size) = info.ok_or_else(no_console)?;
    if physical_id() != Some(session) || !worker.alive() {
        return Err(no_console());
    }
    let engine = InstalledProcess::open(pid, &program(host::CAPTURE_HOST)?, Some(session))
        .map_err(|_| no_console())?;
    let ports = EnginePorts::muster_host_tcp_base(base)?;
    Ok((
        Lease {
            engine: Arc::new(engine),
            ports,
            pipe,
            _worker: worker,
            session,
        },
        size,
    ))
}

// Dropping a lease or exiting the worker closes the last job handle and
// terminates the media process tree, even after an ungraceful disconnect.
pub(super) fn capture_job(process: BorrowedHandle<'_>) -> io::Result<OwnedHandle> {
    unsafe {
        let raw = CreateJobObjectW(std::ptr::null(), std::ptr::null());
        if raw.is_null() {
            return Err(io::Error::last_os_error());
        }
        let job = OwnedHandle::from_raw_handle(raw);
        let mut limits = JOBOBJECT_EXTENDED_LIMIT_INFORMATION::default();
        limits.BasicLimitInformation.LimitFlags = JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE;
        if SetInformationJobObject(
            raw,
            JobObjectExtendedLimitInformation,
            (&raw const limits).cast(),
            size_of_val(&limits) as _,
        ) == 0
            || AssignProcessToJobObject(raw, process.as_raw_handle()) == 0
        {
            return Err(io::Error::last_os_error());
        }
        Ok(job)
    }
}

pub async fn run_console_worker() -> anyhow::Result<()> {
    muster_platform::require_ordinary_user()?;
    let id = process_session(std::process::id())?;
    anyhow::ensure!(
        physical_id() == Some(id),
        "worker requires the physical console"
    );
    let sid = own_sid()?;
    let listener = muster_local::Listener::bind(endpoint(id)?)?;
    let server = async {
        loop {
            let (pipe, ()) = match listener.accept().await {
                Ok(pipe) => pipe,
                Err(error) => return Err(error.into()),
            };
            // Only one physical capture lease can exist at a time.
            if let Err(error) = serve(pipe, id, &sid).await {
                log::debug!("console worker lease ended: {error}");
            }
        }
    };
    tokio::select! {
        biased;
        _ = physical_session_changed(id) => Ok(()),
        result = server => result,
    }
}

async fn serve(mut pipe: NamedPipeServer, id: u32, sid: &str) -> anyhow::Result<()> {
    let mut pid = 0;
    if unsafe { GetNamedPipeClientProcessId(pipe.as_raw_handle(), &mut pid) } == 0 {
        return Err(io::Error::last_os_error().into());
    }
    let _daemon = InstalledProcess::open(
        pid,
        &std::env::current_exe()?.with_file_name("musterd.exe"),
        None,
    )?;
    let request = timeout(Duration::from_secs(3), async {
        let mut launch = [0u8; 25];
        pipe.read_exact(&mut launch).await?;
        let n = pipe.read_u8().await? as usize;
        anyhow::ensure!(n <= 68, "invalid stream profile");
        let mut hello = vec![0; n];
        pipe.read_exact(&mut hello).await?;
        let settings = if hello.is_empty() {
            None
        } else {
            Some(
                DesktopStreamSettings::from_video_hello(&hello)
                    .ok_or_else(|| unavailable("invalid stream profile"))?,
            )
        };
        Ok::<_, anyhow::Error>((Launch::parse(&launch)?, settings))
    })
    .await??;
    if !ordinary_desktop() || !owns_console_account(id, sid).unwrap_or(false) {
        pipe.write_u8(NO_DESKTOP).await?;
        return Ok(());
    }
    let (launch, video) = request;
    let (width, height) = host::video_mode(video.as_ref(), muster_platform::desktop_pixels())?;
    let mode = format!("{width}x{height}x{}", video.map_or(30, |s| s.fps));
    let cursor = video.is_none_or(|s| s.show_cursor);
    let engine = program(host::CAPTURE_HOST)?;
    let (mut owner, mut reply) = tokio::io::split(pipe);
    let media = async {
        let process = match host::start(
            &engine,
            None,
            EnginePorts::MUSTER,
            &launch,
            &mode,
            false,
            cursor,
        )
        .await
        {
            Ok(process) => process,
            Err(error) => {
                reply.write_u8(START_FAILED).await?;
                return Err(error.into());
            }
        };
        let pid = process
            .engine
            .pid()
            .ok_or_else(|| unavailable("stream engine exited"))?;
        if !ordinary_desktop() {
            return Ok(());
        }
        reply.write_u8(READY).await?;
        reply.write_u32(pid).await?;
        reply.write_u16(process.ports.base()).await?;
        reply.write_u16(width).await?;
        reply.write_u16(height).await?;
        Err::<(), anyhow::Error>(process.engine.exited().await.into())
    };
    tokio::select! {
        biased;
        _ = owner.read_u8() => Ok(()),
        _ = async { while ordinary_desktop() { sleep(Duration::from_millis(50)).await; } } => Ok(()),
        result = media => result,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn socket_owner_must_be_the_installed_image_in_the_expected_session() {
        let pid = std::process::id();
        let exe = std::env::current_exe().unwrap();
        let session = process_session(pid).unwrap();
        assert!(InstalledProcess::open(pid, &exe, Some(session)).is_ok());
        assert!(InstalledProcess::open(pid, &exe, Some(session.wrapping_add(1))).is_err());
        assert!(
            InstalledProcess::open(pid, &exe.with_file_name("musterd.exe"), Some(session)).is_err()
        );
    }

    #[test]
    #[ignore = "subprocess for the job lifetime test"]
    fn wait_for_test_job() {
        std::thread::sleep(Duration::from_secs(15));
    }

    #[tokio::test]
    async fn dropping_a_capture_job_terminates_its_child() {
        let mut child = tokio::process::Command::new(std::env::current_exe().unwrap())
            .args([
                "--ignored",
                "--exact",
                "desktop_session::console_worker::tests::wait_for_test_job",
            ])
            .kill_on_drop(true)
            .spawn()
            .unwrap();
        let job = capture_job(unsafe { BorrowedHandle::borrow_raw(child.raw_handle().unwrap()) })
            .unwrap();
        assert!(child.try_wait().unwrap().is_none());
        drop(job);
        timeout(Duration::from_secs(3), child.wait())
            .await
            .unwrap()
            .unwrap();
    }

    #[tokio::test]
    async fn same_account_pipe_client_with_the_wrong_image_is_rejected_before_launch() {
        let path =
            std::env::temp_dir().join(format!("muster-console-test-{}", rand::random::<u64>()));
        let listener = muster_local::Listener::bind(&path).unwrap();
        let _client = muster_local::connect(&path).await.unwrap();
        let (pipe, ()) = listener.accept().await.unwrap();
        assert!(serve(pipe, 0, listener.owner_sid()).await.is_err());
    }
}
