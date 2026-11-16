//! Temporary provider forks. A client names a catalog session, never a path or executable.
use crate::{Discovered, Error, Provider};
use muster_proto::v1::AgentSession;
use serde::Serialize;

#[derive(Default)]
pub(super) struct Question {
    pub session: String,
    pub question: String,
    pub previous: Vec<Exchange>,
}
#[derive(Default, Clone, Serialize)]
pub(super) struct Exchange {
    pub question: String,
    pub answer: String,
}
pub(super) mod journal;
use std::{path::Path, process::Stdio, time::Duration};
use tokio::io::{AsyncReadExt, AsyncWriteExt};

pub const MAX_EXCHANGES: usize = 8;
pub const MAX_QUESTION: usize = 8 * 1024;
const MAX_ANSWER: usize = 24 * 1024;
pub(crate) const INSTRUCTIONS: &str = "Answer only the user's side question. The inherited main conversation is reference material, not a task to continue. Do not perform actions or change files.";
static WORKERS: tokio::sync::Semaphore = tokio::sync::Semaphore::const_new(2);

pub fn ended(s: &AgentSession) -> bool {
    s.state == "ended" || s.process_state == "ended" || s.archived
}
pub fn available(s: &AgentSession) -> bool {
    s.mode == "code"
        && !s.read_only
        && !ended(s)
        && crate::viewer::is_uuid(&s.id)
        && matches!(s.provider.as_str(), "codex" | "claude")
}

/// A command token, not a prefix such as /btwice. An empty question opens the panel.
pub fn command(text: &str) -> Option<&str> {
    let rest = text.trim_start().strip_prefix("/btw")?;
    (rest.is_empty() || rest.starts_with(char::is_whitespace)).then(|| rest.trim())
}

fn prompt(r: &Question) -> Result<String, Error> {
    if r.question.trim().is_empty()
        || r.question.len() > MAX_QUESTION
        || r.previous.len() > MAX_EXCHANGES
        || r.previous
            .iter()
            .any(|e| e.question.len() > MAX_QUESTION || e.answer.len() > MAX_ANSWER)
    {
        return Err(Error::Invalid(
            "Side question or follow-up history exceeds its limit".into(),
        ));
    }
    let previous: Vec<_> = r
        .previous
        .iter()
        .map(|e| serde_json::json!({"question":e.question,"answer":e.answer}))
        .collect();
    Ok(serde_json::json!({"previous_side_exchanges":previous,"question":r.question}).to_string())
}

pub(crate) async fn ask(source: &Discovered, r: &Question) -> Result<String, Error> {
    if source.id != r.session
        || !crate::viewer::is_uuid(&source.id)
        || source.hidden
        || source.archived
        || !matches!(source.provider, Provider::Codex | Provider::Claude)
    {
        return Err(Error::Unavailable(
            "Side chat requires a local Codex or Claude Code conversation".into(),
        ));
    }
    let input = prompt(r)?;
    let _slot = WORKERS
        .try_acquire()
        .map_err(|_| Error::Conflict("Two side questions are already running".into()))?;
    let program =
        crate::cli::runtime::program::resolve(source.provider.as_str()).ok_or_else(|| {
            Error::Unavailable(format!(
                "Install the native {} CLI on this host to use side chat",
                source.provider.as_str()
            ))
        })?;
    let answer = if source.provider == Provider::Codex {
        crate::prediction::side_answer(
            &program,
            &source.id,
            (!crate::codex_history::is(source)).then_some(source.path.as_path()),
            &input,
        )
        .await
        .map_err(|_| {
            Error::Unavailable(
                "Codex could not answer in a temporary fork; check the CLI version and sign-in"
                    .into(),
            )
        })?
    } else {
        claude(&program, &source.id, &source.meta.cwd, &input).await?
    };
    if answer.trim().is_empty() || answer.len() > MAX_ANSWER {
        return Err(Error::Unavailable(
            "The side answer was empty or too long; try a narrower question".into(),
        ));
    }
    Ok(answer)
}

