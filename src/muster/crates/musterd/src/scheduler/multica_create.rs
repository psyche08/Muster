//! Local owner review for one managed Multica first turn (ADR 0184).
use super::*;
use tokio_util::sync::CancellationToken;

fn valid(o: &v1::AgentSchedulerOperate) -> bool {
    o.task_id.strip_prefix("multica:").is_some_and(|run| {
        !run.is_empty()
            && run.len() <= 80
            && run
                .bytes()
                .all(|b| b.is_ascii_alphanumeric() || matches!(b, b'-' | b'_'))
    }) && o.task_id == o.op_id
        && matches!(o.provider.as_str(), "codex" | "claude")
        && o.role == "producer"
        && o.kind == "create"
        && o.session.is_empty()
        && o.model.is_empty()
        && o.effort.is_empty()
        && !o.text.trim().is_empty()
        && o.text.len() <= MAX_TEXT
}

/// The native session's directory and first turn for a Multica run: the
/// run's workspace folder, and a brief naming the run's own directory before
/// its first turn (ADR 0221). The grant still binds the run's own directory
/// and unchanged text.
pub(super) fn native_destination(
    task_id: &str,
    workdir: &str,
    text: &str,
) -> Result<(String, String), Error> {
    let run = task_id.strip_prefix("multica:").unwrap_or_default();
    let workspace = muster_agent::native_runtime::multica_workspace_directory(
        std::path::Path::new(workdir),
        run,
    )
    .and_then(std::path::Path::to_str)
    .ok_or_else(|| {
        Error::Invalid("Multica working directory is not <workspace>/<run>/workdir".into())
    })?;
    let prompt = muster_agent::native_runtime::multica_workspace_prompt(workdir, text);
    if prompt.len() > MAX_TEXT {
        return Err(Error::Invalid(
            "Multica first turn exceeds the text budget".into(),
        ));
    }
    Ok((workspace.to_owned(), prompt))
}

/// Muster never adds a project: a destination the app's own state does not
/// list fails before anything is reserved (ADR 0220, 0221).
fn require_listed(provider: &str, directory: &std::path::Path) -> Result<(), Error> {
    #[cfg(target_os = "macos")]
    return Ok(
        muster_platform::macos::native_create::require_listed_directory(provider, directory)?,
    );
    #[cfg(not(target_os = "macos"))]
    {
        let _ = (provider, directory);
        Ok(())
    }
}

pub(super) fn check_contract(
    o: &v1::AgentSchedulerOperate,
    scope: &grant::Scope,
) -> Result<(), Error> {
    if !valid(o)
        || o.task_id != scope.task_id
        || o.directory != scope.directory
        || o.provider != scope.provider
        || hex::encode(Sha256::digest(o.text.as_bytes())) != scope.contract_sha256
    {
        return Err(Error::Unauthorized(
            "Multica first turn differs from the owner's grant".into(),
        ));
    }
    Ok(())
}

struct Pending<'a>(&'a Scheduler);
impl Drop for Pending<'_> {
    fn drop(&mut self) {
        self.0.pending_multica.lock().unwrap().take();
    }
}

/// Whether the Mac is locked, read while no Locked Use transaction holds the
/// console. A transaction unlocks the Mac under cover for its own operation
/// and locks it again when it ends, so a run that arrives meanwhile waits for
/// it instead of taking the unlocked path (ADR 0221).
async fn console_locked(daemon: &Arc<Daemon>, cancel: &CancellationToken) -> Result<bool, Error> {
    loop {
        if let Ok(_console) = daemon.locked_use.writes.try_read() {
            return Ok(daemon.platform().locked());
        }
        tokio::select! {
            _ = cancel.cancelled() => return Err(Error::Conflict("Multica creation cancelled while queued".into())),
            _ = tokio::time::sleep(Duration::from_millis(200)) => {}
        }
    }
}

