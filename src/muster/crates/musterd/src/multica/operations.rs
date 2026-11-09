//! Host-local typed writes. The CLI owns service authentication; Rust owns
//! validation, exact resource selection and no-replay delivery tracking.
use super::{browse, *};
use muster_proto::{Message, v1::multica_mutation::Action};
use serde_json::Value;
use tokio::io::{AsyncReadExt, AsyncWriteExt};

const MAX_OUTPUT: u64 = 4 * 1024 * 1024;

pub(crate) async fn mutate(
    daemon: &crate::Daemon,
    request: v1::MulticaMutation,
) -> Result<v1::response::Resp, Error> {
    tracked(daemon, &daemon.multica, request).await
}
async fn tracked(
    daemon: &crate::Daemon,
    manager: &Manager,
    request: v1::MulticaMutation,
) -> Result<v1::response::Resp, Error> {
    if let Err(error) = validate(&request) {
        return Ok(v1::response::Resp::OpResult(v1::OpResult {
            status: v1::op_result::Status::Rejected as i32,
            message: error.to_string(),
            ..Default::default()
        }));
    }
    let operation = format!("multica:{}", request.op_id);
    let mut body = b"muster.multica.mutation.v1\0".to_vec();
    body.extend(request.encode_to_vec());
    crate::handler::ledgered(
        daemon,
        &operation,
        "multica.operation",
        &request.workspace_id,
        false,
        &body,
        manager.mutate(daemon, &request),
    )
    .await
}

fn id(value: &str) -> Result<(), Error> {
    if value.is_empty() {
        return Err(Error::Invalid("Select an exact Multica identifier".into()));
    }
    browse::validate_request(&v1::MulticaBrowseRequest {
        workspace_id: value.into(),
        ..Default::default()
    })
}
fn text(value: &str, max: usize, required: bool) -> Result<(), Error> {
    if value.len() > max || value.contains('\0') || required && value.trim().is_empty() {
        return Err(Error::Invalid(
            "Multica input is empty, too long or contains a null byte".into(),
        ));
    }
    Ok(())
}
fn revision(value: &str) -> Result<(), Error> {
    if value.len() != 64 || !value.bytes().all(|c| c.is_ascii_hexdigit()) {
        return Err(Error::Invalid(
            "Refresh Multica details before editing".into(),
        ));
    }
    Ok(())
}
fn validate(request: &v1::MulticaMutation) -> Result<(), Error> {
    id(&request.op_id)?;
    id(&request.workspace_id)?;
    if request.encoded_len() > 128 * 1024 {
        return Err(Error::Invalid("Multica request too large".into()));
    }
    match request
        .action
        .as_ref()
        .ok_or_else(|| Error::Invalid("Select a Multica operation".into()))?
    {
        Action::Agent(edit) => {
            id(&edit.id)?;
            revision(&edit.revision)?;
            text(&edit.name, 256, true)?;
            text(&edit.description, 32 * 1024, false)?;
            text(&edit.instructions, 64 * 1024, false)?;
            text(&edit.model, 256, false)?;
            if !(1..=100).contains(&edit.max_concurrent_tasks) {
                return Err(Error::Invalid(
                    "Concurrent task limit must be between 1 and 100".into(),
                ));
            }
        }
        Action::Issue(edit) => {
            if !edit.id.is_empty() {
                id(&edit.id)?;
                revision(&edit.revision)?;
            } else if !edit.revision.is_empty() {
                return Err(Error::Invalid("New issue cannot carry a revision".into()));
            }
            text(&edit.title, 512, true)?;
            text(&edit.description, 64 * 1024, false)?;
        }
        Action::IssueAction(action) => {
            use v1::multica_issue_action::Kind;
            id(&action.id)?;
            revision(&action.revision)?;
            match Kind::try_from(action.kind) {
                Ok(Kind::Assign | Kind::Status | Kind::Cancel) => id(&action.value)?,
                Ok(Kind::Comment) => {
                    text(&action.value, 64 * 1024, true)?;
                    if action.value.contains('@')
                        || action.value.to_ascii_lowercase().contains("mention:")
                    {
                        return Err(Error::Unavailable("@ scheduling requires a verified App task binding; submit a plain comment instead".into()));
                    }
                }
                Ok(Kind::CreateDshSession) if Path::new(&action.value).is_absolute() => {
                    text(&action.value, 1024, true)?
                }
                Ok(Kind::Rerun | Kind::ResumeDshSession) if action.value.is_empty() => {}
                _ => return Err(Error::Invalid("Unsupported Multica issue operation".into())),
            }
        }
    }
    Ok(())
}

