//! The vnet (ARCHITECTURE §3.5): IPv4 packets from a TUN ride QUIC DATAGRAMs
//! to the member that owns the destination vip. Inbound packets must come
//! from the peer's own vip and, for a new flow, pass the same port
//! authorization as forward streams; replies to our own flows and ICMP are
//! let through. Desktop and server only.

use std::collections::HashMap;
use std::net::Ipv4Addr;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use bytes::Bytes;
use muster_core::DeviceId;
use muster_core::policy::{Decision, authorize_port};
use muster_core::scope::Proto;
use tokio::io::{AsyncReadExt, AsyncWriteExt};

use crate::Error;
use crate::node::Node;

const FLOW_TTL: Duration = Duration::from_secs(120);

#[derive(Clone, Debug)]
pub struct VnetConfig {
    pub mtu: u16,
    /// A TUN the platform already opened (macOS: from the NetHelper over XPC).
    /// start() consumes this descriptor, including when initialization fails.
    pub existing_fd: Option<i32>,
    /// Interface name to request when creating (Linux only).
    pub name: Option<String>,
}

impl Default for VnetConfig {
    fn default() -> Self {
        Self {
            mtu: crate::tls::VNET_MTU,
            existing_fd: None,
            name: None,
        }
    }
}

fn interface_config(address: Ipv4Addr, prefix: u8, config: &VnetConfig) -> tun::Configuration {
    let mut tun_config = tun::Configuration::default();
    tun_config
        .address(address)
        .netmask(Ipv4Addr::from(!0u32 << (32 - u32::from(prefix))))
        .mtu(config.mtu)
        .up();
    if let Some(name) = &config.name {
        tun_config.tun_name(name);
    }
    if let Some(fd) = config.existing_fd {
        tun_config.raw_fd(fd);
    }
    #[cfg(target_os = "macos")]
    tun_config.platform_config(|p| {
        // utun always carries a four-byte address-family header. This tells
        // tun to strip it on reads and prepend it on writes, including for
        // descriptors received from the privileged helper.
        p.packet_information(true);
    });
    tun_config
}

#[derive(Clone, Copy, Debug, Default)]
pub struct VnetStats {
    pub tx_packets: u64,
    pub rx_packets: u64,
    pub dropped_unknown_dst: u64,
    pub dropped_too_large: u64,
    /// "Fragmentation needed" replies for those, so the sender shrinks.
    pub sent_too_big: u64,
    pub dropped_spoofed: u64,
    pub dropped_unauthorized: u64,
    pub dropped_no_link: u64,
}

#[derive(Clone, Copy, PartialEq, Eq, Hash)]
struct FlowKey {
    proto: u8,
    local: (Ipv4Addr, u16),
    remote: (Ipv4Addr, u16),
}

#[derive(Default)]
struct Flows {
    outbound: HashMap<FlowKey, Instant>,
    inbound: HashMap<FlowKey, Instant>,
}

/// The header fields the filter looks at.
#[derive(Clone, Copy, Debug)]
pub struct Parsed {
    pub proto: u8,
    pub src: Ipv4Addr,
    pub dst: Ipv4Addr,
    pub sport: u16,
    pub dport: u16,
    pub tcp_syn_only: bool,
}

