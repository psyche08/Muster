//! The managed caller's read-only view of its own current assignment.
use muster_agent::cloud::{Http, Request};
use serde_json::Value;
use std::{collections::HashMap, sync::Arc, time::Duration};
use url::Url;

pub(crate) struct IssueSnapshot {
    pub json: String,
    pub first_turn: String,
}

pub struct TaskIssue {
    origin: Url,
    token: String,
    workspace: String,
    agent: String,
    run: String,
    http: Arc<dyn Http>,
}

fn uuid(id: &str) -> bool {
    id.len() == 36
        && id.bytes().enumerate().all(|(i, b)| {
            if matches!(i, 8 | 13 | 18 | 23) {
                b == b'-'
            } else {
                b.is_ascii_hexdigit()
            }
        })
}

fn reference(prompt: &str) -> Option<&str> {
    let mut ids = prompt
        .lines()
        .filter_map(|l| l.strip_prefix("Your assigned issue ID is: "));
    let id = ids.next()?;
    (uuid(id) && ids.next().is_none()).then_some(id)
}

impl TaskIssue {
    pub fn new(
        configured: &str,
        env: &HashMap<String, String>,
        http: Arc<dyn Http>,
    ) -> Option<Self> {
        let origin = Url::parse(configured).ok()?;
        let supplied = Url::parse(env.get("MULTICA_SERVER_URL")?).ok()?;
        if origin != supplied
            || origin.scheme() != "https"
            || !origin.username().is_empty()
            || origin.password().is_some()
            || origin.path() != "/"
            || origin.query().is_some()
            || origin.fragment().is_some()
        {
            return None;
        }
        let field = |k| env.get(k).filter(|v| uuid(v)).cloned();
        Some(Self {
            origin,
            token: env.get("MULTICA_TOKEN").filter(|t| !t.is_empty())?.clone(),
            workspace: field("MULTICA_WORKSPACE_ID")?,
            agent: field("MULTICA_AGENT_ID")?,
            run: field("MULTICA_TASK_ID")?,
            http,
        })
    }
    async fn get(&self, path: &str) -> Option<Value> {
        let response = self
            .http
            .send(Request {
                method: "GET",
                url: self.origin.join(path).ok()?.to_string(),
                headers: vec![
                    ("Authorization".into(), format!("Bearer {}", self.token)),
                    ("X-Workspace-ID".into(), self.workspace.clone()),
                ],
                body: None,
            })
            .await
            .ok()?;
        if response.status != 200 || response.body.len() > 1024 * 1024 {
            return None;
        }
        serde_json::from_str(&response.body).ok()
    }
    pub(crate) async fn read(&self, prompt: &str) -> Option<IssueSnapshot> {
        let id = reference(prompt)?;
        let path = format!("/api/issues/{id}");
        let runs = format!("{path}/task-runs");
        let (issue, runs) = tokio::time::timeout(Duration::from_secs(6), async {
            tokio::join!(self.get(&path), self.get(&runs))
        })
        .await
        .ok()?;
        self.snapshot(id, &issue?, &runs?)
    }
    fn snapshot(&self, id: &str, issue: &Value, runs: &Value) -> Option<IssueSnapshot> {
        if issue["id"] != id
            || issue["workspace_id"] != self.workspace
            || issue["assignee_type"] != "agent"
            || issue["assignee_id"] != self.agent
        {
            return None;
        }
        let mut matches = runs.as_array()?.iter().filter(|r| r["id"] == self.run);
        let run = matches.next()?;
        if matches.next().is_some()
            || run["issue_id"] != id
            || run["workspace_id"] != self.workspace
            || run["agent_id"] != self.agent
            || run["status"] != "running"
        {
            return None;
        }
        let title = issue["title"].as_str()?;
        let description = issue["description"].as_str().unwrap_or("");
        if title.trim().is_empty() || title.len() + description.len() > 64 * 1024 {
            return None;
        }
        Some(IssueSnapshot {
            json: serde_json::json!({"title":title,"description":description}).to_string(),
            first_turn: muster_agent::native_runtime::multica_issue_prompt(id, title, description),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use async_trait::async_trait;
    struct NoNetwork;
    #[async_trait]
    impl Http for NoNetwork {
        async fn send(&self, _: Request) -> Result<muster_agent::cloud::Response, String> {
            panic!("unexpected network call")
        }
    }
    #[test]
    fn snapshot_binds_issue_run_agent_and_origin() {
        let id = "00000000-0000-4000-8000-000000000001";
        let env = HashMap::from_iter(
            [
                ("MULTICA_SERVER_URL", "https://example.invalid"),
                ("MULTICA_TOKEN", "fixture"),
                ("MULTICA_TASK_ID", id),
                ("MULTICA_WORKSPACE_ID", id),
                ("MULTICA_AGENT_ID", id),
            ]
            .map(|(k, v)| (k.into(), v.into())),
        );
        assert!(TaskIssue::new("https://other.invalid", &env, Arc::new(NoNetwork)).is_none());
        let reader = TaskIssue::new("https://example.invalid", &env, Arc::new(NoNetwork)).unwrap();
        let issue = serde_json::json!({"id":id,"workspace_id":id,"assignee_type":"agent","assignee_id":id,"title":"Example","description":"Add two numbers."});
        let mut runs = serde_json::json!([{"id":id,"workspace_id":id,"agent_id":id,"issue_id":id,"status":"running"}]);
        assert!(reader.snapshot(id, &issue, &runs).is_some());
        runs[0]["issue_id"] = "other".into();
        assert!(reader.snapshot(id, &issue, &runs).is_none());
        let hint = format!("Your assigned issue ID is: {id}");
        assert_eq!(reference(&hint), Some(id));
        assert_eq!(reference(&format!("{hint}\n{hint}")), None);
    }
}