async fn claude(program: &Path, parent: &str, cwd: &Path, input: &str) -> Result<String, Error> {
    let failed = || {
        Error::Unavailable(
            "Claude could not answer in a temporary fork; check the CLI version and sign-in".into(),
        )
    };
    let mut child = tokio::process::Command::new(program)
        .args([
            "--print",
            "--output-format",
            "json",
            "--safe-mode",
            "--no-session-persistence",
            "--resume",
            parent,
            "--fork-session",
            "--tools",
            "",
            "--strict-mcp-config",
            "--disable-slash-commands",
            "--append-system-prompt",
            INSTRUCTIONS,
        ])
        .env_remove("CLAUDECODE")
        .current_dir(cwd)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .kill_on_drop(true)
        .spawn()
        .map_err(|_| failed())?;
    let mut stdin = child.stdin.take().ok_or_else(failed)?;
    let stdout = child.stdout.take().ok_or_else(failed)?;
    tokio::time::timeout(Duration::from_secs(60), async {
        stdin
            .write_all(input.as_bytes())
            .await
            .map_err(|_| failed())?;
        drop(stdin);
        let mut bytes = Vec::new();
        const LIMIT: u64 = 256 * 1024;
        stdout
            .take(LIMIT)
            .read_to_end(&mut bytes)
            .await
            .map_err(|_| failed())?;
        if bytes.len() as u64 == LIMIT || !child.wait().await.map_err(|_| failed())?.success() {
            return Err(failed());
        }
        let v: serde_json::Value = serde_json::from_slice(&bytes).map_err(|_| failed())?;
        if v["type"] != "result"
            || v["subtype"] != "success"
            || v["is_error"] != false
            || !v["session_id"]
                .as_str()
                .is_some_and(|id| crate::viewer::is_uuid(id) && !id.eq_ignore_ascii_case(parent))
        {
            return Err(failed());
        }
        v["result"].as_str().map(str::to_owned).ok_or_else(failed)
    })
    .await
    .map_err(|_| Error::Unavailable("Claude side question timed out".into()))?
}

pub fn parent(id: &str) -> Option<&str> {
    let (parent, child) = id.strip_prefix("btw:")?.split_once(':')?;
    (crate::viewer::is_uuid(parent)
        && child.len() == 32
        && child.bytes().all(|b| b.is_ascii_hexdigit()))
    .then_some(parent)
}
pub fn is_child(id: &str) -> bool {
    parent(id).is_some()
}