pub(super) async fn start(
    daemon: &Arc<Daemon>,
    mut o: v1::AgentSchedulerOperate,
    cancel: &CancellationToken,
) -> Result<Resp, Error> {
    o.kind = "create".into();
    if !valid(&o) {
        return Err(Error::Invalid("Invalid Multica first turn".into()));
    }
    let (workspace, workspace_prompt) = native_destination(&o.task_id, &o.directory, &o.text)?;
    // The trusted enrollment names one Codex agent and runtime (ADR 0192).
    let enrolled = o.provider == "codex";
    let scheduler = &daemon.scheduler;
    let granted = || scheduler.read(|r| Ok(r.get(&o.task_id).is_some()));
    // Wait outside the provider/console locks and the once-only delivery
    // ledger. Each live Multica run owns its own cancellable wait.
    let project = if enrolled && !granted()? && !console_locked(daemon, cancel).await? {
        super::trusted_multica::verified_destination(daemon, &o, true).await?
    } else {
        None
    };
    require_listed(
        &o.provider,
        project
            .as_ref()
            .map_or(std::path::Path::new(&workspace), |route| {
                route.directory.as_path()
            }),
    )?;
    #[cfg(target_os = "macos")]
    if !granted()? && !console_locked(daemon, cancel).await? {
        let directory = project
            .as_ref()
            .map(|route| route.directory.as_path())
            .unwrap_or_else(|| std::path::Path::new(&workspace));
        let cwd = std::fs::canonicalize(directory)
            .map_err(|_| Error::Invalid("Multica workspace folder is unavailable".into()))?;
        let mut queued = false;
        loop {
            if cancel.is_cancelled() {
                return Err(Error::Conflict(
                    "Multica creation cancelled while queued".into(),
                ));
            }
            preflight(daemon, &o.provider)?;
            if granted()? || console_locked(daemon, cancel).await? {
                break;
            }
            if project
                .as_ref()
                .is_some_and(|route| !route.still_enrolled())
            {
                return Err(Error::Conflict(
                    "Multica project enrollment changed while queued".into(),
                ));
            }
            let path = cwd.clone();
            let provider = o.provider.clone();
            if tokio::task::spawn_blocking(move || {
                muster_platform::macos::native_create::multica_directory_admissible(
                    &provider, &path,
                )
            })
            .await
            .map_err(|e| Error::Unavailable(e.to_string()))??
            {
                break;
            }
            if !queued {
                log::info!("scheduler: Multica creation queued for native workspace");
                queued = true;
            }
            tokio::select! {
                _ = cancel.cancelled() => return Err(Error::Conflict("Multica creation cancelled while queued".into())),
                _ = tokio::time::sleep(Duration::from_millis(500)) => {}
            }
        }
    }
    if !granted()? && !console_locked(daemon, cancel).await? {
        let (directory, prompt) = if let Some(route) = &project {
            log::info!(
                "scheduler: verified unlocked Multica project for {}",
                o.task_id
            );
            (
                route
                    .directory
                    .to_str()
                    .ok_or_else(|| Error::Invalid("Project path must be UTF-8".into()))?
                    .to_owned(),
                route.prompt.clone(),
            )
        } else {
            (workspace, workspace_prompt)
        };
        let cancel = cancel.clone();
        let request = v1::AgentNativeCreateRequest {
            op_id: o.op_id,
            provider: o.provider,
            directory,
            prompt,
            ..Default::default()
        };
        return muster_agent::control_guard::scope(
            Arc::new(move || {
                !cancel.is_cancelled()
                    && project.as_ref().is_none_or(|route| route.still_enrolled())
            }),
            crate::native_create::create_unlocked(daemon, request, true),
        )
        .await;
    }
    if !granted()? {
        preflight(daemon, &o.provider)?;
        // A locked first turn is always created in the workspace folder.
        require_listed(&o.provider, std::path::Path::new(&workspace))?;
        if enrolled
            && let Some(route) =
                super::trusted_multica::verified_destination(daemon, &o, false).await?
        {
            if cancel.is_cancelled() || !console_locked(daemon, cancel).await? {
                return Err(Error::Conflict(
                    "Multica dispatch or locked console changed".into(),
                ));
            }
            o.text = route.prompt;
            let task_id = o.task_id.clone();
            let scope = grant::Scope {
                task_id: task_id.clone(),
                contract_sha256: hex::encode(Sha256::digest(o.text.as_bytes())),
                provider: o.provider.clone(),
                directory: o.directory.clone(),
                model: "native-app".into(),
                effort: "default".into(),
            };
            scheduler.update(daemon.store(), |registry| {
                if registry.get(&task_id).is_some() {
                    return Err(Error::Conflict(
                        "This Multica run already has a grant".into(),
                    ));
                }
                registry
                    .grant(scope, now_ms(), 90_000)
                    .map(|_| ())
                    .map_err(refused)
            })?;
            log::info!("scheduler: host verified Multica dispatch and issued grant for {task_id}");
            let result = if cancel.is_cancelled() {
                Err(Error::Conflict(
                    "Multica dispatch cancelled before execution".into(),
                ))
            } else {
                operate(daemon, o).await
            };
            scheduler.update(daemon.store(), |registry| {
                registry.revoke(&task_id, now_ms()).map_err(refused)
            })?;
            log::info!("scheduler: trusted Multica grant closed for {task_id}");
            return result;
        }
        let _pending = {
            let mut slot = scheduler.pending_multica.lock().unwrap();
            if slot.is_some() {
                return Err(Error::Conflict(
                    "Another Multica first turn is awaiting authorization".into(),
                ));
            }
            *slot = Some(v1::AgentSchedulerStatus {
                task_id: o.task_id.clone(),
                provider: o.provider.clone(),
                directory: o.directory.clone(),
                contract_sha256: hex::encode(Sha256::digest(o.text.as_bytes())),
                op: Some(v1::AgentSchedulerOp {
                    op_id: o.op_id.clone(),
                    role: o.role.clone(),
                    kind: o.kind.clone(),
                    state: "awaiting_grant".into(),
                    message: o.text.clone(),
                    ..Default::default()
                }),
                ..Default::default()
            });
            Pending(scheduler)
        };
        let until = tokio::time::Instant::now() + Duration::from_secs(60);
        while !granted()? {
            tokio::select! {
                _ = cancel.cancelled() => return Err(Error::Conflict("Multica request cancelled before authorization".into())),
                _ = tokio::time::sleep_until(until) => return Err(Error::Unauthorized("No local grant for this Multica first turn".into())),
                _ = tokio::time::sleep(Duration::from_millis(200)) => {}
            }
        }
    }
    if cancel.is_cancelled() {
        return Err(Error::Conflict(
            "Multica request cancelled before submission".into(),
        ));
    }
    operate(daemon, o).await
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn a_run_reads_the_lock_only_once_locked_use_gives_the_console_back() {
        let dir = tempfile::tempdir().unwrap();
        let mut config = crate::DaemonConfig::new(dir.path().join("state"));
        config.listen = false;
        config.agent = None;
        config.cloud = false;
        config.bind = "127.0.0.1:0".parse().unwrap();
        let daemon = Daemon::start_with(config, Arc::new(crate::DefaultPlatform), None)
            .await
            .unwrap();
        let cancel = CancellationToken::new();
        let held = daemon.locked_use.writes.clone().write_owned().await;
        let early =
            tokio::time::timeout(Duration::from_millis(500), console_locked(&daemon, &cancel))
                .await;
        assert!(early.is_err(), "read while a transaction held the console");
        drop(held);
        assert_eq!(
            console_locked(&daemon, &cancel).await.unwrap(),
            daemon.platform().locked()
        );
        let _held = daemon.locked_use.writes.clone().write_owned().await;
        cancel.cancel();
        assert!(matches!(
            console_locked(&daemon, &cancel).await,
            Err(Error::Conflict(_))
        ));
    }
    #[test]
    fn grant_is_for_one_unchanged_first_turn() {
        let mut o = v1::AgentSchedulerOperate {
            task_id: "multica:test-run".into(),
            op_id: "multica:test-run".into(),
            role: "producer".into(),
            kind: "create".into(),
            provider: "codex".into(),
            directory: "/work".into(),
            text: "first".into(),
            ..Default::default()
        };
        let scope = grant::Scope {
            task_id: o.task_id.clone(),
            contract_sha256: hex::encode(Sha256::digest(b"first")),
            provider: o.provider.clone(),
            directory: o.directory.clone(),
            model: "native-app".into(),
            effort: "default".into(),
        };
        assert!(check_contract(&o, &scope).is_ok());
        for changed in [
            v1::AgentSchedulerOperate {
                text: "second".into(),
                ..o.clone()
            },
            v1::AgentSchedulerOperate {
                directory: "/other".into(),
                ..o.clone()
            },
            v1::AgentSchedulerOperate {
                op_id: "another".into(),
                ..o.clone()
            },
            v1::AgentSchedulerOperate {
                kind: "send".into(),
                ..o.clone()
            },
            v1::AgentSchedulerOperate {
                role: "reviewer".into(),
                ..o.clone()
            },
        ] {
            assert!(check_contract(&changed, &scope).is_err());
        }
        o.task_id = "multica:other".into();
        o.op_id = o.task_id.clone();
        assert!(check_contract(&o, &scope).is_err());
    }

    #[test]
    fn both_providers_create_in_the_workspace_folder_under_the_runs_grant() {
        let run = "6df77065-0000-4000-8000-000000000001";
        let workdir = "/srv/multica/ws-1/6df77065/workdir";
        for provider in ["codex", "claude"] {
            let o = v1::AgentSchedulerOperate {
                task_id: format!("multica:{run}"),
                op_id: format!("multica:{run}"),
                role: "producer".into(),
                kind: "create".into(),
                provider: provider.into(),
                directory: workdir.into(),
                text: "first".into(),
                ..Default::default()
            };
            assert!(valid(&o));
            let scope = grant::Scope {
                task_id: o.task_id.clone(),
                contract_sha256: hex::encode(Sha256::digest(b"first")),
                provider: provider.into(),
                directory: workdir.into(),
                model: "native-app".into(),
                effort: "default".into(),
            };
            let parsed = parse(&o, &scope).unwrap();
            let Inner::Create(r) = parsed.inner else {
                panic!("a first turn creates a session");
            };
            assert_eq!(r.directory, "/srv/multica/ws-1");
            assert_eq!(
                r.prompt,
                muster_agent::native_runtime::multica_workspace_prompt(workdir, "first")
            );
            // The grant still binds the run's own directory.
            let moved = v1::AgentSchedulerOperate {
                directory: "/srv/multica/ws-1".into(),
                ..o.clone()
            };
            assert!(check_contract(&moved, &scope).is_err());
        }
        let other = v1::AgentSchedulerOperate {
            task_id: format!("multica:{run}"),
            op_id: format!("multica:{run}"),
            role: "producer".into(),
            kind: "create".into(),
            provider: "gemini".into(),
            directory: workdir.into(),
            text: "first".into(),
            ..Default::default()
        };
        assert!(!valid(&other));
        assert!(matches!(
            native_destination(&other.task_id, "/srv/multica/ws-1/other/workdir", "first"),
            Err(Error::Invalid(_))
        ));
    }
}
