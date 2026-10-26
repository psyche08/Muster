use super::*;
#[cfg(unix)]
pub(super) async fn ensure(
    manager: &Manager,
    c: &Config,
    socket: &Path,
    gateway: &str,
) -> Result<(), Error> {
    use std::sync::Arc;
    use tokio::io::{AsyncBufReadExt, AsyncReadExt, BufReader};
    let mut bridge = manager.bridge.lock().await;
    if bridge.as_ref().is_some_and(|task| !task.is_finished()) {
        return Ok(());
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
    private_write(&root.join("bridge-token"), token.as_bytes())?;
    let wrapper = format!(
        "#!/bin/sh\nexport MUSTER_BRIDGE_HOST={} MUSTER_BRIDGE_PORT={}\nMUSTER_BRIDGE_TOKEN=$(cat /muster/bridge-token)\nexport MUSTER_BRIDGE_TOKEN\nexec /bin/sh /muster/runtime-command \"$@\"\n",
        quote(gateway),
        port
    );
    private_write(&root.join("kiro-cli"), wrapper.as_bytes())?;
    manager
        .command(
            &[
                "exec",
                &c.container_name,
                "install",
                "-m",
                "0755",
                "/muster/kiro-cli",
                "/usr/local/bin/kiro-cli",
            ],
            10,
        )
        .await?;
    let socket = socket.to_path_buf();
    let extra = root.join("bin");
    *bridge = Some(tokio::spawn(async move {
        let permits = Arc::new(tokio::sync::Semaphore::new(8));
        let mut clients = tokio::task::JoinSet::new();
        loop {
            tokio::select! {
                _ = clients.join_next(), if !clients.is_empty() => {},
                accepted = listener.accept() => {
                    let Ok((stream, _)) = accepted else { break };
                    let Ok(permit) = permits.clone().try_acquire_owned() else { continue };
                    let token = token.clone(); let socket = socket.clone(); let extra = extra.clone();
                    clients.spawn(async move {
                        let _permit = permit;
                        let (read, write) = stream.into_split(); let mut reader = BufReader::new(read);
                        let handshake = async {
                            let mut env = std::collections::HashMap::new();
                            for i in 0..=65 {
                                let mut line = String::new();
                                (&mut reader).take(8193).read_line(&mut line).await.ok()?;
                                if line.len() > 8192 { return None; }
                                let line = line.trim_end_matches(['\r','\n']);
                                if i == 0 { if line != token { return None; } continue; }
                                if line.is_empty() { return Some(env); }
                                let (name,value) = line.split_once('=')?;
                                if !name.starts_with("MULTICA_") { return None; }
                                env.insert(name.to_string(),value.to_string());
                            }
                            None
                        };
                        let Ok(Some(mut env)) = tokio::time::timeout(Duration::from_secs(5), handshake).await else { return };
                        env.insert("PATH".into(), format!("{}:{}", extra.display(), std::env::var("PATH").unwrap_or_default()));
                        let Ok(local) = tokio::net::UnixStream::connect(socket).await else { return };
                        let host = Arc::new(muster_acp::RpcHost::new(muster_rpc::Client::new(local), "codex", env));
                        let _ = muster_acp::serve(reader, write, host, muster_acp::Identity::default()).await;
                    });
                }
            }
        }
    }));
    Ok(())
}
#[cfg(not(unix))]
pub(super) async fn ensure(_: &Manager, _: &Config, _: &Path, _: &str) -> Result<(), Error> {
    Err(Error::Unavailable("Apple container requires macOS".into()))
}
