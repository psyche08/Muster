//! Question drafts survive rejection; acknowledgement owns the card lifetime.
use muster_app_core::{AppConfig, AppCore, Mode, Shell, app};
use muster_proto::{Message, v1};
use muster_rpc::{Context, EventSink, Handler, HandlerFuture, Principal};
use std::{
    sync::{
        Arc, Mutex,
        atomic::{AtomicUsize, Ordering},
    },
    time::Duration,
};
use tokio::sync::Notify;

#[derive(Default)]
struct View {
    latest: Mutex<app::ViewModel>,
    changed: Notify,
}
impl Shell for View {
    fn on_view(&self, b: &[u8]) {
        *self.latest.lock().unwrap() = app::ViewModel::decode(b).unwrap();
        self.changed.notify_one();
    }
    fn on_effect(&self, _: u64, _: &[u8]) {}
    fn hw_public_key(&self) -> Option<Vec<u8>> {
        None
    }
    fn hw_sign(&self, _: &[u8]) -> Option<Vec<u8>> {
        None
    }
}
impl View {
    async fn wait(&self, test: impl Fn(&app::ChatView) -> bool) -> app::ChatView {
        tokio::time::timeout(Duration::from_secs(5), async {
            loop {
                if let Some(chat) = self.latest.lock().unwrap().chat.clone() {
                    if test(&chat) {
                        return chat;
                    }
                }
                self.changed.notified().await;
            }
        })
        .await
        .unwrap()
    }
}
fn question() -> v1::PendingInteraction {
    v1::PendingInteraction {
        id: "question:call".into(),
        session: "session".into(),
        kind: "question".into(),
        questions: vec![v1::InteractionQuestion {
            id: "q".into(),
            prompt: "Pick a mode".into(),
            allow_text: true,
            options: vec![v1::QuestionOption {
                id: "0".into(),
                label: "Read only".into(),
                ..Default::default()
            }],
            ..Default::default()
        }],
        ..Default::default()
    }
}
#[derive(Default)]
struct Server {
    send_test: bool,
    unknown: bool,
    submissions: AtomicUsize,
    release: Notify,
    events: Mutex<Option<EventSink>>,
    multi: bool,
    manual_count: usize,
    rpc_error: Option<v1::error::Code>,
}
impl Handler for Server {
    fn call(&self, _: Context, r: v1::request::Req, events: EventSink) -> HandlerFuture<'_> {
        use v1::{request::Req, response::Resp};
        Box::pin(async move {
            match r {
                Req::DeviceInfo(_) => Resp::DeviceInfo(v1::DeviceInfo {
                    device_id: "local".into(),
                    services: vec!["agent.questions.v1".into()],
                    ..Default::default()
                }),
                Req::NetworkStatus(_) => Resp::NetworkStatus(v1::NetworkStatus {
                    has_network: true,
                    ..Default::default()
                }),
                Req::PolicyGet(_) => Resp::Policy(v1::Policy::default()),
                Req::NetworkPending(_) => Resp::PendingJoins(v1::PendingJoins::default()),
                Req::AgentMessages(_) => Resp::AgentMessages(v1::AgentMessagePage::default()),
                Req::AgentWatch(r) if self.send_test => {
                    events
                        .send(v1::event::Ev::AgentEvent(v1::AgentEvent {
                            ev: Some(v1::agent_event::Ev::State(v1::AgentSession {
                                id: r.session,
                                provider: "codex".into(),
                                ..Default::default()
                            })),
                        }))
                        .await
                        .unwrap();
                    events.cancelled().cancelled().await;
                    Resp::Ack(v1::Ack {})
                }
                Req::AgentSend(r) if self.send_test => {
                    self.submissions.fetch_add(1, Ordering::SeqCst);
                    self.release.notified().await;
                    Resp::OpResult(v1::OpResult {
                        status: v1::op_result::Status::Applied as i32,
                        result_id: r.session,
                        ..Default::default()
                    })
                }
                Req::AgentWatch(_) => {
                    for ev in [
                        v1::agent_event::Ev::State(v1::AgentSession {
                            id: "session".into(),
                            state: "waiting_approval".into(),
                            ..Default::default()
                        }),
                        v1::agent_event::Ev::Interaction(self.question()),
                        v1::agent_event::Ev::State(v1::AgentSession {
                            id: "session".into(),
                            state: "unknown".into(),
                            ..Default::default()
                        }),
                    ] {
                        events
                            .send(v1::event::Ev::AgentEvent(v1::AgentEvent { ev: Some(ev) }))
                            .await
                            .unwrap();
                    }
                    *self.events.lock().unwrap() = Some(events.clone());
                    events.cancelled().cancelled().await;
                    Resp::Ack(v1::Ack {})
                }
                Req::DeviceWatch(_) => {
                    events.cancelled().cancelled().await;
                    Resp::Ack(v1::Ack {})
                }
                Req::AgentRespond(r) => {
                    if self.manual_count > 0 {
                        assert_eq!(r.answers.len(), self.manual_count);
                        assert!(r.answers.iter().all(|a| a.option_ids == ["0"]));
                        self.submissions.fetch_add(1, Ordering::SeqCst);
                        return Resp::OpResult(v1::OpResult {
                            status: v1::op_result::Status::Applied as i32,
                            result_id: r.interaction,
                            message: format!(
                                "已执行 {} 道题的答题操作，请在原会话确认结果。",
                                self.manual_count
                            ),
                            ..Default::default()
                        });
                    }
                    assert_eq!(r.answers[0].text, "custom answer");
                    if self.multi {
                        assert_eq!(r.answers.len(), 2);
                        assert_eq!(r.answers[1].option_ids, ["0"]);
                    }
                    let attempt = self.submissions.fetch_add(1, Ordering::SeqCst);
                    self.release.notified().await;
                    if let Some(code) = self.rpc_error {
                        return Resp::Error(v1::Error {
                            code: code as i32,
                            message: "RPC result after provider answer".into(),
                        });
                    }
                    Resp::OpResult(v1::OpResult {
                        status: if self.unknown {
                            v1::op_result::Status::Unknown
                        } else if attempt == 0 {
                            v1::op_result::Status::Rejected
                        } else {
                            v1::op_result::Status::Applied
                        } as i32,
                        result_id: r.interaction,
                        message: if attempt == 0 {
                            "Native form temporarily unavailable".into()
                        } else {
                            String::new()
                        },
                        ..Default::default()
                    })
                }
                _ => Resp::Ack(v1::Ack {}),
            }
        })
    }
}
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn preserves_question_on_rejection_and_deduplicates_pending_submission() {
    run(false).await;
}
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn unknown_outcome_keeps_question_and_prevents_resubmission() {
    run(true).await;
}
async fn run(unknown: bool) {
    let dir = tempfile::tempdir().unwrap();
    let socket = dir.path().join("rpc.sock");
    let listener = tokio::net::UnixListener::bind(&socket).unwrap();
    let server = Arc::new(Server {
        unknown,
        ..Default::default()
    });
    let h = server.clone();
    let task = tokio::spawn(async move {
        let (stream, _) = listener.accept().await.unwrap();
        muster_rpc::serve(stream, Principal::Local, h).await;
    });
    let shell = Arc::new(View::default());
    let core = AppCore::start(
        AppConfig {
            dir: dir.path().into(),
            mode: Mode::Socket { path: socket },
            allow_loopback: true,
        },
        shell.clone(),
    )
    .await
    .unwrap();
    // Wait for the device identity before routing the local operation.
    tokio::time::timeout(Duration::from_secs(3), async {
        while shell.latest.lock().unwrap().self_.is_none() {
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
    })
    .await
    .unwrap();
    core.intent(app::Intent {
        it: Some(app::intent::It::OpenAgent(app::OpenAgent {
            session: "session".into(),
            device_id: String::new(),
        })),
    });
    shell
        .wait(|c| c.interactions.len() == 1 && !c.loading)
        .await;
    let respond = |answers| app::Intent {
        it: Some(app::intent::It::AgentRespond(app::AgentRespond {
            interaction: "question:call".into(),
            answers,
            ..Default::default()
        })),
    };
    core.intent(respond(vec![]));
    assert_eq!(
        shell
            .wait(|c| !c.response_error.is_empty())
            .await
            .interactions
            .len(),
        1
    );
    assert_eq!(server.submissions.load(Ordering::SeqCst), 0);
    let answers = vec![v1::QuestionAnswer {
        question_id: "q".into(),
        text: "custom answer".into(),
        ..Default::default()
    }];
    core.intent(respond(answers.clone()));
    assert_eq!(
        shell
            .wait(|c| !c.responding_interaction.is_empty())
            .await
            .interactions
            .len(),
        1
    );
    core.intent(respond(answers.clone()));
    tokio::time::sleep(Duration::from_millis(100)).await;
    assert_eq!(server.submissions.load(Ordering::SeqCst), 1);
    server.release.notify_one();
    let rejected = shell
        .wait(|c| c.responding_interaction.is_empty() && !c.response_error.is_empty())
        .await;
    assert_eq!(rejected.interactions.len(), 1);
    core.intent(respond(answers));
    if unknown {
        assert!(rejected.response_unknown);
        tokio::time::sleep(Duration::from_millis(100)).await;
        assert_eq!(server.submissions.load(Ordering::SeqCst), 1);
        core.shutdown();
        task.abort();
        return;
    }
    shell.wait(|c| !c.responding_interaction.is_empty()).await;
    server.release.notify_one();
    shell
        .wait(|c| c.interactions.is_empty() && c.response_error.is_empty())
        .await;
    assert_eq!(server.submissions.load(Ordering::SeqCst), 2);
    core.shutdown();
    task.abort();
}

impl Server {
    fn question(&self) -> v1::PendingInteraction {
        let mut q = question();
        if self.manual_count > 0 {
            q.questions = (0..self.manual_count)
                .map(|i| {
                    let mut item = q.questions[0].clone();
                    item.id = format!("q{i}");
                    item.prompt = format!("Question {i}");
                    item
                })
                .collect();
        }
        if self.multi {
            let mut second = q.questions[0].clone();
            second.id = "q2".into();
            second.prompt = "Second question".into();
            q.questions.push(second);
        }
        q
    }
    async fn emit(&self, ev: v1::agent_event::Ev) {
        let sink = self.events.lock().unwrap().clone().unwrap();
        sink.send(v1::event::Ev::AgentEvent(v1::AgentEvent { ev: Some(ev) }))
            .await
            .unwrap();
    }
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn provider_acknowledgement_wins_in_both_reply_orders() {
    for (unknown, rpc_error) in [
        (false, None),
        (true, None),
        (false, Some(v1::error::Code::Timeout)),
        (false, Some(v1::error::Code::Unavailable)),
    ] {
        for ack_first in [false, true] {
            let dir = tempfile::tempdir().unwrap();
            let socket = dir.path().join("rpc.sock");
            let listener = tokio::net::UnixListener::bind(&socket).unwrap();
            let server = Arc::new(Server {
                unknown,
                multi: true,
                rpc_error,
                ..Default::default()
            });
            let h = server.clone();
            let task = tokio::spawn(async move {
                let (stream, _) = listener.accept().await.unwrap();
                muster_rpc::serve(stream, Principal::Local, h).await;
            });
            let shell = Arc::new(View::default());
            let core = AppCore::start(
                AppConfig {
                    dir: dir.path().into(),
                    mode: Mode::Socket { path: socket },
                    allow_loopback: true,
                },
                shell.clone(),
            )
            .await
            .unwrap();
            tokio::time::timeout(Duration::from_secs(3), async {
                while shell.latest.lock().unwrap().self_.is_none() {
                    tokio::time::sleep(Duration::from_millis(10)).await;
                }
            })
            .await
            .unwrap();
            core.intent(app::Intent {
                it: Some(app::intent::It::OpenAgent(app::OpenAgent {
                    session: "session".into(),
                    device_id: String::new(),
                })),
            });
            shell
                .wait(|c| c.interactions.len() == 1 && !c.loading)
                .await;
            core.intent(app::Intent {
                it: Some(app::intent::It::AgentRespond(app::AgentRespond {
                    interaction: "question:call".into(),
                    answers: vec![
                        v1::QuestionAnswer {
                            question_id: "q".into(),
                            text: "custom answer".into(),
                            ..Default::default()
                        },
                        v1::QuestionAnswer {
                            question_id: "q2".into(),
                            option_ids: vec!["0".into()],
                            ..Default::default()
                        },
                    ],
                    ..Default::default()
                })),
            });
            shell.wait(|c| !c.responding_interaction.is_empty()).await;
            let mut partial = server.question();
            partial.questions.remove(0);
            server.emit(v1::agent_event::Ev::Interaction(partial)).await;
            shell.wait(|c| c.interactions[0].questions.len() == 1).await;
            if !ack_first {
                server.release.notify_one();
                shell.wait(|c| !c.response_error.is_empty()).await;
                server
                    .emit(v1::agent_event::Ev::InteractionResolved("unrelated".into()))
                    .await;
                server
                    .emit(v1::agent_event::Ev::State(v1::AgentSession {
                        id: "session".into(),
                        state: "running".into(),
                        ..Default::default()
                    }))
                    .await;
                assert!(
                    !shell
                        .wait(|c| c.session.as_ref().is_some_and(|s| s.state == "running"))
                        .await
                        .response_error
                        .is_empty()
                );
            }
            server
                .emit(v1::agent_event::Ev::InteractionResolved(
                    "question:call".into(),
                ))
                .await;
            shell
                .wait(|c| {
                    c.interactions.is_empty()
                        && c.responding_interaction.is_empty()
                        && c.response_error.is_empty()
                        && !c.response_unknown
                })
                .await;
            if ack_first {
                server.release.notify_one();
            }
            tokio::time::sleep(Duration::from_millis(100)).await;
            let view = shell.latest.lock().unwrap().clone();
            let chat = view.chat.unwrap();
            assert!(chat.response_error.is_empty());
            assert!(!chat.response_unknown);
            assert!(
                view.toasts.iter().all(|t| !t.error),
                "late answer error must not create a toast"
            );
            assert_eq!(server.submissions.load(Ordering::SeqCst), 1);
            core.shutdown();
            task.abort();
        }
    }
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn independent_send_receipts_do_not_clear_another_sessions_pending_draft() {
    let dir = tempfile::tempdir().unwrap();
    let socket = dir.path().join("rpc.sock");
    let listener = tokio::net::UnixListener::bind(&socket).unwrap();
    let server = Arc::new(Server {
        send_test: true,
        ..Default::default()
    });
    let h = server.clone();
    let task = tokio::spawn(async move {
        let (stream, _) = listener.accept().await.unwrap();
        muster_rpc::serve(stream, Principal::Local, h).await;
    });
    let shell = Arc::new(View::default());
    let core = AppCore::start(
        AppConfig {
            dir: dir.path().into(),
            mode: Mode::Socket { path: socket },
            allow_loopback: true,
        },
        shell.clone(),
    )
    .await
    .unwrap();
    tokio::time::timeout(Duration::from_secs(3), async {
        while shell.latest.lock().unwrap().self_.is_none() {
            tokio::task::yield_now().await;
        }
    })
    .await
    .unwrap();
    for (index, session) in ["first", "second"].into_iter().enumerate() {
        core.intent(app::Intent {
            it: Some(app::intent::It::OpenAgent(app::OpenAgent {
                session: session.into(),
                device_id: String::new(),
            })),
        });
        shell
            .wait(|c| !c.loading && c.session.as_ref().is_some_and(|s| s.id == session))
            .await;
        core.intent(app::Intent {
            it: Some(app::intent::It::AgentSend(app::AgentSend {
                text: session.into(),
                draft_id: format!("draft-{session}"),
                ..Default::default()
            })),
        });
        shell.wait(|c| c.sending).await;
        tokio::time::timeout(Duration::from_secs(2), async {
            while server.submissions.load(Ordering::SeqCst) < index + 1 {
                tokio::task::yield_now().await;
            }
        })
        .await
        .unwrap();
    }
    server.release.notify_one();
    tokio::time::sleep(Duration::from_millis(200)).await;
    let current = shell.latest.lock().unwrap().chat.clone().unwrap();
    assert!(
        current.sending,
        "first session's receipt cannot end second session's submission"
    );
    assert!(current.sent_draft_id.is_empty());
    server.release.notify_one();
    shell
        .wait(|c| !c.sending && c.sent_draft_id == "draft-second")
        .await;
    assert_eq!(server.submissions.load(Ordering::SeqCst), 2);
    core.shutdown();
    task.abort();
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn composer_navigation_persists_before_switch_and_does_not_resurrect_sent_text() {
    use app::intent::It;
    let dir = tempfile::tempdir().unwrap();
    let socket = dir.path().join("rpc.sock");
    let listener = tokio::net::UnixListener::bind(&socket).unwrap();
    let server = Arc::new(Server {
        send_test: true,
        ..Default::default()
    });
    let handler = server.clone();
    let task = tokio::spawn(async move {
        let (stream, _) = listener.accept().await.unwrap();
        muster_rpc::serve(stream, Principal::Local, handler).await;
    });
    let shell = Arc::new(View::default());
    let core = AppCore::start(
        AppConfig {
            dir: dir.path().into(),
            mode: Mode::Socket { path: socket },
            allow_loopback: true,
        },
        shell.clone(),
    )
    .await
    .unwrap();
    tokio::time::timeout(Duration::from_secs(3), async {
        while shell.latest.lock().unwrap().self_.is_none() {
            tokio::task::yield_now().await;
        }
    })
    .await
    .unwrap();
    let open = |id: &str| {
        It::OpenAgent(app::OpenAgent {
            session: id.into(),
            device_id: String::new(),
        })
    };
    let snapshot = |draft: app::ChatDraft, action: It| app::Intent {
        it: Some(It::ComposerAction(Box::new(app::ComposerAction {
            draft: Some(draft),
            action: Some(Box::new(app::Intent { it: Some(action) })),
        }))),
    };
    let loaded = |chat: &app::ChatView, id: &str| {
        !chat.loading && chat.session.as_ref().is_some_and(|s| s.id == id)
    };
    core.intent(app::Intent {
        it: Some(open("first")),
    });
    let first = shell.wait(|c| loaded(c, "first")).await;
    let first_draft = app::ChatDraft {
        text: "unsent first".into(),
        revision: "first-edit".into(),
        ..first.draft.unwrap()
    };
    let cache = dir.path().join("view-cache-v1");
    let first_path = cache.join(&first_draft.key);
    assert!(
        !first_path.exists(),
        "opening/typing does not persist a draft"
    );
    core.intent(snapshot(first_draft.clone(), open("second")));
    let second = shell.wait(|c| loaded(c, "second")).await;
    let disk = || app::ChatDraft::decode(std::fs::read(&first_path).unwrap().as_slice()).unwrap();
    assert_eq!(
        disk(),
        first_draft,
        "old composer is saved before navigating"
    );
    let second_draft = app::ChatDraft {
        text: "unsent second".into(),
        revision: "second-edit".into(),
        ..second.draft.unwrap()
    };
    core.intent(snapshot(second_draft.clone(), open("first")));
    let restored = shell.wait(|c| loaded(c, "first")).await;
    assert_eq!(restored.draft, Some(first_draft.clone()));

    let submitted = app::ChatDraft {
        text: "edited and sent".into(),
        revision: "submitted".into(),
        ..first_draft
    };
    core.intent(snapshot(
        submitted.clone(),
        It::AgentSend(app::AgentSend {
            text: submitted.text.clone(),
            draft_id: "send-1".into(),
            ..Default::default()
        }),
    ));
    shell.wait(|c| c.sending).await;
    assert_eq!(
        disk().text,
        "unsent first",
        "submission captures in memory, without a disk write"
    );
    core.intent(snapshot(submitted.clone(), open("second")));
    shell.wait(|c| loaded(c, "second")).await;
    assert_eq!(disk(), submitted);
    server.release.notify_one();
    tokio::time::timeout(Duration::from_secs(3), async {
        while !disk().text.is_empty() {
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
    })
    .await
    .unwrap();
    core.intent(snapshot(second_draft.clone(), open("first")));
    assert!(
        shell
            .wait(|c| loaded(c, "first"))
            .await
            .draft
            .unwrap()
            .text
            .is_empty()
    );
    // A UI snapshot queued before the receipt must not bring sent text back.
    core.intent(snapshot(submitted.clone(), open("second")));
    shell.wait(|c| loaded(c, "second")).await;
    assert!(disk().text.is_empty());
    core.intent(snapshot(second_draft, open("first")));
    shell.wait(|c| loaded(c, "first")).await;

    // An unsuccessful save cannot navigate away and discard the native buffer.
    std::fs::rename(&cache, dir.path().join("cache-backup")).unwrap();
    std::fs::write(&cache, b"not a directory").unwrap();
    let unsaved = app::ChatDraft {
        text: "keep this input".into(),
        revision: "failed-write".into(),
        ..submitted
    };
    core.intent(snapshot(unsaved.clone(), open("second")));
    let kept = shell.wait(|c| c.draft.as_ref() == Some(&unsaved)).await;
    assert_eq!(kept.session.unwrap().id, "first");
    assert!(
        shell
            .latest
            .lock()
            .unwrap()
            .toasts
            .iter()
            .any(|t| t.text.contains("Draft could not be saved"))
    );
    core.shutdown();
    task.abort();
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn one_or_many_answers_return_manual_confirmation_without_provider_receipt() {
    for count in [1, 3, 16] {
        let dir = tempfile::tempdir().unwrap();
        let socket = dir.path().join("rpc.sock");
        let listener = tokio::net::UnixListener::bind(&socket).unwrap();
        let server = Arc::new(Server {
            manual_count: count,
            ..Default::default()
        });
        let h = server.clone();
        let task = tokio::spawn(async move {
            let (stream, _) = listener.accept().await.unwrap();
            muster_rpc::serve(stream, Principal::Local, h).await;
        });
        let shell = Arc::new(View::default());
        let core = AppCore::start(
            AppConfig {
                dir: dir.path().into(),
                mode: Mode::Socket { path: socket },
                allow_loopback: true,
            },
            shell.clone(),
        )
        .await
        .unwrap();
        tokio::time::timeout(Duration::from_secs(3), async {
            while shell.latest.lock().unwrap().self_.is_none() {
                tokio::time::sleep(Duration::from_millis(10)).await;
            }
        })
        .await
        .unwrap();
        core.intent(app::Intent {
            it: Some(app::intent::It::OpenAgent(app::OpenAgent {
                session: "session".into(),
                device_id: String::new(),
            })),
        });
        shell
            .wait(|c| c.interactions.len() == 1 && !c.loading)
            .await;
        core.intent(app::Intent {
            it: Some(app::intent::It::AgentRespond(app::AgentRespond {
                interaction: "question:call".into(),
                answers: (0..count)
                    .map(|i| v1::QuestionAnswer {
                        question_id: format!("q{i}"),
                        option_ids: vec!["0".into()],
                        text: String::new(),
                    })
                    .collect(),
                ..Default::default()
            })),
        });
        // No InteractionResolved event is sent, and the provider result is
        // deliberately absent. Action completion alone must return a notice.
        tokio::time::timeout(Duration::from_secs(3), async {
            loop {
                if shell.latest.lock().unwrap().toasts.iter().any(|t| {
                    !t.error
                        && t.text.contains(&format!("{count} 道题"))
                        && t.text.contains("请在原会话确认")
                }) {
                    break;
                }
                shell.changed.notified().await;
            }
        })
        .await
        .unwrap();
        let chat = shell.latest.lock().unwrap().chat.clone().unwrap();
        assert!(chat.responding_interaction.is_empty());
        assert!(chat.response_error.is_empty());
        assert!(!chat.response_unknown);
        assert_eq!(server.submissions.load(Ordering::SeqCst), 1);
        core.shutdown();
        task.abort();
    }
}
