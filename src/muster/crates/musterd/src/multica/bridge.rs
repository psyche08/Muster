use super::*;
use muster_agent::native_runtime::{self, Destination};
use std::{collections::HashMap, net::Ipv4Addr, sync::Arc};
use tokio::io::{AsyncBufReadExt, AsyncReadExt, BufReader};

fn destination(provider: &str) -> Result<Destination, Error> {
    #[cfg(target_os = "macos")]
    let installed = muster_core::apps::app(provider)
        .and_then(|a| muster_platform::macos::automation::installed(a.bundle_id));
    #[cfg(not(target_os = "macos"))]
    let installed = Some(false);
    native_runtime::destination(installed, native_runtime::cli(provider)).map_err(Error::from)
}
// Pin a task's destination before opening a provider process. A resumed
// desktop task cannot become a fresh CLI task when its app disappears.
fn pinned_destination(root: &Path, provider: &str) -> Result<Destination, Error> {
    std::fs::create_dir_all(root).map_err(io)?;
    let path = root.join("destination");
    if let Ok(saved) = std::fs::read_to_string(&path) {
        if saved == "desktop" {
            return match destination(provider)? {
                Destination::Desktop => Ok(Destination::Desktop),
                _ => Err(Error::Unavailable(
                    "This task requires its original desktop application".into(),
                )),
            };
        }
        if let Some(p) = saved.strip_prefix("cli:") {
            let p = PathBuf::from(p);
            if p.is_file() {
                return Ok(Destination::Cli(p));
            }
            return Err(Error::Unavailable(
                "This task's original CLI is unavailable".into(),
            ));
        }
        return Err(Error::Invalid("Task destination is unreadable".into()));
    }
    let selected = destination(provider)?;
    let record = match &selected {
        Destination::Desktop => "desktop".into(),
        Destination::Cli(p) => format!("cli:{}", p.display()),
        Destination::Unavailable => return Ok(selected),
    };
    use std::io::Write;
    let mut options = std::fs::OpenOptions::new();
    options.write(true).create_new(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.mode(0o600);
    }
    match options.open(&path) {
        Ok(mut f) => {
            f.write_all(record.as_bytes()).map_err(io)?;
            f.sync_all().map_err(io)?;
            Ok(selected)
        }
        Err(e) if e.kind() == std::io::ErrorKind::AlreadyExists => {
            pinned_destination(root, provider)
        }
        Err(e) => Err(io(e)),
    }
}
async fn line<R: tokio::io::AsyncBufRead + Unpin>(r: &mut R) -> Option<String> {
    let mut s = String::new();
    let n = r.take(65537).read_line(&mut s).await.ok()?;
    if n == 0 || n > 65536 || !s.ends_with('\n') {
        return None;
    }
    Some(s.trim_end_matches(['\n', '\r']).into())
}
fn decode(s: &str) -> Option<String> {
    use base64::Engine;
    String::from_utf8(base64::engine::general_purpose::STANDARD.decode(s).ok()?).ok()
}
struct Launch {
    provider: String,
    cwd: PathBuf,
    args: Vec<String>,
    env: HashMap<String, String>,
}

/// The launch shapes `muster_acp::scheduler::codex` answers `initialize` for.
///
/// Keep this list identical to that adapter's `launch_supported`: a wider gate
/// would hand the adapter a launch it refuses, and a narrower one would refuse
/// a launch the adapter serves (for example a scheduler that starts the
/// wrapper with no arguments at all).
fn codex_launch_supported(args: &[String]) -> bool {
    let args: Vec<&str> = args.iter().map(String::as_str).collect();
    matches!(
        args.as_slice(),
        [] | ["app-server"] | ["app-server", "--listen", "stdio://"]
    )
}

/// Claude Code emits the newline-delimited JSON the adapter reads only when it
/// runs with stream-json input. A launch without it can never speak this
/// protocol, so the bridge refuses it instead of forwarding a dead stream.
fn claude_stream_json(args: &[String]) -> bool {
    args.iter()
        .any(|a| a == "--input-format" || a == "--input-format=stream-json")
}

