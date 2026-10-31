use muster_agent::{AgentConfig, AgentService, Event};
use serde_json::json;
use std::io::Write;
use std::path::Path;
use std::time::Duration;

const PARENT: &str = "11111111-1111-4111-8111-111111111111";
const CHILD: &str = "22222222-2222-4222-8222-222222222222";
const GRANDCHILD: &str = "33333333-3333-4333-8333-333333333333";
const FORK: &str = "44444444-4444-4444-8444-444444444444";

struct ForbiddenController;
#[async_trait::async_trait]
impl muster_agent::controller::Controller for ForbiddenController {
    fn kind(&self) -> muster_agent::model::ControllerKind {
        muster_agent::model::ControllerKind::NativeApp
    }
    fn session_id(&self) -> String {
        CHILD.into()
    }
    fn state(&self) -> muster_agent::model::SessionState {
        muster_agent::model::SessionState::Running
    }
    fn process_identity(&self) -> muster_agent::controller::ProcessIdentity {
        muster_agent::controller::ProcessIdentity { pid: 42, start: 10 }
    }
    fn process_state(&self) -> muster_agent::controller::ProcessState {
        muster_agent::controller::ProcessState::Running
    }
    fn pending(&self) -> Vec<muster_agent::PendingInteraction> {
        vec![muster_agent::PendingInteraction {
            id: "child-approval".into(),
            session: CHILD.into(),
            ..Default::default()
        }]
    }
    async fn send(&self, _: &str) -> Result<(), muster_agent::Error> {
        panic!("subagent send reached controller")
    }
    async fn respond(&self, _: &str, _: &str, _: &str) -> Result<(), muster_agent::Error> {
        panic!("subagent response reached controller")
    }
    async fn interrupt(&self) -> Result<(), muster_agent::Error> {
        panic!("subagent interrupt reached controller")
    }
    async fn stop(&self) {}
}

fn rollout(home: &Path, id: &str, metadata: serde_json::Value) {
    let dir = home.join("sessions");
    std::fs::create_dir_all(&dir).unwrap();
    let records = [
        json!({"type":"session_meta", "payload":metadata}),
        json!({"type":"turn_context", "payload":{}}),
        json!({"type":"response_item", "payload":{"type":"message", "role":"user", "content":[{"type":"input_text", "text":"inspect changes"}]}}),
    ];
    std::fs::write(
        dir.join(format!("rollout-{id}.jsonl")),
        records.iter().map(|r| format!("{r}\n")).collect::<String>(),
    )
    .unwrap();
}
fn config(codex: Option<&Path>, claude: Option<&Path>) -> AgentConfig {
    AgentConfig {
        codex_home: codex.map(Path::to_owned),
        claude_home: claude.map(Path::to_owned),
        claude_desktop: None,
        catdesk_home: None,
        deepseek_home: None,
        poll: Duration::from_secs(3600),
        rescan: Duration::from_secs(3600),
        ..Default::default()
    }
}