/// ICMP "fragmentation needed" (type 3, code 4) for an IPv4 packet the link
/// cannot carry, telling the local sender's stack to use packets of at most
/// `mtu` bytes. It comes from the destination the packet could not reach, as
/// a router's would. None when the sender allowed fragmentation (DF clear),
/// for a later fragment, or for an ICMP error, which never earns another.
pub fn fragmentation_needed(packet: &[u8], mtu: u16) -> Option<Vec<u8>> {
    if packet.len() < 20 || packet[0] >> 4 != 4 || mtu < 68 {
        return None;
    }
    let ihl = usize::from(packet[0] & 0x0f) * 4;
    if ihl < 20 || packet.len() < ihl {
        return None;
    }
    let flags_offset = u16::from_be_bytes([packet[6], packet[7]]);
    if flags_offset & 0x4000 == 0 || flags_offset & 0x1fff != 0 {
        return None;
    }
    if packet[9] == 1
        && packet
            .get(ihl)
            .is_some_and(|t| matches!(t, 3 | 4 | 5 | 11 | 12))
    {
        return None;
    }
    // The original header and the first eight bytes of its payload.
    let quote = &packet[..packet.len().min(ihl + 8)];
    let mut icmp = vec![3, 4, 0, 0, 0, 0];
    icmp.extend_from_slice(&mtu.to_be_bytes());
    icmp.extend_from_slice(quote);
    let sum = checksum(&icmp);
    icmp[2..4].copy_from_slice(&sum.to_be_bytes());
    let total = u16::try_from(20 + icmp.len()).ok()?;
    let mut reply = vec![0x45, 0];
    reply.extend_from_slice(&total.to_be_bytes());
    reply.extend_from_slice(&[0, 0, 0, 0, 64, 1, 0, 0]);
    reply.extend_from_slice(&packet[16..20]);
    reply.extend_from_slice(&packet[12..16]);
    let sum = checksum(&reply);
    reply[10..12].copy_from_slice(&sum.to_be_bytes());
    reply.extend_from_slice(&icmp);
    Some(reply)
}

/// The Internet checksum (RFC 1071).
fn checksum(bytes: &[u8]) -> u16 {
    let mut sum: u32 = bytes
        .chunks(2)
        .map(|pair| u32::from(u16::from_be_bytes([pair[0], *pair.get(1).unwrap_or(&0)])))
        .sum();
    while sum >> 16 != 0 {
        sum = (sum & 0xffff) + (sum >> 16);
    }
    !(sum as u16)
}

pub fn parse(packet: &[u8]) -> Option<Parsed> {
    if packet.len() < 20 || packet[0] >> 4 != 4 {
        return None;
    }
    let ihl = usize::from(packet[0] & 0x0f) * 4;
    if ihl < 20 || packet.len() < ihl {
        return None;
    }
    let proto = packet[9];
    let src = Ipv4Addr::new(packet[12], packet[13], packet[14], packet[15]);
    let dst = Ipv4Addr::new(packet[16], packet[17], packet[18], packet[19]);
    let (sport, dport, tcp_syn_only) = match proto {
        6 | 17 if packet.len() >= ihl + 4 => {
            let sport = u16::from_be_bytes([packet[ihl], packet[ihl + 1]]);
            let dport = u16::from_be_bytes([packet[ihl + 2], packet[ihl + 3]]);
            let syn_only =
                proto == 6 && packet.len() >= ihl + 14 && packet[ihl + 13] & 0x12 == 0x02;
            (sport, dport, syn_only)
        }
        _ => (0, 0, false),
    };
    Some(Parsed {
        proto,
        src,
        dst,
        sport,
        dport,
        tcp_syn_only,
    })
}

/// The inbound packet filter, separate from the TUN so it can be tested
/// without one: replies to our flows, established inbound flows, ICMP, and
/// new inbound flows the roster and policy allow.
#[derive(Default)]
pub struct Filter {
    flows: Mutex<Flows>,
}

impl Filter {
    pub fn note_outbound(&self, parsed: &Parsed) {
        if parsed.proto == 6 || parsed.proto == 17 {
            self.flows.lock().unwrap().outbound.insert(
                FlowKey {
                    proto: parsed.proto,
                    local: (parsed.src, parsed.sport),
                    remote: (parsed.dst, parsed.dport),
                },
                Instant::now(),
            );
        }
    }

    pub fn expire(&self) {
        let mut flows = self.flows.lock().unwrap();
        let now = Instant::now();
        flows
            .outbound
            .retain(|_, t| now.duration_since(*t) < FLOW_TTL);
        flows
            .inbound
            .retain(|_, t| now.duration_since(*t) < FLOW_TTL);
    }

