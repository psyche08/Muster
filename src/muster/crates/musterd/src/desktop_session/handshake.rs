use super::engine::*;
use crate::{Daemon, Error};
use muster_core::DeviceId;
use muster_net::remote_desktop::Session;
use muster_proto::v1::desktop_session_progress::Phase;
use muster_rpc::EventSink;
use std::time::Duration;
use tokio::time::timeout;

pub(super) async fn open(
    daemon: &Daemon,
    peer: DeviceId,
    hello: &[u8],
    events: &EventSink,
) -> Result<(Session, Vec<u8>), Error> {
    progress(events, Phase::Starting).await?;
    let mut session = timeout(
        Duration::from_secs(25),
        daemon.node().open_remote_desktop(peer),
    )
    .await
    .map_err(|_| unavailable("timed out waiting for paired host authorization"))??;
    session.send_control(hello).await?;
    let mut reply = timeout(Duration::from_secs(50), session.recv_control())
        .await
        .map_err(|_| unavailable("paired host did not start its desktop"))??
        .ok_or_else(|| unavailable("host ended the desktop grant"))?;
    if let Some(raw) = reply.strip_prefix(BUSY) {
        let controller = DeviceId(
            raw.try_into()
                .map_err(|_| unavailable("invalid desktop occupant"))?,
        );
        let name = daemon
            .roster()
            .read()
            .unwrap()
            .state()
            .active(&controller)
            .map(|m| m.record.name.clone())
            .unwrap_or_default();
        let name: String = name
            .chars()
            .filter(|c| {
                !c.is_control() && !matches!(*c, '\u{202a}'..='\u{202e}' | '\u{2066}'..='\u{2069}')
            })
            .take(64)
            .collect();
        let controller = format!("{} ({})", name.trim(), controller.short());
        let mut prompt = daemon.desktop_confirmation.ask(peer);
        events
            .send(muster_proto::v1::event::Ev::DesktopSession(
                muster_proto::v1::DesktopSessionProgress {
                    phase: Phase::Occupied as i32,
                    mouse: false,
                    takeover: Some(muster_proto::v1::DesktopTakeoverPrompt {
                        confirmation: prompt.id,
                        controller,
                    }),
                },
            ))
            .await
            .map_err(|_| unavailable("viewer closed"))?;
        tokio::select! {
            choice = timeout(Duration::from_secs(85), &mut prompt.reply) => {
                choice.map_err(|_| unavailable("desktop confirmation expired"))?
                    .map_err(|_| unavailable("desktop confirmation cancelled"))?;
            }
            _ = session.recv_control() => return Err(unavailable("desktop host ended the pending request")),
        }
        session.send_control(REPLACE).await?;
        progress(events, Phase::Starting).await?;
        reply = timeout(Duration::from_secs(60), session.recv_control())
            .await
            .map_err(|_| unavailable("desktop takeover timed out"))??
            .ok_or_else(|| unavailable("desktop host ended the takeover"))?;
        if reply == CHANGED {
            let _ = session.send_control(b"received").await;
            return Err(unavailable(
                "The active viewer changed. Open the desktop again to see who is connected.",
            ));
        }
    }
    if reply == CAPTURE_UNAVAILABLE {
        let _ = session.send_control(b"received").await;
    }
    Ok((session, reply))
}