#[tokio::test]
async fn codex_subagents_have_bounded_direct_relationships_and_read_only_history() {
    let dir = tempfile::tempdir().unwrap();
    rollout(dir.path(), PARENT, json!({"id":PARENT,"source":"desktop"}));
    rollout(
        dir.path(),
        CHILD,
        json!({"id":CHILD,"source":{"subagent":{"thread_spawn":{"parent_thread_id":PARENT}}}}),
    );
    rollout(
        dir.path(),
        GRANDCHILD,
        json!({"id":GRANDCHILD,"parentThreadId":CHILD}),
    );
    rollout(
        dir.path(),
        FORK,
        json!({"id":FORK,"source":"desktop","forked_from_id":PARENT}),
    );
    let service = AgentService::start(config(Some(dir.path()), None));
    service.rescan().await;
    let listed = service.list(None, 0);
    assert_eq!(listed.len(), 2);
    assert!(
        listed
            .iter()
            .all(|s| s.subagents.is_empty() && !s.read_only)
    );
    let parent = service.session(PARENT).unwrap();
    assert_eq!(parent.subagents.len(), 1);
    assert_eq!(parent.subagents[0].id, CHILD);
    assert!(
        parent.subagents[0].subagents.is_empty(),
        "wire summaries must not recursively expand"
    );
    service.attach(std::sync::Arc::new(ForbiddenController));
    assert_eq!(service.session(CHILD).unwrap().controller, "none");
    let mut events = service.subscribe();
    service
        .controller_events()
        .send((
            CHILD.into(),
            muster_agent::controller::ControllerEvent::Interaction(
                muster_agent::PendingInteraction {
                    id: "child-approval".into(),
                    session: CHILD.into(),
                    ..Default::default()
                },
            ),
        ))
        .await
        .unwrap();
    tokio::time::timeout(Duration::from_secs(2), async {
        loop { if matches!(events.recv().await.unwrap(), Event::Interaction(i) if i.id == "child-approval") { break; } }
    }).await.unwrap();

    assert!(
        service
            .respond("child-approval", "approve", "")
            .await
            .unwrap_err()
            .to_string()
            .contains("read-only")
    );
    assert!(
        service
            .respond_in_session(CHILD, "child-approval", "approve", "")
            .await
            .unwrap_err()
            .to_string()
            .contains("read-only")
    );
    let child = service.session(CHILD).unwrap();
    assert!(child.read_only);
    assert_eq!(child.parent_session_id, PARENT);
    assert_eq!(child.subagents[0].id, GRANDCHILD);
    assert_eq!(
        service.messages(CHILD, 20, None).await.unwrap().0[0].text,
        "inspect changes"
    );
    for id in [CHILD, GRANDCHILD] {
        assert!(
            service
                .require_desktop_input(id)
                .unwrap_err()
                .to_string()
                .contains("read-only")
        );
        assert!(
            service
                .send(id, "blocked")
                .await
                .unwrap_err()
                .to_string()
                .contains("read-only")
        );
        assert!(
            service
                .interrupt(id)
                .await
                .unwrap_err()
                .to_string()
                .contains("read-only")
        );
    }
    // A cyclic or self-referencing provider record cannot recurse in projection.
    rollout(
        dir.path(),
        PARENT,
        json!({"id":PARENT,"parent_thread_id":GRANDCHILD}),
    );
    service.rescan().await;
    assert_eq!(service.session(GRANDCHILD).unwrap().subagents[0].id, PARENT);
}

#[tokio::test]
async fn claude_subagents_use_parent_scoped_ids_and_preserve_messages_paging_and_append() {
    let dir = tempfile::tempdir().unwrap();
    let project = dir.path().join("projects/test");
    for parent in [PARENT, FORK] {
        let agents = project.join(parent).join("subagents");
        std::fs::create_dir_all(&agents).unwrap();
        std::fs::write(
            project.join(format!("{parent}.jsonl")),
            "{\"type\":\"user\",\"message\":{\"role\":\"user\",\"content\":\"parent\"}}\n",
        )
        .unwrap();
        let mut f = std::fs::File::create(agents.join("agent-same.jsonl")).unwrap();
        for (role, text) in [("user", "child task"), ("assistant", "child answer")] {
            writeln!(f,"{}",json!({"type":role,"isSidechain":true,"sessionId":parent,"agentId":"same","message":{"role":role,"content":text}})).unwrap();
        }
        writeln!(f,"{}",json!({"type":"user","isSidechain":true,"isMeta":true,"message":{"role":"user","content":"hidden metadata"}})).unwrap();
    }
    let service = AgentService::start(config(None, Some(dir.path())));
    service.rescan().await;
    assert_eq!(service.list(None, 0).len(), 2);
    let a = service.session(PARENT).unwrap().subagents.remove(0);
    let b = service.session(FORK).unwrap().subagents.remove(0);
    assert_ne!(a.id, b.id);
    assert_eq!(a.parent_session_id, PARENT);
    assert!(a.read_only && b.read_only);
    let (tail, more) = service.messages(&a.id, 1, None).await.unwrap();
    assert_eq!(tail[0].text, "child answer");
    assert!(more);
    let (earlier, _) = service
        .messages(&a.id, 10, Some(&tail[0].id))
        .await
        .unwrap();
    assert_eq!(earlier[0].text, "child task");
    assert_eq!(earlier.len(), 1);
    let mut events = service.subscribe();
    let mut f = std::fs::OpenOptions::new()
        .append(true)
        .open(project.join(PARENT).join("subagents/agent-same.jsonl"))
        .unwrap();
    writeln!(f,"{}",json!({"type":"assistant","isSidechain":true,"message":{"role":"assistant","content":"later answer"}})).unwrap();
    let (messages, _) = service.messages(&a.id, 10, None).await.unwrap();
    assert_eq!(messages.last().unwrap().text, "later answer");
    assert!(std::iter::from_fn(||events.try_recv().ok()).any(|e|matches!(e,Event::Message{session,message} if session==a.id && message.text=="later answer")));
    assert!(service.require_desktop_input(&a.id).is_err());
    assert_eq!(
        service.messages(PARENT, 10, None).await.unwrap().0[0].text,
        "parent"
    );
}