async fn handshake<R: tokio::io::AsyncBufRead + Unpin>(
    r: &mut R,
    token: &str,
    root: &Path,
) -> Option<Launch> {
    if line(r).await? != token || line(r).await? != "MUSTER_NATIVE_V1" {
        return None;
    }
    let provider = line(r).await?;
    if !matches!(provider.as_str(), "codex" | "claude") {
        return None;
    }
    let cwd = PathBuf::from(decode(&line(r).await?)?);
    let cwd = cwd.canonicalize().ok()?;
    let root = root.canonicalize().ok()?;
    if !cwd.is_dir() || !cwd.starts_with(&root) {
        return None;
    }
    let count = line(r).await?.parse::<usize>().ok()?;
    if count > 128 {
        return None;
    }
    let mut args = Vec::new();
    for _ in 0..count {
        args.push(decode(&line(r).await?)?)
    }
    if provider == "codex" && !codex_launch_supported(&args) {
        return None;
    }
    if provider == "claude" && !claude_stream_json(&args) {
        return None;
    }
    let mut env = HashMap::new();
    for _ in 0..128 {
        let s = line(r).await?;
        if s.is_empty() {
            return Some(Launch {
                provider,
                cwd,
                args,
                env,
            });
        }
        let (k, v) = s.split_once('=')?;
        if !k.starts_with("MULTICA_")
            || !k
                .bytes()
                .all(|b| b.is_ascii_uppercase() || b.is_ascii_digit() || b == b'_')
        {
            return None;
        }
        env.insert(k.into(), v.into());
    }
    None
}

/// One task's session root, keyed by provider, directory and task identity.
///
/// The task identity is required. Without it two scheduler tasks that share a
/// directory would land on the same binding and a resume could continue the
/// wrong conversation, so the connection is refused before any session state
/// is created.
fn session_key(provider: &str, cwd: &Path, task_id: Option<&str>) -> Result<String, Error> {
    let task = task_id
        .map(str::trim)
        .filter(|task| !task.is_empty())
        .ok_or_else(|| Error::Unavailable("Task identity is missing".into()))?;
    use sha2::{Digest, Sha256};
    Ok(hex::encode(Sha256::digest(format!(
        "{}\0{}\0{}",
        provider,
        cwd.display(),
        task
    ))))
}

/// The host data root and the same root as the container sees it.
fn bridge_roots(c: &Config) -> (PathBuf, PathBuf) {
    let root = Path::new(&c.data_dir).to_path_buf();
    let shared = if c.managed {
        PathBuf::from("/muster")
    } else {
        root.clone()
    };
    (root, shared)
}

/// Production accepts only the container bridge's private address. Test builds
/// additionally accept loopback so the listener can be exercised on an
/// ordinary host; shipped builds compile `cfg!(test)` to false and keep the
/// strict rule.
fn gateway_address(gateway: &str) -> Result<Ipv4Addr, Error> {
    let address: Ipv4Addr = gateway
        .parse()
        .map_err(|_| Error::Invalid("Invalid container gateway".into()))?;
    if !address.is_private() && !(cfg!(test) && address.is_loopback()) {
        return Err(Error::Invalid("Container gateway must be private".into()));
    }
    Ok(address)
}

fn bridge_token() -> Result<String, Error> {
    let mut random = [0u8; 32];
    std::io::Read::read_exact(
        &mut std::fs::File::open("/dev/urandom").map_err(io)?,
        &mut random,
    )
    .map_err(io)?;
    Ok(hex::encode(random))
}

/// Bind the private bridge listener and mint this bridge process's token.
async fn bind_bridge(gateway: &str) -> Result<(tokio::net::TcpListener, u16, String), Error> {
    let listener = tokio::net::TcpListener::bind((gateway_address(gateway)?, 0))
        .await
        .map_err(io)?;
    let port = listener.local_addr().map_err(io)?.port();
    Ok((listener, port, bridge_token()?))
}

/// Write the token, the framing command and the provider wrappers the container
/// executes. The wrappers carry no credential; they read the token file at
/// launch, and the runtime command authenticates the connection with it.
fn write_bridge_files(c: &Config, gateway: &str, port: u16, token: &str) -> Result<(), Error> {
    let (root, shared) = bridge_roots(c);
    private_write(&root.join("bridge-token"), token.as_bytes())?;
    private_write(
        &root.join("runtime-command"),
        include_bytes!("../../../../deploy/multica/runtime-command"),
    )?;
    for provider in ["codex", "claude"] {
        let wrapper = format!(
            "#!/bin/sh\n# Muster runtime bridge\nexport MUSTER_RUNTIME_PROVIDER={} MUSTER_BRIDGE_HOST={} MUSTER_BRIDGE_PORT={} MUSTER_BRIDGE_VERSION_FILE={}\nMUSTER_BRIDGE_TOKEN=$(cat {})\nexport MUSTER_BRIDGE_TOKEN\nexec /bin/sh {} \"$@\"\n",
            quote(provider),
            quote(gateway),
            port,
            quote(&shared.join(format!("version-{provider}")).to_string_lossy()),
            quote(&shared.join("bridge-token").to_string_lossy()),
            quote(&shared.join("runtime-command").to_string_lossy())
        );
        private_write(
            &root.join(format!("runtime-{provider}")),
            wrapper.as_bytes(),
        )?;
    }
    Ok(())
}

