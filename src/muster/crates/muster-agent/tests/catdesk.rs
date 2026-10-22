//! CatDesk sessions through the service: discovery from the app's store,
//! history, change push, and the running app's live status and projects.

use std::path::Path;
use std::time::{Duration, Instant};

use muster_agent::catdesk_app::{AppSession, Runtime};
use muster_agent::controller::{ProcessIdentity, ProcessState};
use muster_agent::{AgentConfig, AgentService, Event};
use rusqlite::Connection;

const SESSION: &str = "e6841c79-d515-4c39-aa56-3b70c404b093";

fn store(home: &Path) -> Connection {
    let conn = Connection::open(home.join(muster_agent::catdesk::DB)).unwrap();
    conn.execute_batch(
        "PRAGMA journal_mode=WAL;
         CREATE TABLE sessions (id TEXT PRIMARY KEY, extra TEXT NOT NULL);
         CREATE TABLE conversations (conversation_id TEXT PRIMARY KEY, project_id TEXT NOT NULL,
           messages TEXT NOT NULL, message_count INTEGER NOT NULL DEFAULT 0,
           last_message_time INTEGER, runtime TEXT NOT NULL DEFAULT 'legacy');
         CREATE TABLE ui_sdk_messages (conversation_id TEXT NOT NULL, seq INTEGER NOT NULL,
           message_id TEXT NOT NULL, role TEXT NOT NULL, payload TEXT NOT NULL,
           created_at_ms INTEGER, updated_at_ms INTEGER NOT NULL, PRIMARY KEY (conversation_id, seq));
         CREATE TABLE deleted_sessions (session_id TEXT PRIMARY KEY);",
    )
    .unwrap();
    let extra = serde_json::json!({"id": SESSION, "title": "接入指南", "conversationId": SESSION,
        "status": "success", "timestamp": 1_789_364_192_619_i64, "updatedAt": 1_789_364_314_301_i64});
    conn.execute(
        "INSERT INTO sessions VALUES (?1, ?2)",
        rusqlite::params![SESSION, extra.to_string()],
    )
    .unwrap();
    conn.execute(
        "INSERT INTO conversations (conversation_id, project_id, messages, runtime) VALUES (?1, 'p', '[]', 'ui-sdk')",
        [SESSION],
    )
    .unwrap();
    conn
}

fn row(conn: &Connection, seq: i64, role: &str, payload: serde_json::Value) {
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_millis() as i64;
    conn.execute(
        "INSERT INTO ui_sdk_messages VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?6)",
        rusqlite::params![
            SESSION,
            seq,
            format!("{role}:{seq}"),
            role,
            payload.to_string(),
            now
        ],
    )
    .unwrap();
}

#[tokio::test]
async fn lists_reads_and_follows_a_catdesk_session() {
    let dir = tempfile::tempdir().unwrap();
    let conn = store(dir.path());
    row(
        &conn,
        1,
        "user",
        serde_json::json!({"role":"user","content":"你好"}),
    );
    let service = AgentService::start(AgentConfig {
        codex_home: None,
        claude_home: None,
        catdesk_home: Some(dir.path().into()),
        claude_desktop: None,
        poll: Duration::from_millis(50),
        rescan: Duration::from_millis(200),
        ..Default::default()
    });
    let deadline = Instant::now() + Duration::from_secs(5);
    while service.session(SESSION).is_none() {
        assert!(
            Instant::now() < deadline,
            "the store's session was not listed"
        );
        tokio::time::sleep(Duration::from_millis(20)).await;
    }
    let listed = service.list(None, 0);
    assert_eq!(listed.len(), 1);
    assert_eq!(listed[0].provider, "catdesk");
    assert_eq!(listed[0].title, "接入指南");
    // No running app reported: nothing is known about the process.
    assert_eq!(listed[0].process_state, "unknown");
    assert!(service.files(SESSION, "").await.is_err(), "no project yet");

    let (messages, more) = service.messages(SESSION, 50, None).await.unwrap();
    assert!(!more);
    assert_eq!(messages.len(), 1);
    assert_eq!(messages[0].text, "你好");
    assert_eq!(messages[0].id, "user:1");

    let mut events = service.subscribe();
    row(
        &conn,
        2,
        "assistant",
        serde_json::json!({"role":"assistant","finished":true,"content":"你好！","extra":{"hasToolCalls":false}}),
    );
    let pushed = tokio::time::timeout(Duration::from_secs(5), async {
        loop {
            if let Ok(Event::Message { session, message }) = events.recv().await
                && session == SESSION
            {
                return message;
            }
        }
    })
    .await
    .expect("the new row was not pushed");
    assert_eq!(pushed.text, "你好！");
    assert_eq!(pushed.phase, "final_answer");

    // The running app reports live status and the project directory.
    let project = dir.path().join("project");
    std::fs::create_dir(&project).unwrap();
    std::fs::write(project.join("README.md"), "x").unwrap();
    service.observe_runtime(
        Vec::new(),
        Vec::new(),
        Some(Runtime {
            process: ProcessIdentity {
                pid: 3489,
                start: 1,
            },
            sessions: vec![AppSession {
                id: SESSION.into(),
                title: "接入指南".into(),
                running: true,
                project: Some(project.clone()),
                current: false,
            }],
        }),
    );
    let session = service.session(SESSION).unwrap();
    assert_eq!(session.process_state, ProcessState::Running.as_str());
    assert_eq!(session.state, "running");
    assert_eq!(session.cwd, project.to_string_lossy());
    let files = service.files(SESSION, "").await.unwrap();
    assert!(files.entries.iter().any(|e| e.name == "README.md"));
    // A rescan keeps the reported project.
    service.rescan().await;
    assert_eq!(
        service.session(SESSION).unwrap().cwd,
        project.to_string_lossy()
    );
}
