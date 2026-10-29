use super::*;
use muster_agent::native_runtime::{self, Destination};
use std::{collections::HashMap, sync::Arc};
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
    if provider == "codex" && args.first().map(String::as_str) != Some("app-server") {
        return None;
    }
    if provider == "claude" && !args.iter().any(|a| a == "--input-format") {
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
    let address: std::net::Ipv4Addr = gateway
        .parse()
        .map_err(|_| Error::Invalid("Invalid container gateway".into()))?;
    if !address.is_private() {
        return Err(Error::Invalid("Container gateway must be private".into()));
    }
    let listener = tokio::net::TcpListener::bind((address, 0))
        .await
        .map_err(io)?;
    let port = listener.local_addr().map_err(io)?.port();
    let mut random = [0u8; 32];
    std::io::Read::read_exact(
        &mut std::fs::File::open("/dev/urandom").map_err(io)?,
        &mut random,
    )
    .map_err(io)?;
    let token = hex::encode(random);
    let root = Path::new(&c.data_dir);
    muster_acp::TaskCredential::revoke_stale(&root.join(".runtime-credentials"))
        .map_err(Error::Unavailable)?;
    let shared = if c.managed {
        PathBuf::from("/muster")
    } else {
        root.to_path_buf()
    };
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
    refresh_wrappers(manager, c).await?;
    manager.command(&["exec",&c.container_name,"sh","-c","if [ -f /usr/local/bin/kiro-cli ] && grep -q 'MUSTER_BRIDGE' /usr/local/bin/kiro-cli; then rm /usr/local/bin/kiro-cli; fi"],10).await?;
    let socket = socket.to_path_buf();
    let root = root.to_path_buf();
    let allowed = PathBuf::from(workspaces(c));
    *bridge = Some(tokio::spawn(async move {
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
                        let result:Result<(),String>=async {
                            use sha2::{Digest,Sha256};
                            let key=hex::encode(Sha256::digest(format!("{}\0{}\0{}",launch.provider,launch.cwd.display(),launch.env.get("MULTICA_TASK_ID").map(String::as_str).unwrap_or_default())));
                            let session_root=root.join(".runtime-sessions").join(key);
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
    }));
    Ok(())
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
}