/// Delete task-scoped launchers left by a previous bridge process. The bridge
/// calls this before it serves; `Manager` also calls it at daemon startup so a
/// crashed bridge leaves no usable credential behind.
pub(super) fn revoke_stale(c: &Config) -> Result<(), Error> {
    muster_acp::TaskCredential::revoke_stale(&Path::new(&c.data_dir).join(".runtime-credentials"))
        .map_err(Error::Unavailable)
}

/// Remove the retired kiro-cli compatibility alias, and only when it is ours.
async fn remove_legacy_alias(manager: &Manager, c: &Config) -> Result<(), Error> {
    manager.command(&["exec",&c.container_name,"sh","-c","if [ -f /usr/local/bin/kiro-cli ] && grep -q 'MUSTER_BRIDGE' /usr/local/bin/kiro-cli; then rm /usr/local/bin/kiro-cli; fi"],10).await?;
    Ok(())
}

async fn refresh_wrappers(manager: &Manager, c: &Config) -> Result<(), Error> {
    let root = Path::new(&c.data_dir);
    let shared = if c.managed {
        PathBuf::from("/muster")
    } else {
        root.to_path_buf()
    };
    for provider in ["codex", "claude"] {
        let command = if provider == "claude" {
            "muster-native-claude"
        } else {
            "codex"
        };
        let dest = destination(provider)?;
        if dest == Destination::Unavailable {
            manager.command(&["exec",&c.container_name,"sh","-c","if [ -f \"$1\" ] && grep -q '^# Muster runtime bridge$' \"$1\"; then rm -- \"$1\"; fi","remove",&format!("/usr/local/bin/{command}")],10).await?;
            continue;
        }
        let version = match dest {
            Destination::Desktop => {
                if provider == "codex" {
                    "codex-cli 0.100.0 (Muster protocol compatibility)".to_string()
                } else {
                    "2.0.0 (Claude Code; Muster protocol compatibility)".to_string()
                }
            }
            Destination::Cli(program) => String::from_utf8(run(&program, &["--version"], 5).await?)
                .map_err(|_| Error::Unavailable("CLI version is unreadable".into()))?,
            Destination::Unavailable => unreachable!(),
        };
        private_write(
            &root.join(format!("version-{provider}")),
            version.trim().as_bytes(),
        )?;
        manager
            .command(
                &[
                    "exec",
                    &c.container_name,
                    "sh",
                    "-c",
                    "cmp -s \"$1\" \"$2\" || install -m 0755 \"$1\" \"$2\"",
                    "install",
                    &shared.join(format!("runtime-{provider}")).to_string_lossy(),
                    &format!("/usr/local/bin/{command}"),
                ],
                10,
            )
            .await?;
    }
    Ok(())
}

#[cfg(unix)]
pub(super) async fn ensure(
    manager: &Manager,
    c: &Config,
    socket: &Path,
    gateway: &str,
) -> Result<(), Error> {
    if !Path::new(&c.data_dir).join("bin/multica").is_file() {
        return Err(Error::Unavailable(
            "Host Multica CLI is missing; use Install to provision it".into(),
        ));
    }
    let mut bridge = manager.bridge.lock().await;
    if bridge.as_ref().is_some_and(|t| !t.is_finished()) {
        return refresh_wrappers(manager, c).await;
    }
    let (listener, port, token) = bind_bridge(gateway).await?;
    revoke_stale(c)?;
    write_bridge_files(c, gateway, port, &token)?;
    refresh_wrappers(manager, c).await?;
    remove_legacy_alias(manager, c).await?;
    let allowed = PathBuf::from(workspaces(c));
    *bridge = Some(spawn_bridge(
        listener,
        token,
        socket.to_path_buf(),
        Path::new(&c.data_dir).to_path_buf(),
        allowed,
    ));
    Ok(())
}