#[tokio::test]
async fn encrypted_only_subagent_reports_unavailable_until_readable_content_arrives() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("sessions/2026/01/01");
    std::fs::create_dir_all(&path).unwrap();
    let file = path.join(format!("rollout-{CHILD}.jsonl"));
    std::fs::write(&file, format!("{}\n{}\n",
        json!({"type":"session_meta","payload":{"id":CHILD,"parent_thread_id":PARENT}}),
        json!({"type":"response_item","payload":{"type":"compaction","encrypted_content":"opaque"}})
    )).unwrap();
    let service = AgentService::start(config(Some(dir.path()), None));
    service.rescan().await;
    assert!(
        service
            .messages(CHILD, 10, None)
            .await
            .unwrap()
            .0
            .is_empty()
    );
    assert!(
        service
            .session(CHILD)
            .unwrap()
            .transcript_notice
            .contains("encrypted summary")
    );
    let mut f = std::fs::OpenOptions::new()
        .append(true)
        .open(&file)
        .unwrap();
    writeln!(f, "{}", json!({"type":"turn_context","payload":{}})).unwrap();
    writeln!(f, "{}", json!({"type":"response_item","payload":{"type":"message","role":"assistant","content":[{"type":"output_text","text":"readable result"}]}})).unwrap();
    assert!(
        service
            .messages(CHILD, 10, None)
            .await
            .unwrap()
            .0
            .iter()
            .any(|m| m.text == "readable result")
    );
    assert!(service.session(CHILD).unwrap().transcript_notice.is_empty());
}

fn turn(home: &Path, id: &str, event: &str) {
    let mut file = std::fs::OpenOptions::new()
        .append(true)
        .open(home.join(format!("sessions/rollout-{id}.jsonl")))
        .unwrap();
    writeln!(
        file,
        "{}",
        json!({"type":"event_msg","timestamp":"2026-09-29T01:00:00Z","payload":{"type":event}})
    )
    .unwrap();
}

fn observe(service: &AgentService, home: &Path, ids: &[&str]) {
    let process = muster_agent::controller::ProcessIdentity {
        pid: 42,
        start: muster_agent::model::parse_ts("2026-09-29T00:00:00Z") * 1_000_000,
    };
    service.observe_runtime_files(
        ids.iter()
            .map(|id| (process, home.join(format!("sessions/rollout-{id}.jsonl"))))
            .collect(),
    );
}