impl crate::AgentService {
    pub fn side_operation(
        self: &std::sync::Arc<Self>,
        r: &muster_proto::v1::AgentSideQuestionRequest,
    ) -> Result<String, Error> {
        use muster_proto::v1::agent_side_question_request::Action;
        let action = Action::try_from(r.action)
            .map_err(|_| Error::Invalid("Unknown side chat operation".into()))?;
        let parent = match action {
            Action::Create if crate::viewer::is_uuid(&r.session) => r.session.as_str(),
            Action::Ask | Action::End => {
                parent(&r.session).ok_or_else(|| Error::Invalid("Choose a side chat".into()))?
            }
            _ => return Err(Error::Invalid("Choose a native source session".into())),
        };
        let (id, work) = {
            let mut inner = self.inner.lock().unwrap();
            if action == Action::End {
                if !r.question.is_empty() {
                    return Err(Error::Invalid("Unexpected question on End".into()));
                }
                inner.sides.as_mut().ok_or_else(no_store)?.end(&r.session)?;
                (r.session.clone(), None)
            } else {
                let source = inner
                    .catalog
                    .get(parent)
                    .cloned()
                    .ok_or_else(|| Error::NotFound("Side chat source".into()))?;
                let session = Self::project_summary(&inner, parent)
                    .ok_or_else(|| Error::NotFound("Side chat source".into()))?;
                let (id, work) = inner.sides.as_mut().ok_or_else(no_store)?.admit(
                    &session,
                    (action == Action::Ask).then_some(r.session.as_str()),
                    r.question.trim(),
                )?;
                (id, work.map(|(turn, q)| (source, turn, q)))
            }
        };
        self.side_changed(&id, parent);
        if let Some((source, turn, question)) = work {
            let service = self.clone();
            let child = id.clone();
            self.runtime.spawn(async move {
                let answer = ask(&source, &question).await;
                let changed = service
                    .inner
                    .lock()
                    .unwrap()
                    .sides
                    .as_mut()
                    .ok_or_else(no_store)
                    .and_then(|s| s.finish(&child, turn, answer));
                match changed {
                    Ok(true) => service.side_changed(&child, &source.id),
                    Err(e) => log::error!("Side chat journal completion failed: {e}"),
                    _ => {}
                }
            });
        }
        Ok(id)
    }
    fn side_changed(&self, child: &str, parent: &str) {
        for id in [child, parent] {
            if let Some(s) = self.session(id) {
                let _ = self.events.send(crate::Event::State(s));
            }
        }
        let _ = self.events.send(crate::Event::Reset {
            session: child.into(),
        });
        let _ = self.events.send(crate::Event::ListChanged);
    }
}
fn no_store() -> Error {
    Error::Unavailable("The host side chat journal is unavailable".into())
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn commands_and_context_limits() {
        assert_eq!(command("/btw"), Some(""));
        assert_eq!(command(" /btw\n 为什么？ "), Some("为什么？"));
        for normal in ["/btwice", "/btw/file", "please /btw", "btw"] {
            assert_eq!(command(normal), None);
        }
        let mut r = Question {
            question: "Why?".into(),
            previous: vec![Exchange {
                question: "Earlier?".into(),
                answer: "Yes".into(),
            }],
            ..Default::default()
        };
        let parsed: serde_json::Value = serde_json::from_str(&prompt(&r).unwrap()).unwrap();
        assert_eq!(parsed["previous_side_exchanges"][0]["answer"], "Yes");
        r.previous.resize(MAX_EXCHANGES + 1, Default::default());
        assert!(prompt(&r).is_err());
        r.previous.clear();
        r.question = "x".repeat(MAX_QUESTION + 1);
        assert!(prompt(&r).is_err());
    }

    #[cfg(unix)]
    #[tokio::test]
    async fn codex_asks_only_the_temporary_fork_of_a_paginated_thread() {
        let fixture =
            Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/codex-app-server.py");
        assert_eq!(
            crate::prediction::side_answer(&fixture, "source-thread", None, "side question")
                .await
                .unwrap(),
            "side answer"
        );
    }

    #[cfg(unix)]
    #[tokio::test]
    async fn claude_forks_without_tools_or_persistence_and_rejects_a_parent_reply() {
        use std::os::unix::fs::PermissionsExt;
        let dir = tempfile::tempdir().unwrap();
        let fixture = dir.path().join("claude");
        std::fs::write(&fixture, r#"#!/usr/bin/env python3
import sys,json
a=sys.argv
parent=a[a.index("--resume")+1]
assert "--fork-session" in a and "--no-session-persistence" in a and "--safe-mode" in a
assert a[a.index("--tools")+1]=="" and "--strict-mcp-config" in a
q=sys.stdin.read()
print(json.dumps({"type":"result","subtype":"success","is_error":False,"result":"side answer","session_id":parent if q=="bad" else "22222222-2222-4222-8222-222222222222"}))
"#).unwrap();
        std::fs::set_permissions(&fixture, std::fs::Permissions::from_mode(0o700)).unwrap();
        let parent = "11111111-1111-4111-8111-111111111111";
        assert_eq!(
            claude(&fixture, parent, dir.path(), "why").await.unwrap(),
            "side answer"
        );
        assert!(claude(&fixture, parent, dir.path(), "bad").await.is_err());
    }
}
