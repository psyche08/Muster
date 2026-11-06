//! Local scheduler desktop creation under the same console/provider ownership.
//! One UI submission happens while the console is owned; its receipt is the
//! unique new session whose transcript shows the exact prompt. When that
//! receipt is not seen while the console is owned, `receipt` looks for it
//! later from transcripts alone, never touching the desktop (ADR 0097).
use crate::{Daemon, Error};
use muster_proto::v1::{self, response::Resp};
use std::sync::Arc;
use std::time::Duration;

/// How long an unlocked creation waits for its receipt (the legacy RPC).
#[cfg(target_os = "macos")]
const UNLOCKED_RECEIPT: Duration = Duration::from_secs(15);

pub(crate) fn validate(r: &v1::AgentNativeCreateRequest) -> Result<(), Error> {
    if r.op_id.is_empty()
        || r.prompt.trim().is_empty()
        || r.prompt.len() > 1024 * 1024
        || !matches!(r.provider.as_str(), "codex" | "claude")
        || !std::path::Path::new(&r.directory).is_absolute()
    {
        return Err(Error::Invalid(
            "Native creation requires operation, provider, absolute directory and prompt".into(),
        ));
    }
    Ok(())
}

pub(crate) async fn create(
    daemon: &Arc<Daemon>,
    r: v1::AgentNativeCreateRequest,
) -> Result<Resp, Error> {
    validate(&r)?;
    #[cfg(not(target_os = "macos"))]
    {
        let _ = daemon;
        Err(Error::Unavailable(
            "Native desktop creation requires macOS".into(),
        ))
    }
    #[cfg(target_os = "macos")]
    {
        if daemon.platform().accessibility() != Some(true) || daemon.platform().locked() {
            return Err(Error::Unavailable(
                "Native creation requires an unlocked desktop with Accessibility".into(),
            ));
        }
        let console = daemon
            .locked_use
            .writes
            .clone()
            .try_read_owned()
            .map_err(|_| Error::Conflict("A Locked Use transaction owns the console".into()))?;
        owned(daemon, r, UNLOCKED_RECEIPT, Some(console)).await
    }
}

/// A creation whose caller already owns the console: a scheduled Locked Use
/// lease, or the scheduler's quiet unlocked path holding the console guard.
/// `receipt` bounds the wait for the receipt inside that ownership; an
/// unseen receipt is an unknown outcome, never a replay.
pub(crate) async fn scheduled(
    daemon: &Arc<Daemon>,
    r: v1::AgentNativeCreateRequest,
    receipt: Duration,
) -> Result<Resp, Error> {
    validate(&r)?;
    #[cfg(not(target_os = "macos"))]
    {
        let _ = (daemon, receipt);
        Err(Error::Unavailable(
            "Native desktop creation requires macOS".into(),
        ))
    }
    #[cfg(target_os = "macos")]
    {
        if daemon.platform().accessibility() != Some(true) {
            return Err(Error::Unavailable(
                "Native creation requires Accessibility".into(),
            ));
        }
        owned(daemon, r, receipt, None).await
    }
}

