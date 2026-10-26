//! Fixed, read-only CLI queries. Authentication remains in the original container.
use super::*;
use serde_json::Value;
use tokio::io::AsyncReadExt;
const MAX_OUTPUT: u64 = 4 * 1024 * 1024;
const PAGE_SIZE: u32 = 50;

impl Manager {
    async fn query(&self, c: &Config, workspace: &str, args: &[&str]) -> Result<Value, Error> {
        let executable = self.executable.clone().map(Ok).unwrap_or_else(binary)?;
        let mut command = tokio::process::Command::new(executable);
        command.args(["exec", &c.container_name, "multica"]);
        if !workspace.is_empty() {
            command.args(["--workspace-id", workspace]);
        }
        command
            .args(args)
            .args(["--output", "json"])
            .stdin(std::process::Stdio::null())
            .stdout(std::process::Stdio::piped())
            .stderr(std::process::Stdio::null())
            .kill_on_drop(true);
        let mut child = command.spawn().map_err(io)?;
        let stdout = child
            .stdout
            .take()
            .ok_or_else(|| Error::Unavailable("Multica output unavailable".into()))?;
        let mut bytes = Vec::new();
        let result = tokio::time::timeout(Duration::from_secs(25), async {
            stdout
                .take(MAX_OUTPUT + 1)
                .read_to_end(&mut bytes)
                .await
                .map_err(io)?;
            if bytes.len() as u64 > MAX_OUTPUT {
                return Err(Error::Unavailable(
                    "Multica response exceeded the size limit".into(),
                ));
            }
            if !child.wait().await.map_err(io)?.success() {
                return Err(Error::Unavailable(
                    "Multica query failed; check the container login and network, then refresh"
                        .into(),
                ));
            }
            serde_json::from_slice(&bytes)
                .map_err(|_| Error::Unavailable("Multica returned an unsupported response".into()))
        })
        .await;
        result.map_err(|_| {
            Error::Unavailable("Multica query timed out; refresh to try again".into())
        })?
    }

