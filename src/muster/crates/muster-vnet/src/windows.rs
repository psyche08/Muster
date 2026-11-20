use std::fs::{File, OpenOptions};
use std::io::{self, BufRead};
use std::os::windows::fs::OpenOptionsExt;
use std::path::PathBuf;
use std::sync::{Arc, RwLock};
use std::time::{SystemTime, UNIX_EPOCH};

use anyhow::{Context, Result, bail, ensure};
use clap::Parser;
use muster_core::{DeviceKind, DeviceRecord, Invite, LocalPolicy, SharedSigner};
use muster_net::vnet::{Vnet, VnetConfig};
use muster_net::{Node, NodeConfig, RelayConfig};
use muster_store::Store;
use tokio::sync::broadcast::error::RecvError;

#[derive(Parser)]
#[command(about = "Run a Windows Muster virtual network member")]
struct Options {
    /// Read an existing network invite from standard input on first enrollment.
    #[arg(long)]
    join: bool,
    /// Report the local TPM identity for enrollment verification.
    #[arg(long, conflicts_with = "join")]
    identity: bool,
}

pub async fn run() -> Result<()> {
    let options = Options::parse();
    let path = PathBuf::from(std::env::var_os("LOCALAPPDATA").context("LOCALAPPDATA missing")?)
        .join("Muster")
        .join("Vnet");
    std::fs::create_dir_all(&path)?;
    // A Windows sharing violation excludes a second process, even after a crash.
    let _owner: File = OpenOptions::new()
        .write(true)
        .create(true)
        .truncate(false)
        .share_mode(0)
        .open(path.join("owner.lock"))
        .context("acquire vnet process lock")?;
    let signer: SharedSigner = Arc::new(muster_windows_tpm::Tpm::open()?);
    println!("Device {}", signer.public_key().device_id());
    if options.identity {
        return Ok(());
    }
    let store = Store::open(&path.join("node.sqlite3"))?;
    let roster = store.load_roster()?;
    let invite = if options.join {
        ensure!(roster.is_empty(), "already enrolled");
        let mut text = String::new();
        io::stdin().lock().read_line(&mut text)?;
        let invite = Invite::decode(text.trim())?;
        ensure!(
            !invite.expired(SystemTime::now().duration_since(UNIX_EPOCH)?.as_secs()),
            "invite expired"
        );
        Some(invite)
    } else {
        ensure!(
            !roster.is_empty(),
            "enrollment required: supply an invite on stdin with --join"
        );
        None
    };
    let relay = match &invite {
        Some(invite) => invite.relay.clone(),
        None => store
            .meta_string("relay")?
            .or(store.meta_string("vnet.relay")?)
            .context("enrollment relay missing")?,
    };
    let record = DeviceRecord::new(
        signer.public_key(),
        std::env::var("COMPUTERNAME")?,
        DeviceKind::Desktop,
    );
    let node = Node::start_vnet_only(
        NodeConfig {
            relay: RelayConfig::Secure(relay.clone()),
            ..Default::default()
        },
        signer,
        Arc::new(RwLock::new(roster)),
        Arc::new(RwLock::new(LocalPolicy {
            offer_agent: false,
            offer_terminal: false,
            open_ports: Vec::new(),
            ..Default::default()
        })),
    )
    .await?;
    let mut changes = node.subscribe_entries();
    if let Some(invite) = invite {
        // Without a roster the relay alone grants no membership; writing it
        // first makes a crash after the roster commit safely resumable.
        store.set_meta("vnet.relay", relay.as_bytes())?;
        node.join(&invite, record).await?;
    }
    persist_roster(&node, &store)?;
    let vnet = Vnet::start(
        node.clone(),
        VnetConfig {
            // The Windows backend configures the adapter through netsh.
            name: Some("MusterVnet".into()),
            ..Default::default()
        },
    )
    .await?;
    println!("Muster Vnet up: {}/{}", vnet.address(), vnet.cidr().1);
    let stop = tokio::signal::ctrl_c();
    tokio::pin!(stop);
    let result = loop {
        tokio::select! {
            result = &mut stop => break result.map_err(Into::into),
            change = changes.recv() => {
                let result = match change {
                    Ok(entry) => store.append_entries(&[entry]).map(|_| ()).map_err(Into::into),
                    Err(RecvError::Lagged(_)) => persist_roster(&node, &store),
                    Err(RecvError::Closed) => break Err(anyhow::anyhow!("network node stopped")),
                };
                if result.is_err() { break result; }
                let roster = node.roster().read().unwrap();
                let state = roster.state();
                let address = state.active(&node.device_id()).and_then(|m| m.record.vip);
                if address != Some(vnet.address()) || state.vnet_cidr != Some(vnet.cidr()) {
                    break Err(anyhow::anyhow!("local membership or vnet address changed; stopping adapter"));
                }
            }
        }
    };
    vnet.shutdown_and_wait().await;
    node.shutdown().await;
    result
}

fn persist_roster(node: &Node, store: &Store) -> Result<()> {
    let entries = node
        .roster()
        .read()
        .unwrap()
        .entries()
        .cloned()
        .collect::<Vec<_>>();
    if entries.is_empty() {
        bail!("no enrolled roster");
    }
    store.append_entries(&entries)?;
    Ok(())
}