/// Serve one listener for this process's lifetime. Each connection is one
/// scheduler task: a connection that cannot prove a task identity is dropped
/// before any provider process or session directory is created.
fn spawn_bridge(
    listener: tokio::net::TcpListener,
    token: String,
    socket: PathBuf,
    root: PathBuf,
    allowed: PathBuf,
) -> tokio::task::JoinHandle<()> {
    tokio::spawn(async move {
        let permits = Arc::new(tokio::sync::Semaphore::new(8));
        let mut clients = tokio::task::JoinSet::new();
        loop {
            tokio::select! {
                _=clients.join_next(),if !clients.is_empty()=>{},
                accepted=listener.accept()=>{
                    let Ok((stream,_))=accepted else{break};let Ok(permit)=permits.clone().try_acquire_owned()else{continue};
                    let token=token.clone();let socket=socket.clone();let root=root.clone();let allowed=allowed.clone();
                    clients.spawn(async move {
                        let _permit=permit;let (read,mut write)=stream.into_split();let mut reader=BufReader::new(read);
                        let Ok(Some(launch))=tokio::time::timeout(Duration::from_secs(5),handshake(&mut reader,&token,&allowed)).await else{return};
                        let Ok(key)=session_key(&launch.provider,&launch.cwd,launch.env.get("MULTICA_TASK_ID").map(String::as_str)) else {
                            log::warn!("Multica runtime connection has no task identity; not launching");
                            return;
                        };
                        let session_root=root.join(".runtime-sessions").join(key);
                        let result:Result<(),String>=async {
                            match pinned_destination(&session_root,&launch.provider).map_err(|e|e.to_string())? {
                                Destination::Unavailable=>Err("Provider is unavailable".into()),
                                Destination::Cli(program)=>{
                                    forward_cli(&program,&launch,&root,&mut reader,&mut write).await
                                },
                                Destination::Desktop=>{
                                    let cli=root.join("bin/multica");
                                    let credential=muster_acp::TaskCredential::create(&root.join(".runtime-credentials"),&cli,&launch.env)?;
                                    let local=tokio::net::UnixStream::connect(&socket).await.map_err(|e|e.to_string())?;
                                    let host=Arc::new(muster_acp::NativeHost::new(muster_rpc::Client::new(local),&launch.provider,session_root,credential));
                                    let cwd=launch.cwd.to_string_lossy().into_owned();
                                    if launch.provider=="codex"{muster_acp::scheduler::codex(reader,write,host,cwd,&launch.args).await}else{muster_acp::scheduler::claude(reader,write,host,cwd,&launch.args).await}.map_err(|e|e.to_string())
                                }
                            }
                        }.await;
                        // Protocol and argv can contain credentials; never log raw errors.
                        if result.is_err(){log::warn!("Multica runtime connection ended without a successful protocol result");}
                    });
                }
            }
        }
    })
}
#[cfg(unix)]
struct CliGroup(u32);
#[cfg(unix)]
impl Drop for CliGroup {
    fn drop(&mut self) {
        // The leader has not been reaped, so this owned process-group ID cannot
        // have been recycled. Never signal the daemon's own process group.
        unsafe {
            libc::kill(-(self.0 as i32), libc::SIGKILL);
        }
    }
}
#[cfg(unix)]
async fn forward_cli<R: tokio::io::AsyncRead + Unpin, W: tokio::io::AsyncWrite + Unpin>(
    program: &Path,
    launch: &Launch,
    root: &Path,
    reader: &mut R,
    write: &mut W,
) -> Result<(), String> {
    use tokio::io::AsyncWriteExt;
    let mut command = tokio::process::Command::new(program);
    command
        .args(&launch.args)
        .current_dir(&launch.cwd)
        .envs(&launch.env)
        .env(
            "PATH",
            format!(
                "{}:{}",
                root.join("bin").display(),
                std::env::var("PATH").unwrap_or_default()
            ),
        )
        .process_group(0)
        .kill_on_drop(true)
        .stdin(std::process::Stdio::piped())
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::null());
    let mut child = command.spawn().map_err(|_| "CLI launch failed")?;
    let group = CliGroup(child.id().ok_or("CLI exited before ownership")?);
    let mut stdin = child.stdin.take().unwrap();
    let mut stdout = child.stdout.take().unwrap();
    let input = async move {
        let result = tokio::io::copy(reader, &mut stdin).await;
        let _ = stdin.shutdown().await;
        drop(stdin);
        result
    };
    let output = tokio::io::copy(&mut stdout, write);
    tokio::pin!(input, output);
    tokio::select! { _= &mut output=>{}, _= &mut input=>{let _=output.await;} }
    drop(group);
    let _ = child.wait().await;
    Ok(())
}