#[tokio::test]
async fn descendant_activity_tracks_verified_nested_work_without_changing_parent_authority() {
    let dir = tempfile::tempdir().unwrap();
    for (id, parent) in [
        (PARENT, None),
        (CHILD, Some(PARENT)),
        (GRANDCHILD, Some(CHILD)),
        (FORK, None),
    ] {
        rollout(
            dir.path(),
            id,
            json!({"id":id,"parent_thread_id":parent,"forked_from_id":PARENT}),
        );
        turn(
            dir.path(),
            id,
            if id == CHILD || id == FORK {
                "task_started"
            } else {
                "task_complete"
            },
        );
    }
    let service = AgentService::start(config(Some(dir.path()), None));
    service.rescan().await;
    assert!(
        service
            .session(PARENT)
            .unwrap()
            .descendant_activity
            .is_empty(),
        "transcripts alone cannot establish liveness"
    );
    observe(&service, dir.path(), &[PARENT, CHILD, GRANDCHILD, FORK]);
    let listed = service.list(None, 0);
    let parent = listed.iter().find(|s| s.id == PARENT).unwrap();
    assert_eq!(parent.state, "idle");
    assert_eq!(parent.controller, "none");
    assert_eq!(parent.descendant_activity, "running");
    assert!(parent.subagents.is_empty());
    assert_eq!(
        service.session(PARENT).unwrap().descendant_activity,
        "running"
    );
    assert!(service.require_desktop_input(CHILD).is_err());
    // The immediate child finishes while a grandchild continues.
    turn(dir.path(), CHILD, "task_complete");
    turn(dir.path(), GRANDCHILD, "task_started");
    observe(&service, dir.path(), &[PARENT, CHILD, GRANDCHILD, FORK]);
    assert_eq!(service.session(CHILD).unwrap().state, "idle");
    assert_eq!(
        service.session(PARENT).unwrap().descendant_activity,
        "running"
    );
    turn(dir.path(), GRANDCHILD, "task_complete");
    observe(&service, dir.path(), &[PARENT, CHILD, GRANDCHILD, FORK]);
    assert!(
        service
            .session(PARENT)
            .unwrap()
            .descendant_activity
            .is_empty(),
        "ordinary forks must not count as children"
    );
    turn(dir.path(), GRANDCHILD, "task_started");
    observe(&service, dir.path(), &[PARENT, CHILD, GRANDCHILD]);
    assert_eq!(
        service.session(PARENT).unwrap().descendant_activity,
        "running"
    );
    observe(&service, dir.path(), &[PARENT]);
    assert!(
        service
            .session(PARENT)
            .unwrap()
            .descendant_activity
            .is_empty(),
        "missing worker evidence clears descendant activity"
    );
    service.observe_runtime_files(vec![(
        muster_agent::controller::ProcessIdentity {
            pid: 42,
            start: muster_agent::model::parse_ts("2026-09-29T02:00:00Z") * 1_000_000,
        },
        dir.path()
            .join(format!("sessions/rollout-{GRANDCHILD}.jsonl")),
    )]);
    assert!(
        service
            .session(PARENT)
            .unwrap()
            .descendant_activity
            .is_empty(),
        "a replacement worker cannot revive an old turn"
    );
    // Active cycles terminate and do not attribute a session's own activity to itself.
    rollout(
        dir.path(),
        PARENT,
        json!({"id":PARENT,"parent_thread_id":GRANDCHILD}),
    );
    service.rescan().await;
    observe(&service, dir.path(), &[GRANDCHILD]);
    assert_eq!(
        service.session(PARENT).unwrap().descendant_activity,
        "running"
    );
    assert!(
        service
            .session(GRANDCHILD)
            .unwrap()
            .descendant_activity
            .is_empty()
    );
}

#[tokio::test]
async fn activity_includes_children_outside_the_bounded_summary() {
    let dir = tempfile::tempdir().unwrap();
    rollout(dir.path(), PARENT, json!({"id":PARENT}));
    // The first child is oldest and falls outside the newest 200 summaries.
    let ids: Vec<_> = (0..201)
        .map(|n| format!("55555555-5555-4555-8555-{n:012}"))
        .collect();
    for (i, id) in ids.iter().enumerate() {
        rollout(dir.path(), id, json!({"id":id,"parent_thread_id":PARENT}));
        turn(dir.path(), id, "task_started");
        let file = std::fs::File::options()
            .write(true)
            .open(dir.path().join(format!("sessions/rollout-{id}.jsonl")))
            .unwrap();
        file.set_times(
            std::fs::FileTimes::new().set_modified(
                std::time::UNIX_EPOCH + Duration::from_secs(1_790_000_000 + i as u64),
            ),
        )
        .unwrap();
    }
    let service = AgentService::start(config(Some(dir.path()), None));
    service.rescan().await;
    observe(&service, dir.path(), &[&ids[0]]);
    let parent = service.session(PARENT).unwrap();
    assert_eq!(parent.subagents.len(), 200);
    assert!(parent.subagents.iter().all(|s| s.id != ids[0]));
    assert_eq!(parent.descendant_activity, "running");
    assert_eq!(service.list(None, 0)[0].descendant_activity, "running");
}

