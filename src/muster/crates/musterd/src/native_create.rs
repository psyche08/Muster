//! Local scheduler desktop creation under the same console/provider ownership.
//! One UI submission happens while the console is owned; its receipt is the
//! unique new session whose transcript shows the exact prompt. When that
//! receipt is not seen while the console is owned, `receipt` looks for it
//! later from transcripts alone, never touching the desktop (ADR 0097).
//! DeepSeek Harness is created through Muster's plugin inside it instead,
//! without Accessibility and only on a direct request to an unlocked Mac, in
//! a session named from the operation (ADR 0167).
use crate::{Daemon, Error};
use muster_agent::native_options::valid_name;
use muster_proto::v1::{self, response::Resp};
use std::sync::Arc;
use std::time::Duration;

/// How long an unlocked creation waits for its receipt (the legacy RPC).
#[cfg(target_os = "macos")]
const UNLOCKED_RECEIPT: Duration = Duration::from_secs(15);

/// A creation through the app's windows (Codex, Claude): the only kind the
/// scheduler and Locked Use run.
pub(crate) fn validate(r: &v1::AgentNativeCreateRequest) -> Result<(), Error> {
    if through_plugin_app(&r.provider) {
        return Err(Error::Unavailable(
            "DeepSeek Harness sessions are created only on a direct request to an unlocked Mac"
                .into(),
        ));
    }
    well_formed(r, &["codex", "claude"])
}

fn well_formed(r: &v1::AgentNativeCreateRequest, providers: &[&str]) -> Result<(), Error> {
    if r.op_id.is_empty()
        || r.prompt.trim().is_empty()
        || r.prompt.len() > 1024 * 1024
        || !providers.contains(&r.provider.as_str())
        || !std::path::Path::new(&r.directory).is_absolute()
    {
        return Err(Error::Invalid(
            "Native creation requires operation, provider, absolute directory and prompt".into(),
        ));
    }
    if !valid_name(&r.model) || !valid_name(&r.effort) {
        return Err(Error::Invalid(
            "Native creation model and effort must be names of at most 128 bytes, without control characters or surrounding spaces".into(),
        ));
    }
    Ok(())
}

/// Whether a creation asks for a model or an effort (ADR 0181).
fn has_options(r: &v1::AgentNativeCreateRequest) -> bool {
    !r.model.is_empty() || !r.effort.is_empty()
}

/// Whether the provider's sessions are created through Muster's plugin
/// inside its app rather than its windows (DeepSeek Harness, ADR 0167).
fn through_plugin_app(provider: &str) -> bool {
    muster_agent::profile::app(provider).is_some_and(|app| app.plugin.is_some())
}

pub(crate) async fn create(
    daemon: &Arc<Daemon>,
    r: v1::AgentNativeCreateRequest,
) -> Result<Resp, Error> {
    create_unlocked(daemon, r, false).await
}

