//! vnet on one host: two nodes with point-to-point TUNs (so the kernel routes
//! each side's traffic into the other's interface), IP packets carried as
//! QUIC DATAGRAMs, and the inbound filter deciding by roster scope and local
//! policy. The routed-TUN test needs root (TUN creation): `sudo -E cargo test
//! -p muster-net --test vnet -- --ignored`. On macOS, the packet-size test runs
//! without root on descriptors shaped like the ones NetHelper hands musterd.

use std::net::Ipv4Addr;
use std::sync::{Arc, RwLock};
use std::time::Duration;

use muster_core::roster::{Author, DeviceKind, DeviceRecord, Roster};
use muster_core::{HwSigner, LocalPolicy, PortRule, Proto, Scope, SoftSigner};
use muster_net::magicsock::MagicOptions;
use muster_net::node::{Node, NodeConfig, RelayConfig};
use muster_net::vnet::{Vnet, VnetConfig};

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
#[ignore = "needs root and two network namespaces or hosts: in one namespace both vips are local and never traverse the TUNs"]
async fn packets_flow_and_filter_applies() {
    let _ = env_logger::Builder::from_env(env_logger::Env::default().default_filter_or("info"))
        .is_test(true)
        .try_init();
    let a_policy = LocalPolicy {
        open_ports: vec![
            PortRule {
                proto: Proto::Tcp,
                port: Some(9100),
            },
            PortRule {
                proto: Proto::Tcp,
                port: Some(9101),
            },
        ],
        ..LocalPolicy::default()
    };
    let (a, b, a_vip, b_vip) = members(a_policy).await;

    // Point-to-point TUNs: a's interface routes b_vip, b's routes a_vip.
    let a_vnet = Vnet::start(
        a.clone(),
        VnetConfig {
            name: Some("muster-a".into()),
            ..VnetConfig::default()
        },
    )
    .await
    .unwrap();
    let b_vnet = Vnet::start(
        b.clone(),
        VnetConfig {
            name: Some("muster-b".into()),
            ..VnetConfig::default()
        },
    )
    .await
    .unwrap();
    assert_eq!(a_vnet.address(), a_vip);
    assert_eq!(b_vnet.address(), b_vip);

    // a listens on 9100 (granted to b) and 9101 (open locally, not granted).
    let granted = tokio::net::TcpListener::bind((a_vip, 9100)).await.unwrap();
    let not_granted = tokio::net::TcpListener::bind((a_vip, 9101)).await.unwrap();
    tokio::spawn(async move {
        loop {
            let Ok((mut s, _)) = granted.accept().await else {
                return;
            };
            tokio::spawn(async move {
                use tokio::io::{AsyncReadExt, AsyncWriteExt};
                let mut buf = [0u8; 64];
                if let Ok(n) = s.read(&mut buf).await {
                    let _ = s.write_all(&buf[..n]).await;
                }
            });
        }
    });
    tokio::spawn(async move {
        loop {
            let Ok(_) = not_granted.accept().await else {
                return;
            };
        }
    });

    // From b's vip to a's granted port: connect, echo.
    let socket = tokio::net::TcpSocket::new_v4().unwrap();
    socket.bind((b_vip, 0).into()).unwrap();
    let mut stream = tokio::time::timeout(
        Duration::from_secs(10),
        socket.connect((a_vip, 9100).into()),
    )
    .await
    .expect("connect in time")
    .expect("connect");
    use tokio::io::{AsyncReadExt, AsyncWriteExt};
    stream.write_all(b"over the vnet").await.unwrap();
    let mut buf = [0u8; 13];
    stream.read_exact(&mut buf).await.unwrap();
    assert_eq!(&buf, b"over the vnet");

    // Not granted: the SYN is dropped by a's filter; connect times out.
    let socket = tokio::net::TcpSocket::new_v4().unwrap();
    socket.bind((b_vip, 0).into()).unwrap();
    let refused =
        tokio::time::timeout(Duration::from_secs(3), socket.connect((a_vip, 9101).into())).await;
    assert!(refused.is_err(), "unauthorized port must not connect");
    assert!(a_vnet.stats().dropped_unauthorized > 0);
    assert!(a_vnet.stats().rx_packets > 0);
}

/// A relay and two connected members on loopback; `a_policy` is a's local
/// policy. Returns (a, b, a's vip, b's vip).
async fn members(a_policy: LocalPolicy) -> (Arc<Node>, Arc<Node>, Ipv4Addr, Ipv4Addr) {
    let relay = muster_relay::RelayServer::bind(
        "127.0.0.1:0".parse().unwrap(),
        muster_relay::RelayOptions::default(),
    )
    .await
    .unwrap();
    let relay_addr = relay.udp_addr().unwrap();
    tokio::spawn(async move {
        let _ = relay.serve().await;
    });
    let config = || NodeConfig {
        bind: "127.0.0.1:0".parse().unwrap(),
        relay: RelayConfig::Udp(relay_addr),
        options: MagicOptions {
            allow_loopback: true,
            ..MagicOptions::default()
        },
        datagrams: true,
        build: Default::default(),
    };

    // One roster, built by A, handed to B directly (pairing is covered elsewhere).
    let a_signer: Arc<dyn HwSigner> = Arc::new(SoftSigner::generate());
    let b_signer: Arc<dyn HwSigner> = Arc::new(SoftSigner::generate());
    let mut roster = Roster::new();
    let genesis = Author {
        roster: &roster,
        signer: a_signer.as_ref(),
    }
    .genesis(
        "lab",
        (Ipv4Addr::new(192, 168, 14, 0), 24),
        DeviceRecord::new(a_signer.public_key(), "a", DeviceKind::Desktop),
    )
    .unwrap();
    roster.insert(genesis).unwrap();
    let mut b_record = DeviceRecord::new(b_signer.public_key(), "b", DeviceKind::Server);
    b_record
        .scopes
        .insert("net:tcp/9100".parse::<Scope>().unwrap());
    let add = Author {
        roster: &roster,
        signer: a_signer.as_ref(),
    }
    .add(b_record)
    .unwrap();
    roster.insert(add).unwrap();
    let a_vip = roster
        .state()
        .active(&a_signer.public_key().device_id())
        .unwrap()
        .record
        .vip
        .unwrap();
    let b_vip = roster
        .state()
        .active(&b_signer.public_key().device_id())
        .unwrap()
        .record
        .vip
        .unwrap();
    assert_ne!(a_vip, b_vip);

    let a = Node::start(
        config(),
        a_signer.clone(),
        Arc::new(RwLock::new(roster.clone())),
        Arc::new(RwLock::new(a_policy)),
    )
    .await
    .unwrap();
    let b = Node::start(
        config(),
        b_signer.clone(),
        Arc::new(RwLock::new(roster.clone())),
        Arc::new(RwLock::new(LocalPolicy::default())),
    )
    .await
    .unwrap();

    (a, b, a_vip, b_vip)
}

