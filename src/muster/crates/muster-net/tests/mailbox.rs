//! Roster cold start through the relay mailbox (ARCHITECTURE §3.5): a
//! device that was away learns the entries published meanwhile even when
//! no peer is online, and deposits what the relay lacks.

use std::net::{Ipv4Addr, SocketAddr};
use std::sync::{Arc, RwLock};
use std::time::{Duration, Instant};

use muster_core::roster::{Author, DeviceKind, DeviceRecord, DeviceUpdate, Role, Roster};
use muster_core::{HwSigner, LocalPolicy, SoftSigner};
use muster_net::magicsock::{MagicOptions, RelayState};
use muster_net::node::{Node, NodeConfig, RelayConfig};

async fn spawn_relay() -> (SocketAddr, tokio::task::JoinHandle<()>) {
    let server = muster_relay::RelayServer::bind(
        "127.0.0.1:0".parse().unwrap(),
        muster_relay::RelayOptions::default(),
    )
    .await
    .unwrap();
    let udp = server.udp_addr().unwrap();
    let task = tokio::spawn(async move {
        let _ = server.serve().await;
    });
    (udp, task)
}

fn config(relay: SocketAddr) -> NodeConfig {
    NodeConfig {
        bind: "127.0.0.1:0".parse().unwrap(),
        relay: RelayConfig::Udp(relay),
        options: MagicOptions {
            relay_only: true,
            mobile: false,
            allow_loopback: true,
            extra_candidates: Vec::new(),
            push_token: None,
        },
        datagrams: false,
        build: Default::default(),
    }
}

async fn wait_for<F: Fn() -> bool>(what: &str, timeout: Duration, check: F) {
    let started = Instant::now();
    while !check() {
        assert!(started.elapsed() < timeout, "timed out waiting for {what}");
        tokio::time::sleep(Duration::from_millis(50)).await;
    }
}

async fn start(
    relay: SocketAddr,
    signer: &Arc<dyn HwSigner>,
    roster: &Arc<RwLock<Roster>>,
) -> Arc<Node> {
    let node = Node::start(
        config(relay),
        signer.clone(),
        roster.clone(),
        Arc::new(RwLock::new(LocalPolicy::default())),
    )
    .await
    .unwrap();
    let probe = node.clone();
    wait_for("relay registration", Duration::from_secs(10), move || {
        matches!(probe.relay_state(), RelayState::Registered)
    })
    .await;
    node
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn roster_cold_start_through_the_mailbox() {
    let _ = env_logger::Builder::from_env(env_logger::Env::default().default_filter_or("info"))
        .is_test(true)
        .try_init();
    let (relay, _relay_task) = spawn_relay().await;

    let admin_signer: Arc<dyn HwSigner> = Arc::new(SoftSigner::generate());
    let b_signer: Arc<dyn HwSigner> = Arc::new(SoftSigner::generate());
    let c_signer: Arc<dyn HwSigner> = Arc::new(SoftSigner::generate());
    let b_id = b_signer.public_key().device_id();
    let c_id = c_signer.public_key().device_id();

    // The admin creates the network and enrols B (an admin too) while online.
    let admin_roster = Arc::new(RwLock::new(Roster::new()));
    let genesis = {
        let roster = admin_roster.read().unwrap();
        Author {
            roster: &roster,
            signer: admin_signer.as_ref(),
        }
        .genesis(
            "home",
            (Ipv4Addr::new(192, 168, 14, 0), 24),
            DeviceRecord::new(admin_signer.public_key(), "admin", DeviceKind::Desktop),
        )
        .unwrap()
    };
    admin_roster
        .write()
        .unwrap()
        .insert(genesis.clone())
        .unwrap();
    let admin = start(relay, &admin_signer, &admin_roster).await;

    let add_b = {
        let roster = admin_roster.read().unwrap();
        let mut record = DeviceRecord::new(b_signer.public_key(), "b", DeviceKind::Desktop);
        record.role = Role::Admin;
        Author {
            roster: &roster,
            signer: admin_signer.as_ref(),
        }
        .add(record)
        .unwrap()
    };
    admin.publish(add_b.clone()).unwrap();

    // B holds the roster as of its enrolment, then goes away. Meanwhile the
    // admin adds C, then leaves too.
    let b_roster = Arc::new(RwLock::new(
        Roster::from_entries([genesis.clone(), add_b.clone()]).unwrap(),
    ));
    let add_c = {
        let roster = admin_roster.read().unwrap();
        Author {
            roster: &roster,
            signer: admin_signer.as_ref(),
        }
        .add(DeviceRecord::new(
            c_signer.public_key(),
            "c",
            DeviceKind::Server,
        ))
        .unwrap()
    };
    admin.publish(add_c.clone()).unwrap();
    // Give the deposit its round trip before the admin goes.
    tokio::time::sleep(Duration::from_millis(300)).await;
    admin.shutdown().await;
    drop(admin);

    // B, still offline, renames itself; the entry exists only in B's roster
    // and sits on B's own heads, beside the admin's Add(C).
    let rename_b = {
        let roster = b_roster.read().unwrap();
        Author {
            roster: &roster,
            signer: b_signer.as_ref(),
        }
        .update(
            b_id,
            DeviceUpdate {
                name: Some("b-renamed".into()),
                ..DeviceUpdate::default()
            },
        )
        .unwrap()
    };
    assert_eq!(rename_b.parents, vec![add_b.hash()]);
    b_roster.write().unwrap().insert(rename_b.clone()).unwrap();

    // B comes back with no peer online: the mailbox gives it Add(C), and
    // the same pass deposits the rename the relay never saw.
    let b = start(relay, &b_signer, &b_roster).await;
    let probe = b_roster.clone();
    wait_for("B to learn of C", Duration::from_secs(10), move || {
        probe.read().unwrap().state().active(&c_id).is_some()
    })
    .await;
    // A second pass finds nothing missing on either side.
    assert_eq!(b.sync_mailbox().await.unwrap(), 0);

    // C starts from its enrolment snapshot and learns of B's rename from the
    // relay alone.
    let c_roster = Arc::new(RwLock::new(
        Roster::from_entries([genesis, add_b, add_c]).unwrap(),
    ));
    let _c = start(relay, &c_signer, &c_roster).await;
    let probe = c_roster.clone();
    wait_for(
        "C to learn B's new name",
        Duration::from_secs(10),
        move || {
            probe
                .read()
                .unwrap()
                .state()
                .active(&b_id)
                .is_some_and(|m| m.record.name == "b-renamed")
        },
    )
    .await;
    assert!(c_roster.read().unwrap().get(&rename_b.hash()).is_some());
    assert_eq!(
        c_roster
            .read()
            .unwrap()
            .state()
            .active(&c_id)
            .unwrap()
            .record
            .name,
        "c"
    );
    drop(b);
}
