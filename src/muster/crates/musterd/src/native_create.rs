//! Local scheduler desktop creation under the same console/provider ownership.
use crate::{Daemon, Error};
use muster_proto::v1::{self, response::Resp};
use std::sync::Arc;

pub(crate) async fn create(
    daemon: &Arc<Daemon>,
    r: v1::AgentNativeCreateRequest,
) -> Result<Resp, Error> {
    use muster_proto::Message;
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
    #[cfg(not(target_os = "macos"))]
    {
        let _ = daemon;
        Err(Error::Unavailable(
            "Native desktop creation requires macOS".into(),
        ))
    }
    #[cfg(target_os = "macos")]
    {
        use muster_agent::{control_guard, desktop_lock::LockKind};
        if daemon.platform().accessibility() != Some(true) || daemon.platform().locked() {
            return Err(Error::Unavailable(
                "Native creation requires an unlocked desktop with Accessibility".into(),
            ));
        }
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
        let console = daemon
            .locked_use
            .writes
            .clone()
            .try_read_owned()
            .map_err(|_| Error::Conflict("A Locked Use transaction owns the console".into()))?;
        let ownership = Arc::new((provider_lock, console));
        let body = r.encode_to_vec();
        crate::handler::ledgered(daemon,&r.op_id,"agent.native_create",&r.provider,true,&body,async {
            let before:std::collections::HashSet<_>=agent.list(None,usize::MAX).into_iter().map(|s|s.id).collect();
            let held=ownership.clone();let provider=r.provider.clone();let cwd=path.clone();let prompt=r.prompt.clone();
            let form=control_guard::spawn_blocking_async(async move {let _held=held;muster_platform::macos::native_create::submit(&provider,&cwd,&prompt).await}).await.map_err(|e|Error::UnknownOutcome(e.to_string()))??;
            let deadline=tokio::time::Instant::now()+std::time::Duration::from_secs(15);
            loop {
                control_guard::check().map_err(|e|Error::UnknownOutcome(e.to_string()))?;
                agent.rescan().await;
                let mut matches=Vec::new();
                for s in agent.list(None,usize::MAX) {
                    if before.contains(&s.id) || s.provider!=r.provider || s.mode!="code" || std::path::Path::new(&s.cwd)!=path {continue}
                    if let Ok((messages,_))=agent.messages(&s.id,2000,None).await && messages.iter().find(|m|m.role=="user").is_some_and(|m|m.text.trim_end()==r.prompt.trim_end()) {matches.push(s.id)}
                }
                if matches.len()==1 && form.visible().map_err(|e|Error::UnknownOutcome(e.to_string()))? {return Ok(matches.remove(0))}
                if matches.len()>1 || tokio::time::Instant::now()>=deadline {return Err(Error::UnknownOutcome("Native submission has no unique receipt; inspect the app, never replay".into()))}
                tokio::time::sleep(std::time::Duration::from_millis(200)).await;
            }
        }).await
    }
}
