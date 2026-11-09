//! Attended native sessions. This path cannot launch a process or dispatch a scheduled run.
use super::*;
use muster_agent::deepseek_app::{self, TaskAction};
use muster_store::multica::{NativeSession, Phase, Receipt, Reservation, Task};
use sha2::{Digest, Sha256};

const SLOT: &str = "attended-session-v1";
const ROLE: &str = "workbench";

// Never bring the CLI's Token line back across the container boundary.
const PRINCIPAL: &str = r#"s=$(multica auth status 2>&1) || exit 1
printf '%s\n' "$s" | sed -n '/^Server:/p; /^User:/p'"#;
fn principal(raw: &[u8]) -> Result<(String, String), Error> {
    let bad = || Error::Unavailable("Cannot verify the active Multica account".into());
    let input = std::str::from_utf8(raw).map_err(|_| bad())?;
    let mut lines = input.lines();
    let endpoint = lines
        .next()
        .and_then(|s| s.strip_prefix("Server:"))
        .ok_or_else(bad)?
        .trim();
    let url = url::Url::parse(endpoint).map_err(|_| bad())?;
    let user = lines
        .next()
        .and_then(|s| s.strip_prefix("User:"))
        .and_then(|s| s.trim().strip_suffix(')'))
        .and_then(|s| s.rsplit_once(" ("))
        .map(|(_, email)| email)
        .ok_or_else(bad)?;
    if lines.next().is_some()
        || !user.contains('@')
        || user.len() > 320
        || user.chars().any(char::is_whitespace)
        || url.scheme() != "https"
        || url.host().is_none()
        || !url.username().is_empty()
        || url.password().is_some()
        || url.query().is_some()
        || url.fragment().is_some()
    {
        return Err(bad());
    }
    Ok((url.to_string(), user.to_owned()))
}

fn service_id(
    login: &(String, String),
    workspace: &str,
    rows: &serde_json::Value,
) -> Result<String, Error> {
    let bad = || Error::Unauthorized("No unique authenticated member in this workspace".into());
    let mut matching = rows.as_array().ok_or_else(bad)?.iter().filter(|r| {
        r["workspace_id"] == workspace && r["email"].as_str() == Some(login.1.as_str())
    });
    let id = matching
        .next()
        .and_then(|r| r["user_id"].as_str())
        .filter(|id| !id.is_empty() && id.len() <= 128)
        .ok_or_else(bad)?;
    if matching.next().is_some() {
        return Err(bad());
    }
    let mut h = Sha256::new();
    h.update(b"Muster.Multica.Account.v1\0");
    h.update(login.0.as_bytes());
    h.update([0]);
    h.update(id.as_bytes());
    Ok(hex::encode(h.finalize()))
}

impl Manager {
    async fn account(&self, c: &Config) -> Result<(String, String), Error> {
        let bytes = self
            .command(&["exec", &c.container_name, "sh", "-c", PRINCIPAL], 10)
            .await
            .map_err(|_| Error::Unavailable("Check the Multica container login".into()))?;
        principal(&bytes)
    }

    pub(super) async fn task_session(
        &self,
        daemon: &crate::Daemon,
        c: &Config,
        r: &v1::MulticaMutation,
        a: &v1::MulticaIssueAction,
    ) -> Result<String, Error> {
        use v1::multica_issue_action::Kind;
        let unlocked = || {
            if daemon.platform().locked() {
                Err(Error::Unauthorized(
                    "Unlock the host Mac to manage DSH task sessions".into(),
                ))
            } else {
                Ok(())
            }
        };
        unlocked()?;
        let session_gate = self
            .app_session_gate
            .clone()
            .try_lock_owned()
            .map_err(|_| Error::Busy("DSH App is processing a task session".into()))?;
        let create = a.kind == Kind::CreateDshSession as i32;
        let account = self.account(c).await?;
        let members = self
            .query(
                c,
                &r.workspace_id,
                &["workspace", "member", "list", &r.workspace_id],
            )
            .await?;
        let service = service_id(&account, &r.workspace_id, &members)?;
        self.resource(c, &r.workspace_id, "issue", &a.id, &a.revision)
            .await?;
        if self.account(c).await? != account {
            return Err(Error::Conflict("Multica account changed".into()));
        }
        let device = daemon.device_id().to_string();
        let task = Task {
            device,
            service,
            workspace: r.workspace_id.clone(),
            task: a.id.clone(),
        };
        let agent = daemon
            .agent()
            .ok_or_else(|| Error::Unavailable("DSH App service is unavailable".into()))?;
        let store = agent.deepseek_task_store()?;
        let link = agent
            .deepseek_link()
            .ok_or_else(|| Error::Unavailable("Open DSH App with its desktop plugin".into()))?;
        let host = link.task_host()?;
        if !crate::agent_bridge::deepseek_desktop(daemon.platform().as_ref(), &host) {
            return Err(Error::Unavailable(
                "The plugin is not hosted by the running DSH desktop App".into(),
            ));
        }
        let registry = daemon.store.clone();
        let create_cwd = create.then(|| a.value.clone());
        let op = r.op_id.clone();
        let platform = daemon.platform().clone();
        // Keep exclusion and durable settlement alive even if the caller disconnects.
        muster_agent::control_guard::spawn_blocking(move || {
            let _gate = session_gate;
            open_slot(
                &registry,
                task,
                &store,
                &op,
                create_cwd.as_deref(),
                |action, reservation| {
                    if platform.locked()
                        || !crate::agent_bridge::deepseek_desktop(platform.as_ref(), &host)
                    {
                        return Err(Error::Unavailable(
                            "Original unlocked DSH App is unavailable".into(),
                        ));
                    }
                    link.task_session(
                        &host,
                        action,
                        &reservation.native.session,
                        &op,
                        &reservation.canonical_cwd,
                        &reservation.native.store,
                    )
                    .map_err(Error::from)
                },
            )
        })
        .await
        .map_err(|_| Error::UnknownOutcome("Lost the DSH App operation result".into()))?
    }
}

