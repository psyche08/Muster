#![cfg(unix)]
use muster_agent::cli::{
    Action, Source,
    runtime::{CreateOptions, NativeRuntime},
};
use muster_proto::v1::AgentMessagesRequest;
use std::{collections::HashMap, path::PathBuf, time::Duration};
fn programs() -> HashMap<String, PathBuf> {
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/native-cli.py");
    [("codex".into(), path.clone()), ("claude".into(), path)]
        .into_iter()
        .collect()
}
async fn wait_prompt(source: &Source, id: &str) -> muster_proto::v1::PendingInteraction {
    for _ in 0..100 {
        source.refresh(id).await.unwrap();
        if let Some(p) = source.pending(id).first() {
            return p.clone();
        }
        tokio::time::sleep(Duration::from_millis(20)).await;
    }
    panic!("prompt did not arrive")
}
#[tokio::test]
async fn rust_runtime_owns_both_cli_protocols_and_persists_sessions() {
    for agent in ["codex", "claude"] {
        let dir = tempfile::tempdir().unwrap();
        let cwd = tempfile::tempdir().unwrap();
        let runtime = NativeRuntime::with_programs(dir.path(), programs()).unwrap();
        let (events, _) = tokio::sync::broadcast::channel(64);
        let source = Source::new(runtime.clone(), events);
        let id = source
            .create(
                cwd.path().to_str().unwrap(),
                agent,
                "create-one",
                &HashMap::new(),
            )
            .await
            .unwrap();
        let provider = format!("{agent}-cli");
        assert!(id.starts_with(&format!("{provider}:")));
        let row = source.session(&id).unwrap();
        assert_eq!(
            (row.provider.as_str(), row.mode.as_str()),
            (provider.as_str(), "cli")
        );
        assert!(row.updated_unix > 0);
        assert_eq!(
            source
                .create(
                    cwd.path().to_str().unwrap(),
                    agent,
                    "create-one",
                    &HashMap::new()
                )
                .await
                .unwrap(),
            id
        );
        source
            .operation(&id, "send-one", Action::Send("hello".into()))
            .await
            .unwrap();
        let prompt = wait_prompt(&source, &id).await;
        assert_eq!(prompt.options, vec!["Allow once", "Deny"]);
        assert!(
            source
                .operation(
                    &id,
                    "bad",
                    Action::Respond {
                        interaction: prompt.id.clone(),
                        option: "Allow forever".into()
                    }
                )
                .await
                .is_err()
        );
        source
            .operation(
                &id,
                "approve",
                Action::Respond {
                    interaction: prompt.id.clone(),
                    option: "Allow once".into(),
                },
            )
            .await
            .unwrap();
        assert!(
            source
                .operation(
                    &id,
                    "stale",
                    Action::Respond {
                        interaction: prompt.id,
                        option: "Allow once".into()
                    }
                )
                .await
                .is_err()
        );
        for _ in 0..100 {
            source.refresh(&id).await.unwrap();
            if source.session(&id).unwrap().state == "idle" {
                break;
            }
            tokio::time::sleep(Duration::from_millis(20)).await;
        }
        let request = AgentMessagesRequest {
            session: id.clone(),
            ..Default::default()
        };
        let history = source.history(&request).await.unwrap();
        assert_eq!(
            history
                .messages
                .iter()
                .filter(|m| m.text == "Verified **Rust** response")
                .count(),
            1
        );
        assert!(!history.messages.iter().any(|m| m.text == "Partial"));
        source
            .operation(&id, "send-two", Action::Send("second turn".into()))
            .await
            .unwrap();
        wait_prompt(&source, &id).await;
        source
            .operation(&id, "interrupt", Action::Interrupt)
            .await
            .unwrap();
        source.refresh(&id).await.unwrap();
        let updated_unix = source.session(&id).unwrap().updated_unix;
        source.shutdown();
        drop(source);
        drop(runtime);
        tokio::time::sleep(Duration::from_millis(150)).await;
        let runtime = NativeRuntime::with_programs(dir.path(), programs()).unwrap();
        let (events, _) = tokio::sync::broadcast::channel(64);
        let source = Source::new(runtime, events);
        source.discover().await.unwrap();
        assert_eq!(source.list()[0].id, id);
        assert_eq!(source.list()[0].updated_unix, updated_unix);
        assert!(
            source
                .history(&request)
                .await
                .unwrap()
                .messages
                .iter()
                .any(|m| m.text == "Verified **Rust** response")
        );
        assert!(
            source.pending(&id).is_empty(),
            "old process callbacks must not survive restart"
        );
        source
            .operation(&id, "send-three", Action::Send("resume".into()))
            .await
            .unwrap();
        wait_prompt(&source, &id).await;
        let requests = std::fs::read_to_string(cwd.path().join("provider-requests.jsonl")).unwrap();
        if agent == "codex" {
            assert!(requests.contains("thread/resume"));
        } else {
            assert!(requests.contains(r#""resume": "claude-session-1""#));
        }
        source.shutdown();
    }
}
#[tokio::test]
async fn lost_provider_receipt_is_unknown_and_is_not_replayed() {
    let dir = tempfile::tempdir().unwrap();
    let cwd = tempfile::tempdir().unwrap();
    let runtime = NativeRuntime::with_programs(dir.path(), programs()).unwrap();
    let (events, _) = tokio::sync::broadcast::channel(64);
    let source = Source::new(runtime, events);
    let id = source
        .create(
            cwd.path().to_str().unwrap(),
            "codex",
            "create",
            &HashMap::new(),
        )
        .await
        .unwrap();
    assert!(matches!(
        source
            .operation(&id, "send", Action::Send("lost receipt".into()))
            .await,
        Err(muster_agent::Error::UnknownOutcome(_))
    ));
    tokio::time::sleep(Duration::from_millis(100)).await;
    source.refresh(&id).await.unwrap();
    let requests = std::fs::read_to_string(cwd.path().join("provider-requests.jsonl")).unwrap();
    assert_eq!(
        requests
            .lines()
            .filter(|l| l.contains("turn/start"))
            .count(),
        1
    );
    source.shutdown();
}

#[tokio::test]
async fn question_cards_validate_options_and_route_exact_answers() {
    for agent in ["codex", "claude"] {
        let dir = tempfile::tempdir().unwrap();
        let cwd = tempfile::tempdir().unwrap();
        let rt = NativeRuntime::with_programs(dir.path(), programs()).unwrap();
        let (events, _) = tokio::sync::broadcast::channel(64);
        let source = Source::new(rt, events);
        let id = source
            .create(
                cwd.path().to_str().unwrap(),
                agent,
                "create",
                &HashMap::new(),
            )
            .await
            .unwrap();
        source
            .operation(&id, "send", Action::Send("question".into()))
            .await
            .unwrap();
        let p = wait_prompt(&source, &id).await;
        assert_eq!(p.kind, "question");
        assert_eq!(p.questions[0].options[0].label, "Safe");
        let answer = |option: &str| Action::Answers {
            interaction: p.id.clone(),
            answers: vec![muster_proto::v1::QuestionAnswer {
                question_id: p.questions[0].id.clone(),
                option_ids: vec![option.into()],
                text: String::new(),
            }],
        };
        assert!(
            source
                .operation(&id, "invalid", answer("not offered"))
                .await
                .is_err()
        );
        source.operation(&id, "valid", answer("0")).await.unwrap();
        assert!(
            source
                .operation(&id, "duplicate", answer("0"))
                .await
                .is_err()
        );
        source.shutdown();
    }
}

/// A session's environment has to reach the process that does the work: an
/// external scheduler's per-task credential is what lets the agent act on its
/// own issue, and the daemon cannot hold it (ADR 0009 §7).
#[tokio::test]
async fn a_sessions_environment_reaches_its_provider_child() {
    let dir = tempfile::tempdir().unwrap();
    let cwd = tempfile::tempdir().unwrap();
    let runtime = NativeRuntime::with_programs(dir.path(), programs()).unwrap();
    let (events, _) = tokio::sync::broadcast::channel(64);
    let source = Source::new(runtime.clone(), events);
    let env: HashMap<String, String> = [
        ("MUSTER_TEST_TOKEN".to_owned(), "task-scoped".to_owned()),
        ("MUSTER_TEST_TASK".to_owned(), "WS-1".to_owned()),
    ]
    .into_iter()
    .collect();
    let id = source
        .create(cwd.path().to_str().unwrap(), "codex", "create-env", &env)
        .await
        .unwrap();
    // The child starts lazily, on the first write.
    source
        .operation(&id, "send-env", Action::Send("hello".into()))
        .await
        .unwrap();

    let log = cwd.path().join("provider-requests.jsonl");
    let mut reported = None;
    for _ in 0..100 {
        if let Ok(text) = std::fs::read_to_string(&log) {
            reported = text
                .lines()
                .filter_map(|line| serde_json::from_str::<serde_json::Value>(line).ok())
                .find_map(|frame| frame.get("env").cloned());
            if reported.is_some() {
                break;
            }
        }
        tokio::time::sleep(Duration::from_millis(20)).await;
    }
    let reported = reported.expect("the fixture reported its session environment");
    assert_eq!(reported["MUSTER_TEST_TOKEN"], "task-scoped");
    assert_eq!(reported["MUSTER_TEST_TASK"], "WS-1");
    source.shutdown();
}

/// The PATH the fixture reports for the session created with `env`.
async fn child_path(env: HashMap<String, String>) -> String {
    let dir = tempfile::tempdir().unwrap();
    let cwd = tempfile::tempdir().unwrap();
    let runtime = NativeRuntime::with_programs(dir.path(), programs()).unwrap();
    let (events, _) = tokio::sync::broadcast::channel(64);
    let source = Source::new(runtime.clone(), events);
    let id = source
        .create(cwd.path().to_str().unwrap(), "claude", "create-path", &env)
        .await
        .unwrap();
    source
        .operation(&id, "send-path", Action::Send("hello".into()))
        .await
        .unwrap();
    let log = cwd.path().join("provider-requests.jsonl");
    for _ in 0..100 {
        if let Ok(text) = std::fs::read_to_string(&log) {
            let reported = text
                .lines()
                .filter_map(|line| serde_json::from_str::<serde_json::Value>(line).ok())
                .find_map(|frame| {
                    frame
                        .get("path")
                        .and_then(|p| p.as_str())
                        .map(str::to_owned)
                });
            if let Some(path) = reported {
                source.shutdown();
                return path;
            }
        }
        tokio::time::sleep(Duration::from_millis(20)).await;
    }
    panic!("the fixture did not report its PATH")
}

/// A daemon started by launchd has only the system directories on its PATH.
/// The child gets the user's directories in front of it (ADR 0329), so the
/// tools a session runs are found as in a login shell; the daemon's own
/// entries follow in their order.
#[tokio::test]
async fn a_provider_child_gets_the_users_directories_on_its_path() {
    let path = child_path(HashMap::new()).await;
    let daemon = std::env::var("PATH").unwrap_or_default();
    assert!(
        path.ends_with(&daemon),
        "{path:?} should end with {daemon:?}"
    );
    let entries: Vec<PathBuf> = std::env::split_paths(&path).collect();
    let home = std::env::home_dir().unwrap();
    for dir in [
        home.join(".local/bin"),
        PathBuf::from("/opt/homebrew/bin"),
        PathBuf::from("/usr/local/bin"),
    ] {
        if dir.is_dir() {
            assert!(entries.contains(&dir), "{dir:?} missing from {path:?}");
        }
    }
}

/// A session that names its own PATH gets exactly that one.
#[tokio::test]
async fn a_sessions_own_path_wins() {
    let own = format!(
        "/nonexistent/muster-test:{}",
        std::env::var("PATH").unwrap()
    );
    let env = [("PATH".to_owned(), own.clone())].into_iter().collect();
    assert_eq!(child_path(env).await, own);
}

/// The environment is session state the journal must not hold: it carries a
/// caller's credential, and a session resumed after a restart has to be given
/// a fresh one rather than reuse an expired one from disk.
#[tokio::test]
async fn a_sessions_environment_is_not_written_to_the_journal() {
    let dir = tempfile::tempdir().unwrap();
    let cwd = tempfile::tempdir().unwrap();
    let runtime = NativeRuntime::with_programs(dir.path(), programs()).unwrap();
    let (events, _) = tokio::sync::broadcast::channel(64);
    let source = Source::new(runtime.clone(), events);
    let env: HashMap<String, String> =
        [("MUSTER_TEST_TOKEN".to_owned(), "do-not-persist".to_owned())]
            .into_iter()
            .collect();
    source
        .create(cwd.path().to_str().unwrap(), "codex", "create-env", &env)
        .await
        .unwrap();
    source.shutdown();

    let mut found = Vec::new();
    for entry in std::fs::read_dir(dir.path()).unwrap() {
        let path = entry.unwrap().path();
        if path.is_file() {
            let bytes = std::fs::read(&path).unwrap();
            if String::from_utf8_lossy(&bytes).contains("do-not-persist") {
                found.push(path);
            }
        }
    }
    assert!(
        found.is_empty(),
        "a session credential reached the journal: {found:?}"
    );
}

/// Seed the deployed schema and opaque IDs independently of the current create path.
#[tokio::test]
async fn opens_a_pre_cutover_session_and_expires_its_old_prompt() {
    use rusqlite::{Connection, params};
    use serde_json::json;

    let dir = tempfile::tempdir().unwrap();
    let cwd = tempfile::tempdir().unwrap();
    let db = Connection::open(dir.path().join("sessions.db")).unwrap();
    db.execute_batch(
        "
        CREATE TABLE meta (key TEXT PRIMARY KEY, value TEXT NOT NULL);
        CREATE TABLE sessions (id TEXT PRIMARY KEY, data TEXT NOT NULL);
        CREATE TABLE items (session TEXT, id TEXT, seq INTEGER PRIMARY KEY AUTOINCREMENT,
                            revision INTEGER, observed INTEGER, body TEXT, UNIQUE(session,id));
        INSERT INTO meta VALUES ('runtime', 'host-a');
    ",
    )
    .unwrap();
    db.execute(
        "INSERT INTO sessions VALUES ('cli-a', ?)",
        [json!({
            "id":"cli-a", "agent":"codex", "cwd":cwd.path(), "provider_id":"thread-1",
            "generation":"old-child", "turn":"turn-1", "state":"running"
        })
        .to_string()],
    )
    .unwrap();
    for (id, body) in [
        (
            "message",
            json!({"kind":"message","role":"assistant","blocks":[{"type":"text","text":"Prior turn"}]}),
        ),
        (
            "question",
            json!({"kind":"approval","generation":"old-child","resolution":{"state":"pending"}}),
        ),
    ] {
        db.execute(
            "INSERT INTO items(session,id,revision,observed,body) VALUES ('cli-a',?,1,1000,?)",
            params![id, body.to_string()],
        )
        .unwrap();
    }
    drop(db);

    let runtime = NativeRuntime::with_programs(dir.path(), programs()).unwrap();
    let (events, _) = tokio::sync::broadcast::channel(32);
    let source = Source::new(runtime, events);
    source.discover().await.unwrap();
    let id = "orca:686f73742d61:636c692d61";
    let page = source
        .history(&AgentMessagesRequest {
            session: id.into(),
            ..Default::default()
        })
        .await
        .unwrap();
    assert!(page.messages.iter().any(|m| m.text == "Prior turn"));
    assert!(source.pending(id).is_empty());
    assert_eq!(source.session_provider(id).as_deref(), Some("codex-cli"));
    source
        .operation(id, "after-upgrade", Action::Send("Continue".into()))
        .await
        .unwrap();
    wait_prompt(&source, id).await;
    let requests = std::fs::read_to_string(cwd.path().join("provider-requests.jsonl")).unwrap();
    assert!(requests.contains("thread/resume"));
    assert!(!requests.contains("thread/start"));
    source.shutdown();
}

fn git(dir: &std::path::Path, args: &[&str]) -> String {
    let out = std::process::Command::new("git")
        .arg("-C")
        .arg(dir)
        .args([
            "-c",
            "user.name=Fixture",
            "-c",
            "user.email=fixture@example.com",
        ])
        .args(args)
        .output()
        .unwrap();
    assert!(out.status.success(), "git {args:?}: {out:?}");
    String::from_utf8(out.stdout).unwrap()
}

/// A repository with one commit, as a caller's project checkout.
fn repository() -> tempfile::TempDir {
    let repo = tempfile::tempdir().unwrap();
    git(repo.path(), &["init", "-q"]);
    std::fs::write(repo.path().join("README.md"), "fixture\n").unwrap();
    git(repo.path(), &["add", "README.md"]);
    git(repo.path(), &["commit", "-q", "-m", "fixture"]);
    repo
}

async fn reported_argv(cwd: &std::path::Path) -> Vec<String> {
    let log = cwd.join("provider-requests.jsonl");
    for _ in 0..100 {
        if let Ok(text) = std::fs::read_to_string(&log) {
            let argv = text
                .lines()
                .filter_map(|line| serde_json::from_str::<serde_json::Value>(line).ok())
                .find_map(|frame| frame.get("argv").cloned());
            if let Some(argv) = argv {
                return serde_json::from_value(argv).unwrap();
            }
        }
        tokio::time::sleep(Duration::from_millis(20)).await;
    }
    panic!("the fixture did not report its arguments")
}

/// The Tusk-style create: a Claude session in auto mode, in a worktree of the
/// project, with that worktree trusted in Claude's own config.
#[tokio::test]
async fn a_claude_session_opens_in_a_trusted_worktree_in_auto_mode() {
    let dir = tempfile::tempdir().unwrap();
    let repo = repository();
    let config = tempfile::tempdir().unwrap();
    std::fs::write(
        config.path().join(".claude.json"),
        "{\n  \"numStartups\": 7,\n  \"projects\": {}\n}\n",
    )
    .unwrap();
    let runtime = NativeRuntime::with_programs(dir.path(), programs()).unwrap();
    let (events, _) = tokio::sync::broadcast::channel(64);
    let source = Source::new(runtime.clone(), events);
    let env: HashMap<String, String> = [(
        "CLAUDE_CONFIG_DIR".to_owned(),
        config.path().to_str().unwrap().to_owned(),
    )]
    .into_iter()
    .collect();
    let options = CreateOptions {
        permission_mode: "auto".into(),
        worktree: "pr-demo-7".into(),
        trust: true,
    };
    let repo_path = repo.path().to_str().unwrap();
    let id = source
        .create_with(repo_path, "claude", "create-pr", &env, &options)
        .await
        .unwrap();

    let main = std::fs::canonicalize(repo.path()).unwrap();
    let worktree = main.join(".claude/worktrees/pr-demo-7");
    let cwd = source.session(&id).unwrap().cwd;
    assert_eq!(cwd, worktree.to_str().unwrap());
    assert!(git(&main, &["worktree", "list", "--porcelain"]).contains(&cwd));
    assert_eq!(
        git(&worktree, &["rev-parse", "HEAD"]),
        git(&main, &["rev-parse", "HEAD"])
    );
    // The main checkout stays clean: the worktrees are excluded locally.
    assert!(git(&main, &["status", "--porcelain"]).is_empty());
    let exclude = std::fs::read_to_string(main.join(".git/info/exclude")).unwrap();
    assert!(exclude.lines().any(|line| line == "/.claude/worktrees/"));

    // Trusted in the session's Claude config, everything else kept.
    let saved: serde_json::Value =
        serde_json::from_str(&std::fs::read_to_string(config.path().join(".claude.json")).unwrap())
            .unwrap();
    assert_eq!(saved["projects"][&cwd]["hasTrustDialogAccepted"], true);
    assert_eq!(saved["numStartups"], 7);
    assert!(!config.path().join(".claude.json.lock").exists());

    // Claude starts in auto mode, in the worktree.
    source
        .operation(&id, "send-pr", Action::Send("hello".into()))
        .await
        .unwrap();
    let argv = reported_argv(&worktree).await;
    let mode = argv.iter().position(|a| a == "--permission-mode").unwrap();
    assert_eq!(argv[mode + 1], "auto");

    // A replay answers with the same session; the same operation asking for
    // something else is refused.
    assert_eq!(
        source
            .create_with(repo_path, "claude", "create-pr", &env, &options)
            .await
            .unwrap(),
        id
    );
    let default_mode = CreateOptions {
        permission_mode: String::new(),
        ..options.clone()
    };
    assert!(
        source
            .create_with(repo_path, "claude", "create-pr", &env, &default_mode)
            .await
            .is_err()
    );
    // Another session may open the same worktree; it is reused as it is.
    let again = source
        .create_with(repo_path, "claude", "create-pr-2", &env, &options)
        .await
        .unwrap();
    assert_ne!(again, id);
    assert_eq!(source.session(&again).unwrap().cwd, cwd);
    let exclude = std::fs::read_to_string(main.join(".git/info/exclude")).unwrap();
    assert_eq!(
        exclude
            .lines()
            .filter(|line| *line == "/.claude/worktrees/")
            .count(),
        1
    );
    source.shutdown();
}

#[tokio::test]
async fn options_that_do_not_apply_are_refused_before_anything_is_made() {
    let dir = tempfile::tempdir().unwrap();
    let repo = repository();
    let plain = tempfile::tempdir().unwrap();
    let runtime = NativeRuntime::with_programs(dir.path(), programs()).unwrap();
    let (events, _) = tokio::sync::broadcast::channel(64);
    let source = Source::new(runtime.clone(), events);
    let repo_path = repo.path().to_str().unwrap();
    let refused = [
        (
            "codex",
            CreateOptions {
                permission_mode: "auto".into(),
                ..Default::default()
            },
            repo_path,
        ),
        (
            "codex",
            CreateOptions {
                trust: true,
                ..Default::default()
            },
            repo_path,
        ),
        (
            "claude",
            CreateOptions {
                permission_mode: "bypassPermissions".into(),
                ..Default::default()
            },
            repo_path,
        ),
        (
            "claude",
            CreateOptions {
                permission_mode: "dontAsk".into(),
                ..Default::default()
            },
            repo_path,
        ),
        (
            "claude",
            CreateOptions {
                worktree: "../escape".into(),
                ..Default::default()
            },
            repo_path,
        ),
        (
            "claude",
            CreateOptions {
                worktree: "pr-1".into(),
                ..Default::default()
            },
            plain.path().to_str().unwrap(),
        ),
    ];
    for (index, (agent, options, directory)) in refused.iter().enumerate() {
        let op = format!("refused-{index}");
        assert!(
            source
                .create_with(directory, agent, &op, &HashMap::new(), options)
                .await
                .is_err(),
            "{agent} {options:?} in {directory}"
        );
    }
    assert!(source.list().is_empty());
    assert!(!repo.path().join(".claude").exists());

    // A directory squatting on the worktree path is not taken over.
    std::fs::create_dir_all(repo.path().join(".claude/worktrees/pr-2")).unwrap();
    let squatted = CreateOptions {
        worktree: "pr-2".into(),
        ..Default::default()
    };
    assert!(
        source
            .create_with(repo_path, "claude", "squatted", &HashMap::new(), &squatted)
            .await
            .is_err()
    );
    source.shutdown();
}
