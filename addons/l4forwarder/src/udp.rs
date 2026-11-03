//! UDP: each client address gets its own upstream socket, so the upstream's
//! replies find their way back to the right client. A session ends after
//! `udp_idle` without traffic either way.

use std::collections::HashMap;
use std::net::{Ipv4Addr, Ipv6Addr, SocketAddr};
use std::sync::{Arc, Mutex};
use std::time::Instant;

use tokio::net::UdpSocket;

use crate::config::{Options, Rule};

/// The largest datagram UDP can carry.
const MAX_DATAGRAM: usize = 65_535;

struct Session {
    upstream: Arc<UdpSocket>,
    last: Arc<Mutex<Instant>>,
}

type Sessions = Arc<Mutex<HashMap<SocketAddr, Session>>>;

pub async fn serve(rule: Rule, options: Options) {
    let listen =
        Arc::new(crate::bind_until(&rule, options.rebind, || UdpSocket::bind(rule.listen)).await);
    let sessions: Sessions = Arc::default();
    let mut buf = vec![0u8; MAX_DATAGRAM];
    let mut refused = false;
    loop {
        let (n, client) = match listen.recv_from(&mut buf).await {
            Ok(received) => received,
            Err(error) => {
                log::warn!("{rule}: receive: {error}");
                tokio::time::sleep(std::time::Duration::from_millis(100)).await;
                continue;
            }
        };
        let existing = sessions
            .lock()
            .unwrap()
            .get(&client)
            .map(|s| (s.upstream.clone(), s.last.clone()));
        let (upstream, last) = match existing {
            Some(session) => session,
            None => {
                if sessions.lock().unwrap().len() >= options.max_udp_sessions {
                    if !refused {
                        log::warn!(
                            "{rule}: {} sessions open; dropping datagrams from new clients",
                            options.max_udp_sessions
                        );
                        refused = true;
                    }
                    continue;
                }
                refused = false;
                match open(&rule, &options, &listen, &sessions, client).await {
                    Ok(session) => session,
                    Err(error) => {
                        log::info!("{rule}: from {client}: upstream: {error}");
                        continue;
                    }
                }
            }
        };
        *last.lock().unwrap() = Instant::now();
        if let Err(error) = upstream.send(&buf[..n]).await {
            log::debug!("{rule}: from {client}: send upstream: {error}");
        }
    }
}

/// Opens `client`'s upstream socket and starts carrying its replies back.
async fn open(
    rule: &Rule,
    options: &Options,
    listen: &Arc<UdpSocket>,
    sessions: &Sessions,
    client: SocketAddr,
) -> std::io::Result<(Arc<UdpSocket>, Arc<Mutex<Instant>>)> {
    let any: SocketAddr = match rule.upstream {
        SocketAddr::V4(_) => (Ipv4Addr::UNSPECIFIED, 0).into(),
        SocketAddr::V6(_) => (Ipv6Addr::UNSPECIFIED, 0).into(),
    };
    let upstream = UdpSocket::bind(any).await?;
    upstream.connect(rule.upstream).await?;
    let upstream = Arc::new(upstream);
    let last = Arc::new(Mutex::new(Instant::now()));
    sessions.lock().unwrap().insert(
        client,
        Session {
            upstream: upstream.clone(),
            last: last.clone(),
        },
    );
    log::debug!("{rule}: session for {client}");
    tokio::spawn(replies(
        rule.clone(),
        options.clone(),
        listen.clone(),
        sessions.clone(),
        client,
        upstream.clone(),
        last.clone(),
    ));
    Ok((upstream, last))
}

async fn replies(
    rule: Rule,
    options: Options,
    listen: Arc<UdpSocket>,
    sessions: Sessions,
    client: SocketAddr,
    upstream: Arc<UdpSocket>,
    last: Arc<Mutex<Instant>>,
) {
    let mut buf = vec![0u8; MAX_DATAGRAM];
    loop {
        let deadline = *last.lock().unwrap() + options.udp_idle;
        tokio::select! {
            received = upstream.recv(&mut buf) => match received {
                Ok(n) => {
                    *last.lock().unwrap() = Instant::now();
                    if let Err(error) = listen.send_to(&buf[..n], client).await {
                        log::debug!("{rule}: to {client}: {error}");
                    }
                }
                // An earlier datagram met a closed port; the upstream may
                // come back, so the session stays.
                Err(error) if error.kind() == std::io::ErrorKind::ConnectionRefused => {}
                Err(error) => {
                    log::info!("{rule}: from {client}: receive upstream: {error}");
                    break;
                }
            },
            _ = tokio::time::sleep_until(deadline.into()) => {
                if last.lock().unwrap().elapsed() >= options.udp_idle {
                    break;
                }
            }
        }
    }
    let mut sessions = sessions.lock().unwrap();
    if sessions
        .get(&client)
        .is_some_and(|s| Arc::ptr_eq(&s.upstream, &upstream))
    {
        sessions.remove(&client);
    }
    log::debug!("{rule}: session for {client} ended");
}
