//! Public-PKI data-plane probe with two ephemeral peers and an in-memory roster.
//! No existing identities, pairing, on-disk state or server configuration used.
use anyhow::Context;
use muster_core::{
    LocalPolicy, SharedSigner, SoftSigner,
    roster::{Author, DeviceKind, DeviceRecord, Roster},
};
use muster_net::{
    magicsock::{MagicOptions, RelayState},
    node::{Node, NodeConfig, RelayConfig, ServiceHandler},
    stream::Hello,
};
use std::{
    sync::{Arc, RwLock},
    time::Duration,
};

const PAYLOAD_BYTES: usize = 256 * 1024;

struct Echo;
impl ServiceHandler for Echo {
    fn handle(
        &self,
        _node: Arc<Node>,
        _peer: muster_core::DeviceId,
        _hello: Hello,
        mut send: quinn::SendStream,
        mut recv: quinn::RecvStream,
    ) -> std::pin::Pin<Box<dyn std::future::Future<Output = ()> + Send>> {
        Box::pin(async move {
            let result: anyhow::Result<()> = async {
                let bytes = recv.read_to_end(PAYLOAD_BYTES).await?;
                send.write_all(&bytes).await?;
                send.finish()?;
                Ok(())
            }
            .await;
            if let Err(error) = result {
                log::warn!("probe echo failed: {error:#}");
            }
        })
    }
}

async fn echo(a: &Arc<Node>, b: &Arc<Node>, seed: u8) -> anyhow::Result<()> {
    let bytes: Vec<u8> = (0..PAYLOAD_BYTES)
        .map(|i| (i as u8).wrapping_mul(31).wrapping_add(seed))
        .collect();
    let (mut send, mut recv) = a.open(b.device_id(), "echo", 1).await?;
    send.write_all(&bytes).await?;
    send.finish()?;
    let reply = recv.read_to_end(PAYLOAD_BYTES).await?;
    anyhow::ensure!(reply == bytes, "echo response differs from probe payload");
    Ok(())
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    env_logger::Builder::from_env(env_logger::Env::default().default_filter_or("info")).init();
    let host = std::env::args()
        .nth(1)
        .unwrap_or_else(|| "muster.linsheng.dev:9443".into());
    let authority = if host.contains(':') {
        host
    } else {
        format!("{host}:9443")
    };
    let a: SharedSigner = Arc::new(SoftSigner::generate());
    let b: SharedSigner = Arc::new(SoftSigner::generate());
    let mut roster = Roster::new();
    let genesis = Author {
        roster: &roster,
        signer: a.as_ref(),
    }
    .genesis(
        "ephemeral relay transfer probe",
        ("10.44.0.0".parse()?, 24),
        DeviceRecord::new(a.public_key(), "probe-a", DeviceKind::Desktop),
    )?;
    roster.insert(genesis)?;
    let member = Author {
        roster: &roster,
        signer: a.as_ref(),
    }
    .add(DeviceRecord::new(
        b.public_key(),
        "probe-b",
        DeviceKind::Desktop,
    ))?;
    roster.insert(member)?;
    let mut nodes = Vec::new();
    // Bound network work plus shutdown to at most 30 seconds.
    let result = tokio::time::timeout(Duration::from_secs(25), async {
        for signer in [a, b] {
            let node = Node::start(
                NodeConfig {
                    bind: "0.0.0.0:0".parse()?,
                    relay: RelayConfig::Secure(authority.clone()),
                    options: MagicOptions {
                        relay_only: true,
                        mobile: false,
                        ..Default::default()
                    },
                    datagrams: false,
                    build: Default::default(),
                },
                signer,
                Arc::new(RwLock::new(roster.clone())),
                Arc::new(RwLock::new(LocalPolicy::default())),
            )
            .await?;
            node.set_handler(Arc::new(Echo));
            nodes.push(node);
        }
        tokio::time::timeout(Duration::from_secs(10), async {
            while nodes
                .iter()
                .any(|node| node.relay_state() != RelayState::Registered)
            {
                tokio::time::sleep(Duration::from_millis(50)).await;
            }
        })
        .await
        .context("both ephemeral peers must register within 10 seconds")?;
        // Background reconnectors may dial each other simultaneously. Allow
        // duplicate-connection arbitration before sending any payload; the
        // application operations below are never automatically retried.
        let mut stable = None;
        loop {
            let pair = nodes[0]
                .connection_to(&nodes[1].device_id())
                .zip(nodes[1].connection_to(&nodes[0].device_id()))
                .map(|(a, b)| (a.stable_id(), b.stable_id()));
            match (pair, stable) {
                (Some(pair), Some((previous, since))) if pair == previous => {
                    if tokio::time::Instant::now() - since >= Duration::from_secs(3) {
                        break;
                    }
                }
                (Some(pair), _) => stable = Some((pair, tokio::time::Instant::now())),
                (None, _) => stable = None,
            }
            tokio::time::sleep(Duration::from_millis(50)).await;
        }
        tokio::try_join!(
            echo(&nodes[0], &nodes[1], 7),
            echo(&nodes[1], &nodes[0], 19)
        )?;
        Ok::<_, anyhow::Error>(())
    })
    .await
    .context("relay-only transfers exceeded 25-second deadline")
    .and_then(|result| result);
    let shutdown = tokio::time::timeout(Duration::from_secs(5), async {
        for node in &nodes {
            node.shutdown().await;
        }
    })
    .await;
    result?;
    shutdown.context("probe shutdown exceeded 5-second deadline")?;
    println!(
        "PASS: {authority}, relay-only=true, public-PKI verified; \
         A->B and B->A each echoed {PAYLOAD_BYTES} bytes exactly \
         ({} bytes total payload including replies); ephemeral peers shut down",
        4 * PAYLOAD_BYTES
    );
    Ok(())
}