    pub fn admit(
        &self,
        state: &muster_core::roster::State,
        policy: &muster_core::LocalPolicy,
        peer: &DeviceId,
        parsed: &Parsed,
    ) -> bool {
        match parsed.proto {
            1 => return true,
            6 | 17 => {}
            _ => return false,
        }
        let now = Instant::now();
        let mut flows = self.flows.lock().unwrap();
        let reply_key = FlowKey {
            proto: parsed.proto,
            local: (parsed.dst, parsed.dport),
            remote: (parsed.src, parsed.sport),
        };
        if let Some(seen) = flows.outbound.get_mut(&reply_key) {
            *seen = now;
            return true;
        }
        if let Some(seen) = flows.inbound.get_mut(&reply_key)
            && !parsed.tcp_syn_only
        {
            *seen = now;
            return true;
        }
        let proto = if parsed.proto == 6 {
            Proto::Tcp
        } else {
            Proto::Udp
        };
        if authorize_port(state, peer, policy, proto, parsed.dport) == Decision::Allow {
            flows.inbound.insert(reply_key, now);
            true
        } else {
            false
        }
    }
}

pub struct Vnet {
    node: Arc<Node>,
    stats: Mutex<VnetStats>,
    filter: Filter,
    address: Ipv4Addr,
    cidr: (Ipv4Addr, u8),
    tasks: Mutex<Vec<tokio::task::JoinHandle<()>>>,
    stop: tokio::sync::Mutex<()>,
    #[cfg(unix)]
    helper_fd: Mutex<Option<std::os::fd::OwnedFd>>,
    /// Packets this device answers itself (ICMP), written to the TUN.
    replies: tokio::sync::mpsc::Sender<Bytes>,
}

impl Vnet {
    /// Brings the interface up with this device's vip and starts both
    /// directions. Fails when this device has no vip (phones) or no network.
    pub async fn start(node: Arc<Node>, config: VnetConfig) -> Result<Arc<Self>, Error> {
        // Take ownership before any fallible validation. tun borrows this fd;
        // the Vnet owner closes it only after both packet tasks have stopped.
        #[cfg(unix)]
        let helper_fd = config.existing_fd.map(|fd| {
            use std::os::fd::FromRawFd;
            // SAFETY: existing_fd transfers ownership to start().
            unsafe { std::os::fd::OwnedFd::from_raw_fd(fd) }
        });
        let (address, cidr) = {
            let roster = node.roster().read().unwrap();
            let state = roster.state();
            let me = state
                .active(&node.device_id())
                .ok_or_else(|| Error::Protocol("this device is not an active member".into()))?;
            let vip = me
                .record
                .vip
                .ok_or_else(|| Error::Protocol("this device has no vip".into()))?;
            let cidr = state
                .vnet_cidr
                .ok_or_else(|| Error::Protocol("no vnet range".into()))?;
            (vip, cidr)
        };
        #[allow(unused_mut)]
        let mut tun_config = interface_config(address, cidr.1, &config);
        #[cfg(unix)]
        if helper_fd.is_some() {
            tun_config.close_fd_on_drop(false);
        }
        let device =
            tun::create_as_async(&tun_config).map_err(|e| Error::Io(format!("tun: {e}")))?;
        log::info!("vnet up: {address}/{} mtu {}", cidr.1, config.mtu);
        let (writer, reader) = device
            .split()
            .map_err(|e| Error::Io(format!("tun split: {e}")))?;
        let (replies, local) = tokio::sync::mpsc::channel(64);
        let vnet = Arc::new(Self {
            node: node.clone(),
            stats: Mutex::default(),
            filter: Filter::default(),
            address,
            cidr,
            tasks: Mutex::default(),
            stop: tokio::sync::Mutex::new(()),
            #[cfg(unix)]
            helper_fd: Mutex::new(helper_fd),
            replies,
        });
        let inbound = node
            .take_datagrams()
            .ok_or_else(|| Error::Protocol("node was started without datagrams".into()))?;
        let outbound = tokio::spawn(vnet.clone().outbound_loop(reader));
        let inbound = tokio::spawn(vnet.clone().inbound_loop(inbound, local, writer));
        *vnet.tasks.lock().unwrap() = vec![outbound, inbound];
        Ok(vnet)
    }

    pub fn stats(&self) -> VnetStats {
        *self.stats.lock().unwrap()
    }

    pub fn address(&self) -> Ipv4Addr {
        self.address
    }

    pub fn cidr(&self) -> (Ipv4Addr, u8) {
        self.cidr
    }

    /// Retire both packet loops. Transport replacement keeps this Vnet alive;
    /// only final shutdown/address removal retires its single datagram reader.
    pub fn shutdown(&self) {
        for task in self.tasks.lock().unwrap().iter() {
            task.abort();
        }
    }