struct Prepared {
    args: Vec<String>,
    stdin: Option<String>,
    resource: String,
    creates_issue: bool,
}
impl Prepared {
    fn new(args: &[&str], resource: &str) -> Self {
        Self {
            args: args.iter().map(|s| (*s).into()).collect(),
            stdin: None,
            resource: resource.into(),
            creates_issue: false,
        }
    }
    fn flag(&mut self, name: &str, value: &str) {
        self.args.push(format!("--{name}={value}"));
    }
    fn input(&mut self, name: &str, value: &str) {
        self.args.push(format!("--{name}-stdin"));
        self.stdin = Some(value.into());
    }
}

impl Manager {
    pub(super) async fn resource(
        &self,
        c: &Config,
        workspace: &str,
        kind: &str,
        resource: &str,
        expected: &str,
    ) -> Result<Value, Error> {
        let data = self.query(c, workspace, &[kind, "get", resource]).await?;
        browse::check_workspace(&data, workspace)?;
        if browse::field(&data, "id") != resource {
            return Err(Error::Invalid(
                "Multica returned a different resource".into(),
            ));
        }
        if !expected.is_empty() && browse::revision(&data) != expected {
            return Err(Error::Conflict(
                "Multica details changed; refresh before submitting".into(),
            ));
        }
        Ok(data)
    }
    async fn prepare(&self, c: &Config, r: &v1::MulticaMutation) -> Result<Prepared, Error> {
        let workspace = &r.workspace_id;
        let workspaces = self.query(c, "", &["workspace", "list"]).await?;
        if !workspaces
            .as_array()
            .is_some_and(|rows| rows.iter().any(|w| w["id"] == *workspace))
        {
            return Err(Error::Unauthorized("Multica workspace unavailable".into()));
        }
        match r.action.as_ref().expect("validated action") {
            Action::Agent(edit) => {
                let before = self
                    .resource(c, workspace, "agent", &edit.id, &edit.revision)
                    .await?;
                if before["viewer_is_editor"] != true
                    || before["sensitive_config_redacted"] != false
                {
                    return Err(Error::Unauthorized(
                        "This account cannot edit the agent configuration".into(),
                    ));
                }
                let mut p = Prepared::new(&["agent", "update", &edit.id], &edit.id);
                // Patch changed fields only; unrelated provider/identity fields
                // never enter the ABI or command line.
                for (flag, value) in [
                    ("name", &edit.name),
                    ("description", &edit.description),
                    ("instructions", &edit.instructions),
                    ("model", &edit.model),
                ] {
                    if browse::field(&before, flag) != *value {
                        p.flag(flag, value);
                    }
                }
                if before["max_concurrent_tasks"].as_u64() != Some(edit.max_concurrent_tasks.into())
                {
                    p.flag(
                        "max-concurrent-tasks",
                        &edit.max_concurrent_tasks.to_string(),
                    );
                }
                if p.args.len() == 3 {
                    return Err(Error::Invalid("No agent configuration changes".into()));
                }
                Ok(p)
            }
            Action::Issue(edit) => {
                let mut p = if edit.id.is_empty() {
                    let mut p = Prepared::new(&["issue", "create"], "");
                    p.creates_issue = true;
                    p.flag("status", "backlog");
                    p
                } else {
                    self.resource(c, workspace, "issue", &edit.id, &edit.revision)
                        .await?;
                    let mut p = Prepared::new(&["issue", "update", &edit.id], &edit.id);
                    p.args.push("--no-start".into());
                    p
                };
                p.flag("title", &edit.title);
                p.input("description", &edit.description);
                Ok(p)
            }
            Action::IssueAction(action) => {
                use v1::multica_issue_action::Kind;
                self.resource(c, workspace, "issue", &action.id, &action.revision)
                    .await?;
                let p = match Kind::try_from(action.kind).expect("validated kind") {
                    Kind::Assign => {
                        // Only exact agents in this workspace are offered by
                        // this UI; no fuzzy names, users, or cross-workspace IDs.
                        let agent = self
                            .resource(c, workspace, "agent", &action.value, "")
                            .await?;
                        if agent["can_use"] != true || !agent["archived_at"].is_null() {
                            return Err(Error::Unauthorized(
                                "This agent cannot be assigned".into(),
                            ));
                        }
                        let mut p = Prepared::new(&["issue", "assign", &action.id], &action.id);
                        p.flag("to-id", &action.value);
                        p.args.push("--no-start".into());
                        p
                    }
                    Kind::Status => {
                        let mut p = Prepared::new(&["issue", "status", &action.id, &action.value], &action.id);
                        p.args.push("--no-start".into());
                        p
                    }
                    Kind::Comment => {
                        let mut p =
                            Prepared::new(&["issue", "comment", "add", &action.id], &action.id);
                        p.input("content", &action.value);
                        p
                    }
                    Kind::Rerun => return Err(Error::Unavailable("Starting or rerunning requires a verified App task binding; no CLI runtime is substituted".into())),
                    Kind::Cancel => {
                        let runs = self
                            .query(c, workspace, &["issue", "runs", &action.id])
                            .await?;
                        if !browse::runs_projection(&runs, workspace, &action.id)?
                            .iter()
                            .any(|run| run.id == action.value && run.can_cancel)
                        {
                            return Err(Error::Invalid(
                                "Select a queued or running task from this issue".into(),
                            ));
                        }
                        let mut p =
                            Prepared::new(&["issue", "cancel-task", &action.value], &action.id);
                        p.flag("issue", &action.id);
                        p
                    }
                    Kind::CreateDshSession | Kind::ResumeDshSession | Kind::Unspecified => unreachable!(),
                };
                Ok(p)
            }
        }
    }
    async fn mutate(
        &self,
        daemon: &crate::Daemon,
        r: &v1::MulticaMutation,
    ) -> Result<String, Error> {
        if !cfg!(target_os = "macos") {
            return Err(Error::Unavailable(
                "Multica host operations require macOS".into(),
            ));
        }
        // Also excludes installation/config changes while this snapshot executes.
        let _gate = self
            .gate
            .try_lock()
            .map_err(|_| Error::Busy("Multica is processing another operation".into()))?;
        let (c, row) = self.observed(self.load()?).await?;
        if !row
            .as_ref()
            .is_some_and(|v| v["status"]["state"] == "running")
        {
            return Err(Error::Unavailable(
                "Start the Multica container first".into(),
            ));
        }
        if let Some(Action::IssueAction(a)) = &r.action {
            use v1::multica_issue_action::Kind;
            if matches!(
                Kind::try_from(a.kind),
                Ok(Kind::CreateDshSession | Kind::ResumeDshSession)
            ) {
                return self.task_session(daemon, &c, r, a).await;
            }
        }
        let prepared = self.prepare(&c, r).await?;
        self.submit(&c, &r.workspace_id, prepared).await
    }
    async fn submit(&self, c: &Config, workspace: &str, p: Prepared) -> Result<String, Error> {
        let executable = self.executable.clone().map(Ok).unwrap_or_else(binary)?;
        let mut command = tokio::process::Command::new(executable);
        command.args([
            "exec",
            "-i",
            &c.container_name,
            "multica",
            "--workspace-id",
            workspace,
        ]);
        command
            .args(&p.args)
            .args(["--output", "json"])
            .stdin(std::process::Stdio::piped())
            .stdout(std::process::Stdio::piped())
            .stderr(std::process::Stdio::null())
            .kill_on_drop(true);
        // Failure to spawn is definitively before submission. Every failure
        // after this point may have reached the server and is never retried.
        let mut child = command.spawn().map_err(io)?;
        let unknown = || {
            Error::UnknownOutcome("Multica result is unconfirmed; inspect current details and execution history before taking another action".into())
        };
        let result = tokio::time::timeout(Duration::from_secs(45), async {
            let mut input = child.stdin.take().ok_or_else(unknown)?;
            let stdout = child.stdout.take().ok_or_else(unknown)?;
            let mut output = Vec::new();
            tokio::try_join!(
                async {
                    input
                        .write_all(p.stdin.as_deref().unwrap_or("").as_bytes())
                        .await
                        .map_err(|_| unknown())?;
                    input.shutdown().await.map_err(|_| unknown())?;
                    drop(input);
                    Ok(())
                },
                async {
                    stdout
                        .take(MAX_OUTPUT + 1)
                        .read_to_end(&mut output)
                        .await
                        .map_err(|_| unknown())?;
                    if output.len() as u64 > MAX_OUTPUT {
                        return Err(unknown());
                    }
                    Ok(())
                }
            )?;
            if !child.wait().await.map_err(|_| unknown())?.success() {
                return Err(unknown());
            }
            let value: Value = serde_json::from_slice(&output).map_err(|_| unknown())?;
            if !value.is_object() || value.get("error").is_some_and(|e| !e.is_null()) {
                return Err(unknown());
            }
            if p.creates_issue {
                let created = browse::field(&value, "id");
                id(&created).map_err(|_| unknown())?;
                browse::check_workspace(&value, workspace).map_err(|_| unknown())?;
                Ok(created)
            } else {
                Ok(p.resource)
            }
        })
        .await;
        result.map_err(|_| unknown())?
    }
}

