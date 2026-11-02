//! Idle compaction on this Mac (ADR 0083). Off until the local owner turns it
//! on (`muster debug idle-compact on`). Shared Rust picks the conversations;
//! this loop adds this Mac's facts (screen lock, Locked Use, local input) and
//! takes the same provider queue, navigation and native write as a send.

use std::sync::Weak;

use crate::{Daemon, Error};

/// Local setting; absent means off.
const KEY: &str = "agent.idle_compact";

pub(crate) fn enabled(daemon: &Daemon) -> bool {
    matches!(daemon.store().kv_get(KEY), Ok(Some(value)) if value == b"on")
}

/// `muster debug idle-compact [on|off|status]` (local only).
pub(crate) fn debug(daemon: &Daemon, arg: &str) -> Result<String, Error> {
    match arg.trim() {
        "" | "status" => {}
        "on" => daemon.store().kv_set(KEY, b"on")?,
        "off" => daemon.store().kv_set(KEY, b"off")?,
        _ => {
            return Err(Error::Invalid("usage: idle-compact [on|off|status]".into()));
        }
    }
    let idle = muster_agent::idle_compact::IDLE.as_secs() / 60;
    let latest = muster_agent::idle_compact::LATEST.as_secs() / 60;
    Ok(if enabled(daemon) {
        format!(
            "idle compaction on: Claude Code conversations idle {idle}-{latest} min get /compact once"
        )
    } else {
        "idle compaction off".into()
    })
}

#[cfg(target_os = "macos")]
pub(crate) async fn run(daemon: Weak<Daemon>) {
    use muster_agent::idle_compact::{TICK, Tracker};
    let mut tracker = Tracker::default();
    let mut tick = tokio::time::interval(TICK);
    tick.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);
    loop {
        tick.tick().await;
        let Some(daemon) = daemon.upgrade() else {
            return;
        };
        if enabled(&daemon) {
            once(&daemon, &mut tracker).await;
        }
    }
}

/// At most one conversation per tick: each may select another conversation in
/// its app, and Claude takes a while to compact.
#[cfg(target_os = "macos")]
async fn once(daemon: &Daemon, tracker: &mut muster_agent::idle_compact::Tracker) {
    use muster_agent::idle_compact::LOCAL_IDLE;
    let Some(agent) = daemon.agent() else {
        return;
    };
    let now = muster_agent::model::now_unix();
    tracker.prune(now);
    let ready = || {
        !daemon.platform().locked()
            && daemon.platform().accessibility() == Some(true)
            && muster_platform::macos::input::user_input_idle() >= LOCAL_IDLE
    };
    if !ready() {
        return;
    }
    let agent = agent.clone();
    let lister = agent.clone();
    let Ok(due) = tokio::task::spawn_blocking(move || lister.idle_compact_due(now)).await else {
        return;
    };
    let Some(session) = due.into_iter().find(|s| tracker.untried(s)) else {
        return;
    };
    // A Locked Use transaction owns the console: leave it alone.
    let Ok(_console) = daemon.locked_use.writes.clone().try_read_owned() else {
        return;
    };
    tracker.tried(&session);
    let short: String = session.id.chars().take(8).collect();
    let result = async {
        let _app =
            crate::agent_bridge::hold_session(daemon, &agent, &session.id, "compact").await?;
        // Facts may have changed while the app was held by someone else.
        if !ready() {
            return Err(Error::Conflict("the Mac is in use or locked".into()));
        }
        crate::agent_bridge::ensure_native(daemon.platform().as_ref(), agent.clone(), &session.id)
            .await?;
        agent.compact(&session.id).await.map_err(Error::from)
    }
    .await;
    match result {
        Ok(()) => log::info!("idle compaction {short}: /compact confirmed"),
        // Claude writes the command record only after compacting; the
        // session's unconfirmed-write guard clears when it appears.
        Err(Error::UnknownOutcome(message)) => {
            log::info!("idle compaction {short}: pressed, awaiting receipt ({message})")
        }
        Err(error) => log::info!("idle compaction {short}: not sent ({error})"),
    }
}