    pub async fn browse(&self, r: v1::MulticaBrowseRequest) -> Result<v1::MulticaPage, Error> {
        if !cfg!(target_os = "macos") {
            return Err(Error::Unavailable(
                "Multica requires a local Mac container".into(),
            ));
        }
        validate_request(&r)?;
        let _permit = self
            .browse_gate
            .try_acquire()
            .map_err(|_| Error::Busy("Multica is loading; refresh shortly".into()))?;
        let (c, row) = self.observed(self.load()?).await?;
        if !row
            .as_ref()
            .is_some_and(|row| row["status"]["state"] == "running")
        {
            return Err(Error::Unavailable(
                "Start the Multica container in Settings".into(),
            ));
        }
        let workspace_json = self.query(&c, "", &["workspace", "list"]).await?;
        let workspaces: Vec<_> = array(&workspace_json)?
            .iter()
            .map(|w| v1::MulticaWorkspace {
                id: field(w, "id"),
                name: field(w, "name"),
            })
            .collect();
        if workspaces.len() > 1000 {
            return Err(Error::Unavailable("Too many Multica workspaces".into()));
        }
        let workspace = if r.workspace_id.is_empty() {
            workspaces.first().map(|w| w.id.clone()).unwrap_or_default()
        } else {
            r.workspace_id.clone()
        };
        let mut page = v1::MulticaPage {
            workspaces,
            workspace_id: workspace.clone(),
            offset: r.offset,
            next_offset: r.offset.saturating_add(PAGE_SIZE),
            previous_offset: r.offset.saturating_sub(PAGE_SIZE),
            ..Default::default()
        };
        if workspace.is_empty() {
            return Ok(page);
        }
        validate_request(&v1::MulticaBrowseRequest {
            workspace_id: workspace.clone(),
            ..Default::default()
        })?;
        if !page.workspaces.iter().any(|w| w.id == workspace) {
            return Err(Error::Invalid(
                "Select an available Multica workspace".into(),
            ));
        }
        if !r.issue_id.is_empty() {
            let data = self
                .query(&c, &workspace, &["issue", "get", &r.issue_id])
                .await?;
            check_workspace(&data, &workspace)?;
            if field(&data, "id") != r.issue_id {
                return Err(Error::Invalid("Multica returned a different issue".into()));
            }
            page.issue = Some(issue(&data));
            return Ok(page);
        }
        if !r.agent_id.is_empty() {
            let data = self
                .query(&c, &workspace, &["agent", "get", &r.agent_id])
                .await?;
            check_workspace(&data, &workspace)?;
            if field(&data, "id") != r.agent_id {
                return Err(Error::Invalid("Multica returned a different agent".into()));
            }
            page.agent = Some(agent(&data));
            return Ok(page);
        }
        let offset = r.offset.to_string();
        let limit = PAGE_SIZE.to_string();
        let issue_args = ["issue", "list", "--limit", &limit, "--offset", &offset];
        let (issues, agents, runtimes) = tokio::try_join!(
            self.query(&c, &workspace, &issue_args),
            self.query(&c, &workspace, &["agent", "list"]),
            self.query(&c, &workspace, &["runtime", "list"])
        )?;
        let rows = array(&issues["issues"])?;
        if rows.len() > PAGE_SIZE as usize {
            return Err(Error::Unavailable(
                "Multica returned too many issues".into(),
            ));
        }
        for row in rows {
            check_workspace(row, &workspace)?;
        }
        page.issues = rows.iter().map(issue).collect();
        page.has_more = issues["has_more"]
            .as_bool()
            .ok_or_else(|| Error::Unavailable("Multica pagination state is missing".into()))?;
        let agent_rows = array(&agents)?;
        let runtime_rows = array(&runtimes)?;
        if agent_rows.len() > 2000 || runtime_rows.len() > 2000 {
            return Err(Error::Unavailable(
                "Multica roster exceeded the limit".into(),
            ));
        }
        for row in agent_rows.iter().chain(runtime_rows) {
            check_workspace(row, &workspace)?;
        }
        page.agents = agent_rows.iter().map(agent).collect();
        page.runtimes = runtime_rows
            .iter()
            .map(|v| v1::MulticaRuntime {
                id: field(v, "id"),
                name: field(v, "name"),
                provider: field(v, "provider"),
                status: field(v, "status"),
                last_seen_at: field(v, "last_seen_at"),
                device_info: field(v, "device_info"),
            })
            .collect();
        Ok(page)
    }
}
fn validate_request(r: &v1::MulticaBrowseRequest) -> Result<(), Error> {
    for id in [&r.workspace_id, &r.issue_id, &r.agent_id] {
        if id.len() > 128
            || id.starts_with('-')
            || !id
                .bytes()
                .all(|b| b.is_ascii_alphanumeric() || b == b'-' || b == b'_')
        {
            return Err(Error::Invalid("Invalid Multica identifier".into()));
        }
    }
    if r.offset > 100_000 || (!r.issue_id.is_empty() && !r.agent_id.is_empty()) {
        return Err(Error::Invalid("Invalid Multica query".into()));
    }
    Ok(())
}
fn array(value: &Value) -> Result<&Vec<Value>, Error> {
    value
        .as_array()
        .ok_or_else(|| Error::Unavailable("Unsupported Multica list response".into()))
}
fn field(v: &Value, key: &str) -> String {
    v[key].as_str().unwrap_or_default().to_owned()
}
fn check_workspace(v: &Value, workspace: &str) -> Result<(), Error> {
    if v["workspace_id"].as_str() != Some(workspace) {
        return Err(Error::Invalid(
            "Multica result belongs to a different workspace".into(),
        ));
    }
    Ok(())
}
fn issue(v: &Value) -> v1::MulticaIssue {
    v1::MulticaIssue {
        id: field(v, "id"),
        identifier: field(v, "identifier"),
        title: field(v, "title"),
        description: field(v, "description"),
        status: field(v, "status"),
        priority: field(v, "priority"),
        assignee_id: field(v, "assignee_id"),
        assignee_type: field(v, "assignee_type"),
        updated_at: field(v, "updated_at"),
    }
}
fn agent(v: &Value) -> v1::MulticaAgent {
    v1::MulticaAgent {
        id: field(v, "id"),
        name: field(v, "name"),
        description: field(v, "description"),
        status: field(v, "status"),
        runtime_id: field(v, "runtime_id"),
        model: field(v, "model"),
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    #[test]
    fn queries_cannot_inject_flags_or_cross_workspaces() {
        for id in ["--server-url", "id\n--debug", "../private", "valid value"] {
            assert!(
                validate_request(&v1::MulticaBrowseRequest {
                    workspace_id: id.into(),
                    ..Default::default()
                })
                .is_err(),
                "{id}"
            );
        }
        assert!(check_workspace(&json!({"workspace_id":"other"}), "selected").is_err());
        assert!(check_workspace(&json!({}), "selected").is_err());
    }
    #[test]
    fn projection_excludes_custom_environment_and_server_metadata() {
        let source = json!({"id":"agent-a","name":"Demo","custom_env":{"DEMO_TOKEN":"not-a-credential"},"mcp_config":{"secret":"synthetic"},"instructions":"private instructions"});
        let encoded = serde_json::to_value(agent(&source)).unwrap();
        assert_eq!(encoded["name"], "Demo");
        assert!(encoded.get("custom_env").is_none());
        assert!(encoded.get("instructions").is_none());
    }
}

#[cfg(all(test, target_os = "macos"))]
mod integration {
    use super::*;
    use std::os::unix::fs::PermissionsExt;
    #[tokio::test]
    async fn oversized_cli_output_is_stopped_before_json_decoding() {
        let dir = tempfile::tempdir().unwrap();
        let executable = dir.path().join("container");
        std::fs::write(
            &executable,
            "#!/bin/sh\nexec /usr/bin/head -c 4194305 /dev/zero\n",
        )
        .unwrap();
        std::fs::set_permissions(&executable, std::fs::Permissions::from_mode(0o755)).unwrap();
        let mut manager = Manager::new(dir.path());
        manager.executable = Some(executable);
        let error = manager
            .query(&Config::default(), "", &["workspace", "list"])
            .await
            .unwrap_err();
        assert!(error.to_string().contains("size limit"));
    }
    #[tokio::test]
    async fn reads_paged_workspace_data_without_switching_login_or_exposing_secrets() {
        let dir = tempfile::tempdir().unwrap();
        let executable = dir.path().join("container");
        std::fs::write(&executable,r##"#!/bin/sh
set -eu
cd "$(dirname "$0")"
printf '%s\n' "$*" >> calls
case "$*" in
 'list --all --format json') echo '[{"id":"mlt","status":{"state":"running"}}]' ;;
 'exec mlt multica workspace list --output json') echo '[{"id":"w","name":"Demo"}]' ;;
 'exec mlt multica --workspace-id w issue list --limit 50 --offset 50 --output json') echo '{"issues":[{"id":"issue-a","title":"Review","workspace_id":"w"}],"has_more":true}' ;;
 'exec mlt multica --workspace-id w agent list --output json') echo '[{"id":"agent-a","name":"Reviewer","workspace_id":"w","custom_env":{"SECRET":"synthetic"}}]' ;;
 'exec mlt multica --workspace-id w runtime list --output json') echo '[{"id":"runtime-a","provider":"codex","status":"online","workspace_id":"w"}]' ;;
 'exec mlt multica --workspace-id w issue get issue-a --output json') echo '{"id":"issue-a","title":"Review","description":"Details","workspace_id":"w"}' ;;
 'exec mlt multica --workspace-id w issue get other --output json') echo '{"id":"other","workspace_id":"another"}' ;;
 *) exit 90 ;;