    /// Wait for both TUN halves and the datagram receiver to be dropped before
    /// the platform releases its helper connection. Safe after shutdown().
    pub async fn shutdown_and_wait(&self) {
        let _stop = self.stop.lock().await;
        self.shutdown();
        let tasks: Vec<_> = self.tasks.lock().unwrap().drain(..).collect();
        for task in tasks {
            let _ = task.await;
        }
        #[cfg(unix)]
        self.helper_fd.lock().unwrap().take();
    }

    async fn outbound_loop(self: Arc<Self>, mut reader: tun::DeviceReader) {
        let mut buf = vec![0u8; 65536];
        loop {
            let n = match reader.read(&mut buf).await {
                Ok(0) => return,
                Ok(n) => n,
                Err(error) => {
                    log::error!("tun read: {error}");
                    return;
                }
            };
            let packet = &buf[..n];
            let Some(parsed) = parse(packet) else {
                continue;
            };
            let target = self
                .node
                .roster()
                .read()
                .unwrap()
                .state()
                .member_by_vip(parsed.dst)
                .map(|m| m.record.device_id);
            let Some(target) = target else {
                self.stats.lock().unwrap().dropped_unknown_dst += 1;
                continue;
            };
            if target == self.node.device_id() {
                continue;
            }
            self.filter.note_outbound(&parsed);
            let connection = match self.node.connect(target).await {
                Ok(connection) => connection,
                Err(error) => {
                    log::debug!("no link to {} for {}: {error}", target.short(), parsed.dst);
                    self.stats.lock().unwrap().dropped_no_link += 1;
                    continue;
                }
            };
            match connection.send_datagram(Bytes::copy_from_slice(packet)) {
                Ok(()) => self.stats.lock().unwrap().tx_packets += 1,
                Err(quinn::SendDatagramError::TooLarge) => {
                    // A sender that forbade fragmentation learns the size
                    // that fits instead of retrying into silence.
                    let capacity = connection.max_datagram_size().unwrap_or(0);
                    let reply = u16::try_from(capacity)
                        .ok()
                        .and_then(|mtu| fragmentation_needed(packet, mtu));
                    log::debug!(
                        "{n}-byte packet for {} exceeds the link's {capacity}-byte datagrams",
                        parsed.dst
                    );
                    let mut stats = self.stats.lock().unwrap();
                    stats.dropped_too_large += 1;
                    if let Some(reply) = reply
                        && self.replies.try_send(Bytes::from(reply)).is_ok()
                    {
                        stats.sent_too_big += 1;
                    }
                }
                Err(error) => log::debug!("send_datagram to {}: {error}", target.short()),
            }
        }
    }

