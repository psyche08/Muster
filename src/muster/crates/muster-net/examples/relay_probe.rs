//! Public-PKI probe of both secure carriers; ephemeral identity, no pairing.
use muster_core::SoftSigner;
use muster_net::secure_relay::{Carrier, SecureRelay, Strategy};
use std::{sync::Arc, time::Duration};

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
    let require_both = std::env::args().any(|arg| arg == "--require-both");
    let link = SecureRelay::new(
        authority,
        Arc::new(SoftSigner::generate()),
        Strategy::Standby,
    )?;
    let mut seen = [false; 2];
    for second in 1..=40 {
        tokio::time::sleep(Duration::from_secs(1)).await;
        for (i, carrier) in [Carrier::Quic, Carrier::Tls].into_iter().enumerate() {
            seen[i] |= link.carrier_ready(carrier);
        }
        println!(
            "{second}s: {:?}, active={:?}, quic_rtt={:?}, tls_rtt={:?}",
            link.status(),
            link.active(),
            link.carrier_rtt(Carrier::Quic),
            link.carrier_rtt(Carrier::Tls)
        );
    }
    link.shutdown();
    anyhow::ensure!(
        seen.iter().any(|ok| *ok),
        "neither certificate-verified relay carrier registered"
    );
    anyhow::ensure!(
        !require_both || seen.iter().all(|ok| *ok),
        "both TCP/TLS and UDP/QUIC must pass"
    );
    Ok(())
}
