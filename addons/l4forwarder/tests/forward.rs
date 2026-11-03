//! Forwarding over real loopback sockets: TCP and UDP round trips, UDP
//! sessions kept apart per client and ended when idle, a listener that waits
//! for its port, an upstream that comes and goes, and the TCP limit.

use std::net::SocketAddr;
use std::time::Duration;

use l4forwarder::{Options, Proto, Rule};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::{TcpListener, TcpStream, UdpSocket};

const PATIENCE: Duration = Duration::from_secs(5);

fn options() -> Options {
    Options {
        connect_timeout: Duration::from_secs(2),
        udp_idle: Duration::from_secs(60),
        max_tcp_connections: 16,
        max_udp_sessions: 16,
        rebind: Duration::from_millis(100),
    }
}

/// A loopback port nothing listens on right now.
fn free_port() -> u16 {
    std::net::TcpListener::bind("127.0.0.1:0")
        .unwrap()
        .local_addr()
        .unwrap()
        .port()
}

fn rule(proto: Proto, listen: SocketAddr, upstream: SocketAddr) -> Rule {
    Rule {
        proto,
        listen,
        upstream,
    }
}

/// Echoes every TCP connection on `listener` until the peer closes.
fn tcp_echo(listener: TcpListener) {
    tokio::spawn(async move {
        while let Ok((mut stream, _)) = listener.accept().await {
            tokio::spawn(async move {
                let (mut read, mut write) = stream.split();
                let _ = tokio::io::copy(&mut read, &mut write).await;
            });
        }
    });
}

/// Answers every datagram with the sender's address followed by the payload.
fn udp_echo(socket: UdpSocket) {
    tokio::spawn(async move {
        let mut buf = vec![0u8; 65_535];
        while let Ok((n, from)) = socket.recv_from(&mut buf).await {
            let reply = format!("{from}|{}", String::from_utf8_lossy(&buf[..n]));
            let _ = socket.send_to(reply.as_bytes(), from).await;
        }
    });
}

async fn connect(addr: SocketAddr) -> TcpStream {
    let started = std::time::Instant::now();
    loop {
        match TcpStream::connect(addr).await {
            Ok(stream) => return stream,
            Err(error) => {
                assert!(started.elapsed() < PATIENCE, "{addr}: {error}");
                tokio::time::sleep(Duration::from_millis(50)).await;
            }
        }
    }
}

async fn round_trip(stream: &mut TcpStream, text: &[u8]) -> Vec<u8> {
    stream.write_all(text).await.unwrap();
    let mut back = vec![0u8; text.len()];
    tokio::time::timeout(PATIENCE, stream.read_exact(&mut back))
        .await
        .expect("no echo")
        .unwrap();
    back
}

/// Sends `text` and returns the echo's (upstream-seen source, payload).
async fn ask(client: &UdpSocket, to: SocketAddr, text: &str) -> (String, String) {
    let mut buf = vec![0u8; 65_535];
    // The forwarder may still be binding; resend until it answers.
    let started = std::time::Instant::now();
    loop {
        client.send_to(text.as_bytes(), to).await.unwrap();
        if let Ok(Ok((n, _))) =
            tokio::time::timeout(Duration::from_millis(200), client.recv_from(&mut buf)).await
        {
            let reply = String::from_utf8_lossy(&buf[..n]).to_string();
            let (source, payload) = reply.split_once('|').unwrap();
            return (source.to_string(), payload.to_string());
        }
        assert!(started.elapsed() < PATIENCE, "no UDP echo");
    }
}

#[tokio::test]
async fn tcp_round_trips_and_closes_with_its_client() {
    let upstream = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let upstream_addr = upstream.local_addr().unwrap();
    tcp_echo(upstream);
    let listen: SocketAddr = format!("127.0.0.1:{}", free_port()).parse().unwrap();
    let task = l4forwarder::spawn(rule(Proto::Tcp, listen, upstream_addr), options());

    let mut stream = connect(listen).await;
    assert_eq!(round_trip(&mut stream, b"hello").await, b"hello");
    let large = vec![7u8; 1 << 20];
    assert_eq!(round_trip(&mut stream, &large).await, large);
    // Half-close travels through: the echo sees EOF and closes in turn.
    stream.shutdown().await.unwrap();
    let mut rest = Vec::new();
    tokio::time::timeout(PATIENCE, stream.read_to_end(&mut rest))
        .await
        .expect("never closed")
        .unwrap();
    assert!(rest.is_empty());
    task.abort();
}

