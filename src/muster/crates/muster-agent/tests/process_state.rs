use muster_agent::controller::{Controller, ControllerEvent, ProcessIdentity, ProcessState};
use muster_agent::model::{ControllerKind, PendingInteraction, SessionState};
use muster_agent::{AgentConfig, AgentService, Error, Event};
use std::sync::{Arc, Mutex};

const SESSION: &str = "01a0e659-2de9-7f63-b80b-3b53fc7754e0";
struct Observed {
    identity: ProcessIdentity,
    process: Mutex<ProcessState>,
    activity: Mutex<SessionState>,
}
#[async_trait::async_trait]
impl Controller for Observed {
    fn kind(&self) -> ControllerKind {
        ControllerKind::NativeApp
    }
    fn session_id(&self) -> String {
        SESSION.into()
    }
    fn state(&self) -> SessionState {
        *self.activity.lock().unwrap()
    }
    fn process_identity(&self) -> ProcessIdentity {
        self.identity
    }
    fn process_state(&self) -> ProcessState {
        *self.process.lock().unwrap()
    }
    fn pending(&self) -> Vec<PendingInteraction> {
        vec![]
    }
    async fn send(&self, _: &str) -> Result<(), Error> {
        unreachable!()
    }
    async fn respond(&self, _: &str, _: &str, _: &str) -> Result<(), Error> {
        unreachable!()
    }
    async fn interrupt(&self) -> Result<(), Error> {
        unreachable!()
    }
    async fn stop(&self) {}
}

#[test]
fn launch_generation_distinguishes_reuse_exit_and_unavailable_evidence() {
    let process = ProcessIdentity { pid: 42, start: 10 };
    assert_eq!(process.observe(Ok(Some(10))), ProcessState::Running);
    assert_eq!(process.observe(Ok(Some(11))), ProcessState::Ended);
    assert_eq!(process.observe(Ok(None)), ProcessState::Ended);
    assert_eq!(process.observe(Err(())), ProcessState::Unknown);
}

#[tokio::test]
async fn a_native_attach_never_replaces_a_controller_of_a_live_app() {
    let service = AgentService::start(AgentConfig {
        codex_home: None,
        claude_home: None,
        catdesk_home: None,
        claude_desktop: None,
        ..Default::default()
    });
    let observed = |start, process| {
        Arc::new(Observed {
            identity: ProcessIdentity { pid: 42, start },
            process: Mutex::new(process),
            activity: Mutex::new(SessionState::Idle),
        })
    };
    let first = observed(10, ProcessState::Running);
    assert!(service.attach_native(first.clone()));
    // A concurrent scan of the same live app keeps the operation's controller.
    let duplicate = observed(10, ProcessState::Running);
    assert!(!service.attach_native(duplicate.clone()));
    assert_eq!(
        Arc::strong_count(&duplicate),
        1,
        "the duplicate was dropped"
    );
    assert!(service.has_controller(SESSION));
    assert!(service.retains_native_controller(SESSION));
    *first.process.lock().unwrap() = ProcessState::Unknown;
    assert!(service.retains_native_controller(SESSION));
    assert!(!service.attach_native(duplicate));
    // Once that app process is gone, a controller of its successor attaches.
    *first.process.lock().unwrap() = ProcessState::Ended;
    assert!(!service.retains_native_controller(SESSION));
    let successor = observed(11, ProcessState::Running);
    assert!(service.attach_native(successor.clone()));
    assert!(service.retains_native_controller(SESSION));
    assert_eq!(
        Arc::strong_count(&first),
        1,
        "the ended controller was replaced"
    );
}

