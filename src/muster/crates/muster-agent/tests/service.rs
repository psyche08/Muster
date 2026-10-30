//! The service over real transcript directories: discovery, paging, and
//! change push while a file grows.

use std::io::Write;
use std::time::{Duration, Instant};

use muster_agent::model::Provider;
use muster_agent::{AgentConfig, AgentService, Event};

#[tokio::test]
async fn claude_navigation_uses_current_registry_titles_and_refuses_real_duplicates() {
    let dir = tempfile::tempdir().unwrap();
    let home = dir.path().join("claude");
    let project = home.join("projects/repo");
    let registry = dir.path().join("desktop");
    let user = registry.join("org/user");
    std::fs::create_dir_all(&project).unwrap();
    std::fs::create_dir_all(&user).unwrap();
    let target = "356f09a5-50c5-408f-8960-465d73225098";
    let other = "177f73c7-3f0f-4beb-9eb1-b455c81a07d9";
    let title = "1.18.2 客户端 PR 修改覆盖";
    for id in [target, other] {
        std::fs::write(
            project.join(format!("{id}.jsonl")),
            format!(
                "{{\"type\":\"custom-title\",\"customTitle\":\"{title}\",\"sessionId\":\"{id}\"}}\n"
            ),
        )
        .unwrap();
    }
    let entry = |id: &str, name: &str, archived: bool| {
        std::fs::write(
            user.join(format!("local_{id}.json")),
            serde_json::json!({
                "cliSessionId": id, "title": name, "isArchived": archived
            })
            .to_string(),
        )
        .unwrap();
    };
    entry(target, title, false);
    entry(other, "Side chat", false);
    std::fs::write(user.join("scheduled-tasks.json"), "{\"scheduledTasks\":[]}").unwrap();
    let service = AgentService::start(AgentConfig {
        codex_home: None,
        claude_home: Some(home),
        catdesk_home: None,
        claude_desktop: Some(registry),
        poll: Duration::from_secs(60),
        rescan: Duration::from_secs(60),
        ..AgentConfig::default()
    });
    service.rescan().await;
    assert!(service.native_session_by_title("claude", title).is_none());
    assert_eq!(
        service.native_navigation_title(target),
        Some((title.into(), true))
    );
    assert_eq!(
        service.native_navigation_title(other),
        Some(("Side chat".into(), true))
    );
    entry(other, title, false);
    assert_eq!(
        service.native_navigation_title(target),
        Some((title.into(), false))
    );
    entry(other, title, true);
    assert_eq!(
        service.native_navigation_title(target),
        Some((title.into(), true))
    );
    // A partial registry read must not establish uniqueness.
    std::fs::write(
        user.join("local_11111111-1111-4111-8111-111111111111.json"),
        "{",
    )
    .unwrap();
    assert_eq!(
        service.native_navigation_title(target),
        Some((title.into(), false))
    );
}

#[tokio::test]
async fn native_codex_title_must_identify_one_session_even_when_another_is_newer() {
    let dir = tempfile::tempdir().unwrap();
    let day = dir.path().join("sessions/2026/09/28");
    std::fs::create_dir_all(&day).unwrap();
    let target = "01a0e659-2de9-7f63-b80b-3b53fc7754e0";
    let other = "01a0e659-2de9-7f63-b80b-3b53fc7754e1";
    for (index, id) in [target, other].iter().enumerate() {
        let path = day.join(format!("rollout-2026-09-28T12-00-00-{id}.jsonl"));
        std::fs::write(
            &path,
            format!("{{\"type\":\"session_meta\",\"payload\":{{\"id\":\"{id}\"}}}}\n"),
        )
        .unwrap();
        std::fs::File::options()
            .write(true)
            .open(path)
            .unwrap()
            .set_times(std::fs::FileTimes::new().set_modified(
                std::time::SystemTime::UNIX_EPOCH + Duration::from_secs(100 + index as u64),
            ))
            .unwrap();
    }
    let index_path = dir.path().join("session_index.jsonl");
    let index = |other_title: &str| {
        std::fs::write(
            &index_path,
            format!(
                "{{\"id\":\"{target}\",\"thread_name\":\"Locked Use Test\"}}\n\
                 {{\"id\":\"{other}\",\"thread_name\":\"{other_title}\"}}\n"
            ),
        )
        .unwrap();
    };
    index("Another Task");
    let service = AgentService::start(AgentConfig {
        codex_home: Some(dir.path().into()),
        claude_home: None,
        catdesk_home: None,
        claude_desktop: None,
        poll: Duration::from_secs(60),
        rescan: Duration::from_secs(60),
        ..AgentConfig::default()
    });
    service.rescan().await;
    assert_eq!(
        service.codex_session_by_title("Locked Use Test").as_deref(),
        Some(target)
    );
    assert!(service.codex_session_by_title("Missing Task").is_none());
    assert!(service.codex_session_by_title(" ").is_none());
    // Native receipt discovery opens both viewers before the desktop may
    // publish a generated name. Cached viewers must not mask index changes.
    service.messages(target, 10, None).await.unwrap();
    service.messages(other, 10, None).await.unwrap();

    // A more recently modified rollout with the same name must never steal
    // the AX title binding from the requested session.
    index("Locked Use Test");
    service.rescan().await;
    assert!(service.codex_session_by_title("Locked Use Test").is_none());

    index("Another Task");
    service.rescan().await;
    assert_eq!(
        service.codex_session_by_title("Locked Use Test").as_deref(),
        Some(target)
    );
    assert_eq!(
        service.codex_session_by_title("Another Task").as_deref(),
        Some(other)
    );

    std::fs::write(
        &index_path,
        format!("{{\"id\":\"{target}\",\"thread_name\":\"Renamed Native Task\"}}\n{{\"id\":\"{other}\",\"thread_name\":\"Another Task\"}}\n"),
    ).unwrap();
    service.rescan().await;
    assert_eq!(
        service.session(target).unwrap().title,
        "Renamed Native Task"
    );
    assert_eq!(
        service.native_navigation_title(target),
        Some(("Renamed Native Task".into(), true))
    );
    assert!(service.codex_session_by_title("Locked Use Test").is_none());
    let path = day.join(format!("rollout-2026-09-28T12-00-00-{target}.jsonl"));
    writeln!(std::fs::OpenOptions::new().append(true).open(path).unwrap(),
        "{}", serde_json::json!({"type":"event_msg","payload":{"type":"user_message","message":"Follow-up prompt"}})).unwrap();
    service.messages(target, 10, None).await.unwrap();
    assert_eq!(
        service.session(target).unwrap().title,
        "Renamed Native Task"
    );
    assert_eq!(
        service
            .codex_session_by_title("Renamed Native Task")
            .as_deref(),
        Some(target)
    );
}