#[cfg(target_os = "macos")]
async fn owned(
    daemon: &Arc<Daemon>,
    r: v1::AgentNativeCreateRequest,
    receipt_budget: Duration,
    console: Option<tokio::sync::OwnedRwLockReadGuard<()>>,
) -> Result<Resp, Error> {
    use muster_agent::{control_guard, desktop_lock::LockKind};
    use muster_proto::Message;
    let app = muster_agent::profile::app(&r.provider).unwrap();
    if !daemon
        .policy()
        .read()
        .unwrap()
        .ax_allowlist
        .iter()
        .any(|b| b == app.app.bundle_id)
    {
        return Err(Error::Unauthorized("Native app is not allowlisted".into()));
    }
    let path = std::fs::canonicalize(&r.directory)
        .map_err(|_| Error::Invalid("Native directory is unavailable".into()))?;
    if !path.is_dir() {
        return Err(Error::Invalid(
            "Native directory must be a directory".into(),
        ));
    }
    let agent = daemon
        .agent()
        .ok_or_else(|| Error::Unavailable("Agent service unavailable".into()))?
        .clone();
    let provider_lock = crate::agent_bridge::hold(
        daemon,
        app,
        LockKind::Agent,
        "create native scheduler session",
    )
    .await?;
    let ownership = Arc::new((provider_lock, console));
    let body = r.encode_to_vec();
    crate::handler::ledgered(daemon,&r.op_id,"agent.native_create",&r.provider,true,&body,async {
        let before:Vec<String>=agent.list(None,usize::MAX).into_iter().map(|s|s.id).collect();
        let held=ownership.clone();let provider=r.provider.clone();let cwd=path.clone();let prompt=r.prompt.clone();
        let form=control_guard::spawn_blocking_async(async move {let _held=held;muster_platform::macos::native_create::submit(&provider,&cwd,&prompt).await}).await.map_err(|e|Error::UnknownOutcome(e.to_string()))??;
        let deadline=tokio::time::Instant::now()+receipt_budget;
        loop {
            control_guard::check().map_err(|e|Error::UnknownOutcome(e.to_string()))?;
            let mut matches=candidates(&agent,&r.provider,&path,&r.prompt,&before).await;
            if matches.len()==1 && form.visible().map_err(|e|Error::UnknownOutcome(e.to_string()))? {return Ok(matches.remove(0))}
            if matches.len()>1 || tokio::time::Instant::now()>=deadline {return Err(Error::UnknownOutcome("Native submission has no unique receipt yet; reconcile from transcripts, never replay".into()))}
            tokio::time::sleep(Duration::from_millis(200)).await;
        }
    }).await
}

/// New sessions of this provider in this directory, outside `exclude`, whose
/// first user message is exactly the prompt.
async fn candidates(
    agent: &Arc<muster_agent::AgentService>,
    provider: &str,
    path: &std::path::Path,
    prompt: &str,
    exclude: &[String],
) -> Vec<String> {
    agent.rescan().await;
    let mut matches = Vec::new();
    for s in agent.list(None, usize::MAX) {
        if exclude.contains(&s.id)
            || s.provider != provider
            || s.mode != "code"
            || std::path::Path::new(&s.cwd) != path
        {
            continue;
        }
        if let Ok((messages, _)) = agent.messages(&s.id, 2000, None).await
            && messages
                .iter()
                .find(|m| m.role == "user")
                .is_some_and(|m| m.text.trim_end() == prompt.trim_end())
        {
            matches.push(s.id)
        }
    }
    matches
}

/// The sessions of this provider in this directory now: none of them can be
/// the receipt of a creation that has not been submitted yet.
pub(crate) fn baseline(daemon: &Daemon, provider: &str, directory: &str) -> Vec<String> {
    let path = std::path::Path::new(directory);
    daemon
        .agent()
        .map(|agent| {
            agent
                .list(None, usize::MAX)
                .into_iter()
                .filter(|s| s.provider == provider && std::path::Path::new(&s.cwd) == path)
                .map(|s| s.id)
                .collect()
        })
        .unwrap_or_default()
}

/// Read-only receipt lookup after the console is given back: the unique new
/// session (outside `exclude`) whose transcript starts with the exact prompt.
/// `Ok(None)`: not seen yet. More than one match is an integrity anomaly.
pub(crate) async fn receipt(
    daemon: &Daemon,
    r: &v1::AgentNativeCreateRequest,
    exclude: &[String],
) -> Result<Option<String>, Error> {
    let agent = daemon
        .agent()
        .ok_or_else(|| Error::Unavailable("Agent service unavailable".into()))?;
    let path = std::path::Path::new(&r.directory);
    let mut matches = candidates(agent, &r.provider, path, &r.prompt, exclude).await;
    match matches.len() {
        0 => Ok(None),
        1 => Ok(Some(matches.remove(0))),
        _ => Err(Error::Conflict(
            "more than one new session shows this creation's prompt; not binding any".into(),
        )),
    }
}
