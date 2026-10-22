//! The phone's SSH client (§3.6.3, D10): russh over any byte stream (a
//! forward stream from the link), authenticating with the device's own
//! P-256 key through the `HwSigner` callback, so the private key never
//! leaves the secure element. Output goes through the VT grid.

use std::sync::Arc;
use std::time::Duration;

use muster_core::{HwSigner, PublicKey};
use russh::client::{self, Handle};
use russh::keys::agent::AgentIdentity;
use russh::keys::{HashAlg, PublicKeyOrCertificate};
use russh::{ChannelMsg, ChannelReadHalf, ChannelWriteHalf, Disconnect};
use tokio::io::{AsyncRead, AsyncWrite};

use crate::Error;

pub const KEY_TYPE: &str = "ecdsa-sha2-nistp256";

#[derive(Clone, Debug)]
pub struct SshConfig {
    pub user: String,
    pub term: String,
    pub cols: u16,
    pub rows: u16,
}

impl Default for SshConfig {
    fn default() -> Self {
        Self {
            user: String::new(),
            term: "xterm-256color".into(),
            cols: 80,
            rows: 24,
        }
    }
}

/// Decides whether a server host key is the one we expect (trust on first
/// use lives in the app core's store; this only answers).
pub trait HostVerifier: Send + Sync {
    /// `key_type` like "ssh-ed25519"; `fingerprint` like "SHA256:…".
    fn accept(&self, key_type: &str, fingerprint: &str) -> bool;
}

/// The device key as an OpenSSH public key (what goes in authorized_keys).
pub fn ssh_public_key(key: &PublicKey) -> Result<russh::keys::PublicKey, Error> {
    let ecdsa = russh::keys::ssh_key::public::EcdsaPublicKey::from_sec1_bytes(key.raw())
        .map_err(|e| Error::Protocol(format!("device key: {e}")))?;
    Ok(russh::keys::PublicKey::new(
        russh::keys::ssh_key::public::KeyData::Ecdsa(ecdsa),
        "muster",
    ))
}

/// The authorized_keys line for this device.
pub fn authorized_key_line(key: &PublicKey, comment: &str) -> Result<String, Error> {
    let mut pk = ssh_public_key(key)?;
    pk.set_comment(comment);
    pk.to_openssh().map_err(|e| Error::Protocol(e.to_string()))
}

fn mpint(bytes: &[u8]) -> Vec<u8> {
    let mut start = 0;
    while start < bytes.len() && bytes[start] == 0 {
        start += 1;
    }
    let body = &bytes[start..];
    let mut out = Vec::with_capacity(4 + body.len() + 1);
    let pad = body.first().is_some_and(|b| b & 0x80 != 0);
    out.extend_from_slice(&((body.len() + pad as usize) as u32).to_be_bytes());
    if pad {
        out.push(0);
    }
    out.extend_from_slice(body);
    out
}

/// Appends the SSH signature (as the userauth request carries it) made by
/// `signer` over `to_sign` — the same layout ssh-agent would return.
pub fn append_signature(signer: &dyn HwSigner, to_sign: &mut Vec<u8>) -> Result<(), Error> {
    let der = signer
        .sign(to_sign)
        .map_err(|e| Error::Unavailable(format!("hardware signature: {e}")))?;
    let sig = p256::ecdsa::Signature::from_der(&der)
        .map_err(|e| Error::Protocol(format!("signature: {e}")))?;
    let mut blob = mpint(&sig.r().to_bytes());
    blob.extend(mpint(&sig.s().to_bytes()));
    let inner = 4 + KEY_TYPE.len() + 4 + blob.len();
    to_sign.extend_from_slice(&(inner as u32).to_be_bytes());
    to_sign.extend_from_slice(&(KEY_TYPE.len() as u32).to_be_bytes());
    to_sign.extend_from_slice(KEY_TYPE.as_bytes());
    to_sign.extend_from_slice(&(blob.len() as u32).to_be_bytes());
    to_sign.extend_from_slice(&blob);
    Ok(())
}