esac
"##).unwrap();
        std::fs::set_permissions(&executable, std::fs::Permissions::from_mode(0o755)).unwrap();
        let mut manager = Manager::new(dir.path());
        manager.executable = Some(executable);
        manager
            .save(&Config {
                container_name: "mlt".into(),
                server_url: "https://example.invalid".into(),
                app_url: "https://example.invalid".into(),
                ..Default::default()
            })
            .unwrap();
        let page = manager
            .browse(v1::MulticaBrowseRequest {
                offset: 50,
                ..Default::default()
            })
            .await
            .unwrap();
        assert_eq!(page.workspace_id, "w");
        assert_eq!(page.issues[0].title, "Review");
        assert_eq!(page.next_offset, 100);
        assert_eq!(page.previous_offset, 0);
        assert!(page.has_more);
        assert!(!serde_json::to_string(&page).unwrap().contains("SECRET"));
        let detail = manager
            .browse(v1::MulticaBrowseRequest {
                workspace_id: "w".into(),
                issue_id: "issue-a".into(),
                ..Default::default()
            })
            .await
            .unwrap();
        assert_eq!(detail.issue.unwrap().description, "Details");
        assert!(
            manager
                .browse(v1::MulticaBrowseRequest {
                    workspace_id: "w".into(),
                    issue_id: "other".into(),
                    ..Default::default()
                })
                .await
                .is_err()
        );
        let calls = std::fs::read_to_string(dir.path().join("calls")).unwrap();
        for forbidden in [
            "workspace switch",
            "daemon start",
            "config set",
            "issue create",
            "comment create",
        ] {
            assert!(!calls.contains(forbidden));
        }
    }
}

