//! Verify an owner-enrolled direct dispatch over a host-only service connection.
use super::*;
use serde::Deserialize;
use serde_json::Value;
use std::path::{Path, PathBuf};

const POLICY: &str = "multica-trusted-dispatch.json";
const MAX_REPLY: usize = 2 * 1024 * 1024;
const ISSUE_PREFIX: &str = "Your assigned issue ID is: ";

fn deny(why: &str) -> Error {
    Error::Unauthorized(format!("Trusted Multica dispatch: {why}"))
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Policy {
    origin: url::Url,
    token: String,
    workspace: String,
    owner: String,
    agent: String,
    runtime: String,
    runtime_daemon: String,
    root: PathBuf,
}

fn uuid(s: &str) -> bool {
    s.len() == 36
        && s.bytes().enumerate().all(|(i, b)| {
            if [8, 13, 18, 23].contains(&i) {
                b == b'-'
            } else {
                b.is_ascii_hexdigit() && !b.is_ascii_uppercase()
            }
        })
}

fn load(dir: &Path) -> Result<Option<Policy>, Error> {
    use std::io::Read as _;
    let mut opts = std::fs::OpenOptions::new();
    opts.read(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        opts.custom_flags(libc::O_NOFOLLOW | libc::O_NONBLOCK);
    }
    let file = match opts.open(dir.join(POLICY)) {
        Ok(f) => f,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        Err(_) => return Err(deny("host enrollment is unreadable")),
    };
    let meta = file
        .metadata()
        .map_err(|_| deny("host enrollment metadata unavailable"))?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::MetadataExt;
        // SAFETY: geteuid reads the calling process's effective user id.
        if meta.uid() != unsafe { libc::geteuid() } || meta.mode() & 0o077 != 0 {
            return Err(deny("host enrollment must be owner-private"));
        }
    }
    if !meta.is_file() || meta.len() > 16 * 1024 {
        return Err(deny("invalid host enrollment file"));
    }
    let p: Policy = serde_json::from_reader(file.take(16 * 1024))
        .map_err(|_| deny("invalid host enrollment"))?;
    if p.origin.scheme() != "https"
        || p.origin.host_str().is_none()
        || !p.origin.username().is_empty()
        || p.origin.password().is_some()
        || p.origin.path() != "/"
        || p.origin.fragment().is_some()
        || p.origin.query().is_some()
        || p.token.is_empty()
        || !p.root.is_absolute()
        || ![
            &p.workspace,
            &p.owner,
            &p.agent,
            &p.runtime,
            &p.runtime_daemon,
        ]
        .iter()
        .all(|s| uuid(s))
    {
        return Err(deny("incomplete or invalid host enrollment"));
    }
    Ok(Some(p))
}

impl Policy {
    async fn get(&self, client: &reqwest::Client, path: &str) -> Result<Value, Error> {
        let url = self
            .origin
            .join(path)
            .map_err(|_| deny("invalid API path"))?;
        let mut r = client
            .get(url)
            .header("X-Workspace-ID", &self.workspace)
            .bearer_auth(&self.token)
            .send()
            .await
            .map_err(|_| deny("service unavailable"))?;
        if !r.status().is_success() {
            return Err(deny(&format!("service returned {}", r.status().as_u16())));
        }
        let mut data = Vec::new();
        while let Some(chunk) = r
            .chunk()
            .await
            .map_err(|_| deny("service response incomplete"))?
        {
            if data.len() + chunk.len() > MAX_REPLY {
                return Err(deny("service response too large"));
            }
            data.extend_from_slice(&chunk);
        }
        serde_json::from_slice(&data).map_err(|_| deny("invalid service response"))
    }