#[tokio::test]
async fn udp_keeps_clients_apart_and_ends_idle_sessions() {
    let upstream = UdpSocket::bind("127.0.0.1:0").await.unwrap();
    let upstream_addr = upstream.local_addr().unwrap();
    udp_echo(upstream);
    let listen: SocketAddr = format!("127.0.0.1:{}", free_port()).parse().unwrap();
    let mut options = options();
    options.udp_idle = Duration::from_millis(500);
    let task = l4forwarder::spawn(rule(Proto::Udp, listen, upstream_addr), options);

    let a = UdpSocket::bind("127.0.0.1:0").await.unwrap();
    let b = UdpSocket::bind("127.0.0.1:0").await.unwrap();
    let (source_a, payload) = ask(&a, listen, "from a").await;
    assert_eq!(payload, "from a");
    let (source_b, payload) = ask(&b, listen, "from b").await;
    assert_eq!(payload, "from b");
    // Each client has its own upstream socket.
    assert_ne!(source_a, source_b);
    // Within the idle time the session stays.
    assert_eq!(ask(&a, listen, "again").await.0, source_a);
    // After it, the next datagram starts a new one.
    tokio::time::sleep(Duration::from_millis(900)).await;
    let (source_later, payload) = ask(&a, listen, "later").await;
    assert_eq!(payload, "later");
    assert_ne!(source_later, source_a);
    task.abort();
}

/// The address or port may not be available yet, as a vnet address is
/// before its interface comes up: the rule waits and then serves.
#[tokio::test]
async fn a_rule_waits_for_its_port() {
    let upstream = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let upstream_addr = upstream.local_addr().unwrap();
    tcp_echo(upstream);
    let squatter = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let listen = squatter.local_addr().unwrap();
    let task = l4forwarder::spawn(rule(Proto::Tcp, listen, upstream_addr), options());
    tokio::time::sleep(Duration::from_millis(300)).await;
    drop(squatter);

    let mut stream = connect(listen).await;
    let started = std::time::Instant::now();
    // A connection the squatter's backlog took may close unanswered; retry.
    while stream.write_all(b"x").await.is_err() || {
        let mut one = [0u8; 1];
        !matches!(
            tokio::time::timeout(Duration::from_millis(300), stream.read_exact(&mut one)).await,
            Ok(Ok(_))
        )
    } {
        assert!(started.elapsed() < PATIENCE, "never served");
        stream = connect(listen).await;
    }
    task.abort();
}

#[tokio::test]
async fn an_absent_upstream_closes_the_client_and_is_found_when_back() {
    let upstream_addr: SocketAddr = format!("127.0.0.1:{}", free_port()).parse().unwrap();
    let listen: SocketAddr = format!("127.0.0.1:{}", free_port()).parse().unwrap();
    let task = l4forwarder::spawn(rule(Proto::Tcp, listen, upstream_addr), options());

    let mut stream = connect(listen).await;
    let mut buf = [0u8; 1];
    let read = tokio::time::timeout(PATIENCE, stream.read(&mut buf))
        .await
        .expect("left hanging");
    assert!(matches!(read, Ok(0) | Err(_)));

    tcp_echo(TcpListener::bind(upstream_addr).await.unwrap());
    let mut stream = connect(listen).await;
    assert_eq!(round_trip(&mut stream, b"back").await, b"back");
    task.abort();
}

#[tokio::test]
async fn a_full_rule_serves_the_next_client_when_one_leaves() {
    let upstream = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let upstream_addr = upstream.local_addr().unwrap();
    tcp_echo(upstream);
    let listen: SocketAddr = format!("127.0.0.1:{}", free_port()).parse().unwrap();
    let mut options = options();
    options.max_tcp_connections = 1;
    let task = l4forwarder::spawn(rule(Proto::Tcp, listen, upstream_addr), options);

    let mut first = connect(listen).await;
    assert_eq!(round_trip(&mut first, b"one").await, b"one");
    let mut second = connect(listen).await;
    second.write_all(b"two").await.unwrap();
    let mut back = [0u8; 3];
    assert!(
        tokio::time::timeout(Duration::from_millis(300), second.read_exact(&mut back))
            .await
            .is_err(),
        "served beyond the limit"
    );
    drop(first);
    tokio::time::timeout(PATIENCE, second.read_exact(&mut back))
        .await
        .expect("never served")
        .unwrap();
    assert_eq!(&back, b"two");
    task.abort();
}