    async fn inbound_loop(
        self: Arc<Self>,
        mut inbound: tokio::sync::mpsc::Receiver<(DeviceId, Bytes)>,
        mut local: tokio::sync::mpsc::Receiver<Bytes>,
        mut writer: tun::DeviceWriter,
    ) {
        let mut last_expiry = Instant::now();
        loop {
            let (peer, packet) = tokio::select! {
                received = inbound.recv() => match received {
                    Some(received) => received,
                    None => return,
                },
                Some(reply) = local.recv() => {
                    if let Err(error) = writer.write_all(&reply).await {
                        log::warn!("tun write: {error}");
                    }
                    continue;
                }
            };
            if last_expiry.elapsed() > FLOW_TTL {
                self.filter.expire();
                last_expiry = Instant::now();
            }
            let Some(parsed) = parse(&packet) else {
                continue;
            };
            let expected_src = self
                .node
                .roster()
                .read()
                .unwrap()
                .state()
                .active(&peer)
                .and_then(|m| m.record.vip);
            if expected_src != Some(parsed.src) {
                self.stats.lock().unwrap().dropped_spoofed += 1;
                continue;
            }
            if parsed.dst != self.address {
                self.stats.lock().unwrap().dropped_unknown_dst += 1;
                continue;
            }
            let admitted = {
                let roster = self.node.roster().read().unwrap();
                let policy = self.node.policy().read().unwrap();
                self.filter.admit(roster.state(), &policy, &peer, &parsed)
            };
            if !admitted {
                self.stats.lock().unwrap().dropped_unauthorized += 1;
                continue;
            }
            match writer.write_all(&packet).await {
                Ok(()) => self.stats.lock().unwrap().rx_packets += 1,
                Err(error) => log::warn!("tun write: {error}"),
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use muster_core::roster::{Author, DeviceKind, DeviceRecord, Roster};
    use muster_core::{HwSigner, LocalPolicy, PortRule, Scope, SoftSigner};

    #[cfg(target_os = "macos")]
    #[tokio::test]
    async fn helper_descriptor_adapts_utun_headers_in_both_directions() {
        use std::os::fd::IntoRawFd;
        use std::os::unix::net::UnixDatagram;

        let (device, kernel) = UnixDatagram::pair().unwrap();
        kernel.set_nonblocking(true).unwrap();
        let kernel = tokio::net::UnixDatagram::from_std(kernel).unwrap();
        let config = interface_config(
            Ipv4Addr::new(192, 168, 13, 11),
            24,
            &VnetConfig {
                existing_fd: Some(device.into_raw_fd()),
                ..Default::default()
            },
        );
        let (mut writer, mut reader) = tun::create_as_async(&config).unwrap().split().unwrap();
        let packet = ipv4(
            6,
            Ipv4Addr::new(192, 168, 13, 11),
            Ipv4Addr::new(192, 168, 13, 12),
            40000,
            22,
            2,
        );
        let mut frame = (libc::AF_INET as u32).to_be_bytes().to_vec();
        frame.extend_from_slice(&packet);
        kernel.send(&frame).await.unwrap();
        let mut buf = [0u8; 1500];
        let n = tokio::time::timeout(Duration::from_secs(1), reader.read(&mut buf))
            .await
            .unwrap()
            .unwrap();
        assert_eq!(&buf[..n], packet.as_slice());
        writer.write_all(&packet).await.unwrap();
        let n = tokio::time::timeout(Duration::from_secs(1), kernel.recv(&mut buf))
            .await
            .unwrap()
            .unwrap();
        assert_eq!(&buf[..n], frame.as_slice());
    }

    #[test]
    fn fragmentation_needed_only_for_whole_packets_that_forbid_fragmenting() {
        let (local, remote) = (
            Ipv4Addr::new(192, 168, 13, 13),
            Ipv4Addr::new(192, 168, 13, 12),
        );
        let mut packet = ipv4(6, local, remote, 52000, 22, 0x10);
        packet.resize(1400, 0xab);
        packet[2..4].copy_from_slice(&1400u16.to_be_bytes());
        // May be fragmented (DF clear): no reply.
        assert!(fragmentation_needed(&packet, 1162).is_none());
        packet[6] = 0x40;
        let reply = fragmentation_needed(&packet, 1162).unwrap();
        assert_eq!(reply.len(), 20 + 8 + 28);
        assert_eq!(
            u16::from_be_bytes([reply[2], reply[3]]) as usize,
            reply.len()
        );
        assert_eq!((reply[0], reply[9]), (0x45, 1));
        assert_eq!(&reply[12..16], &remote.octets());
        assert_eq!(&reply[16..20], &local.octets());
        assert_eq!(checksum(&reply[..20]), 0, "IP header checksum");
        assert_eq!(checksum(&reply[20..]), 0, "ICMP checksum");
        assert_eq!((reply[20], reply[21]), (3, 4));
        assert_eq!(u16::from_be_bytes([reply[26], reply[27]]), 1162);
        assert_eq!(&reply[28..], &packet[..28]);
        assert!(parse(&reply).is_some_and(|p| p.proto == 1 && p.dst == local));
        // A later fragment, an ICMP error, or a nonsense size: no reply.
        let mut later = packet.clone();
        later[7] = 0x10;
        assert!(fragmentation_needed(&later, 1162).is_none());
        assert!(fragmentation_needed(&reply, 1162).is_none());
        assert!(fragmentation_needed(&packet, 40).is_none());
        // An echo request with DF may earn one.
        let mut echo = packet.clone();
        echo[9] = 1;
        echo[20] = 8;
        assert!(fragmentation_needed(&echo, 1162).is_some());
    }

    fn ipv4(proto: u8, src: Ipv4Addr, dst: Ipv4Addr, sport: u16, dport: u16, flags: u8) -> Vec<u8> {
        let mut p = vec![0u8; 40];
        p[0] = 0x45;
        p[9] = proto;
        p[12..16].copy_from_slice(&src.octets());
        p[16..20].copy_from_slice(&dst.octets());
        p[20..22].copy_from_slice(&sport.to_be_bytes());
        p[22..24].copy_from_slice(&dport.to_be_bytes());
        p[33] = flags;
        p
    }

    #[test]
    fn filter_decides_by_scope_policy_and_flow_state() {
        let admin = SoftSigner::generate();
        let peer = SoftSigner::generate();
        let mut roster = Roster::new();
        let genesis = Author {
            roster: &roster,
            signer: &admin,
        }
        .genesis(
            "lab",
            (Ipv4Addr::new(192, 168, 14, 0), 24),
            DeviceRecord::new(admin.public_key(), "a", DeviceKind::Desktop),
        )
        .unwrap();
        roster.insert(genesis).unwrap();
        let mut record = DeviceRecord::new(peer.public_key(), "b", DeviceKind::Server);
        record.scopes.insert("net:tcp/22".parse::<Scope>().unwrap());
        let add = Author {
            roster: &roster,
            signer: &admin,
        }
        .add(record)
        .unwrap();
        roster.insert(add).unwrap();
        let peer_id = peer.public_key().device_id();
        let a = Ipv4Addr::new(192, 168, 14, 1);
        let b = Ipv4Addr::new(192, 168, 14, 2);
        let policy = LocalPolicy {
            open_ports: vec![
                PortRule {
                    proto: Proto::Tcp,
                    port: Some(22),
                },
                PortRule {
                    proto: Proto::Tcp,
                    port: Some(80),
                },
            ],
            ..LocalPolicy::default()
        };
        let filter = Filter::default();
        let state = roster.state();

        let syn = |dport: u16| parse(&ipv4(6, b, a, 40000, dport, 0x02)).unwrap();
        let ack = |dport: u16| parse(&ipv4(6, b, a, 40000, dport, 0x10)).unwrap();
        // Granted and open: a new inbound SSH flow is admitted, then its follow-ups.
        assert!(filter.admit(state, &policy, &peer_id, &syn(22)));
        assert!(filter.admit(state, &policy, &peer_id, &ack(22)));
        // Open locally but not granted: dropped, and a stray ACK does not sneak in.
        assert!(!filter.admit(state, &policy, &peer_id, &syn(80)));
        assert!(!filter.admit(state, &policy, &peer_id, &ack(80)));
        // Granted but not open locally.
        let closed = LocalPolicy {
            open_ports: vec![],
            ..LocalPolicy::default()
        };
        assert!(!Filter::default().admit(state, &closed, &peer_id, &syn(22)));
        // Replies to our own outbound flow are admitted regardless of grants.
        let outbound = parse(&ipv4(6, a, b, 51000, 5900, 0x02)).unwrap();
        filter.note_outbound(&outbound);
        let reply = parse(&ipv4(6, b, a, 5900, 51000, 0x12)).unwrap();
        assert!(filter.admit(state, &policy, &peer_id, &reply));
        // ICMP passes; anything else does not.
        assert!(filter.admit(
            state,
            &policy,
            &peer_id,
            &parse(&ipv4(1, b, a, 0, 0, 0)).unwrap()
        ));
        assert!(!filter.admit(
            state,
            &policy,
            &peer_id,
            &parse(&ipv4(47, b, a, 0, 0, 0)).unwrap()
        ));
        // A non-member gets nothing.
        let stranger = SoftSigner::generate().public_key().device_id();
        assert!(!Filter::default().admit(state, &policy, &stranger, &syn(22)));
    }

    #[test]
    fn parse_rejects_junk() {
        assert!(parse(&[0x45; 10]).is_none());
        assert!(parse(&[0x60; 40]).is_none());
        let p = parse(&ipv4(17, Ipv4Addr::LOCALHOST, Ipv4Addr::LOCALHOST, 1, 2, 0)).unwrap();
        assert_eq!((p.proto, p.sport, p.dport), (17, 1, 2));
    }
}