fn open_slot(
    registry: &muster_store::Store,
    task: Task,
    store: &str,
    op: &str,
    create_cwd: Option<&str>,
    mut call: impl FnMut(TaskAction, &Reservation) -> Result<(), Error>,
) -> Result<String, Error> {
    let device = task.device.clone();
    let create = create_cwd.is_some();
    let existing = registry.multica_task_slot(&task, SLOT, ROLE)?;
    let record = match (create_cwd, existing) {
        (Some(_), Some(_)) => {
            return Err(Error::Conflict(
                "This task has a reserved session; use Reopen original session".into(),
            ));
        }
        (Some(value), None) => {
            let cwd = Path::new(value)
                .canonicalize()
                .ok()
                .filter(|p| p.is_dir())
                .and_then(|p| p.to_str().map(str::to_owned))
                .ok_or_else(|| {
                    Error::Invalid("Choose an existing directory on the host Mac".into())
                })?;
            let native = NativeSession {
                device: device.clone(),
                provider: "deepseek".into(),
                store: store.to_owned(),
                session: deepseek_app::fresh_task_session(),
            };
            registry.multica_reserve(&Reservation {
                task,
                run: SLOT.into(),
                role: ROLE.into(),
                profile: "desktop".into(),
                generation: 1,
                native,
                canonical_cwd: cwd,
                requester: device,
                operation: op.to_owned(),
                provider_request: op.to_owned(),
            })?
        }
        (None, Some(record))
            if matches!(
                record.phase,
                Phase::Bound | Phase::Unconfirmed | Phase::Reserved
            ) =>
        {
            record
        }
        _ => {
            return Err(Error::Unavailable(
                "No recoverable original DSH App session for this task".into(),
            ));
        }
    };
    let native = &record.reservation.native;
    if native.store != store
        || native.provider != "deepseek"
        || record.reservation.generation != 1
        || record.reservation.profile != "desktop"
    {
        return Err(Error::Conflict(
            "Original DSH App profile/store has changed".into(),
        ));
    }
    let reference = record.reference.clone();
    let recovery = !create && record.phase != Phase::Bound;
    if recovery {
        call(TaskAction::Inspect, &record.reservation)?;
    }
    let result = call(
        if create {
            TaskAction::Create
        } else {
            TaskAction::Resume
        },
        &record.reservation,
    );
    if let Err(error) = result {
        if create {
            registry.multica_unconfirmed(&reference)?;
        }
        return Err(error);
    }
    if create || recovery {
        registry
            .multica_bind(
                &reference,
                &Receipt {
                    native: native.clone(),
                    canonical_cwd: record.reservation.canonical_cwd,
                    provider_request: record.reservation.provider_request,
                    evidence: "verified signed desktop host and native header v1".into(),
                },
            )
            .map_err(|_| {
                Error::UnknownOutcome(
                    "DSH App responded but the task binding was not confirmed".into(),
                )
            })?;
    }
    Ok(reference)
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    #[cfg(target_os = "macos")]
    #[test]
    #[ignore = "requires the running signed DSH desktop host; read-only identity check"]
    fn installed_desktop_host_parent_matches() {
        use muster_platform::macos::codesign::{process_start, signed_gui_child};
        let pid: u32 = std::env::var("MUSTER_TEST_DSH_HOST_PID")
            .unwrap()
            .parse()
            .unwrap();
        let start = process_start(pid).unwrap();
        let requirement = muster_core::apps::DEEPSEEK.requirement();
        assert!(signed_gui_child(pid, start, &requirement));
        assert!(!signed_gui_child(pid, start + 1, &requirement));
        assert!(!signed_gui_child(
            std::process::id(),
            process_start(std::process::id()).unwrap(),
            &requirement
        ));
    }
    #[test]
    fn lost_create_and_cold_reopen_never_create_another_native_session() {
        use muster_store::Store;
        let dir = tempfile::tempdir().unwrap();
        let cwd = dir.path().canonicalize().unwrap();
        let cwd = cwd.to_str().unwrap();
        let db = dir.path().join("owner.db");
        let registry = Store::open(&db).unwrap();
        let task = Task {
            device: "host".into(),
            service: "account".into(),
            workspace: "workspace".into(),
            task: "issue".into(),
        };
        let mut original = None;
        let lost = open_slot(
            &registry,
            task.clone(),
            "store",
            "first",
            Some(cwd),
            |action, r| {
                assert_eq!(action, TaskAction::Create);
                assert_eq!(registry.multica_owner(&r.native)?.as_ref(), Some(&task));
                original = Some(r.native.clone());
                Err(Error::UnknownOutcome("lost receipt".into()))
            },
        );
        assert!(matches!(lost, Err(Error::UnknownOutcome(_))));
        drop(registry);
        let registry = Store::open(&db).unwrap();
        for (t, store, create) in [
            (task.clone(), "store", Some(cwd)),
            (
                Task {
                    service: "another".into(),
                    ..task.clone()
                },
                "store",
                None,
            ),
            (
                Task {
                    workspace: "other".into(),
                    ..task.clone()
                },
                "store",
                None,
            ),
            (
                Task {
                    device: "other".into(),
                    ..task.clone()
                },
                "store",
                None,
            ),
            (task.clone(), "changed", None),
        ] {
            assert!(
                open_slot(&registry, t, store, "new-op", create, |_, _| panic!(
                    "unexpected host call"
                ))
                .is_err()
            );
        }
        assert!(
            open_slot(
                &registry,
                task.clone(),
                "store",
                "missing",
                None,
                |action, _| {
                    assert_eq!(action, TaskAction::Inspect);
                    Err(Error::NotFound("missing original".into()))
                }
            )
            .is_err()
        );
        let mut calls = vec![];
        let reference = open_slot(
            &registry,
            task.clone(),
            "store",
            "recover",
            None,
            |action, r| {
                assert_eq!(Some(&r.native), original.as_ref());
                calls.push(action);
                Ok(())
            },
        )
        .unwrap();
        assert_eq!(calls, [TaskAction::Inspect, TaskAction::Resume]);
        assert_eq!(
            registry.multica_record(&reference).unwrap().unwrap().phase,
            Phase::Bound
        );
        assert_eq!(
            open_slot(&registry, task, "store", "open-again", None, |action, r| {
                assert_eq!(action, TaskAction::Resume);
                assert_eq!(Some(&r.native), original.as_ref());
                Ok(())
            })
            .unwrap(),
            reference
        );
    }

    #[test]
    fn guest_principal_filter_keeps_token_and_diagnostics_inside_container() {
        let script = format!(
            "multica() {{ printf '%s\\n' 'Server: https://example.invalid' 'User: Example (user@example.invalid)' 'Token: test-only' 'diagnostic' >&2; }}\n{PRINCIPAL}"
        );
        let output = std::process::Command::new("sh")
            .args(["-c", &script])
            .output()
            .unwrap();
        assert!(output.status.success() && output.stderr.is_empty());
        assert_eq!(principal(&output.stdout).unwrap().1, "user@example.invalid");
    }

    #[test]
    fn binding_requires_exact_account_and_workspace() {
        let login =
            principal(b"Server: https://example.invalid\nUser: Example (user@example.invalid)\n")
                .unwrap();
        let row =
            json!({"user_id":"member-one","workspace_id":"ws","email":"user@example.invalid"});
        let rows = json!([row]);
        let first = service_id(&login, "ws", &rows).unwrap();
        assert!(service_id(&login, "other", &rows).is_err());
        assert!(service_id(&login, "ws", &json!([row, row])).is_err());
        assert_ne!(
            first,
            service_id(
                &login,
                "ws",
                &json!([{"user_id":"another","workspace_id":"ws","email":"user@example.invalid"}])
            )
            .unwrap()
        );
        assert!(principal(b"Server: https://example.invalid\nUser: Example\n").is_err());
    }
}