struct KeySigner {
    signer: Arc<dyn HwSigner>,
}

#[derive(Debug, thiserror::Error)]
enum SignError {
    #[error(transparent)]
    Send(#[from] russh::SendError),
    #[error("{0}")]
    Sign(Error),
}

impl russh::Signer for KeySigner {
    type Error = SignError;

    fn auth_sign(
        &mut self,
        _key: &AgentIdentity,
        _hash_alg: Option<HashAlg>,
        mut to_sign: Vec<u8>,
    ) -> impl std::future::Future<Output = Result<Vec<u8>, Self::Error>> + Send {
        let signer = self.signer.clone();
        async move {
            append_signature(signer.as_ref(), &mut to_sign).map_err(SignError::Sign)?;
            Ok(to_sign)
        }
    }
}

struct ClientHandler {
    verifier: Arc<dyn HostVerifier>,
    host_key: Arc<std::sync::Mutex<Option<(String, String)>>>,
}

impl client::Handler for ClientHandler {
    type Error = russh::Error;

    fn check_server_key(
        &mut self,
        key: &PublicKeyOrCertificate,
    ) -> impl std::future::Future<Output = Result<bool, Self::Error>> + Send {
        let (key_type, fingerprint) = match key {
            PublicKeyOrCertificate::PublicKey { key, .. } => (
                key.algorithm().to_string(),
                key.fingerprint(HashAlg::Sha256).to_string(),
            ),
            PublicKeyOrCertificate::Certificate(cert) => (
                cert.algorithm().to_string(),
                cert.public_key().fingerprint(HashAlg::Sha256).to_string(),
            ),
        };
        let ok = self.verifier.accept(&key_type, &fingerprint);
        *self.host_key.lock().unwrap() = Some((key_type, fingerprint));
        async move { Ok(ok) }
    }
}

/// What the session reports.
#[derive(Debug)]
pub enum SshEvent {
    Output(Vec<u8>),
    Exit(u32),
    Closed,
}

pub struct SshSession {
    handle: Handle<ClientHandler>,
    read: ChannelReadHalf,
    write: ChannelWriteHalf<client::Msg>,
    pub host_key_type: String,
    pub host_fingerprint: String,
}

/// Connects, authenticates with the device key and opens a shell with a PTY.
pub async fn connect<S>(
    stream: S,
    config: &SshConfig,
    signer: Arc<dyn HwSigner>,
    verifier: Arc<dyn HostVerifier>,
) -> Result<SshSession, Error>
where
    S: AsyncRead + AsyncWrite + Unpin + Send + 'static,
{
    let ssh_config = Arc::new(client::Config {
        keepalive_interval: Some(Duration::from_secs(30)),
        keepalive_max: 3,
        ..client::Config::default()
    });
    let host_key = Arc::new(std::sync::Mutex::new(None));
    let handler = ClientHandler {
        verifier,
        host_key: host_key.clone(),
    };
    let mut handle = tokio::time::timeout(
        Duration::from_secs(30),
        client::connect_stream(ssh_config, stream, handler),
    )
    .await
    .map_err(|_| Error::Unavailable("ssh handshake timed out".into()))?
    .map_err(|e| Error::Unavailable(format!("ssh: {e}")))?;
    let public = ssh_public_key(&signer.public_key())?;
    let mut key_signer = KeySigner { signer };
    let result = handle
        .authenticate_publickey_with(config.user.clone(), public, None, &mut key_signer)
        .await
        .map_err(|e| Error::Unavailable(format!("ssh auth: {e}")))?;
    if !matches!(result, russh::client::AuthResult::Success) {
        let _ = handle
            .disconnect(Disconnect::ByApplication, "authentication refused", "")
            .await;
        return Err(Error::Unavailable(
            "the server did not accept this device's key; add it to authorized_keys".into(),
        ));
    }
    let channel = handle
        .channel_open_session()
        .await
        .map_err(|e| Error::Unavailable(format!("ssh session: {e}")))?;
    channel
        .request_pty(
            false,
            &config.term,
            config.cols as u32,
            config.rows as u32,
            0,
            0,
            &[],
        )
        .await
        .map_err(|e| Error::Unavailable(format!("ssh pty: {e}")))?;
    channel
        .request_shell(false)
        .await
        .map_err(|e| Error::Unavailable(format!("ssh shell: {e}")))?;
    let (read, write) = channel.split();
    let (host_key_type, host_fingerprint) = host_key.lock().unwrap().clone().unwrap_or_default();
    Ok(SshSession {
        host_key_type,
        host_fingerprint,
        handle,
        read,
        write,
    })
}

impl SshSession {
    pub async fn write(&self, bytes: &[u8]) -> Result<(), Error> {
        self.write
            .data(bytes)
            .await
            .map_err(|e| Error::Unavailable(format!("ssh write: {e}")))
    }

