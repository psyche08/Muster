//! TCP: each accepted connection gets its own upstream connection, and bytes
//! are copied both ways until both sides have closed.

use std::sync::Arc;

use tokio::net::{TcpListener, TcpStream};
use tokio::sync::Semaphore;
use tokio::task::JoinSet;

use crate::config::{Options, Rule};

pub async fn serve(rule: Rule, options: Options) {
    let slots = Arc::new(Semaphore::new(options.max_tcp_connections));
    // Outlives each listener, so rebinding keeps the connections it carries.
    let mut connections = JoinSet::new();
    loop {
        let listener =
            crate::bind_until(&rule, options.rebind, || TcpListener::bind(rule.listen)).await;
        loop {
            // A full rule leaves further clients in the listen backlog.
            let slot = tokio::select! {
                slot = slots.clone().acquire_owned() => slot.expect("never closed"),
                Some(_) = connections.join_next(), if !connections.is_empty() => continue,
            };
            let accepted = tokio::select! {
                accepted = listener.accept() => accepted,
                Some(_) = connections.join_next(), if !connections.is_empty() => continue,
            };
            match accepted {
                Ok((client, peer)) => {
                    let (rule, options) = (rule.clone(), options.clone());
                    connections.spawn(async move {
                        carry(&rule, &options, client, peer).await;
                        drop(slot);
                    });
                }
                Err(error) => {
                    log::warn!("{rule}: accept: {error}; listening again");
                    tokio::time::sleep(options.rebind).await;
                    break;
                }
            }
        }
    }
}

async fn carry(rule: &Rule, options: &Options, mut client: TcpStream, peer: std::net::SocketAddr) {
    let connect = tokio::time::timeout(options.connect_timeout, TcpStream::connect(rule.upstream));
    let mut upstream = match connect.await {
        Ok(Ok(upstream)) => upstream,
        Ok(Err(error)) => {
            log::info!("{rule}: from {peer}: upstream: {error}");
            return;
        }
        Err(_) => {
            log::info!("{rule}: from {peer}: upstream did not answer in time");
            return;
        }
    };
    let _ = client.set_nodelay(true);
    let _ = upstream.set_nodelay(true);
    match tokio::io::copy_bidirectional(&mut client, &mut upstream).await {
        Ok((up, down)) => log::debug!("{rule}: from {peer}: {up} bytes up, {down} down"),
        Err(error) => log::debug!("{rule}: from {peer}: {error}"),
    }
}