#[cfg(not(unix))]
pub(super) async fn ensure(_: &Manager, _: &Config, _: &Path, _: &str) -> Result<(), Error> {
    Err(Error::Unavailable("Apple container requires macOS".into()))
}

#[cfg(test)]
mod tests {
    use super::*;
    use base64::Engine;
    fn encoded(s: &str) -> String {
        base64::engine::general_purpose::STANDARD.encode(s)
    }
    #[cfg(unix)]
    #[tokio::test]
    async fn cli_fallback_preserves_protocol_arguments_directory_and_environment() {
        use std::os::unix::fs::PermissionsExt;
        let root = tempfile::tempdir().unwrap();
        let script = root.path().join("fixture-cli");
        std::fs::write(
            &script,
            "#!/bin/sh\nprintf '%s\\n' \"$PWD\" \"$MULTICA_TASK_ID\" \"$1\" \"$2\"\ncat\n",
        )
        .unwrap();
        std::fs::set_permissions(&script, std::fs::Permissions::from_mode(0o700)).unwrap();
        let cwd = root.path().canonicalize().unwrap();
        let launch = Launch {
            provider: "codex".into(),
            cwd: cwd.clone(),
            args: vec!["app-server".into(), "literal argument".into()],
            env: HashMap::from([("MULTICA_TASK_ID".into(), "fixture-task".into())]),
        };
        let selected = native_runtime::destination(Some(false), Some(script.clone())).unwrap();
        let Destination::Cli(program) = selected else {
            panic!("expected CLI fallback")
        };
        let mut input = &b"{\"id\":1}\n"[..];
        let mut output = Vec::new();
        forward_cli(&program, &launch, root.path(), &mut input, &mut output)
            .await
            .unwrap();
        assert_eq!(
            String::from_utf8(output).unwrap(),
            format!(
                "{}\nfixture-task\napp-server\nliteral argument\n{{\"id\":1}}\n",
                cwd.display()
            )
        );
    }
    #[tokio::test]
    async fn bridge_requires_token_provider_directory_and_bounded_launch() {
        let root = tempfile::tempdir().unwrap();
        let valid = format!(
            "fixture-token\nMUSTER_NATIVE_V1\ncodex\n{}\n1\n{}\nMULTICA_TASK_ID=fixture-task\n\n",
            encoded(root.path().to_str().unwrap()),
            encoded("app-server")
        );
        let mut reader = BufReader::new(valid.as_bytes());
        let launch = handshake(&mut reader, "fixture-token", root.path())
            .await
            .unwrap();
        assert_eq!(launch.provider, "codex");
        assert_eq!(launch.env["MULTICA_TASK_ID"], "fixture-task");
        for bad in [
            valid.replacen("fixture-token", "wrong", 1),
            valid.replacen("codex", "kiro", 1),
            valid.replace("MULTICA_TASK_ID=fixture-task", "PATH=/tmp"),
            valid.replace("\n1\n", "\n999\n"),
        ] {
            assert!(
                handshake(
                    &mut BufReader::new(bad.as_bytes()),
                    "fixture-token",
                    root.path()
                )
                .await
                .is_none()
            );
        }
    }

    /// Build one complete bridge handshake for the given provider, directory,
    /// argv and task environment.
    #[cfg(unix)]
    fn handshake_payload(
        token: &str,
        provider: &str,
        cwd: &Path,
        args: &[&str],
        env: &[(&str, &str)],
    ) -> Vec<u8> {
        let mut text = format!(
            "{token}\nMUSTER_NATIVE_V1\n{provider}\n{}\n{}\n",
            encoded(&cwd.to_string_lossy()),
            args.len()
        );
        for arg in args {
            text.push_str(&encoded(arg));
            text.push('\n');
        }
        for (name, value) in env {
            text.push_str(&format!("{name}={value}\n"));
        }
        text.push('\n');
        text.into_bytes()
    }