    pub async fn resize(&self, cols: u16, rows: u16) -> Result<(), Error> {
        self.write
            .window_change(cols as u32, rows as u32, 0, 0)
            .await
            .map_err(|e| Error::Unavailable(format!("ssh resize: {e}")))
    }

    /// The next thing the server sent; `Closed` once, then `None`.
    pub async fn next(&mut self) -> Option<SshEvent> {
        loop {
            match self.read.wait().await? {
                ChannelMsg::Data { data } => return Some(SshEvent::Output(data.to_vec())),
                ChannelMsg::ExtendedData { data, .. } => {
                    return Some(SshEvent::Output(data.to_vec()));
                }
                ChannelMsg::ExitStatus { exit_status } => return Some(SshEvent::Exit(exit_status)),
                ChannelMsg::Eof | ChannelMsg::Close => return Some(SshEvent::Closed),
                _ => continue,
            }
        }
    }

    pub async fn close(&self) {
        let _ = self.write.eof().await;
        let _ = self.write.close().await;
        let _ = self
            .handle
            .disconnect(Disconnect::ByApplication, "closed", "")
            .await;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use muster_core::SoftSigner;

    #[test]
    fn mpint_pads_and_strips() {
        assert_eq!(mpint(&[0x80]), vec![0, 0, 0, 2, 0, 0x80]);
        assert_eq!(mpint(&[0, 0, 0x7f]), vec![0, 0, 0, 1, 0x7f]);
    }

    #[test]
    fn signature_appends_an_ssh_signature_string_the_key_verifies() {
        let signer = SoftSigner::generate();
        let mut buf = b"session-id + userauth request".to_vec();
        let data_len = buf.len();
        append_signature(&signer, &mut buf).unwrap();
        let sig_part = &buf[data_len..];
        let total =
            u32::from_be_bytes([sig_part[0], sig_part[1], sig_part[2], sig_part[3]]) as usize;
        assert_eq!(total, sig_part.len() - 4);
        let tlen =
            u32::from_be_bytes([sig_part[4], sig_part[5], sig_part[6], sig_part[7]]) as usize;
        assert_eq!(&sig_part[8..8 + tlen], KEY_TYPE.as_bytes());
        // ssh-key can parse and verify what we produced.
        let signature = russh::keys::ssh_key::Signature::new(
            russh::keys::Algorithm::Ecdsa {
                curve: russh::keys::ssh_key::EcdsaCurve::NistP256,
            },
            sig_part[8 + tlen + 4..].to_vec(),
        )
        .unwrap();
        let public = ssh_public_key(&signer.public_key()).unwrap();
        use russh::keys::signature::Verifier;
        public
            .key_data()
            .verify(&buf[..data_len], &signature)
            .unwrap();
        assert!(
            authorized_key_line(&signer.public_key(), "phone")
                .unwrap()
                .starts_with("ecdsa-sha2-nistp256 ")
        );
    }
}