#[tokio::test]
async fn runtime_overrides_stale_transcript_and_late_exit_cannot_detach_new_launch() {
    let dir = tempfile::tempdir().unwrap();
    let day = dir.path().join("sessions/2026/09/28");
    std::fs::create_dir_all(&day).unwrap();
    std::fs::write(day.join(format!("rollout-2026-09-28T12-00-00-{SESSION}.jsonl")),
        format!("{{\"type\":\"session_meta\",\"payload\":{{\"id\":\"{SESSION}\"}}}}\n{{\"type\":\"event_msg\",\"payload\":{{\"type\":\"task_started\"}}}}\n")).unwrap();
    let service = AgentService::start(AgentConfig {
        codex_home: Some(dir.path().into()),
        claude_home: None,
        catdesk_home: None,
        claude_desktop: None,
        ..Default::default()
    });
    service.rescan().await;
    service.messages(SESSION, 10, None).await.unwrap();
    let state = service.session(SESSION).unwrap();
    assert_eq!(
        (state.process_state.as_str(), state.state.as_str()),
        ("unknown", "unknown")
    );
    let controller = Arc::new(Observed {
        identity: ProcessIdentity { pid: 42, start: 10 },
        process: Mutex::new(ProcessState::Running),
        activity: Mutex::new(SessionState::Idle),
    });
    service.attach(controller.clone());
    assert_eq!(service.session(SESSION).unwrap().process_state, "running");
    assert_eq!(service.session(SESSION).unwrap().state, "idle");
    // A missing AX surface or locked screen cannot terminate a live process.
    *controller.activity.lock().unwrap() = SessionState::Unknown;
    assert_eq!(service.session(SESSION).unwrap().process_state, "running");
    *controller.process.lock().unwrap() = ProcessState::Unknown;
    assert_eq!(service.session(SESSION).unwrap().process_state, "unknown");
    *controller.activity.lock().unwrap() = SessionState::Running;
    *controller.process.lock().unwrap() = ProcessState::Ended;
    assert_eq!(service.session(SESSION).unwrap().state, "ended");
    let mut events = service.subscribe();
    service
        .controller_events()
        .send((
            SESSION.into(),
            ControllerEvent::Detached {
                process: controller.identity,
            },
        ))
        .await
        .unwrap();
    let ended = tokio::time::timeout(std::time::Duration::from_secs(2), async {
        loop {
            if let Event::State(s) = events.recv().await.unwrap() {
                if s.controller == "none" {
                    break s;
                }
            }
        }
    })
    .await
    .unwrap();
    assert_eq!(
        (ended.state.as_str(), ended.process_state.as_str()),
        ("ended", "ended")
    );
    service.rescan().await;
    assert_eq!(service.session(SESSION).unwrap().state, "ended");
    let replacement = Arc::new(Observed {
        identity: ProcessIdentity { pid: 42, start: 11 },
        process: Mutex::new(ProcessState::Running),
        activity: Mutex::new(SessionState::Idle),
    });
    service.attach(replacement);
    let mut events = service.subscribe();
    let tx = service.controller_events();
    tx.send((
        SESSION.into(),
        ControllerEvent::Detached {
            process: controller.identity,
        },
    ))
    .await
    .unwrap();
    // This following event establishes that the stale exit was processed.
    tx.send((SESSION.into(), ControllerEvent::State(SessionState::Idle)))
        .await
        .unwrap();
    let Event::State(current) =
        tokio::time::timeout(std::time::Duration::from_secs(2), events.recv())
            .await
            .unwrap()
            .unwrap()
    else {
        panic!("expected state")
    };
    assert_eq!(
        (current.controller.as_str(), current.process_state.as_str()),
        ("native_app", "running")
    );
}

#[tokio::test]
async fn worker_file_evidence_tracks_background_turn_without_granting_control() {
    use std::io::Write;
    let dir = tempfile::tempdir().unwrap();
    let day = dir.path().join("sessions");
    std::fs::create_dir_all(&day).unwrap();
    let path = day.join(format!("rollout-{SESSION}.jsonl"));
    std::fs::write(&path, format!("{{\"type\":\"session_meta\",\"payload\":{{\"id\":\"{SESSION}\"}}}}\n{{\"type\":\"event_msg\",\"timestamp\":\"2026-09-29T01:00:00Z\",\"payload\":{{\"type\":\"task_started\"}}}}\n")).unwrap();
    let service = AgentService::start(AgentConfig {
        codex_home: Some(dir.path().into()),
        claude_home: None,
        catdesk_home: None,
        claude_desktop: None,
        ..Default::default()
    });
    service.rescan().await;
    let worker = ProcessIdentity {
        pid: 42,
        start: muster_agent::model::parse_ts("2026-09-29T00:00:00Z") * 1_000_000,
    };
    service.observe_runtime_files(vec![(worker, path.clone())]);
    let observed = service.session(SESSION).unwrap();
    assert_eq!(
        (
            observed.process_state.as_str(),
            observed.state.as_str(),
            observed.controller.as_str()
        ),
        ("running", "running", "none")
    );
    assert!(service.send(SESSION, "must not execute").await.is_err());
    // A live shared worker cannot revive an incomplete turn from an older launch.
    service.observe_runtime_files(vec![(
        ProcessIdentity {
            start: worker.start + 7_200_000_000,
            ..worker
        },
        path.clone(),
    )]);
    assert_eq!(service.session(SESSION).unwrap().state, "unknown");
    let mut file = std::fs::OpenOptions::new()
        .append(true)
        .open(&path)
        .unwrap();
    writeln!(file, "{{\"type\":\"event_msg\",\"timestamp\":\"2026-09-29T01:01:00Z\",\"payload\":{{\"type\":\"task_complete\"}}}}").unwrap();
    service.observe_runtime_files(vec![(worker, path)]);
    assert_eq!(service.session(SESSION).unwrap().state, "idle");
    // Loss of observation is not positive evidence that a process exited.
    service.observe_runtime_files(vec![]);
    assert_eq!(service.session(SESSION).unwrap().process_state, "unknown");
}

