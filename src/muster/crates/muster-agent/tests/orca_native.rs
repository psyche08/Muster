#![cfg(unix)]
use muster_agent::orca::{Action, Source, runtime::NativeRuntime};
use muster_proto::v1::AgentMessagesRequest;
use std::{collections::HashMap, path::PathBuf, time::Duration};
fn programs() -> HashMap<String, PathBuf> {
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/orca-cli.py");
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
        assert!(source.session(&id).unwrap().updated_unix > 0);
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