#[tokio::test]
async fn claude_inline_child_uses_its_own_turn_and_the_verified_parent_worker() {
    use muster_agent::controller::{ProcessIdentity, ProcessState};
    use muster_agent::model::{SessionState, parse_ts};
    let dir = tempfile::tempdir().unwrap();
    let project = dir.path().join("projects/test");
    let children = project.join(PARENT).join("subagents");
    std::fs::create_dir_all(&children).unwrap();
    std::fs::write(
        project.join(format!("{PARENT}.jsonl")),
        "{\"type\":\"user\",\"message\":{\"role\":\"user\",\"content\":\"parent\"}}\n",
    )
    .unwrap();
    let path = children.join("agent-worker.jsonl");
    let write = |end: bool| {
        let mut file = std::fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open(&path)
            .unwrap();
        writeln!(file, "{}", json!({"type":"assistant","isSidechain":true,"timestamp":"2026-09-29T01:00:00Z","sessionId":PARENT,"agentId":"worker","message":{"role":"assistant","content":"child work","stop_reason":if end {"end_turn"} else {"tool_use"}}})).unwrap();
        // Metadata remains excluded even inside the child's scope.
        writeln!(file, "{}", json!({"type":"assistant","isSidechain":true,"isMeta":true,"timestamp":"2026-09-29T01:30:00Z","message":{"role":"assistant","content":"metadata"}})).unwrap();
    };
    write(false);
    let service = AgentService::start(config(None, Some(dir.path())));
    service.rescan().await;
    let observe = |id: &str, process_state, start| {
        service.observe_runtime(
            vec![],
            vec![(
                muster_agent::claude_runtime::Registration {
                    session: id.into(),
                    process: ProcessIdentity { pid: 42, start },
                    activity: SessionState::Idle,
                    status_unix: parse_ts("2026-09-29T01:30:00Z"),
                },
                process_state,
            )],
            None,
        );
    };
    let start = parse_ts("2026-09-29T00:00:00Z") * 1_000_000;
    assert!(
        service
            .session(PARENT)
            .unwrap()
            .descendant_activity
            .is_empty()
    );
    observe(FORK, ProcessState::Running, start);
    assert!(
        service
            .session(PARENT)
            .unwrap()
            .descendant_activity
            .is_empty(),
        "another worker cannot bind this child"
    );
    observe(PARENT, ProcessState::Running, start);
    let parent = service.session(PARENT).unwrap();
    assert_eq!(parent.state, "idle");
    assert_eq!(parent.descendant_activity, "running");
    assert_eq!(parent.subagents[0].state, "running");
    assert!(parent.subagents[0].read_only);
    assert_eq!(parent.subagents[0].controller, "none");
    write(true);
    observe(PARENT, ProcessState::Running, start);
    assert!(
        service
            .session(PARENT)
            .unwrap()
            .descendant_activity
            .is_empty()
    );
    write(false);
    // Real desktop subagents finish through SubagentHandback, not end_turn.
    // A tool name or failed result without the provider flag must not finish it.
    let handback = |ended: bool| {
        let mut file = std::fs::OpenOptions::new()
            .append(true)
            .open(&path)
            .unwrap();
        writeln!(file, "{}", json!({"type":"user","isSidechain":true,"toolEndsTurn":ended,"timestamp":"2026-09-29T01:00:01Z","message":{"role":"user","content":[{"type":"tool_result","tool_use_id":"handback","content":"returned"}]}})).unwrap();
        writeln!(file, "{}", json!({"type":"attachment","isSidechain":true,"timestamp":"2026-09-29T01:00:02Z","attachment":{"type":"hook_non_blocking_error"}})).unwrap();
    };
    handback(false);
    observe(PARENT, ProcessState::Running, start);
    assert_eq!(
        service.session(PARENT).unwrap().descendant_activity,
        "running"
    );
    handback(true);
    observe(PARENT, ProcessState::Running, start);
    let parent = service.session(PARENT).unwrap();
    assert!(parent.descendant_activity.is_empty());
    assert_eq!(parent.subagents[0].state, "idle");
    assert!(service.list(None, 0)[0].descendant_activity.is_empty());
    // A resumed child can become active again after handing back.
    write(false);
    observe(PARENT, ProcessState::Running, start);
    assert_eq!(
        service.session(PARENT).unwrap().descendant_activity,
        "running"
    );
    observe(
        PARENT,
        ProcessState::Running,
        parse_ts("2026-09-29T02:00:00Z") * 1_000_000,
    );
    assert!(
        service
            .session(PARENT)
            .unwrap()
            .descendant_activity
            .is_empty()
    );
    for process_state in [ProcessState::Unknown, ProcessState::Ended] {
        observe(PARENT, process_state, start);
        assert!(
            service
                .session(PARENT)
                .unwrap()
                .descendant_activity
                .is_empty()
        );
    }
    service.observe_runtime(vec![], vec![], None);
    assert!(
        service
            .session(PARENT)
            .unwrap()
            .descendant_activity
            .is_empty()
    );
}