    #[cfg(unix)]
    async fn handshake_with(
        root: &Path,
        provider: &str,
        args: &[&str],
        env: &[(&str, &str)],
    ) -> Option<Launch> {
        let payload = handshake_payload("fixture-token", provider, root, args, env);
        handshake(
            &mut BufReader::new(payload.as_slice()),
            "fixture-token",
            root,
        )
        .await
    }

    /// The bridge gate must equal what `muster_acp::scheduler::codex` serves, so
    /// the adapter never receives a launch it refuses and the bridge never
    /// refuses one the adapter would accept.
    #[cfg(unix)]
    #[tokio::test]
    async fn handshake_accepts_exactly_the_adapter_launch_shapes() {
        let root = tempfile::tempdir().unwrap();
        for args in [
            &[][..],
            &["app-server"][..],
            &["app-server", "--listen", "stdio://"][..],
        ] {
            assert!(
                handshake_with(root.path(), "codex", args, &[])
                    .await
                    .is_some(),
                "{args:?}"
            );
        }
        for args in [
            &["app-server", "--listen", "tcp://0.0.0.0:1"][..],
            &["--full-auto"][..],
            &["exec"][..],
        ] {
            assert!(
                handshake_with(root.path(), "codex", args, &[])
                    .await
                    .is_none(),
                "{args:?}"
            );
        }
        // Claude needs stream-json input to speak this protocol at all.
        assert!(
            handshake_with(
                root.path(),
                "claude",
                &["--input-format", "stream-json"],
                &[]
            )
            .await
            .is_some()
        );
        assert!(
            handshake_with(
                root.path(),
                "claude",
                &["--output-format", "stream-json"],
                &[]
            )
            .await
            .is_none()
        );
    }

    #[test]
    fn session_keys_require_a_task_identity_and_separate_tasks() {
        let cwd = Path::new("/tmp/workspaces/task");
        let first = session_key("codex", cwd, Some("task-a")).unwrap();
        assert_eq!(first.len(), 64);
        assert!(first.bytes().all(|b| b.is_ascii_hexdigit()));
        assert_ne!(first, session_key("codex", cwd, Some("task-b")).unwrap());
        assert_ne!(first, session_key("claude", cwd, Some("task-a")).unwrap());
        assert_ne!(
            first,
            session_key("codex", Path::new("/tmp/workspaces/other"), Some("task-a")).unwrap()
        );
        for missing in [None, Some(""), Some("   ")] {
            assert!(matches!(
                session_key("codex", cwd, missing),
                Err(Error::Unavailable(_))
            ));
        }
    }

    #[test]
    fn gateway_must_be_a_private_container_address() {
        assert!(gateway_address("192.168.64.1").is_ok());
        assert!(gateway_address("10.1.2.3").is_ok());
        // Test builds accept loopback so the listener runs on an ordinary host;
        // shipped builds keep the private-only rule (see `gateway_address`).
        assert!(gateway_address("127.0.0.1").is_ok());
        for rejected in ["8.8.8.8", "0.0.0.0", "::1", "container", ""] {
            assert!(gateway_address(rejected).is_err(), "{rejected}");
        }
    }

    /// Wait until the bridge drops a connection, or report that it stayed open.
    #[cfg(unix)]
    async fn closes_without_launch(port: u16, payload: Vec<u8>) -> bool {
        use tokio::io::{AsyncReadExt, AsyncWriteExt};
        let mut stream = tokio::net::TcpStream::connect(("127.0.0.1", port))
            .await
            .expect("connect to the bridge listener");
        stream.write_all(&payload).await.expect("send handshake");
        let mut byte = [0u8; 1];
        match tokio::time::timeout(Duration::from_secs(3), stream.read(&mut byte)).await {
            Ok(Ok(0)) | Ok(Err(_)) => true,
            Ok(Ok(_)) | Err(_) => false,
        }
    }