    fn verify(&self, hint: &Hint<'_>, records: &Records, now: i64) -> Result<String, Error> {
        let Records {
            me,
            issue,
            agent,
            runtimes,
            runs,
        } = records;
        if me["id"] != self.owner
            || issue["id"] != hint.issue
            || issue["workspace_id"] != self.workspace
            || issue["creator_type"] != "member"
            || issue["creator_id"] != self.owner
            || issue["initiator_user_id"] != self.owner
            || issue["assignee_type"] != "agent"
            || issue["assignee_id"] != self.agent
        {
            return Err(deny("issue owner or assignment mismatch"));
        }
        if agent["id"] != self.agent
            || agent["workspace_id"] != self.workspace
            || agent["owner_id"] != self.owner
            || agent["runtime_id"] != self.runtime
            || !agent["archived_at"].is_null()
        {
            return Err(deny("agent binding mismatch"));
        }
        let matches: Vec<_> = runtimes
            .as_array()
            .ok_or_else(|| deny("runtime list unavailable"))?
            .iter()
            .filter(|r| r["id"] == self.runtime)
            .collect();
        let [rt] = matches.as_slice() else {
            return Err(deny("runtime identity ambiguous"));
        };
        if rt["provider"] != "codex"
            || rt["workspace_id"] != self.workspace
            || rt["owner_id"] != self.owner
            || rt["daemon_id"] != self.runtime_daemon
            || rt["runtime_mode"] != "local"
            || rt["status"] != "online"
        {
            return Err(deny("runtime does not match the enrolled executor"));
        }
        let Some([run]) = runs.as_array().map(Vec::as_slice) else {
            return Err(deny("not the sole initial issue run"));
        };
        if run["id"] != hint.run
            || run["issue_id"] != hint.issue
            || run["workspace_id"] != self.workspace
            || run["agent_id"] != self.agent
            || run["runtime_id"] != self.runtime
            || run["kind"] != "direct"
            || run["attempt"].as_i64() != Some(1)
            || !matches!(run["status"].as_str(), Some("running" | "dispatched"))
            || run["delivered_comment_ids"]
                .as_array()
                .is_some_and(|v| !v.is_empty())
        {
            return Err(deny("run does not match the direct assignment"));
        }
        let created = issue["created_at"]
            .as_str()
            .ok_or_else(|| deny("missing creation time"))?;
        let dispatched = run["dispatched_at"]
            .as_str()
            .ok_or_else(|| deny("missing dispatch time"))?;
        let ts = chrono::DateTime::parse_from_rfc3339(dispatched)
            .map_err(|_| deny("invalid dispatch time"))?
            .timestamp();
        if issue["updated_at"] != created
            || run["created_at"] != created
            || !(-5..=180).contains(&(now - ts))
        {
            return Err(deny("issue was changed or dispatch is not fresh"));
        }
        let relative = format!("{}/{}/workdir", self.workspace, &hint.run[..8]);
        let expected = self.root.join(&relative);
        if expected != Path::new(hint.cwd)
            || run["work_dir"]
                .as_str()
                .is_some_and(|s| !s.is_empty() && Path::new(s) != expected)
            || run["relative_work_dir"]
                .as_str()
                .is_some_and(|s| !s.is_empty() && s != relative)
        {
            return Err(deny("dispatch directory mismatch"));
        }
        let title = issue["title"]
            .as_str()
            .filter(|t| !t.trim().is_empty())
            .ok_or_else(|| deny("empty issue title"))?;
        let body = issue["description"].as_str().unwrap_or("").trim();
        let prompt = format!(
            "Multica issue {}\n\n{}\n\n{}",
            hint.issue,
            title.trim(),
            body
        )
        .trim_end()
        .to_owned();
        if prompt.len() > MAX_TEXT {
            return Err(deny("issue exceeds first-turn budget"));
        }
        Ok(prompt)
    }
}

struct Records {
    me: Value,
    issue: Value,
    agent: Value,
    runtimes: Value,
    runs: Value,
}
struct Hint<'a> {
    run: &'a str,
    issue: &'a str,
    cwd: &'a str,
}
impl<'a> Hint<'a> {
    fn read(o: &'a v1::AgentSchedulerOperate) -> Result<Self, Error> {
        let run = o
            .task_id
            .strip_prefix("multica:")
            .filter(|s| uuid(s))
            .ok_or_else(|| deny("invalid run identity"))?;
        let mut ids = o.text.lines().filter_map(|l| l.strip_prefix(ISSUE_PREFIX));
        let issue = ids
            .next()
            .filter(|s| uuid(s))
            .ok_or_else(|| deny("missing issue identity"))?;
        if ids.next().is_some() {
            return Err(deny("ambiguous issue identity"));
        }
        Ok(Self {
            run,
            issue,
            cwd: &o.directory,
        })
    }
}