/// A helper-opened TUN descriptor (as macOS's NetHelper hands musterd) and
/// the kernel's end of it. Frames carry utun's 4-byte address family.
#[cfg(target_os = "macos")]
fn helper_tun() -> (i32, tokio::net::UnixDatagram) {
    use std::os::fd::IntoRawFd;
    let (device, kernel) = std::os::unix::net::UnixDatagram::pair().unwrap();
    kernel.set_nonblocking(true).unwrap();
    (
        device.into_raw_fd(),
        tokio::net::UnixDatagram::from_std(kernel).unwrap(),
    )
}

/// An IPv4 ICMP echo request of `len` bytes with Don't Fragment set.
#[cfg(target_os = "macos")]
fn echo(src: Ipv4Addr, dst: Ipv4Addr, len: usize) -> Vec<u8> {
    let mut p = vec![0u8; len];
    p[0] = 0x45;
    p[2..4].copy_from_slice(&(len as u16).to_be_bytes());
    p[6] = 0x40;
    p[8] = 64;
    p[9] = 1;
    p[12..16].copy_from_slice(&src.octets());
    p[16..20].copy_from_slice(&dst.octets());
    p[20] = 8;
    p
}

/// The bug of 2026-10-03: once QUIC fell back to its minimum MTU, a 1,280-byte
/// TUN lost full-sized packets silently, and TCP with Don't Fragment stalled.
/// A full vnet MTU packet now crosses, and a packet the link cannot carry
/// earns the sender an ICMP "fragmentation needed" with a size that fits.
#[cfg(target_os = "macos")]
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn full_size_packets_cross_and_oversized_ones_report_the_size_that_fits() {
    let (a, b, a_vip, b_vip) = members(LocalPolicy::default()).await;
    let (a_fd, a_kernel) = helper_tun();
    let (b_fd, b_kernel) = helper_tun();
    assert_eq!(VnetConfig::default().mtu, muster_net::tls::VNET_MTU);
    let a_vnet = Vnet::start(
        a.clone(),
        VnetConfig {
            existing_fd: Some(a_fd),
            ..VnetConfig::default()
        },
    )
    .await
    .unwrap();
    let _b_vnet = Vnet::start(
        b.clone(),
        VnetConfig {
            existing_fd: Some(b_fd),
            ..VnetConfig::default()
        },
    )
    .await
    .unwrap();
    let frame = |packet: &[u8]| {
        let mut frame = (libc::AF_INET as u32).to_be_bytes().to_vec();
        frame.extend_from_slice(packet);
        frame
    };
    let mut buf = vec![0u8; 2048];

    // The first packet also brings the link up; resend until one arrives.
    let full = echo(a_vip, b_vip, usize::from(muster_net::tls::VNET_MTU));
    let arrived = tokio::time::timeout(Duration::from_secs(10), async {
        loop {
            a_kernel.send(&frame(&full)).await.unwrap();
            if let Ok(Ok(n)) =
                tokio::time::timeout(Duration::from_millis(500), b_kernel.recv(&mut buf)).await
            {
                return buf[4..n].to_vec();
            }
        }
    })
    .await
    .expect("a full-sized vnet packet crosses the link");
    assert_eq!(arrived, full);

    // Larger than any DATAGRAM the link carries (its MTU stays below 1,400).
    let oversized = echo(a_vip, b_vip, 1400);
    a_kernel.send(&frame(&oversized)).await.unwrap();
    let n = tokio::time::timeout(Duration::from_secs(5), a_kernel.recv(&mut buf))
        .await
        .expect("the sender is told")
        .unwrap();
    let reply = &buf[4..n];
    assert_eq!(reply[9], 1, "ICMP");
    assert_eq!(
        &reply[12..16],
        &b_vip.octets(),
        "from the unreachable destination"
    );
    assert_eq!(&reply[16..20], &a_vip.octets(), "to the sender");
    assert_eq!((reply[20], reply[21]), (3, 4), "fragmentation needed");
    let mtu = u16::from_be_bytes([reply[26], reply[27]]);
    assert!((muster_net::tls::VNET_MTU..1400).contains(&mtu), "{mtu}");
    assert_eq!(
        &reply[28..56],
        &oversized[..28],
        "quotes the header and 8 bytes"
    );
    assert_eq!(a_vnet.stats().sent_too_big, 1);
}
