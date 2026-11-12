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
        && o.provider == "codex"
        && o.role == "producer"
        && o.kind == "create"
        && o.session.is_empty()
        && o.model.is_empty()
        && o.effort.is_empty()
        && !o.text.trim().is_empty()
        && o.text.len() <= MAX_TEXT
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

pub(super) async fn start(
    daemon: &Arc<Daemon>,
    mut o: v1::AgentSchedulerOperate,
    cancel: &CancellationToken,
) -> Result<Resp, Error> {
    o.kind = "create".into();
    if !valid(&o) {
        return Err(Error::Invalid("Invalid Multica Codex first turn".into()));
    }
    let scheduler = &daemon.scheduler;
    let granted = || scheduler.read(|r| Ok(r.get(&o.task_id).is_some()));
    // Wait outside the provider/console locks and the once-only delivery
    // ledger. Each live Multica run owns its own cancellable wait.
    let project = if !granted()? && !daemon.platform().locked() {
        // Failure to qualify for project access only permits the original run directory.
        super::trusted_multica::verified_destination(daemon, &o, true)
            .await
            .unwrap_or_default()
    } else {
        None
    };
    #[cfg(target_os = "macos")]
    if !granted()? && !daemon.platform().locked() {
        let directory = project
            .as_ref()
            .map(|(path, _)| path.as_path())
            .unwrap_or_else(|| std::path::Path::new(&o.directory));
        let cwd = std::fs::canonicalize(directory)
            .map_err(|_| Error::Invalid("Multica working directory is unavailable".into()))?;
        let mut queued = false;
        loop {
            if cancel.is_cancelled() {
                return Err(Error::Conflict(
                    "Multica creation cancelled while queued".into(),
                ));
            }
            preflight(daemon, "codex")?;
            if daemon.platform().locked() || granted()? {
                break;
            }
            let path = cwd.clone();
            if tokio::task::spawn_blocking(move || {
                muster_platform::macos::native_create::multica_directory_admissible(&path)
            })
            .await
            .map_err(|e| Error::Unavailable(e.to_string()))?
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
    if !granted()? && !daemon.platform().locked() {
        if let Some((directory, prompt)) = project {
            if std::fs::canonicalize(&directory).ok().as_ref() != Some(&directory) {
                return Err(Error::Conflict("Enrolled project directory changed".into()));
            }
            o.directory = directory
                .into_os_string()
                .into_string()
                .map_err(|_| Error::Invalid("Project path must be UTF-8".into()))?;
            o.text = prompt;
            log::info!(
                "scheduler: verified unlocked Multica project for {}",
                o.task_id
            );
        }
        let cancel = cancel.clone();
        let request = v1::AgentNativeCreateRequest {
            op_id: o.op_id,
            provider: o.provider,
            directory: o.directory,
            prompt: o.text,
            ..Default::default()
        };
        return muster_agent::control_guard::scope(
            Arc::new(move || !cancel.is_cancelled()),
            crate::native_create::create_unlocked(daemon, request, true),
        )
        .await;
    }
    if !granted()? {
        preflight(daemon, "codex")?;
        if let Some((_, prompt)) =
            super::trusted_multica::verified_destination(daemon, &o, false).await?
        {
            if cancel.is_cancelled() || !daemon.platform().locked() {
                return Err(Error::Conflict(
                    "Multica dispatch or locked console changed".into(),
                ));
            }
            o.text = prompt;
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
}