pub(super) async fn first_turn(
    daemon: &Daemon,
    o: &v1::AgentSchedulerOperate,
) -> Result<Option<String>, Error> {
    let Some(p) = load(&daemon.config.dir)? else {
        return Ok(None);
    };
    let hint = Hint::read(o)?;
    let expected = p
        .root
        .join(&p.workspace)
        .join(&hint.run[..8])
        .join("workdir");
    if expected != Path::new(hint.cwd)
        || std::fs::canonicalize(&expected).ok().as_ref() != Some(&expected)
        || !expected.is_dir()
    {
        return Err(deny("work directory is not the enrolled canonical path"));
    }
    let client = crate::cloud_http::with_platform_trust(
        reqwest::Client::builder()
            .https_only(true)
            .redirect(reqwest::redirect::Policy::none())
            .timeout(Duration::from_secs(6)),
    )
    .and_then(|b| Ok(b.build()?))
    .map_err(|_| deny("host TLS client unavailable"))?;
    let issue_path = format!("/api/issues/{}", hint.issue);
    let run_path = format!("{issue_path}/task-runs");
    let agent_path = format!("/api/agents/{}", p.agent);
    let (me, issue, agent, runtimes, runs) = tokio::try_join!(
        p.get(&client, "/api/me"),
        p.get(&client, &issue_path),
        p.get(&client, &agent_path),
        p.get(&client, "/api/runtimes"),
        p.get(&client, &run_path)
    )?;
    let records = Records {
        me,
        issue,
        agent,
        runtimes,
        runs,
    };
    p.verify(&hint, &records, (now_ms() / 1000) as i64)
        .map(Some)
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    const WHEN: &str = "2026-01-01T00:00:00Z";
    const RUN: &str = "aaaaaaaa-0000-4000-8000-000000000001";
    const ISSUE: &str = "bbbbbbbb-0000-4000-8000-000000000001";
    fn fixture() -> (Policy, Hint<'static>, Records) {
        let policy = Policy {
            origin: "https://example.invalid".parse().unwrap(),
            token: "fixture".into(),
            workspace: "ws".into(),
            owner: "owner".into(),
            agent: "agent".into(),
            runtime: "runtime".into(),
            runtime_daemon: "registered-executor".into(),
            root: "/work".into(),
        };
        let hint = Hint {
            run: RUN,
            issue: ISSUE,
            cwd: "/work/ws/aaaaaaaa/workdir",
        };
        let records = Records {
            me: json!({"id":"owner"}),
            issue: json!({"id":ISSUE,"workspace_id":"ws","creator_type":"member","creator_id":"owner","initiator_user_id":"owner","assignee_type":"agent","assignee_id":"agent","created_at":WHEN,"updated_at":WHEN,"title":"Example","description":"Compute an integer."}),
            agent: json!({"id":"agent","owner_id":"owner","workspace_id":"ws","runtime_id":"runtime"}),
            runtimes: json!([{"id":"runtime","workspace_id":"ws","owner_id":"owner","daemon_id":"registered-executor","runtime_mode":"local","provider":"codex","status":"online"}]),
            runs: json!([{"id":RUN,"issue_id":ISSUE,"workspace_id":"ws","agent_id":"agent","runtime_id":"runtime","kind":"direct","attempt":1,"status":"running","dispatched_at":WHEN,"created_at":WHEN,"work_dir":hint.cwd}]),
        };
        (policy, hint, records)
    }
    #[test]
    fn dispatch_scope_and_first_prompt_come_from_host_evidence() {
        let (p, h, r) = fixture();
        let now = chrono::DateTime::parse_from_rfc3339(WHEN)
            .unwrap()
            .timestamp();
        let o = v1::AgentSchedulerOperate {
            task_id: format!("multica:{RUN}"),
            directory: h.cwd.into(),
            text: format!("{ISSUE_PREFIX}{ISSUE}\nUntrusted extra instructions"),
            ..Default::default()
        };
        let hint = Hint::read(&o).unwrap();
        assert_eq!(
            p.verify(&hint, &r, now).unwrap(),
            format!("Multica issue {ISSUE}\n\nExample\n\nCompute an integer.")
        );
        assert!(p.verify(&hint, &r, now + 300).is_err());
        let ambiguous = v1::AgentSchedulerOperate {
            text: format!("{}\n{ISSUE_PREFIX}{ISSUE}", o.text),
            ..o
        };
        assert!(Hint::read(&ambiguous).is_err());
    }
    #[test]
    fn foreign_edits_retries_and_misdirected_runs_get_no_grant() {
        let now = chrono::DateTime::parse_from_rfc3339(WHEN)
            .unwrap()
            .timestamp();
        type Change = fn(&mut Records);
        for change in [
            (|r| r.me["id"] = json!("foreign")) as Change,
            |r| r.issue["creator_id"] = json!("foreign"),
            |r| r.issue["initiator_user_id"] = json!("foreign"),
            |r| r.issue["updated_at"] = json!("2026-01-01T00:00:01Z"),
            |r| r.issue["assignee_id"] = json!("other"),
            |r| r.agent["runtime_id"] = json!("another"),
            |r| r.runtimes[0]["daemon_id"] = json!("wrong-device"),
            |r| r.runs[0]["kind"] = json!("wakeup"),
            |r| r.runs[0]["issue_id"] = json!("different"),
            |r| r.runs[0]["attempt"] = json!(2),
            |r| r.runs[0]["work_dir"] = json!("/outside"),
            |r| r.runs[0]["delivered_comment_ids"] = json!(["unreviewed"]),
            |r| {
                let prior = r.runs[0].clone();
                r.runs.as_array_mut().unwrap().push(prior);
            },
        ] {
            let (p, h, mut r) = fixture();
            change(&mut r);
            assert!(p.verify(&h, &r, now).is_err());
        }
    }
}