pub(crate) async fn create_unlocked(
    daemon: &Arc<Daemon>,
    r: v1::AgentNativeCreateRequest,
    managed: bool,
) -> Result<Resp, Error> {
    let windows = !through_plugin_app(&r.provider);
    if windows {
        validate(&r)?;
    } else {
        well_formed(&r, &["deepseek"])?;
        // The plugin's task contract (ADR 0163) takes no model or effort.
        if has_options(&r) {
            return Err(muster_agent::native_options::create_refused(
                "DeepSeek Harness sessions take no model or effort from Muster",
            )
            .into());
        }
    }
    #[cfg(not(target_os = "macos"))]
    {
        let _ = (daemon, managed);
        Err(Error::Unavailable(
            "Native desktop creation requires macOS".into(),
        ))
    }
    #[cfg(target_os = "macos")]
    {
        if (windows && daemon.platform().accessibility() != Some(true))
            || daemon.platform().locked()
        {
            return Err(Error::Unavailable(if windows {
                "Native creation requires an unlocked desktop with Accessibility".into()
            } else {
                "Native creation requires an unlocked desktop".into()
            }));
        }
        let console = if managed {
            None
        } else {
            Some(
                daemon
                    .locked_use
                    .writes
                    .clone()
                    .try_read_owned()
                    .map_err(|_| {
                        Error::Conflict("A Locked Use transaction owns the console".into())
                    })?,
            )
        };
        owned(daemon, r, UNLOCKED_RECEIPT, console, false, managed).await
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
    covered: bool,
) -> Result<Resp, Error> {
    validate(&r)?;
    #[cfg(not(target_os = "macos"))]
    {
        let _ = (daemon, receipt, covered);
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
        owned(daemon, r, receipt, None, covered, false).await
    }
}

#[cfg(target_os = "macos")]
async fn owned(
    daemon: &Arc<Daemon>,
    r: v1::AgentNativeCreateRequest,
    receipt_budget: Duration,
    console: Option<tokio::sync::OwnedRwLockReadGuard<()>>,
    covered: bool,
    managed: bool,
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
    if app.plugin.is_some() {
        return through_plugin(daemon, r, agent, path, receipt_budget, console).await;
    }
    let (ownership, prepared) = loop {
        control_guard::check()?;
        let mut prepared = if managed {
            let path = path.clone();
            Some(
                control_guard::spawn_blocking_async(async move {
                    muster_platform::macos::native_create::managed_admission(&path).await
                })
                .await
                .map_err(|e| Error::Unavailable(e.to_string()))??,
            )
        } else {
            None
        };
        let lock = crate::agent_bridge::hold(
            daemon,
            app,
            LockKind::Agent,
            "create native scheduler session",
        )
        .await;
        if managed && matches!(lock, Err(Error::Timeout(_))) {
            continue;
        }
        let provider_lock = lock?;
        let console = if let Some(observation) = prepared.take() {
            let Ok(console) = daemon.locked_use.writes.clone().try_read_owned() else {
                drop(provider_lock);
                tokio::time::sleep(Duration::from_millis(100)).await;
                continue;
            };
            let cwd = path.clone();
            prepared = control_guard::spawn_blocking(move || {
                observation.still_ready(&cwd).then_some(observation)
            })
            .await
            .map_err(|e| Error::Unavailable(e.to_string()))?;
            if prepared.is_none() {
                continue;
            }
            if !daemon
                .policy()
                .read()
                .unwrap()
                .ax_allowlist
                .iter()
                .any(|b| b == app.app.bundle_id)
                || daemon.platform().accessibility() != Some(true)
            {
                return Err(Error::Unauthorized(
                    "Native app permission changed while queued".into(),
                ));
            }
            Some(console)
        } else {
            console
        };
        break (Arc::new((provider_lock, console)), prepared);
    };
    let body = r.encode_to_vec();
    crate::handler::ledgered(daemon,&r.op_id,"agent.native_create",&r.provider,true,&body,async {
        let before:Vec<String>=agent.list(None,usize::MAX).into_iter().map(|s|s.id).collect();
        let held=ownership.clone();let provider=r.provider.clone();let cwd=path.clone();let prompt=r.prompt.clone();
        let (model,effort)=(r.model.clone(),r.effort.clone());
        let form=control_guard::spawn_blocking_async(async move {let _held=held;muster_platform::macos::native_create::submit(&provider,&cwd,&prompt,&model,&effort,covered,prepared).await}).await.map_err(|e|Error::UnknownOutcome(e.to_string()))??;
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

/// Creation through Muster's plugin inside the app (ADR 0167): the plugin's
/// `task.create.v1` makes the session named from the operation in the
/// canonical directory, then its first message is sent as any prompt is. The
/// receipt is exact, and a retry of the operation can only name the same
/// session. No window is touched.
#[cfg(target_os = "macos")]
async fn through_plugin(
    daemon: &Arc<Daemon>,
    r: v1::AgentNativeCreateRequest,
    agent: Arc<muster_agent::AgentService>,
    path: std::path::PathBuf,
    receipt_budget: Duration,
    console: Option<tokio::sync::OwnedRwLockReadGuard<()>>,
) -> Result<Resp, Error> {
    use crate::agent_bridge::deepseek_desktop;
    use muster_agent::{control_guard, desktop_lock::LockKind};
    use muster_proto::Message;
    let app = muster_agent::profile::app(&r.provider).unwrap();
    let cwd = path
        .to_str()
        .ok_or_else(|| Error::Invalid("Native directory is not UTF-8".into()))?
        .to_owned();
    let store = agent.deepseek_task_store()?;
    let link = agent
        .deepseek_link()
        .ok_or_else(|| Error::Unavailable("DeepSeek Harness is not set up for Muster".into()))?;
    // Before the ledger: while the App, its desktop plugin or the task
    // contract is missing, the operation stays retryable.
    let host = link.task_host()?;
    if !deepseek_desktop(daemon.platform().as_ref(), &host) {
        return Err(Error::Unavailable(
            "The plugin is not hosted by the running DSH desktop App".into(),
        ));
    }
    let provider_lock =
        crate::agent_bridge::hold(daemon, app, LockKind::Agent, "create native session").await?;
    let session = muster_agent::plugin_control::created_session(&r.op_id);
    let body = r.encode_to_vec();
    crate::handler::ledgered(
        daemon,
        &r.op_id,
        "agent.native_create",
        &r.provider,
        true,
        &body,
        async {
            let held = (provider_lock, console);
            let platform = daemon.platform().clone();
            let (named, operation, prompt) = (session.clone(), r.op_id.clone(), r.prompt.clone());
            let created = control_guard::spawn_blocking(move || {
                let _held = held;
                // The queue may have waited: check the Mac and the App again
                // just before the App is touched.
                if platform.locked() || !deepseek_desktop(platform.as_ref(), &host) {
                    return Err(muster_agent::Error::Unavailable(
                        "The unlocked DSH desktop App is no longer available".into(),
                    ));
                }
                link.create(&host, &named, &operation, &cwd, &store, &prompt)
            })
            .await
            .map_err(|e| Error::UnknownOutcome(e.to_string()))?;
            let created = match created {
                Ok(()) => Ok(session.clone()),
                Err(muster_agent::Error::UnknownOutcome(why)) => Err(Error::UnknownOutcome(why)),
                Err(refused) => return Err(refused.into()),
            };
            // Wait for the app's transcript, which the session's later reads
            // need; an acknowledged creation is applied even before it shows,
            // and an unacknowledged one is applied once it does.
            let deadline = tokio::time::Instant::now() + receipt_budget;
            loop {
                if shows(&agent, &session, &path, &r.prompt).await {
                    return Ok(session);
                }
                if tokio::time::Instant::now() >= deadline || control_guard::check().is_err() {
                    return created;
                }
                tokio::time::sleep(Duration::from_millis(200)).await;
            }
        },
    )
    .await
}

/// Whether `session` is listed in this directory with the prompt as its
/// first user message.
#[cfg(target_os = "macos")]
async fn shows(
    agent: &Arc<muster_agent::AgentService>,
    session: &str,
    path: &std::path::Path,
    prompt: &str,
) -> bool {
    agent.rescan().await;
    let Some(s) = agent.session(session) else {
        return false;
    };
    std::path::Path::new(&s.cwd) == path
        && first_message_is(agent, &s.provider, session, prompt).await
}

async fn first_message_is(
    agent: &Arc<muster_agent::AgentService>,
    provider: &str,
    session: &str,
    prompt: &str,
) -> bool {
    agent
        .messages(session, 2000, None)
        .await
        .is_ok_and(|(messages, _)| {
            messages.iter().find(|m| m.role == "user").is_some_and(|m| {
                muster_agent::native_runtime::first_prompt_matches(provider, &m.text, prompt)
            })
        })
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
        if first_message_is(agent, provider, &s.id, prompt).await {
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

#[cfg(test)]
mod tests {
    use super::*;

    fn request(provider: &str) -> v1::AgentNativeCreateRequest {
        v1::AgentNativeCreateRequest {
            op_id: "op".into(),
            provider: provider.into(),
            directory: "/tmp".into(),
            prompt: "hi".into(),
            ..Default::default()
        }
    }

    /// The scheduler and Locked Use run only the windows path; DeepSeek
    /// Harness is created through its plugin on a direct request alone.
    #[test]
    fn only_window_providers_pass_the_shared_validation() {
        for provider in ["codex", "claude"] {
            validate(&request(provider)).unwrap();
        }
        assert!(matches!(
            validate(&request("deepseek")),
            Err(Error::Unavailable(_))
        ));
        assert!(matches!(
            validate(&request("gemini")),
            Err(Error::Invalid(_))
        ));
    }

    /// A model or effort is a name exactly as the app shows it; anything
    /// else is refused before the ledger, the console or the app.
    #[test]
    fn options_are_exact_short_names() {
        let mut r = request("claude");
        r.model = "Opus 5.5".into();
        r.effort = "High".into();
        validate(&r).unwrap();
        assert!(has_options(&r));
        assert!(!has_options(&request("claude")));
        for (model, effort) in [
            (" Opus 5.5", ""),
            ("", "High\n"),
            (&*"m".repeat(129), ""),
            ("Opus\u{0}5.5", ""),
        ] {
            let r = v1::AgentNativeCreateRequest {
                model: model.into(),
                effort: effort.into(),
                ..request("codex")
            };
            assert!(
                matches!(validate(&r), Err(Error::Invalid(_))),
                "{model:?} {effort:?}"
            );
        }
    }
}