/// Claude Desktop lists a renamed session under its new name; the head of
/// the transcript still holds the old one. Sends select the sidebar row by
/// title, so the new title must identify exactly this session.
#[tokio::test]
async fn a_renamed_claude_session_is_found_by_its_new_title() {
    let dir = tempfile::tempdir().unwrap();
    let project = dir.path().join("projects/-Users-x-tusk");
    std::fs::create_dir_all(&project).unwrap();
    let id = "93ce5c70-7289-48fd-8d80-26ff3e14cfbb";
    let other = "43a1e62f-bf17-466c-a707-6f22e8b24046";
    let path = project.join(format!("{id}.jsonl"));
    let step = |i: usize| {
        format!(
            r#"{{"type":"user","uuid":"u{i}","cwd":"/repo","timestamp":"2026-10-02T13:00:00Z","message":{{"role":"user","content":"step {i} {}"}}}}"#,
            "x".repeat(200)
        ) + "\n"
    };
    let mut text = format!(
        r#"{{"type":"custom-title","customTitle":"iOS 开发基于 macOS","sessionId":"{id}"}}"#
    ) + "\n";
    // More records than the head reader looks at come before the rename.
    for i in 0..200 {
        text += &step(i);
    }
    text += "{\"type\":\"custom-title\",\"customTitle\":\"Tusk iOS 开发\"}\n";
    text += "{\"type\":\"ai-title\",\"aiTitle\":\"Generated later\"}\n";
    std::fs::write(&path, text).unwrap();
    std::fs::write(
        project.join(format!("{other}.jsonl")),
        "{\"type\":\"custom-title\",\"customTitle\":\"磁盘空间分析\"}\n",
    )
    .unwrap();
    let service = AgentService::start(AgentConfig {
        codex_home: None,
        claude_home: Some(dir.path().into()),
        catdesk_home: None,
        claude_desktop: None,
        poll: Duration::from_secs(60),
        rescan: Duration::from_secs(60),
        ..AgentConfig::default()
    });
    service.rescan().await;
    let found = |title: &str| service.native_session_by_title("claude", title);
    assert_eq!(found("Tusk iOS 开发").as_deref(), Some(id));
    assert_eq!(service.session(id).unwrap().title, "Tusk iOS 开发");
    // A generated title never replaces the user's name, and the old name
    // no longer identifies the session.
    assert!(found("Generated later").is_none());
    assert!(found("iOS 开发基于 macOS").is_none());
    assert_eq!(found("磁盘空间分析").as_deref(), Some(other));

    // An open viewer reads the same title from the whole file.
    service.messages(id, 10, None).await.unwrap();
    assert_eq!(found("Tusk iOS 开发").as_deref(), Some(id));

    // Once the conversation pushes the rename out of the tail window, the
    // known title stays rather than falling back to the head's.
    let mut file = std::fs::File::options().append(true).open(&path).unwrap();
    for i in 200..6000 {
        file.write_all(step(i).as_bytes()).unwrap();
    }
    drop(file);
    assert!(std::fs::metadata(&path).unwrap().len() > 1 << 20);
    service.rescan().await;
    assert_eq!(found("Tusk iOS 开发").as_deref(), Some(id));
    assert!(found("iOS 开发基于 macOS").is_none());
}

