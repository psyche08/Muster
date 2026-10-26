//! Chrome launches this executable; stdout contains framed JSON only.
use anyhow::{Context, Result, bail, ensure};
use muster_browser::*;
use serde_json::{Value, json};
use std::path::Path;

fn install(dir: &Path) -> Result<()> {
    install_host(&std::env::current_exe()?, dir)?;
    eprintln!("registered {}", dir.join(format!("{HOST}.json")).display());
    Ok(())
}

#[cfg(unix)]
async fn run() -> Result<()> {
    use std::os::fd::AsRawFd;
    use std::os::unix::fs::{
        DirBuilderExt, FileTypeExt, MetadataExt, OpenOptionsExt, PermissionsExt,
    };
    use tokio::net::UnixListener;
    let path = socket_path()?;
    let dir = path.parent().context("missing socket parent")?;
    std::fs::create_dir_all(dir.parent().context("missing state parent")?)?;
    match std::fs::DirBuilder::new().mode(0o700).create(dir) {
        Ok(()) => {}
        Err(e) if e.kind() == std::io::ErrorKind::AlreadyExists => {}
        Err(e) => return Err(e.into()),
    }
    let meta = std::fs::symlink_metadata(dir)?;
    // The private directory protects the bind/chmod interval and socket path.
    ensure!(
        meta.is_dir() && meta.uid() == unsafe { libc::geteuid() } && meta.mode() & 0o077 == 0,
        "browser directory must be owned by this user and mode 0700"
    );
    let lock = std::fs::OpenOptions::new()
        .create(true)
        .truncate(false)
        .write(true)
        .mode(0o600)
        .custom_flags(libc::O_NOFOLLOW)
        .open(dir.join("host.lock"))?;
    ensure!(
        unsafe { libc::flock(lock.as_raw_fd(), libc::LOCK_EX | libc::LOCK_NB) } == 0,
        "another Chrome profile already owns the browser bridge"
    );
    if let Ok(meta) = std::fs::symlink_metadata(&path) {
        ensure!(
            meta.file_type().is_socket(),
            "refusing to replace non-socket bridge path"
        );
        std::fs::remove_file(&path)?;
    }
    let listener = UnixListener::bind(&path)?;
    struct Cleanup(std::path::PathBuf);
    impl Drop for Cleanup {
        fn drop(&mut self) {
            let _ = std::fs::remove_file(&self.0);
        }
    }
    let _cleanup = Cleanup(path.clone());
    std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o600))?;
    let (tx, mut rx) = tokio::sync::mpsc::channel(1);
    tokio::spawn(async move {
        let mut stdin = tokio::io::stdin();
        loop {
            let reply = read_frame(&mut stdin, MAX_RESPONSE).await;
            let failed = reply.is_err();
            if tx.send(reply).await.is_err() || failed {
                break;
            }
        }
    });
    let mut stdout = tokio::io::stdout();
    let mut next_id = 0u64;
    let mut idle = Box::pin(tokio::time::sleep(std::time::Duration::from_secs(15)));
    loop {
        let accepted = tokio::select! {
            accepted = listener.accept() => Some(accepted?),
            _ = &mut idle => None,
            _ = rx.recv() => bail!("extension disconnected or sent unsolicited data"),
        };
        next_id += 1;
        let Some((mut client, _)) = accepted else {
            exchange(
                &mut stdout,
                &mut rx,
                json!({"op": "detachAll"}),
                next_id,
                DEADLINE,
            )
            .await?;
            idle.as_mut()
                .reset(tokio::time::Instant::now() + std::time::Duration::from_secs(15));
            continue;
        };
        let request = async {
            write_frame(&mut client, &json!({"ready": true}), 128).await?;
            // The widest frame any operation may use; `validate` then holds
            // this one to its own limit.
            let raw = read_frame(&mut client, MAX_FETCH_REQUEST).await?;
            let request: Request = serde_json::from_value(raw)?;
            request.validate()?;
            Ok::<_, anyhow::Error>(request)
        };
        let request = match tokio::time::timeout(DEADLINE, request).await {
            Ok(Ok(request)) => request,
            other => {
                let reply = json!({"error": format!("invalid request: {other:?}")});
                let _ =
                    tokio::time::timeout(DEADLINE, write_frame(&mut client, &reply, MAX_RESPONSE))
                        .await;
                continue;
            }
        };
        let response = exchange(
            &mut stdout,
            &mut rx,
            serde_json::to_value(&request)?,
            next_id,
            request.deadline(),
        )
        .await;
        let (reply, fatal) = match response {
            Ok(mut reply) => {
                reply
                    .as_object_mut()
                    .context("invalid reply")?
                    .remove("reqId");
                if let Some(result) = reply.get("result") {
                    reply = match project(&request, result.clone()) {
                        Ok(result) => json!({"result": result}),
                        Err(e) => json!({"error": e.to_string()}),
                    };
                }
                if request.is_mutation() {
                    if let Some(error) = reply.get("error") {
                        reply = json!({"error": format!("{error}; outcome may be unknown; do not automatically retry")});
                    }
                }
                (reply, false)
            }
            Err(e) => (
                json!({"error": format!("browser bridge failed: {e}; {}", if request.is_mutation() { "outcome unknown; do not automatically retry" } else { "request failed" })}),
                true,
            ),
        };
        let _ =
            tokio::time::timeout(DEADLINE, write_frame(&mut client, &reply, MAX_RESPONSE)).await;
        if fatal {
            bail!("native channel failed; close rather than replay");
        }
        idle.as_mut()
            .reset(tokio::time::Instant::now() + std::time::Duration::from_secs(15));
    }
}

async fn exchange<W: tokio::io::AsyncWrite + Unpin>(
    stdout: &mut W,
    rx: &mut tokio::sync::mpsc::Receiver<Result<Value>>,
    mut command: Value,
    id: u64,
    deadline: std::time::Duration,
) -> Result<Value> {
    command["reqId"] = json!(id);
    tokio::time::timeout(deadline, async {
        write_frame(stdout, &command, MAX_FETCH_REQUEST).await?;
        let reply = rx.recv().await.context("extension disconnected")??;
        ensure!(
            reply["reqId"].as_u64() == Some(id),
            "mismatched native response"
        );
        ensure!(
            reply.get("result").is_some() ^ reply.get("error").is_some(),
            "invalid native response"
        );
        Ok(reply)
    })
    .await
    .with_context(|| format!("{} second browser deadline exceeded", deadline.as_secs()))?
}

fn main() -> Result<()> {
    let args: Vec<_> = std::env::args().skip(1).collect();
    if args.first().map(String::as_str) == Some("install") {
        ensure!(
            args.len() == 2,
            "usage: muster-browser-host install <NativeMessagingHosts directory>"
        );
        return install(Path::new(&args[1]));
    }
    ensure!(
        args.first().map(String::as_str) == Some(ORIGIN),
        "native host requires the registered extension origin"
    );
    #[cfg(unix)]
    {
        let runtime = tokio::runtime::Builder::new_multi_thread()
            .enable_all()
            .build()?;
        let result = runtime.block_on(run());
        // Tokio stdin uses a blocking read. Do not wait for Chrome to close its
        // pipe after a timeout: returning closes this process and the native port.
        runtime.shutdown_background();
        result
    }
    #[cfg(not(unix))]
    {
        bail!("browser native host currently supports macOS and Linux")
    }
}