#[tokio::test]
async fn claude_desktop_subsessions_resolve_scoped_ids_and_clear_descendant_activity() {
    use muster_agent::controller::{ProcessIdentity, ProcessState};
    use muster_agent::model::{SessionState, parse_ts};
    let dir = tempfile::tempdir().unwrap();
    let project = dir.path().join("projects/test");
    let registry = dir.path().join("desktop/org/user");
    std::fs::create_dir_all(&project).unwrap();
    std::fs::create_dir_all(&registry).unwrap();
    for (id, parent, fork) in [
        (PARENT, None, false),
        (CHILD, Some(PARENT), false),
        (GRANDCHILD, Some(CHILD), false),
        (FORK, Some(PARENT), true),
    ] {
        std::fs::write(project.join(format!("{id}.jsonl")),format!("{}\n",json!({"type":"assistant","timestamp":"2026-09-29T01:00:00Z","message":{"role":"assistant","content":"done","stop_reason":"end_turn"}}))).unwrap();
        let mut v = json!({"sessionId":format!("local_{id}"),"cliSessionId":id});
        if let Some(parent) = parent {
            v["spawnedFrom"] =
                json!({"sessionId":format!("local_{parent}"),"sideSession":true,"userFork":fork});
        }
        std::fs::write(registry.join(format!("{id}.json")), v.to_string()).unwrap();
    }
    let mut cfg = config(None, Some(dir.path()));
    cfg.claude_desktop = Some(dir.path().join("desktop"));
    let service = AgentService::start(cfg);
    service.rescan().await;
    let observe = |active: bool| {
        service.observe_runtime(
            vec![],
            [PARENT, CHILD, GRANDCHILD, FORK]
                .into_iter()
                .enumerate()
                .map(|(i, id)| {
                    (
                        muster_agent::claude_runtime::Registration {
                            session: id.into(),
                            process: ProcessIdentity {
                                pid: 42 + i as u32,
                                start: parse_ts("2026-09-29T00:00:00Z") * 1_000_000,
                            },
                            activity: if id == FORK || (id == GRANDCHILD && active) {
                                SessionState::Running
                            } else {
                                SessionState::Idle
                            },
                            status_unix: parse_ts("2026-09-29T01:01:00Z"),
                        },
                        ProcessState::Running,
                    )
                })
                .collect(),
            None,
        );
    };
    observe(true);
    let parent = service.session(PARENT).unwrap();
    assert_eq!(parent.state, "idle");
    assert_eq!(parent.descendant_activity, "running");
    assert_eq!(parent.subsessions.len(), 2);
    assert!(parent.subagents.is_empty());
    let child = service.session(CHILD).unwrap();
    assert!(!child.read_only);
    assert_eq!(child.relationship, "subsession");
    assert_eq!(child.parent_session_id, PARENT);
    assert_eq!(child.subsessions[0].id, GRANDCHILD);
    assert_eq!(service.session(FORK).unwrap().relationship, "fork");
    observe(false);
    assert!(
        service
            .session(PARENT)
            .unwrap()
            .descendant_activity
            .is_empty()
    );
    assert!(
        service
            .list(None, 0)
            .iter()
            .find(|s| s.id == PARENT)
            .unwrap()
            .descendant_activity
            .is_empty()
    );
    // A registry-only change must notify clients even without a transcript write.
    let mut events = service.subscribe();
    // A registry alias in another account cannot supply a missing parent.
    std::fs::remove_file(registry.join(format!("{PARENT}.json"))).unwrap();
    let other = dir.path().join("desktop/org/other");
    std::fs::create_dir_all(&other).unwrap();
    std::fs::write(
        other.join("parent.json"),
        json!({"sessionId":format!("local_{PARENT}"),"cliSessionId":PARENT}).to_string(),
    )
    .unwrap();
    service.rescan().await;
    assert!(service.session(CHILD).unwrap().parent_session_id.is_empty());
    assert!(matches!(events.try_recv(), Ok(Event::ListChanged)));
}