#[tokio::test]
async fn resumed_rollout_is_selected_by_message_time_not_directory_order() {
    let dir = tempfile::tempdir().unwrap();
    let day = dir.path().join("sessions");
    std::fs::create_dir_all(&day).unwrap();
    let old = day.join(format!("rollout-z-{SESSION}.jsonl"));
    let current = day.join(format!("rollout-a-{SESSION}.jsonl"));
    let record = |time: &str| {
        format!(
            "{{\"type\":\"session_meta\",\"payload\":{{\"id\":\"{SESSION}\"}}}}\n{{\"type\":\"response_item\",\"timestamp\":\"{time}\",\"payload\":{{\"type\":\"message\",\"role\":\"user\",\"content\":\"hello\"}}}}\n{{\"type\":\"event_msg\",\"timestamp\":\"{time}\",\"payload\":{{\"type\":\"task_started\"}}}}\n"
        )
    };
    std::fs::write(&current, record("2026-09-29T01:00:00Z")).unwrap();
    std::fs::write(&old, record("2026-09-28T01:00:00Z")).unwrap();
    let service = AgentService::start(AgentConfig {
        codex_home: Some(dir.path().into()),
        claude_home: None,
        catdesk_home: None,
        claude_desktop: None,
        ..Default::default()
    });
    service.rescan().await;
    service.observe_runtime_files(vec![(
        ProcessIdentity {
            pid: 42,
            start: muster_agent::model::parse_ts("2026-09-29T00:00:00Z") * 1_000_000,
        },
        current,
    )]);
    let observed = service.session(SESSION).unwrap();
    assert_eq!(
        (observed.process_state.as_str(), observed.state.as_str()),
        ("running", "running")
    );
}

#[tokio::test]
async fn claude_reply_end_does_not_end_a_busy_worker_or_its_stop_hooks() {
    use std::io::Write;
    let dir = tempfile::tempdir().unwrap();
    let project = dir.path().join("projects/test");
    std::fs::create_dir_all(&project).unwrap();
    let path = project.join(format!("{SESSION}.jsonl"));
    std::fs::write(&path, "{\"type\":\"user\",\"timestamp\":\"2026-09-29T01:00:00Z\",\"message\":{\"role\":\"user\",\"content\":\"hello\"}}\n").unwrap();
    let service = AgentService::start(AgentConfig {
        codex_home: None,
        claude_home: Some(dir.path().into()),
        catdesk_home: None,
        claude_desktop: None,
        ..Default::default()
    });
    service.rescan().await;
    service.attach(Arc::new(Observed {
        identity: ProcessIdentity { pid: 7, start: 1 },
        process: Mutex::new(ProcessState::Running),
        activity: Mutex::new(SessionState::Running),
    }));
    assert_eq!(
        service.session(SESSION).unwrap().process_state,
        "unknown",
        "GUI process is not session evidence"
    );
    let registration = || muster_agent::claude_runtime::Registration {
        session: SESSION.into(),
        process: ProcessIdentity {
            pid: 42,
            start: muster_agent::model::parse_ts("2026-09-29T00:00:00Z") * 1_000_000,
        },
        activity: SessionState::Running,
        status_unix: muster_agent::model::parse_ts("2026-09-29T01:00:00Z"),
    };
    service.observe_runtime(vec![], vec![(registration(), ProcessState::Running)], None);
    assert_eq!(service.session(SESSION).unwrap().state, "running");
    let mut f = std::fs::OpenOptions::new().append(true).open(path).unwrap();
    writeln!(f,"{{\"type\":\"assistant\",\"timestamp\":\"2026-09-29T02:00:00Z\",\"message\":{{\"role\":\"assistant\",\"content\":\"done\",\"stop_reason\":\"end_turn\"}}}}").unwrap();
    service.observe_runtime(vec![], vec![(registration(), ProcessState::Running)], None);
    assert_eq!(
        service.session(SESSION).unwrap().state,
        "running",
        "reply end_turn must not end active Stop hooks"
    );
    let mut waiting = registration();
    waiting.activity = SessionState::WaitingApproval;
    service.observe_runtime(vec![], vec![(waiting, ProcessState::Running)], None);
    assert_eq!(service.session(SESSION).unwrap().state, "waiting_approval");
    let mut idle = registration();
    idle.activity = SessionState::Idle;
    idle.status_unix = muster_agent::model::parse_ts("2026-09-29T02:01:00Z");
    service.observe_runtime(vec![], vec![(idle, ProcessState::Running)], None);
    assert_eq!(service.session(SESSION).unwrap().state, "idle");
    let mut unknown = registration();
    unknown.activity = SessionState::Unknown;
    service.observe_runtime(vec![], vec![(unknown, ProcessState::Running)], None);
    assert_eq!(
        service.session(SESSION).unwrap().state,
        "idle",
        "without worker activity, the transcript remains the fallback"
    );
    service.observe_runtime(vec![], vec![(registration(), ProcessState::Ended)], None);
    assert_eq!(service.session(SESSION).unwrap().state, "ended");
}