#[cfg(all(test, target_os = "macos"))]
mod tests {
    use super::*;
    use serde_json::json;
    use std::os::unix::fs::PermissionsExt;
    use v1::op_result::Status;

    fn issue() -> Value {
        json!({"id":"issue-a","workspace_id":"workspace-a","title":"Task","description":"Before","status":"todo","updated_at":"v1"})
    }
    fn agent() -> Value {
        json!({"id":"agent-a","workspace_id":"workspace-a","name":"Agent","instructions":"Before","max_concurrent_tasks":2,"viewer_is_editor":true,"sensitive_config_redacted":false,"can_use":true})
    }
    fn fixture(dir: &Path) -> Manager {
        let executable = dir.join("container");
        std::fs::write(dir.join("issue.json"), issue().to_string()).unwrap();
        std::fs::write(dir.join("agent.json"), agent().to_string()).unwrap();
        std::fs::write(&executable, r#"#!/usr/bin/python3
import sys,json
from pathlib import Path
root=Path(__file__).parent
a=sys.argv[1:]
if a==['list','--all','--format','json']:
 print(json.dumps([{'id':'mlt','status':{'state':'running'}}]));sys.exit(0)
write=a[1]=='-i'
if write:a.pop(1)
a=a[3:]
if a[:1]==['--workspace-id']:a=a[2:]
if write:
 with (root/'writes').open('a') as f:f.write(json.dumps({'args':a,'input':sys.stdin.read()})+'\n')
 if (root/'fail').exists():sys.exit(1)
 print(json.dumps({'id':'created','workspace_id':'workspace-a'}));sys.exit(0)
if a[:2]==['workspace','list']:print(json.dumps([{'id':'workspace-a'}]))
elif a[:2]==['issue','get']:print((root/'issue.json').read_text())
elif a[:2]==['agent','get']:print((root/'agent.json').read_text())
elif a[:2]==['issue','runs']:print(json.dumps([{'id':'run-a','issue_id':'issue-a','workspace_id':'workspace-a','status':'running'}]))
else:sys.exit(2)
"#).unwrap();
        std::fs::set_permissions(&executable, std::fs::Permissions::from_mode(0o755)).unwrap();
        let mut m = Manager::new(dir);
        m.executable = Some(executable);
        m.save(&Config {
            container_name: "mlt".into(),
            server_url: "https://example.invalid".into(),
            app_url: "https://example.invalid".into(),
            ..Default::default()
        })
        .unwrap();
        m
    }
    fn request(action: Action) -> v1::MulticaMutation {
        v1::MulticaMutation {
            op_id: "op-a".into(),
            workspace_id: "workspace-a".into(),
            action: Some(action),
        }
    }
    fn issue_action(kind: v1::multica_issue_action::Kind, value: &str) -> v1::MulticaMutation {
        request(Action::IssueAction(v1::MulticaIssueAction {
            kind: kind as i32,
            id: "issue-a".into(),
            revision: browse::revision(&issue()),
            value: value.into(),
        }))
    }
    async fn daemon(dir: &Path) -> std::sync::Arc<crate::Daemon> {
        let mut config = crate::DaemonConfig::new(dir);
        config.bind = "127.0.0.1:0".parse().unwrap();
        config.listen = false;
        config.terminal = None;
        config.agent = None;
        crate::Daemon::start_with(config, std::sync::Arc::new(crate::DefaultPlatform), None)
            .await
            .unwrap()
    }
    fn status(response: v1::response::Resp) -> i32 {
        let v1::response::Resp::OpResult(result) = response else {
            panic!("operation result");
        };
        result.status
    }
    #[tokio::test]
    async fn ledger_prevents_duplicate_conflicting_and_ambiguous_submissions() {
        let root = tempfile::tempdir().unwrap();
        let m = fixture(root.path());
        let d = daemon(&root.path().join("daemon")).await;
        let body = "literal \\n and Unicode 中文\nnext line";
        let r = request(Action::Issue(v1::MulticaIssueEdit {
            title: "--server-url=not-an-option".into(),
            description: body.into(),
            ..Default::default()
        }));
        assert_eq!(
            status(tracked(&d, &m, r.clone()).await.unwrap()),
            Status::Applied as i32
        );
        assert_eq!(
            status(tracked(&d, &m, r.clone()).await.unwrap()),
            Status::Duplicate as i32
        );
        let mut conflict = r.clone();
        if let Some(Action::Issue(edit)) = &mut conflict.action {
            edit.title = "Different".into();
        }
        assert_eq!(
            status(tracked(&d, &m, conflict).await.unwrap()),
            Status::Rejected as i32
        );
        let calls = std::fs::read_to_string(root.path().join("writes")).unwrap();
        assert_eq!(calls.lines().count(), 1);
        let call: Value = serde_json::from_str(calls.lines().next().unwrap()).unwrap();
        assert_eq!(call["input"], body);
        assert!(
            call["args"]
                .as_array()
                .unwrap()
                .contains(&json!("--title=--server-url=not-an-option"))
        );
        assert!(
            call["args"]
                .as_array()
                .unwrap()
                .contains(&json!("--status=backlog"))
        );
        std::fs::write(root.path().join("fail"), "").unwrap();
        let mut unknown = r;
        unknown.op_id = "op-unknown".into();
        assert_eq!(
            status(tracked(&d, &m, unknown.clone()).await.unwrap()),
            Status::Unknown as i32
        );
        assert_eq!(
            status(tracked(&d, &m, unknown).await.unwrap()),
            Status::Unknown as i32
        );
        assert_eq!(
            std::fs::read_to_string(root.path().join("writes"))
                .unwrap()
                .lines()
                .count(),
            2
        );
    }
    #[tokio::test]
    async fn stale_foreign_and_uneditable_resources_never_reach_mutation() {
        let root = tempfile::tempdir().unwrap();
        let m = fixture(root.path());
        let c = m.load().unwrap();
        let r = issue_action(v1::multica_issue_action::Kind::Status, "done");
        let mut changed = issue();
        changed["updated_at"] = json!("v2");
        std::fs::write(root.path().join("issue.json"), changed.to_string()).unwrap();
        assert!(m.prepare(&c, &r).await.is_err());
        changed = issue();
        changed["workspace_id"] = json!("another");
        std::fs::write(root.path().join("issue.json"), changed.to_string()).unwrap();
        assert!(m.prepare(&c, &r).await.is_err());
        let mut hidden = agent();
        hidden["viewer_is_editor"] = json!(false);
        std::fs::write(root.path().join("agent.json"), hidden.to_string()).unwrap();
        let r = request(Action::Agent(v1::MulticaAgentEdit {
            id: "agent-a".into(),
            revision: browse::revision(&hidden),
            name: "Changed".into(),
            max_concurrent_tasks: 2,
            ..Default::default()
        }));
        assert!(m.prepare(&c, &r).await.is_err());
        assert!(!root.path().join("writes").exists());
    }
    #[tokio::test]
    async fn cancellation_requires_exact_run_and_assignment_uses_verified_current_flags() {
        let root = tempfile::tempdir().unwrap();
        let m = fixture(root.path());
        let c = m.load().unwrap();
        use v1::multica_issue_action::Kind;
        assert!(
            m.prepare(&c, &issue_action(Kind::Cancel, "issue-a"))
                .await
                .is_err()
        );
        let p = m
            .prepare(&c, &issue_action(Kind::Cancel, "run-a"))
            .await
            .unwrap();
        assert_eq!(p.args, ["issue", "cancel-task", "run-a", "--issue=issue-a"]);
        let p = m
            .prepare(&c, &issue_action(Kind::Assign, "agent-a"))
            .await
            .unwrap();
        assert_eq!(
            p.args,
            [
                "issue",
                "assign",
                "issue-a",
                "--to-id=agent-a",
                "--no-start"
            ]
        );
        let status = m
            .prepare(&c, &issue_action(Kind::Status, "todo"))
            .await
            .unwrap();
        assert!(status.args.iter().any(|a| a == "--no-start"));
        assert!(m.prepare(&c, &issue_action(Kind::Rerun, "")).await.is_err());
        assert!(validate(&issue_action(Kind::Comment, "@agent start work")).is_err());
        assert!(
            validate(&issue_action(
                Kind::Comment,
                "[Agent](mention://agent/agent-a) start"
            ))
            .is_err()
        );
        assert!(
            validate(&issue_action(
                Kind::Comment,
                "[Member](MENTION://member/member-a)"
            ))
            .is_err()
        );
        assert!(validate(&issue_action(Kind::Status, "--server-url")).is_err());
        assert!(validate(&issue_action(Kind::Rerun, "unexpected")).is_err());
    }
}