#[tokio::test]
async fn lists_pages_and_pushes_appended_messages() {
    let dir = tempfile::tempdir().unwrap();
    let codex_home = dir.path().join("codex");
    let claude_home = dir.path().join("claude");
    let day = codex_home.join("sessions/2026/09/24");
    std::fs::create_dir_all(&day).unwrap();
    let project = claude_home.join("projects/-repo");
    std::fs::create_dir_all(&project).unwrap();

    let codex_id = "0f8d6b3a-2f0e-4c53-9d1a-3a4b5c6d7e8f";
    let codex_path = day.join(format!("rollout-2026-09-24T10-00-00-{codex_id}.jsonl"));
    let mut codex_file = std::fs::File::create(&codex_path).unwrap();
    writeln!(
        codex_file,
        r#"{{"type":"session_meta","payload":{{"id":"{codex_id}","cwd":"{}"}}}}"#,
        dir.path().display()
    )
    .unwrap();
    writeln!(codex_file, r#"{{"type":"turn_context","payload":{{}}}}"#).unwrap();
    for i in 0..5 {
        writeln!(codex_file, r#"{{"type":"response_item","payload":{{"type":"message","role":"user","content":[{{"type":"input_text","text":"m{i}"}}]}}}}"#).unwrap();
    }
    let claude_id = "a1b2c3d4-e5f6-4a7b-8c9d-0e1f2a3b4c5d";
    std::fs::write(
        project.join(format!("{claude_id}.jsonl")),
        format!("{{\"type\":\"user\",\"cwd\":\"{}\",\"message\":{{\"role\":\"user\",\"content\":\"hi claude\"}}}}\n", dir.path().display()),
    )
    .unwrap();

    let service = AgentService::start(AgentConfig {
        codex_home: Some(codex_home),
        claude_home: Some(claude_home),
        catdesk_home: None,
        claude_desktop: None,
        poll: Duration::from_millis(50),
        rescan: Duration::from_millis(200),
        ..AgentConfig::default()
    });
    let mut events = service.subscribe();
    service.rescan().await;
    let all = service.list(None, 0);
    assert_eq!(all.len(), 2);
    assert_eq!(service.list(Some(Provider::Claude), 0).len(), 1);
    let claude = service.session(claude_id).unwrap();
    assert_eq!(claude.title, "hi claude");
    assert_eq!(claude.provider, "claude");

    // Paging backwards by id.
    let (page, more) = service.messages(codex_id, 2, None).await.unwrap();
    assert_eq!(
        page.iter().map(|m| m.text.as_str()).collect::<Vec<_>>(),
        vec!["m3", "m4"]
    );
    assert!(more);
    let (earlier, more) = service
        .messages(codex_id, 2, Some(&page[0].id))
        .await
        .unwrap();
    assert_eq!(
        earlier.iter().map(|m| m.text.as_str()).collect::<Vec<_>>(),
        vec!["m1", "m2"]
    );
    assert!(more);
    let (first, more) = service
        .messages(codex_id, 2, Some(&earlier[0].id))
        .await
        .unwrap();
    assert_eq!(first.len(), 1);
    assert!(!more);

    // Append: the poll loop pushes the new message.
    writeln!(codex_file, r#"{{"type":"response_item","timestamp":"2026-09-29T04:05:12Z","payload":{{"type":"message","role":"assistant","content":[{{"type":"output_text","text":"reply"}}]}}}}"#).unwrap();
    let pushed = tokio::time::timeout(Duration::from_secs(5), async {
        loop {
            match events.recv().await {
                Ok(Event::Message { session, message }) if session == codex_id => break message,
                Ok(_) => continue,
                Err(tokio::sync::broadcast::error::RecvError::Lagged(_)) => continue,
                Err(_) => panic!("closed"),
            }
        }
    })
    .await
    .expect("message pushed");
    assert_eq!(pushed.text, "reply");
    assert_eq!(pushed.role, "assistant");
    assert_eq!(service.session(codex_id).unwrap().message_count, 6);
    // An open viewer must update the time together with its file stat. Otherwise
    // rescan sees the new stat and permanently reuses the stale catalog time.
    let latest = muster_agent::model::parse_ts("2026-09-29T04:05:12Z");
    assert_eq!(service.session(codex_id).unwrap().updated_unix, latest);
    service.rescan().await;
    assert_eq!(service.session(codex_id).unwrap().updated_unix, latest);
    writeln!(codex_file, r#"{{"type":"event_msg","timestamp":"2026-09-29T05:00:00Z","payload":{{"type":"token_count"}}}}"#).unwrap();
    service.rescan().await;
    assert_eq!(service.session(codex_id).unwrap().updated_unix, latest);

    // A new transcript shows up on rescan as a list change.
    let new_id = "1f8d6b3a-2f0e-4c53-9d1a-3a4b5c6d7e8f";
    std::fs::write(
        day.join(format!("rollout-2026-09-24T11-00-00-{new_id}.jsonl")),
        format!(r#"{{"type":"session_meta","payload":{{"id":"{new_id}"}}}}"#),
    )
    .unwrap();
    tokio::time::timeout(Duration::from_secs(5), async {
        loop {
            match events.recv().await {
                Ok(Event::ListChanged) if service.session(new_id).is_some() => break,
                Ok(_) => continue,
                Err(tokio::sync::broadcast::error::RecvError::Lagged(_)) => continue,
                Err(_) => panic!("closed"),
            }
        }
    })
    .await
    .expect("list change pushed");
    assert_eq!(service.list(Some(Provider::Codex), 0).len(), 2);

    // Files and git are confined to the session cwd.
    let listing = service.files(codex_id, "").await.unwrap();
    assert!(listing.entries.iter().any(|e| e.name == "codex" && e.dir));
    assert!(service.file(codex_id, "../x", 0).await.is_err());
}

/// With polling effectively off, the filesystem watcher alone must push an
/// appended message and notice a new transcript. GitHub's macOS runners
/// deliver no FSEvents at all (the poll fallback carries the service
/// there); run it by hand on a real Mac with `--ignored`.
#[tokio::test]
#[cfg_attr(
    target_os = "macos",
    ignore = "FSEvents are not delivered on GitHub's macOS runners"
)]
async fn watcher_pushes_without_polling() {
    let _ = env_logger::Builder::from_env(env_logger::Env::default().default_filter_or("debug"))
        .is_test(true)
        .try_init();
    let dir = tempfile::tempdir().unwrap();
    let codex_home = dir.path().join("codex");
    let day = codex_home.join("sessions/2026/09/25");
    std::fs::create_dir_all(&day).unwrap();
    let id = "2f8d6b3a-2f0e-4c53-9d1a-3a4b5c6d7e8f";
    let path = day.join(format!("rollout-2026-09-25T10-00-00-{id}.jsonl"));
    let mut file = std::fs::File::create(&path).unwrap();
    writeln!(
        file,
        r#"{{"type":"session_meta","payload":{{"id":"{id}","cwd":"{}"}}}}"#,
        dir.path().display()
    )
    .unwrap();
    writeln!(file, r#"{{"type":"turn_context","payload":{{}}}}"#).unwrap();

    let service = AgentService::start(AgentConfig {
        codex_home: Some(codex_home.clone()),
        claude_home: None,
        catdesk_home: None,
        claude_desktop: None,
        poll: Duration::from_secs(60),
        rescan: Duration::from_secs(60),
        ..AgentConfig::default()
    });
    let mut events = service.subscribe();
    service.rescan().await;
    // Opening the session makes it a watched transcript.
    let (page, _) = service.messages(id, 10, None).await.unwrap();
    assert!(page.is_empty());

    // The budget is well under the 60 s poll but generous: FSEvents on a
    // macOS CI runner can take seconds to deliver.
    let budget = Duration::from_secs(20);
    let started = Instant::now();
    writeln!(file, r#"{{"type":"response_item","payload":{{"type":"message","role":"user","content":[{{"type":"input_text","text":"quick"}}]}}}}"#).unwrap();
    let pushed = tokio::time::timeout(budget, async {
        loop {
            match events.recv().await {
                Ok(Event::Message { session, message }) if session == id => break message,
                Ok(_) => continue,
                Err(tokio::sync::broadcast::error::RecvError::Lagged(_)) => continue,
                Err(_) => panic!("closed"),
            }
        }
    })
    .await
    .expect("the watcher woke the poll");
    assert_eq!(pushed.text, "quick");
    let latency = started.elapsed();
    assert!(latency < budget, "{latency:?}");
    eprintln!("watcher latency: {latency:?}");

    // A new day directory and transcript: the watcher follows created
    // directories and a create asks for a rescan.
    let later = codex_home.join("sessions/2026/09/26");
    std::fs::create_dir_all(&later).unwrap();
    let new_id = "3f8d6b3a-2f0e-4c53-9d1a-3a4b5c6d7e8f";
    std::fs::write(
        later.join(format!("rollout-2026-09-26T09-00-00-{new_id}.jsonl")),
        format!(r#"{{"type":"session_meta","payload":{{"id":"{new_id}"}}}}"#),
    )
    .unwrap();
    tokio::time::timeout(budget, async {
        loop {
            match events.recv().await {
                Ok(Event::ListChanged) if service.session(new_id).is_some() => break,
                Ok(_) => continue,
                Err(tokio::sync::broadcast::error::RecvError::Lagged(_)) => continue,
                Err(_) => panic!("closed"),
            }
        }
    })
    .await
    .expect("the new transcript was listed without a timed rescan");
}

/// Discovering an idle transcript must not resume a provider process. Both
/// providers remain readable but require an attached desktop AX controller.
#[tokio::test]
async fn transcript_discovery_never_grants_cli_control() {
    let dir = tempfile::tempdir().unwrap();
    let codex = dir.path().join("codex");
    let claude = dir.path().join("claude");
    let day = codex.join("sessions/2026/09/28");
    let project = claude.join("projects/-fixture");
    std::fs::create_dir_all(&day).unwrap();
    std::fs::create_dir_all(&project).unwrap();
    let codex_id = "01a0e659-2de9-7f63-b80b-3b53fc7754e0";
    let claude_id = "a1b2c3d4-e5f6-4a7b-8c9d-0e1f2a3b4c5d";
    std::fs::write(day.join(format!("rollout-2026-09-28T12-00-00-{codex_id}.jsonl")),
        format!("{{\"type\":\"session_meta\",\"payload\":{{\"id\":\"{codex_id}\"}}}}\n{{\"type\":\"turn_context\",\"payload\":{{}}}}\n{{\"type\":\"response_item\",\"payload\":{{\"type\":\"message\",\"role\":\"user\",\"content\":[{{\"type\":\"input_text\",\"text\":\"history\"}}]}}}}\n")).unwrap();
    std::fs::write(
        project.join(format!("{claude_id}.jsonl")),
        "{\"type\":\"user\",\"message\":{\"role\":\"user\",\"content\":\"history\"}}\n",
    )
    .unwrap();
    let service = AgentService::start(AgentConfig {
        codex_home: Some(codex),
        claude_home: Some(claude),
        catdesk_home: None,
        claude_desktop: None,
        ..AgentConfig::default()
    });
    service.rescan().await;
    for id in [codex_id, claude_id] {
        assert_eq!(service.session(id).unwrap().controller, "none");
        let (messages, _) = service.messages(id, 10, None).await.unwrap();
        assert_eq!(messages[0].text, "history");
        for _ in 0..2 {
            let error = service.send(id, "do not start a CLI").await.unwrap_err();
            assert!(matches!(error, muster_agent::Error::Unavailable(_)));
            assert!(error.to_string().contains("Accessibility"));
        }
        assert!(service.interrupt(id).await.is_err());
        assert_eq!(service.session(id).unwrap().controller, "none");
    }
}

#[tokio::test]
async fn list_recency_uses_messages_not_file_touch_or_metadata() {
    let dir = tempfile::tempdir().unwrap();
    let home = dir.path();
    let sessions = home.join("sessions");
    std::fs::create_dir_all(&sessions).unwrap();
    let older = "00000000-0000-0000-0000-000000000001";
    let newer = "00000000-0000-0000-0000-000000000002";
    let record = |time: &str| {
        format!(
            "{{\"type\":\"response_item\",\"timestamp\":\"{time}\",\"payload\":{{\"type\":\"message\",\"role\":\"assistant\",\"content\":[{{\"type\":\"output_text\",\"text\":\"done\"}}]}}}}\n"
        )
    };
    let mut paths = Vec::new();
    for (id, time, mtime) in [
        (older, "2026-09-28T01:00:00Z", 200),
        (newer, "2026-09-28T02:00:00Z", 100),
    ] {
        let path = sessions.join(format!("rollout-{id}.jsonl"));
        std::fs::write(&path, format!("{{\"type\":\"session_meta\",\"payload\":{{\"id\":\"{id}\"}}}}\n{}{{\"type\":\"event_msg\",\"timestamp\":\"2026-09-28T09:00:00Z\",\"payload\":{{\"type\":\"token_count\"}}}}\n", record(time))).unwrap();
        std::fs::File::options()
            .write(true)
            .open(&path)
            .unwrap()
            .set_times(
                std::fs::FileTimes::new()
                    .set_modified(std::time::SystemTime::UNIX_EPOCH + Duration::from_secs(mtime)),
            )
            .unwrap();
        paths.push(path);
    }
    let service = AgentService::start(AgentConfig {
        codex_home: Some(home.into()),
        claude_home: None,
        catdesk_home: None,
        claude_desktop: None,
        poll: Duration::from_secs(60),
        rescan: Duration::from_secs(60),
        ..Default::default()
    });
    service.rescan().await;
    assert_eq!(service.list(None, 1)[0].id, newer);
    let mut file = std::fs::OpenOptions::new()
        .append(true)
        .open(&paths[0])
        .unwrap();
    file.write_all(record("2026-09-28T03:00:00Z").as_bytes())
        .unwrap();
    service.rescan().await;
    assert_eq!(service.list(None, 1)[0].id, older);
    assert_eq!(
        service.list(None, 1)[0].updated_unix,
        muster_agent::model::parse_ts("2026-09-28T03:00:00Z")
    );
}

#[tokio::test]
async fn queue_changes_push_without_a_transcript_write_or_visible_controller() {
    let dir = tempfile::tempdir().unwrap();
    let day = dir.path().join("sessions/2026/10/01");
    std::fs::create_dir_all(&day).unwrap();
    let session = "01a0e659-2de9-7f63-b80b-3b53fc7754e0";
    let record = serde_json::json!({"type":"session_meta","payload":{"id":session}});
    std::fs::write(
        day.join(format!("rollout-2026-10-01T10-00-00-{session}.jsonl")),
        format!("{record}\n"),
    )
    .unwrap();
    let db = rusqlite::Connection::open(dir.path().join("queue_1.sqlite")).unwrap();
    db.execute_batch("PRAGMA journal_mode=WAL; CREATE TABLE queued_items (id TEXT, thread_id TEXT, payload_json TEXT, queue_order INTEGER, created_at_ms INTEGER);").unwrap();
    let service = AgentService::start(AgentConfig {
        codex_home: Some(dir.path().into()),
        claude_home: None,
        catdesk_home: None,
        claude_desktop: None,
        poll: Duration::from_millis(30),
        ..AgentConfig::default()
    });
    let mut events = service.subscribe();
    service.rescan().await;
    service.messages(session, 1, None).await.unwrap();
    let payload =
        serde_json::json!({"input":[{"type":"text","text":"queued follow-up","text_elements":[]}]});
    db.execute(
        "INSERT INTO queued_items VALUES ('pending',?1,?2,0,0)",
        rusqlite::params![session, payload.to_string()],
    )
    .unwrap();
    let interaction = tokio::time::timeout(Duration::from_secs(3), async {
        loop {
            if let Event::Interaction(q) = events.recv().await.unwrap() {
                if q.kind == "queued_message" {
                    break q;
                }
            }
        }
    })
    .await
    .unwrap();
    assert_eq!(interaction.prompt, "queued follow-up");
    assert_eq!(interaction.options, ["Send now"]);
    assert!(service.pending(session).contains(&interaction));
    db.execute("DELETE FROM queued_items", []).unwrap();
    tokio::time::timeout(Duration::from_secs(3), async {
        loop {
            if let Event::InteractionResolved {
                interaction: id, ..
            } = events.recv().await.unwrap()
            {
                if id == interaction.id {
                    break;
                }
            }
        }
    })
    .await
    .unwrap();
    assert!(
        service
            .pending(session)
            .iter()
            .all(|i| i.kind != "queued_message")
    );
}

#[tokio::test]
async fn command_line_claude_sessions_are_refused_before_any_desktop_lookup() {
    let dir = tempfile::tempdir().unwrap();
    let project = dir.path().join("projects/-repo");
    std::fs::create_dir_all(&project).unwrap();
    // The 2026-10-02 18:55 target: a resumable `claude -p` session. Claude.app
    // never showed it, so the sidebar search could only fail after an unlock.
    let cli = "c86f6c98-b9c2-4d8c-9c4f-7291c03453c0";
    let desktop = "93ce5c70-7289-48fd-8d80-26ff3e14cfbb";
    for (id, entrypoint) in [(cli, "sdk-cli"), (desktop, "claude-desktop")] {
        std::fs::write(
            project.join(format!("{id}.jsonl")),
            format!("{{\"type\":\"user\",\"entrypoint\":\"{entrypoint}\",\"cwd\":\"/repo\",\"message\":{{\"role\":\"user\",\"content\":\"hi\"}}}}\n"),
        )
        .unwrap();
    }
    let service = AgentService::start(AgentConfig {
        codex_home: None,
        claude_home: Some(dir.path().into()),
        catdesk_home: None,
        claude_desktop: None,
        poll: Duration::from_secs(60),
        rescan: Duration::from_secs(60),
        ..AgentConfig::default()
    });
    service.rescan().await;
    let refused = service.require_desktop_input(cli).unwrap_err();
    assert!(
        matches!(&refused, muster_agent::Error::Unavailable(m)
            if m.contains("sdk-cli") && m.contains("cannot be controlled") && m.contains("nothing was sent")),
        "{refused}"
    );
    assert!(service.require_desktop_input(desktop).is_ok());
    assert!(service.require_desktop_input("unknown-session").is_ok());
}

/// The 2026-10-03 Mac-B target: Claude.app had archived the conversation,
/// so its sidebar never listed it and every send failed after an unlock.
#[tokio::test]
async fn sessions_archived_in_claude_app_are_hidden_and_refused_before_unlock() {
    let dir = tempfile::tempdir().unwrap();
    let project = dir.path().join("projects/-repo");
    std::fs::create_dir_all(&project).unwrap();
    let archived = "b413419f-3897-4cb2-920a-32f938d3d908";
    let shown = "93ce5c70-7289-48fd-8d80-26ff3e14cfbb";
    for id in [archived, shown] {
        std::fs::write(
            project.join(format!("{id}.jsonl")),
            "{\"type\":\"user\",\"entrypoint\":\"claude-desktop\",\"cwd\":\"/repo\",\"message\":{\"role\":\"user\",\"content\":\"hi\"}}\n",
        )
        .unwrap();
    }
    let registry = tempfile::tempdir().unwrap();
    let user = registry.path().join("org/user");
    std::fs::create_dir_all(&user).unwrap();
    for (name, id, is_archived) in [
        ("local_a.json", archived, true),
        ("local_b.json", shown, false),
    ] {
        std::fs::write(
            user.join(name),
            format!(r#"{{"cliSessionId":"{id}","isArchived":{is_archived}}}"#),
        )
        .unwrap();
    }
    let service = AgentService::start(AgentConfig {
        codex_home: None,
        claude_home: Some(dir.path().into()),
        catdesk_home: None,
        claude_desktop: Some(registry.path().into()),
        poll: Duration::from_secs(60),
        rescan: Duration::from_secs(60),
        ..AgentConfig::default()
    });
    service.rescan().await;
    let listed: Vec<String> = service.list(None, 0).into_iter().map(|s| s.id).collect();
    assert_eq!(listed, vec![shown.to_string()]);
    let refused = service.require_desktop_input(archived).unwrap_err();
    assert!(
        matches!(&refused, muster_agent::Error::Unavailable(m)
            if m.contains("archived") && m.contains("Nothing was sent")),
        "{refused}"
    );
    assert!(service.require_desktop_input(shown).is_ok());
}

/// The providers' cloud sessions sit in the same list as the local ones,
/// read through the provider's API once someone opens them, take a prompt
/// where the profile allows it, and offer no files, interrupts or answers.
#[tokio::test]
async fn cloud_sessions_are_listed_read_and_written_through_the_provider_api() {
    use muster_agent::cloud::{Http, Request, Response};
    use std::sync::{Arc, Mutex};

    struct Fake {
        requests: Mutex<Vec<Request>>,
        reply: Mutex<String>,
    }
    #[async_trait::async_trait]
    impl Http for Fake {
        async fn send(&self, request: Request) -> Result<Response, String> {
            self.requests.lock().unwrap().push(request.clone());
            let body = if request.url.contains("/v1/code/sessions?") {
                serde_json::json!({"data": [
                    {"id": "session_cloud1", "title": "Cloud fix", "status": "active", "worker_status": "idle",
                     "created_at": "2026-10-04T10:00:00Z", "last_event_at": "2026-10-04T12:00:00Z",
                     "config": {"sources": [{"type": "git_repository", "url": "https://github.com/o/r", "revision": "main"}]}}
                ]})
            } else if request
                .url
                .contains("/v1/code/sessions/session_cloud1/events")
                && request.method == "GET"
            {
                serde_json::json!({"data": [
                    {"sequence_num": 1, "payload": {"type": "user", "uuid": "u1", "timestamp": "2026-10-04T12:00:00Z",
                        "message": {"role": "user", "content": "Fix it"}}},
                    {"sequence_num": 2, "payload": {"type": "assistant", "uuid": "a1", "timestamp": "2026-10-04T12:00:05Z",
                        "message": {"role": "assistant", "stop_reason": "end_turn", "content": [{"type": "text", "text": "Done."}]}}}
                ]})
            } else if request.url.contains("/wham/tasks/list") {
                serde_json::json!({"items": [
                    {"id": "task_e_9", "title": "Codex task", "updated_at": 1_759_600_000,
                     "task_status_display": {"environment_label": "o/r",
                         "latest_turn_status_display": {"turn_status": "in_progress"}}}
                ]})
            } else if request.url.contains("/wham/tasks/task_e_9") {
                let reply = self.reply.lock().unwrap().clone();
                serde_json::json!({
                    "current_user_turn": {"input_items": [{"type": "message", "role": "user", "content": [{"content_type": "text", "text": "Write tests"}]}]},
                    "current_assistant_turn": {"turn_status": "in_progress", "output_items": if reply.is_empty() { vec![] } else { vec![serde_json::json!({"type":"message","content":[{"content_type":"text","text":reply}]})] }}
                })
            } else {
                serde_json::json!({"ok": true})
            };
            Ok(Response {
                status: 200,
                body: body.to_string(),
            })
        }
    }

    let dir = tempfile::tempdir().unwrap();
    let claude = dir.path().join("claude");
    let codex = dir.path().join("codex");
    std::fs::create_dir_all(&claude).unwrap();
    std::fs::create_dir_all(&codex).unwrap();
    let expires = (std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_secs()
        + 3600)
        * 1000;
    std::fs::write(
        claude.join(".credentials.json"),
        serde_json::json!({"claudeAiOauth": {"accessToken": "tok", "expiresAt": expires}})
            .to_string(),
    )
    .unwrap();
    std::fs::write(
        codex.join("auth.json"),
        serde_json::json!({"tokens": {"access_token": "opaque-token", "account_id": "acct"}})
            .to_string(),
    )
    .unwrap();
    let fake = Arc::new(Fake {
        requests: Mutex::new(Vec::new()),
        reply: Mutex::new(String::new()),
    });
    let service = AgentService::start(AgentConfig {
        codex_home: Some(codex),
        claude_home: Some(claude),
        catdesk_home: None,
        claude_desktop: None,
        poll: Duration::from_secs(60),
        rescan: Duration::from_secs(60),
        cloud_http: Some(fake.clone()),
        ..AgentConfig::default()
    });
    let deadline = Instant::now() + Duration::from_secs(10);
    let listed = loop {
        let sessions = service.list(None, 0);
        if sessions.len() == 2 {
            break sessions;
        }
        assert!(
            Instant::now() < deadline,
            "cloud sessions never listed: {sessions:?}"
        );
        tokio::time::sleep(Duration::from_millis(50)).await;
    };
    let claude_id = "cloud:claude:session_cloud1";
    let codex_id = "cloud:codex:task_e_9";
    let by_id = |id: &str| listed.iter().find(|s| s.id == id).cloned().unwrap();
    let cloud = by_id(claude_id);
    assert_eq!(
        (cloud.mode.as_str(), cloud.controller.as_str()),
        ("cloud", "cloud")
    );
    assert_eq!(
        (cloud.provider.as_str(), cloud.title.as_str()),
        ("claude", "Cloud fix")
    );
    assert_eq!(cloud.cwd, "o/r@main");
    assert_eq!(
        (cloud.state.as_str(), cloud.process_state.as_str()),
        ("idle", "running")
    );
    assert_eq!(by_id(codex_id).state, "running");
    assert_eq!(service.list(Some(Provider::Codex), 0).len(), 1);
    assert!(service.session(claude_id).is_some());
    assert!(service.pending(claude_id).is_empty());

    let (messages, earlier) = service.messages(claude_id, 0, None).await.unwrap();
    assert!(!earlier);
    let texts: Vec<&str> = messages.iter().map(|m| m.text.as_str()).collect();
    assert_eq!(texts[..2], ["Fix it", "Done."]);
    assert_eq!(
        service.session(claude_id).unwrap().message_count,
        messages.len() as u32
    );
    let (codex_messages, _) = service.messages(codex_id, 0, None).await.unwrap();
    assert_eq!(codex_messages.len(), 1);
    assert_eq!(codex_messages[0].text, "Write tests");
    *fake.reply.lock().unwrap() = "Fresh on re-entry".into();
    let fresh = service.messages(codex_id, 0, None).await.unwrap().0;
    assert!(fresh.iter().any(|m| m.text == "Fresh on re-entry"));
    let _watch = service.watch_session(codex_id);
    let mut live = service.subscribe();
    *fake.reply.lock().unwrap() = "The same reply keeps growing".into();
    tokio::time::timeout(Duration::from_secs(6), async {
        loop {
            if let Event::Reset { session } = live.recv().await.unwrap()
                && session == codex_id
            {
                break;
            }
        }
    })
    .await
    .expect("visible cloud sessions refresh before the 20-second catalog interval");

    assert_eq!(
        service.send_to(claude_id, "And the tests").await.unwrap(),
        claude_id
    );
    let posted = fake
        .requests
        .lock()
        .unwrap()
        .iter()
        .find(|r| r.method == "POST")
        .cloned()
        .expect("the prompt was posted");
    assert!(
        posted
            .url
            .ends_with("/v1/code/sessions/session_cloud1/events")
    );
    assert!(posted.body.unwrap().contains("And the tests"));
    let refused = service
        .send_to(codex_id, "more")
        .await
        .unwrap_err()
        .to_string();
    assert!(refused.contains("Codex Cloud"), "{refused}");
    let interrupt = service.interrupt(claude_id).await.unwrap_err().to_string();
    assert!(interrupt.contains("Claude Cloud"), "{interrupt}");
    assert!(service.files(claude_id, "").await.is_err());
    assert!(service.require_desktop_input(claude_id).is_err());
    assert!(
        service
            .respond_in_session(claude_id, "x", "Yes", "")
            .await
            .is_err()
    );
    assert!(service.answer_questions(claude_id, "x", &[]).await.is_err());
}

#[tokio::test]
async fn reopening_reads_new_bytes_before_the_background_poll() {
    let dir = tempfile::tempdir().unwrap();
    let day = dir.path().join("sessions/2026/10/05");
    std::fs::create_dir_all(&day).unwrap();
    let id = "01a0e659-2de9-7f63-b80b-3b53fc7754e0";
    let path = day.join(format!("rollout-2026-10-05T12-00-00-{id}.jsonl"));
    let mut file = std::fs::File::create(path).unwrap();
    writeln!(
        file,
        r#"{{"type":"session_meta","payload":{{"id":"{id}"}}}}"#
    )
    .unwrap();
    writeln!(file, r#"{{"type":"turn_context","payload":{{}}}}"#).unwrap();
    let service = AgentService::start(AgentConfig {
        codex_home: Some(dir.path().into()),
        claude_home: None,
        catdesk_home: None,
        claude_desktop: None,
        poll: Duration::from_secs(60),
        rescan: Duration::from_secs(60),
        ..AgentConfig::default()
    });
    service.rescan().await;
    assert!(service.messages(id, 10, None).await.unwrap().0.is_empty());
    writeln!(file, r#"{{"type":"response_item","payload":{{"type":"message","role":"assistant","content":[{{"type":"output_text","text":"available immediately"}}]}}}}"#).unwrap();
    // No yield for the background timer or filesystem watcher between the
    // write and this read of an already-open viewer.
    let page = service
        .history_page(&muster_proto::v1::AgentMessagesRequest {
            session: id.into(),
            ..Default::default()
        })
        .await
        .unwrap();
    assert_eq!(page.messages.last().unwrap().text, "available immediately");
}

#[tokio::test]
async fn active_session_watches_outlive_idle_eviction_and_release_independently() {
    let dir = tempfile::tempdir().unwrap();
    let day = dir.path().join("sessions/2026/10/05");
    std::fs::create_dir_all(&day).unwrap();
    let id = "01a0e659-2de9-7f63-b80b-3b53fc7754e1";
    let path = day.join(format!("rollout-2026-10-05T12-00-00-{id}.jsonl"));
    let mut file = std::fs::File::create(path).unwrap();
    writeln!(
        file,
        r#"{{"type":"session_meta","payload":{{"id":"{id}"}}}}"#
    )
    .unwrap();
    writeln!(file, r#"{{"type":"turn_context","payload":{{}}}}"#).unwrap();
    let service = AgentService::start(AgentConfig {
        codex_home: Some(dir.path().into()),
        claude_home: None,
        catdesk_home: None,
        claude_desktop: None,
        poll: Duration::from_millis(10),
        idle_evict: Duration::from_millis(30),
        rescan: Duration::from_secs(60),
        ..AgentConfig::default()
    });
    service.rescan().await;
    let first = service.watch_session(id);
    let second = service.watch_session(id);
    service.messages(id, 10, None).await.unwrap();
    tokio::time::sleep(Duration::from_millis(100)).await;
    drop(first);
    tokio::time::sleep(Duration::from_millis(100)).await;
    let mut events = service.subscribe();
    writeln!(file, r#"{{"type":"response_item","payload":{{"type":"message","role":"assistant","content":[{{"type":"output_text","text":"still live"}}]}}}}"#).unwrap();
    tokio::time::timeout(Duration::from_secs(2), async {
        loop {
            if let Event::Message { session, message } = events.recv().await.unwrap()
                && session == id
                && message.text == "still live"
            {
                break;
            }
        }
    })
    .await
    .expect("a still-watched session must keep publishing after the idle deadline");
    drop(second);
    tokio::time::timeout(Duration::from_secs(2), async {
        while service.session(id).unwrap().message_count != 0 {
            tokio::time::sleep(Duration::from_millis(20)).await;
        }
    })
    .await
    .expect("the final disconnected watch must allow eviction");
}

#[tokio::test]
async fn resumed_transcript_notifies_watchers_to_reload_the_new_segment() {
    let dir = tempfile::tempdir().unwrap();
    let day = dir.path().join("sessions/2026/10/05");
    std::fs::create_dir_all(&day).unwrap();
    let id = "01a0e659-2de9-7f63-b80b-3b53fc7754e2";
    let segment = |name: &str, text: &str, time: &str| {
        std::fs::write(day.join(format!("rollout-{name}-{id}.jsonl")), format!(
            "{{\"type\":\"session_meta\",\"payload\":{{\"id\":\"{id}\"}}}}\n{{\"type\":\"turn_context\",\"payload\":{{}}}}\n{{\"type\":\"response_item\",\"timestamp\":\"{time}\",\"payload\":{{\"type\":\"message\",\"role\":\"assistant\",\"content\":[{{\"type\":\"output_text\",\"text\":\"{text}\"}}]}}}}\n"
        )).unwrap();
    };
    segment("first", "old segment", "2026-10-05T10:00:00Z");
    let service = AgentService::start(AgentConfig {
        codex_home: Some(dir.path().into()),
        claude_home: None,
        catdesk_home: None,
        claude_desktop: None,
        poll: Duration::from_secs(60),
        rescan: Duration::from_secs(60),
        ..AgentConfig::default()
    });
    service.rescan().await;
    let _watch = service.watch_session(id);
    assert_eq!(
        service.messages(id, 10, None).await.unwrap().0[0].text,
        "old segment"
    );
    let mut events = service.subscribe();
    segment("resumed", "new segment", "2026-10-05T11:00:00Z");
    service.rescan().await;
    tokio::time::timeout(Duration::from_secs(2), async {
        loop {
            if let Event::Reset { session } = events.recv().await.unwrap()
                && session == id
            {
                break;
            }
        }
    })
    .await
    .expect("a replaced rollout must invalidate the watched history");
    assert_eq!(
        service.messages(id, 10, None).await.unwrap().0[0].text,
        "new segment"
    );
}

#[tokio::test]
async fn claude_queue_is_visible_and_retires_when_provider_dequeues_it() {
    let dir = tempfile::tempdir().unwrap();
    let project = dir.path().join("projects/test");
    std::fs::create_dir_all(&project).unwrap();
    let session = "11111111-1111-4111-8111-111111111111";
    let path = project.join(format!("{session}.jsonl"));
    std::fs::write(
        &path,
        "{\"type\":\"user\",\"message\":{\"role\":\"user\",\"content\":\"task\"}}\n",
    )
    .unwrap();
    let service = AgentService::start(AgentConfig {
        codex_home: None,
        claude_home: Some(dir.path().into()),
        catdesk_home: None,
        claude_desktop: None,
        poll: Duration::from_millis(30),
        ..Default::default()
    });
    service.rescan().await;
    let mut events = service.subscribe();
    service.messages(session, 1, None).await.unwrap();
    let mut f = std::fs::OpenOptions::new().append(true).open(path).unwrap();
    writeln!(f, "{}", serde_json::json!({"type":"queue-operation", "operation":"enqueue", "sessionId":session,"content":"follow up"})).unwrap();
    let queued = tokio::time::timeout(Duration::from_secs(3), async {
        loop {
            if let Event::Interaction(q) = events.recv().await.unwrap()
                && q.kind == "queued_message"
            {
                break q;
            }
        }
    })
    .await
    .unwrap();
    assert_eq!(queued.prompt, "follow up");
    assert!(
        queued.options.is_empty(),
        "no invented Claude queue controls"
    );
    assert!(service.pending(session).contains(&queued));
    writeln!(
        f,
        "{}",
        serde_json::json!({"type":"queue-operation", "operation":"dequeue", "sessionId":session})
    )
    .unwrap();
    tokio::time::timeout(Duration::from_secs(3), async {
        loop {
            if let Event::InteractionResolved { interaction, .. } = events.recv().await.unwrap()
                && interaction == queued.id
            {
                break;
            }
        }
    })
    .await
    .unwrap();
    assert!(service.pending(session).is_empty());
}

#[tokio::test]
async fn index_only_rename_pushes_without_waiting_for_periodic_rescan() {
    let dir = tempfile::tempdir().unwrap();
    let day = dir.path().join("sessions/2026/10/26");
    std::fs::create_dir_all(&day).unwrap();
    let id = "11111111-1111-4111-8111-111111111111";
    std::fs::write(
        day.join(format!("rollout-2026-10-26T12-00-00-{id}.jsonl")),
        format!("{{\"type\":\"session_meta\",\"payload\":{{\"id\":\"{id}\"}}}}\n"),
    )
    .unwrap();
    let index = dir.path().join("session_index.jsonl");
    let write_name = |name: &str| {
        std::fs::write(
            &index,
            format!("{{\"id\":\"{id}\",\"thread_name\":\"{name}\"}}\n"),
        )
        .unwrap()
    };
    write_name("Before");
    let service = AgentService::start(AgentConfig {
        codex_home: Some(dir.path().into()),
        claude_home: None,
        catdesk_home: None,
        poll: Duration::from_secs(60),
        rescan: Duration::from_secs(60),
        ..AgentConfig::default()
    });
    service.rescan().await;
    let mut events = service.subscribe();
    write_name("After");
    tokio::time::timeout(Duration::from_secs(5), async {
        loop {
            if matches!(events.recv().await.unwrap(), Event::ListChanged)
                && service.session(id).is_some_and(|s| s.title == "After")
            {
                break;
            }
        }
    })
    .await
    .unwrap();
}
