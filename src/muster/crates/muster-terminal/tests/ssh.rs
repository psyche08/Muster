//! The SSH client against a russh server over an in-memory pipe: the
//! device key authenticates through the HwSigner callback, a PTY shell
//! opens, bytes flow both ways, and the grid renders what came back.

use std::sync::Arc;
use std::time::Duration;

use muster_core::{HwSigner, SoftSigner};
use muster_terminal::grid::Grid;
use muster_terminal::ssh::{HostVerifier, SshConfig, SshEvent, connect, ssh_public_key};

/// Trusts every host key: only the loopback test server is ever dialled.
struct AcceptAny;
impl HostVerifier for AcceptAny {
    fn accept(&self, _: &str, _: &str) -> bool {
        true
    }
}
use russh::server::{Auth, Handler, Msg, Session};
use russh::{Channel, ChannelId};

struct EchoServer {
    allowed: russh::keys::PublicKey,
}

impl Handler for EchoServer {
    type Error = russh::Error;

    async fn auth_publickey(
        &mut self,
        user: &str,
        public_key: &russh::keys::PublicKey,
    ) -> Result<Auth, Self::Error> {
        Ok(
            if user == "phone" && public_key.key_data() == self.allowed.key_data() {
                Auth::Accept
            } else {
                Auth::reject()
            },
        )
    }

    async fn channel_open_session(
        &mut self,
        _channel: Channel<Msg>,
        reply: russh::server::ChannelOpenHandle,
        _session: &mut Session,
    ) -> Result<(), Self::Error> {
        reply.accept().await;
        Ok(())
    }

    #[allow(clippy::too_many_arguments)]
    async fn pty_request(
        &mut self,
        channel: ChannelId,
        _term: &str,
        _col_width: u32,
        _row_height: u32,
        _pix_width: u32,
        _pix_height: u32,
        _modes: &[(russh::Pty, u32)],
        session: &mut Session,
    ) -> Result<(), Self::Error> {
        session.channel_success(channel)?;
        Ok(())
    }

    async fn shell_request(
        &mut self,
        channel: ChannelId,
        session: &mut Session,
    ) -> Result<(), Self::Error> {
        session.channel_success(channel)?;
        session.data(channel, &b"\x1b[1mwelcome\x1b[0m\r\n$ "[..])?;
        Ok(())
    }

    async fn data(
        &mut self,
        channel: ChannelId,
        data: &[u8],
        session: &mut Session,
    ) -> Result<(), Self::Error> {
        if data == b"exit\r" {
            session.data(channel, &b"\r\nbye\r\n"[..])?;
            session.exit_status_request(channel, 0)?;
            session.close(channel)?;
        } else {
            session.data(channel, data.to_vec())?;
        }
        Ok(())
    }
}

#[tokio::test]
async fn hardware_key_auth_shell_and_grid() {
    let _ = env_logger::Builder::from_env(env_logger::Env::default().default_filter_or("debug"))
        .is_test(true)
        .try_init();
    let signer: Arc<dyn HwSigner> = Arc::new(SoftSigner::generate());
    let other: Arc<dyn HwSigner> = Arc::new(SoftSigner::generate());
    let allowed = ssh_public_key(&signer.public_key()).unwrap();

    let seed: [u8; 32] = rand::random();
    let host_key = russh::keys::PrivateKey::new(
        russh::keys::ssh_key::private::KeypairData::Ed25519(
            russh::keys::ssh_key::private::Ed25519Keypair::from_seed(&seed),
        ),
        "host",
    )
    .unwrap();
    let server_config = Arc::new(russh::server::Config {
        keys: vec![host_key],
        auth_rejection_time: Duration::from_millis(10),
        auth_rejection_time_initial: Some(Duration::from_millis(0)),
        ..Default::default()
    });

    // The wrong key is refused.
    let (client_side, server_side) = tokio::io::duplex(65536);
    let cfg = server_config.clone();
    let allowed_copy = allowed.clone();
    tokio::spawn(async move {
        let _ = russh::server::run_stream(
            cfg,
            server_side,
            EchoServer {
                allowed: allowed_copy,
            },
        )
        .await;
    });
    let config = SshConfig {
        user: "phone".into(),
        cols: 20,
        rows: 4,
        ..SshConfig::default()
    };
    let refused = connect(client_side, &config, other, Arc::new(AcceptAny)).await;
    assert!(refused.is_err(), "wrong key must be refused");

    // The device key gets a shell.
    let (client_side, server_side) = tokio::io::duplex(65536);
    let cfg = server_config.clone();
    tokio::spawn(async move {
        let running = russh::server::run_stream(cfg, server_side, EchoServer { allowed })
            .await
            .unwrap();
        let _ = running.await;
    });
    let mut session = tokio::time::timeout(
        Duration::from_secs(10),
        connect(client_side, &config, signer, Arc::new(AcceptAny)),
    )
    .await
    .expect("connect in time")
    .expect("connect");
    assert_eq!(session.host_key_type, "ssh-ed25519");
    assert!(session.host_fingerprint.starts_with("SHA256:"));
    let mut grid = Grid::new(20, 4);
    loop {
        match tokio::time::timeout(Duration::from_secs(5), session.next())
            .await
            .expect("event in time")
        {
            Some(SshEvent::Output(bytes)) => {
                grid.feed(&bytes);
                if grid.row_text(1).starts_with('$') {
                    break;
                }
            }
            other => panic!("unexpected {other:?}"),
        }
    }
    assert_eq!(grid.row_text(0), "welcome");
    let frame = grid.frame("ssh").unwrap();
    assert!(frame.full);
    assert!(
        !frame.lines[0].styles.is_empty(),
        "bold welcome carries a style run"
    );

    session.write(b"echo hi").await.unwrap();
    loop {
        match tokio::time::timeout(Duration::from_secs(5), session.next())
            .await
            .unwrap()
        {
            Some(SshEvent::Output(bytes)) => {
                grid.feed(&bytes);
                if grid.row_text(1) == "$ echo hi" {
                    break;
                }
            }
            other => panic!("unexpected {other:?}"),
        }
    }
    session.resize(30, 5).await.unwrap();
    session.write(b"exit\r").await.unwrap();
    let mut exit = None;
    loop {
        match tokio::time::timeout(Duration::from_secs(5), session.next())
            .await
            .unwrap()
        {
            Some(SshEvent::Output(bytes)) => {
                grid.feed(&bytes);
            }
            Some(SshEvent::Exit(code)) => exit = Some(code),
            Some(SshEvent::Closed) | None => break,
        }
    }
    assert_eq!(exit, Some(0));
    let rows: Vec<String> = (0..4).map(|r| grid.row_text(r)).collect();
    assert!(rows.iter().any(|r| r.contains("bye")), "{rows:?}");
    session.close().await;
}
