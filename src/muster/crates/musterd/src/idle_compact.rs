//! Idle compaction on this Mac (ADR 0083, ADR 0087). Off until the local owner
//! turns it on (`muster debug idle-compact on`). Shared Rust picks the
//! conversations; this loop adds this Mac's facts and takes the same provider
//! queue, navigation and native write as a send. Only missing Accessibility
//! stops it: a locked Mac compacts under its own unattended Locked Use lease,
//! and while the person uses the Mac a notice at the top right of the screen
//! stays up until Claude records the compaction.

use std::sync::Arc;
#[cfg(target_os = "macos")]
use std::sync::Weak;

use crate::{Daemon, Error};

/// Local setting; absent means off.
const KEY: &str = "agent.idle_compact";

pub(crate) fn enabled(daemon: &Daemon) -> bool {
    matches!(daemon.store().kv_get(KEY), Ok(Some(value)) if value == b"on")
}

/// Whether a locked Mac can compact under its own Locked Use lease.
fn locked_allowed(daemon: &Daemon) -> bool {
    daemon
        .policy()
        .read()
        .is_ok_and(|p| p.allow_locked_control && p.offer_agent)
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
    Ok(if !enabled(daemon) {
        "idle compaction off".into()
    } else if locked_allowed(daemon) {
        format!(
            "idle compaction on: Claude Code conversations idle {idle}-{latest} min get /compact once, under Locked Use while this Mac is locked"
        )
    } else {
        format!(
            "idle compaction on: Claude Code conversations idle {idle}-{latest} min get /compact once; Locked Use is off, so a locked Mac waits for an unlock"
        )
    })
}

/// Runs `/compact` in `id` the way a send runs: the session's provider queue,
/// navigation, then the native write. The caller supplies the console checks.
pub(crate) async fn compact(daemon: &Arc<Daemon>, id: &str) -> Result<(), Error> {
    let agent = daemon
        .agent()
        .ok_or_else(|| Error::Unavailable("agent control is off".into()))?
        .clone();
    let _app = crate::agent_bridge::hold_session(daemon, &agent, id, "compact").await?;
    crate::agent_bridge::ensure_native(daemon.platform().as_ref(), agent.clone(), id).await?;
    agent.compact(id).await.map_err(Error::from)
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
async fn once(daemon: &Arc<Daemon>, tracker: &mut muster_agent::idle_compact::Tracker) {
    let Some(agent) = daemon.agent().cloned() else {
        return;
    };
    // Nothing can be pressed until Accessibility is granted.
    if daemon.platform().accessibility() != Some(true) {
        return;
    }
    let now = muster_agent::model::now_unix();
    tracker.prune(now);
    let lister = agent.clone();
    let Ok(due) = tokio::task::spawn_blocking(move || lister.idle_compact_due(now)).await else {
        return;
    };
    let Some(session) = due.into_iter().find(|s| tracker.untried(s)) else {
        return;
    };
    // A Locked Use transaction owns the console: look again next tick.
    if daemon.locked_use.writes.try_read().is_err() {
        return;
    }
    let short: String = session.id.chars().take(8).collect();
    let result = if daemon.platform().locked() {
        if !locked_allowed(daemon) {
            return;
        }
        locked(daemon, &session).await
    } else {
        unlocked(daemon, &agent, &session).await
    };
    match result {
        // Busy: nothing was attempted.
        None => {}
        Some(Ok(())) => {
            tracker.done(&session);
            log::info!("idle compaction {short}: /compact confirmed");
        }
        // Claude writes the command record only after compacting; the
        // session's unconfirmed-write guard clears when it appears.
        Some(Err(Error::UnknownOutcome(message))) => {
            tracker.done(&session);
            log::info!("idle compaction {short}: pressed, awaiting receipt ({message})");
        }
        Some(Err(error)) => {
            tracker.failed(&session);
            log::info!("idle compaction {short}: not sent ({error})");
        }
    }
}

/// A locked Mac: this Mac's own Locked Use lease unlocks under cover, runs
/// `/compact` and locks again (ADR 0087).
#[cfg(target_os = "macos")]
async fn locked(
    daemon: &Arc<Daemon>,
    session: &muster_agent::model::AgentSession,
) -> Option<Result<(), Error>> {
    use muster_proto::v1::{op_result::Status, response::Resp};
    let reply = daemon
        .locked_use
        .unattended_compact(daemon.clone(), &session.provider, &session.id)
        .await;
    Some(match reply {
        Ok(Resp::OpResult(r)) if r.status == Status::Applied as i32 => Ok(()),
        Ok(Resp::OpResult(r)) if r.status == Status::Unknown as i32 => {
            Err(Error::UnknownOutcome(r.message))
        }
        Ok(Resp::OpResult(r)) => Err(Error::Conflict(r.message)),
        Ok(_) => Err(Error::Unavailable("unexpected Locked Use reply".into())),
        Err(error) => Err(error),
    })
}

/// An unlocked Mac: say so at the top right, wait for a pause in local input,
/// then compact as a send would. The notice stays until Claude records the
/// compaction; it closes at once when nothing was pressed.
#[cfg(target_os = "macos")]
async fn unlocked(
    daemon: &Arc<Daemon>,
    agent: &Arc<muster_agent::AgentService>,
    session: &muster_agent::model::AgentSession,
) -> Option<Result<(), Error>> {
    use muster_agent::idle_compact::{NOTICE, QUIET, QUIET_WAIT};
    use muster_platform::macos::notice::Notice;
    let body = if session.title.trim().is_empty() {
        "Claude Code"
    } else {
        session.title.as_str()
    };
    let notice = Notice::show("Compacting an idle Claude conversation", body)
        .inspect_err(|error| log::warn!("idle compaction: notice not shown: {error}"))
        .ok();
    let checked = daemon.clone();
    let check = Arc::new(move || {
        !checked.platform().locked()
            && checked.platform().accessibility() == Some(true)
            && enabled(&checked)
    });
    let attempt = muster_agent::control_guard::scope(check, async {
        let idle = crate::agent_bridge::session_input_idle(agent, &session.id);
        muster_agent::control_guard::wait_for_quiet(idle, QUIET, QUIET_WAIT).await?;
        // Ordinary writes hold this as well; Locked Use waits for it.
        let Ok(_console) = daemon.locked_use.writes.clone().try_read_owned() else {
            return Ok(None);
        };
        Ok::<_, Error>(Some(compact(daemon, &session.id).await))
    })
    .await;
    let result = attempt.unwrap_or_else(|error| Some(Err(error)));
    if matches!(result, Some(Ok(()) | Err(Error::UnknownOutcome(_)))) && notice.is_some() {
        let deadline = tokio::time::Instant::now() + NOTICE;
        while tokio::time::Instant::now() < deadline {
            let agent = agent.clone();
            let id = session.id.clone();
            if tokio::task::spawn_blocking(move || agent.compacted(&id))
                .await
                .unwrap_or(true)
            {
                break;
            }
            tokio::time::sleep(std::time::Duration::from_secs(2)).await;
        }
    }
    drop(notice);
    result
}