    /// Full bridge path: listener on loopback, private token/wrapper files,
    /// the fake container's retired-alias cleanup, and rejected handshakes.
    #[cfg(unix)]
    #[tokio::test]
    async fn bridge_publishes_private_files_and_refuses_unbound_task_connections() {
        use std::os::unix::fs::PermissionsExt;
        let dir = tempfile::tempdir().unwrap();
        let host = dir.path();
        let data = host.join("data");
        let task_dir = data.join("workspaces/task-a");
        std::fs::create_dir_all(&task_dir).unwrap();
        std::fs::create_dir_all(data.join("bin")).unwrap();
        std::fs::write(data.join("bin/multica"), "#!/bin/sh\n").unwrap();
        std::fs::set_permissions(
            data.join("bin/multica"),
            std::fs::Permissions::from_mode(0o700),
        )
        .unwrap();
        let calls = host.join("container-calls");
        let container = host.join("container-fixture");
        std::fs::write(
            &container,
            format!(
                "#!/bin/sh\nset -eu\nprintf '%s\\n' \"$*\" >> {}\n",
                quote(&calls.to_string_lossy())
            ),
        )
        .unwrap();
        std::fs::set_permissions(&container, std::fs::Permissions::from_mode(0o700)).unwrap();
        let mut manager = Manager::new(host);
        manager.executable = Some(container);
        let c = Config {
            data_dir: data.to_string_lossy().into_owned(),
            container_name: "mlt".into(),
            ..Default::default()
        };

        // A crashed bridge's launcher is revoked by the helper the daemon calls
        // at startup.
        let credentials = data.join(".runtime-credentials");
        std::fs::create_dir_all(&credentials).unwrap();
        let stale = credentials.join(format!("multica-{}", "a".repeat(32)));
        std::fs::write(&stale, "#!/bin/sh\n").unwrap();
        revoke_stale(&c).unwrap();
        assert!(!stale.exists());

        let (listener, port, token) = bind_bridge("127.0.0.1").await.unwrap();
        write_bridge_files(&c, "127.0.0.1", port, &token).unwrap();
        for name in ["bridge-token", "runtime-codex", "runtime-claude"] {
            let mode = std::fs::metadata(data.join(name))
                .unwrap()
                .permissions()
                .mode()
                & 0o777;
            assert_eq!(mode, 0o600, "{name} must stay private");
        }
        let codex = std::fs::read_to_string(data.join("runtime-codex")).unwrap();
        for needle in [
            format!("MUSTER_RUNTIME_PROVIDER={}", quote("codex")),
            format!("MUSTER_BRIDGE_HOST={}", quote("127.0.0.1")),
            format!("MUSTER_BRIDGE_PORT={port}"),
            "version-codex".into(),
            "bridge-token".into(),
            "runtime-command".into(),
        ] {
            assert!(
                codex.contains(&needle),
                "runtime-codex is missing {needle}: {codex}"
            );
        }
        let claude = std::fs::read_to_string(data.join("runtime-claude")).unwrap();
        assert!(claude.contains(&format!("MUSTER_RUNTIME_PROVIDER={}", quote("claude"))));
        assert!(
            std::fs::read_to_string(data.join("runtime-command"))
                .unwrap()
                .contains("MUSTER_NATIVE_V1")
        );

        // The fake container returns success and records the cleanup command.
        remove_legacy_alias(&manager, &c).await.unwrap();
        let recorded = std::fs::read_to_string(&calls).unwrap();
        assert!(
            recorded.contains("kiro-cli") && recorded.contains("MUSTER_BRIDGE"),
            "{recorded}"
        );

        let server = spawn_bridge(
            listener,
            token.clone(),
            host.join("musterd.sock"),
            data.clone(),
            data.clone(),
        );
        let task = [("MULTICA_TASK_ID", "task-a")];
        let in_root = task_dir.canonicalize().unwrap();
        let handshake = |provider: &str, cwd: &Path, env: &[(&str, &str)]| {
            handshake_payload(&token, provider, cwd, &["app-server"], env)
        };

        assert!(
            closes_without_launch(
                port,
                handshake_payload("wrong-token", "codex", &in_root, &["app-server"], &task)
            )
            .await
        );
        assert!(closes_without_launch(port, handshake("kiro", &in_root, &task)).await);
        assert!(closes_without_launch(port, handshake("codex", Path::new("/etc"), &task)).await);
        assert!(
            closes_without_launch(port, handshake("codex", &in_root, &[("PATH", "/tmp")])).await
        );
        assert!(closes_without_launch(port, handshake("codex", &in_root, &[])).await);
        assert!(
            closes_without_launch(
                port,
                handshake("codex", &in_root, &[("MULTICA_TASK_ID", "")])
            )
            .await
        );
        assert!(
            !data.join(".runtime-sessions").exists(),
            "a connection without a task identity must not create a session"
        );

        server.abort();
    }
}
