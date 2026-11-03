//! l4forwarder (ADR 0105): forwards TCP and UDP from listening addresses to
//! upstream addresses, byte for byte and datagram for datagram. Its use in a
//! Muster network is to offer a service that listens only on a member's
//! loopback at that member's vnet address; who may reach that address and
//! port stays Muster's decision (`net:` scopes and open ports).

pub mod config;
mod tcp;
mod udp;

use std::future::Future;
use std::time::Duration;

pub use config::{Config, Options, Proto, Rule};
use tokio::task::JoinHandle;

/// Starts one rule. It runs until the handle is aborted, waiting for its
/// listening address whenever binding fails.
pub fn spawn(rule: Rule, options: Options) -> JoinHandle<()> {
    match rule.proto {
        Proto::Tcp => tokio::spawn(tcp::serve(rule, options)),
        Proto::Udp => tokio::spawn(udp::serve(rule, options)),
    }
}

/// Runs every rule of `config` until the process is told to stop.
pub async fn run(config: Config) {
    let options = config.options();
    let tasks: Vec<_> = config
        .rules
        .into_iter()
        .map(|rule| spawn(rule, options.clone()))
        .collect();
    for task in tasks {
        let _ = task.await;
    }
}

/// Binds with `bind` until it works, logging each new reason it does not.
async fn bind_until<T, F, Fut>(rule: &Rule, rebind: Duration, mut bind: F) -> T
where
    F: FnMut() -> Fut,
    Fut: Future<Output = std::io::Result<T>>,
{
    let mut last = String::new();
    loop {
        match bind().await {
            Ok(bound) => {
                log::info!("{rule}: listening");
                return bound;
            }
            Err(error) => {
                let why = error.to_string();
                if why != last {
                    log::warn!("{rule}: cannot listen yet: {why}");
                    last = why;
                }
                tokio::time::sleep(rebind).await;
            }
        }
    }
}