#[cfg(all(test, target_os = "macos"))]
mod live {
    use super::*;
    #[tokio::test]
    #[ignore = "requires an operator-supplied read-only container command adapter"]
    async fn operator_container_read_only_workbench() {
        let adapter =
            std::env::var_os("MUSTER_MULTICA_TEST_CONTAINER").expect("explicit container adapter");
        let dir = tempfile::tempdir().unwrap();
        let mut manager = Manager::new(dir.path());
        manager.executable = Some(PathBuf::from(adapter));
        let page = manager
            .browse(v1::MulticaBrowseRequest::default())
            .await
            .unwrap();
        assert!(!page.workspaces.is_empty());
        assert!(!page.workspace_id.is_empty());
        if let Some(issue) = page.issues.first() {
            let detail = manager
                .browse(v1::MulticaBrowseRequest {
                    workspace_id: page.workspace_id.clone(),
                    issue_id: issue.id.clone(),
                    ..Default::default()
                })
                .await
                .unwrap();
            assert_eq!(detail.issue.unwrap().id, issue.id);
        }
        if let Some(agent) = page.agents.first() {
            let detail = manager
                .browse(v1::MulticaBrowseRequest {
                    workspace_id: page.workspace_id.clone(),
                    agent_id: agent.id.clone(),
                    ..Default::default()
                })
                .await
                .unwrap();
            assert_eq!(detail.agent.unwrap().id, agent.id);
        }
        let final_page = manager
            .browse(v1::MulticaBrowseRequest {
                workspace_id: page.workspace_id.clone(),
                offset: 100_000,
                ..Default::default()
            })
            .await
            .unwrap();
        assert!(final_page.issues.is_empty());
        assert!(!final_page.has_more);
        if let Some(other) = page.workspaces.iter().find(|w| w.id != page.workspace_id) {
            let selected = manager
                .browse(v1::MulticaBrowseRequest {
                    workspace_id: other.id.clone(),
                    ..Default::default()
                })
                .await
                .unwrap();
            assert_eq!(selected.workspace_id, other.id);
        }
        println!(
            "Live read-only workbench: {} workspaces, {} issues, {} agents, {} runtimes",
            page.workspaces.len(),
            page.issues.len(),
            page.agents.len(),
            page.runtimes.len()
        );
        assert!(
            !manager.path.exists(),
            "browsing must not persist or change login/configuration"
        );
    }
}
