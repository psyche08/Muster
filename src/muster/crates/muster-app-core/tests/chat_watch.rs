//! Exercise chat catch-up over the actual bounded, multiplexed RPC transport.
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use muster_app_core::{AppConfig, AppCore, Mode, app};
use muster_proto::v1;
use muster_rpc::{Context, EventSink, Handler, HandlerFuture, Principal};
use tokio::sync::Notify;

mod common;
use common::TestShell;

const SESSION: &str = "chat-watch-test";

struct Server {
    delayed_snapshot: bool,
    watches: AtomicUsize,
    cancelled_watches: AtomicUsize,
    snapshots: AtomicUsize,
    burst_sent: Notify,
    first_snapshot_started: Notify,
    release_first_snapshot: Notify,
}

#[derive(Default)]
struct CatalogServer {
    release_info: Notify,
    lists: AtomicUsize,
    archive: std::sync::atomic::AtomicBool,
}

impl Handler for CatalogServer {
    fn call(&self, _: Context, request: v1::request::Req, events: EventSink) -> HandlerFuture<'_> {
        use v1::{request::Req, response::Resp};
        Box::pin(async move {
            match request {
                Req::DeviceInfo(_) => {
                    self.release_info.notified().await;
                    Resp::DeviceInfo(v1::DeviceInfo {
                        device_id: "local".into(),
                        ..Default::default()
                    })
                }
                Req::NetworkStatus(_) => Resp::NetworkStatus(v1::NetworkStatus {
                    has_network: true,
                    members: vec![v1::MemberView {
                        device_id: "local".into(),
                        name: "Local".into(),
                        kind: "desktop".into(),
                        is_self: true,
                        ..Default::default()
                    }],
                    ..Default::default()
                }),
                Req::AgentList(_) => {
                    let revision = self.lists.fetch_add(1, Ordering::SeqCst);
                    if revision == 0 {
                        Resp::Error(v1::Error {
                            code: v1::error::Code::Unavailable as i32,
                            message: "starting".into(),
                        })
                    } else {
                        Resp::AgentSessions(v1::AgentSessionPage {
                            sessions: vec![
                                v1::AgentSession {
                                    id: SESSION.into(),
                                    updated_unix: revision as u64,
                                    archived: self.archive.load(Ordering::SeqCst),
                                    ..Default::default()
                                },
                                v1::AgentSession {
                                    id: "already-archived".into(),
                                    archived: true,
                                    ..Default::default()
                                },
                            ],
                        })
                    }
                }
                Req::PolicyGet(_) => Resp::Policy(v1::Policy::default()),
                Req::NetworkPending(_) => Resp::PendingJoins(v1::PendingJoins::default()),
                Req::DeviceWatch(_) => {
                    events.cancelled().cancelled().await;
                    Resp::Ack(v1::Ack {})
                }
                _ => Resp::Ack(v1::Ack {}),
            }
        })
    }
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn local_catalog_retries_before_device_info_arrives() {
    let dir = tempfile::tempdir().unwrap();
    let socket = dir.path().join("rpc.sock");
    let listener = tokio::net::UnixListener::bind(&socket).unwrap();
    let server = Arc::new(CatalogServer::default());
    let handler = server.clone();
    let task = tokio::spawn(async move {
        let (stream, _) = listener.accept().await.unwrap();
        muster_rpc::serve(stream, Principal::Local, handler).await;
    });
    let shell = Arc::new(TestShell::default());
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
    let view = shell
        .wait(|v| v.agents.as_ref().is_some_and(|a| a.sessions.len() == 1))
        .await;
    assert!(view.self_.is_none());
    let agents = view.agents.unwrap();
    assert_eq!(agents.device_id, "*");
    assert_eq!(agents.sessions[0].device_id, "local");
    assert_eq!(agents.sessions[0].device_name, "Local");
    assert!(agents.errors.is_empty());
    assert_eq!(server.lists.load(Ordering::SeqCst), 2);
    // Successful catalogs must refresh too, without a network change or a tap.
    let refreshed = shell
        .wait(|v| {
            v.agents
                .as_ref()
                .is_some_and(|a| a.sessions.first().is_some_and(|s| s.updated_unix >= 2))
        })
        .await;
    let revision = refreshed.agents.unwrap().sessions[0].updated_unix;
    core.intent(app::Intent {
        it: Some(app::intent::It::Navigate(app::Navigate {
            screen: app::Screen::Settings as i32,
            back: false,
            ..Default::default()
        })),
    });
    shell
        .wait(|v| v.screen == app::Screen::Settings as i32)
        .await;
    core.intent(app::Intent {
        it: Some(app::intent::It::Navigate(app::Navigate {
            screen: 0,
            back: true,
            ..Default::default()
        })),
    });
    shell
        .wait(|v| {
            v.screen == app::Screen::Agents as i32
                && v.agents.as_ref().is_some_and(|a| {
                    a.sessions
                        .first()
                        .is_some_and(|s| s.updated_unix > revision)
                })
        })
        .await;
    // A peer that includes archived rows must not keep them in the app or
    // in the newly written cache. A fresh archive removes an existing row.
    server.archive.store(true, Ordering::SeqCst);
    core.intent(app::Intent {
        it: Some(app::intent::It::Refresh(app::Refresh {})),
    });
    shell
        .wait(|v| {
            v.agents
                .as_ref()
                .is_some_and(|a| !a.loading && a.sessions.is_empty())
        })
        .await;
    // No periodic catalog traffic after backgrounding.
    core.lifecycle(app::Lifecycle::Background);
    tokio::time::sleep(Duration::from_millis(100)).await;
    let count = server.lists.load(Ordering::SeqCst);
    tokio::time::sleep(Duration::from_millis(3200)).await;
    assert_eq!(server.lists.load(Ordering::SeqCst), count);
    server.release_info.notify_one();
    core.backend().local().close();
    core.shutdown();
    task.abort();
}

fn message(index: usize) -> v1::AgentMessage {
    v1::AgentMessage {
        id: format!("m{index}"),
        role: if index % 2 == 0 { "user" } else { "assistant" }.into(),
        kind: "text".into(),
        text: "same text in different messages".into(),
        ..Default::default()
    }
}

fn event(ev: v1::agent_event::Ev) -> v1::event::Ev {
    v1::event::Ev::AgentEvent(v1::AgentEvent { ev: Some(ev) })
}

impl Handler for Server {
    fn call(&self, _: Context, request: v1::request::Req, events: EventSink) -> HandlerFuture<'_> {
        use v1::{request::Req, response::Resp};
        Box::pin(async move {
            match request {
                Req::DeviceInfo(_) => Resp::DeviceInfo(v1::DeviceInfo {
                    device_id: "self".into(),
                    ..Default::default()
                }),
                Req::NetworkStatus(_) => Resp::NetworkStatus(v1::NetworkStatus {
                    has_network: true,
                    ..Default::default()
                }),
                Req::PolicyGet(_) => Resp::Policy(v1::Policy::default()),
                Req::NetworkPending(_) => Resp::PendingJoins(v1::PendingJoins::default()),
                Req::DeviceWatch(_) => {
                    events.cancelled().cancelled().await;
                    Resp::Ack(v1::Ack {})
                }
                Req::AgentWatch(request) => {
                    assert_eq!(request.session, SESSION);
                    let watch = self.watches.fetch_add(1, Ordering::SeqCst);
                    events
                        .send(event(v1::agent_event::Ev::State(v1::AgentSession {
                            id: SESSION.into(),
                            provider: "codex".into(),
                            state: "idle".into(),
                            ..Default::default()
                        })))
                        .await
                        .unwrap();
                    if !self.delayed_snapshot && watch == 0 {
                        // More events than Client::Call's 256-slot queue. Their
                        // envelopes precede the history response on the wire.
                        for index in 0..300 {
                            events
                                .send(event(v1::agent_event::Ev::Message(message(index))))
                                .await
                                .unwrap();
                        }
                        self.burst_sent.notify_one();
                    }
                    events.cancelled().cancelled().await;
                    self.cancelled_watches.fetch_add(1, Ordering::SeqCst);
                    Resp::Ack(v1::Ack {})
                }
                Req::AgentMessages(_) => {
                    let snapshot = self.snapshots.fetch_add(1, Ordering::SeqCst);
                    let messages = if self.delayed_snapshot {
                        if snapshot == 0 {
                            self.first_snapshot_started.notify_one();
                            self.release_first_snapshot.notified().await;
                        }
                        vec![message(if snapshot == 0 { 0 } else { 1 })]
                    } else {
                        if snapshot == 0 {
                            self.burst_sent.notified().await;
                        }
                        (0..300).map(message).collect()
                    };
                    Resp::AgentMessages(v1::AgentMessagePage {
                        session: SESSION.into(),
                        messages,
                        has_more: false,
                        ..Default::default()
                    })
                }
                _ => Resp::Ack(v1::Ack {}),
            }
        })
    }
}

async fn fixture(
    delayed_snapshot: bool,
) -> (
    tempfile::TempDir,
    AppCore,
    Arc<TestShell>,
    Arc<Server>,
    tokio::task::JoinHandle<()>,
) {
    let dir = tempfile::tempdir().unwrap();
    let socket = dir.path().join("rpc.sock");
    let listener = tokio::net::UnixListener::bind(&socket).unwrap();
    let server = Arc::new(Server {
        delayed_snapshot,
        watches: AtomicUsize::new(0),
        cancelled_watches: AtomicUsize::new(0),
        snapshots: AtomicUsize::new(0),
        burst_sent: Notify::new(),
        first_snapshot_started: Notify::new(),
        release_first_snapshot: Notify::new(),
    });
    let handler = server.clone();
    let task = tokio::spawn(async move {
        let (stream, _) = listener.accept().await.unwrap();
        muster_rpc::serve(stream, Principal::Local, handler).await;
    });
    let shell = Arc::new(TestShell::default());
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
    shell
        .wait(|view| view.screen == app::Screen::Agents as i32)
        .await;
    core.intent(app::Intent {
        it: Some(app::intent::It::OpenAgent(app::OpenAgent {
            session: SESSION.into(),
            ..Default::default()
        })),
    });
    (dir, core, shell, server, task)
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn drains_more_than_rpc_queue_capacity_before_snapshot_response() {
    let (_dir, core, shell, server, task) = fixture(false).await;
    let view = shell
        .wait(|v| v.chat.as_ref().is_some_and(|c| c.message_count == 300))
        .await;
    let messages = common::messages(&view.chat.unwrap());
    assert_eq!(
        messages.iter().map(|m| m.id.clone()).collect::<Vec<_>>(),
        (0..300).map(|i| format!("m{i}")).collect::<Vec<_>>()
    );
    assert_eq!(server.watches.load(Ordering::SeqCst), 1);
    core.backend().local().close();
    core.shutdown();
    task.abort();
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn foreground_replaces_watch_and_rejects_its_late_snapshot() {
    let (_dir, core, shell, server, task) = fixture(true).await;
    tokio::time::timeout(
        Duration::from_secs(5),
        server.first_snapshot_started.notified(),
    )
    .await
    .unwrap();
    core.lifecycle(app::Lifecycle::Foreground);
    shell
        .wait(|v| {
            v.chat
                .as_ref()
                .is_some_and(|c| c.message_count == 1 && common::messages(c)[0].id == "m1")
        })
        .await;
    server.release_first_snapshot.notify_one();
    tokio::time::sleep(Duration::from_millis(150)).await;
    let view = shell.latest.lock().unwrap().clone();
    let messages = common::messages(&view.chat.unwrap());
    assert_eq!(messages.len(), 1);
    assert_eq!(messages[0].id, "m1");
    assert_eq!(server.watches.load(Ordering::SeqCst), 2);
    assert_eq!(server.cancelled_watches.load(Ordering::SeqCst), 1);
    core.backend().local().close();
    core.shutdown();
    task.abort();
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn stalled_history_reports_error_then_reconnects_without_blank_forever() {
    let (_dir, core, shell, server, task) = fixture(true).await;
    let view = shell
        .wait(|v| {
            v.chat
                .as_ref()
                .is_some_and(|c| c.load_error.contains("timed out"))
        })
        .await;
    assert!(!view.chat.unwrap().loading);
    let recovered = shell
        .wait(|v| {
            v.chat
                .as_ref()
                .is_some_and(|c| c.message_count == 1 && common::messages(c)[0].id == "m1")
        })
        .await;
    assert!(recovered.chat.unwrap().load_error.is_empty());
    assert_eq!(server.watches.load(Ordering::SeqCst), 2);
    core.backend().local().close();
    core.shutdown();
    task.abort();
}

#[derive(Default)]
struct HistoryServer {
    messages: Mutex<Vec<v1::AgentMessage>>,
    requests: Mutex<Vec<v1::AgentMessagesRequest>>,
    deliveries: Mutex<Vec<usize>>,
    hold: std::sync::atomic::AtomicBool,
    entered: Notify,
    release: Notify,
}
impl Handler for HistoryServer {
    fn call(&self, _: Context, request: v1::request::Req, events: EventSink) -> HandlerFuture<'_> {
        use v1::{request::Req, response::Resp};
        Box::pin(async move {
            match request {
                Req::DeviceInfo(_) => Resp::DeviceInfo(v1::DeviceInfo {
                    device_id: "self".into(),
                    ..Default::default()
                }),
                Req::NetworkStatus(_) => Resp::NetworkStatus(v1::NetworkStatus {
                    has_network: true,
                    ..Default::default()
                }),
                Req::AgentWatch(_) => {
                    let _ = events
                        .send(event(v1::agent_event::Ev::State(v1::AgentSession {
                            id: SESSION.into(),
                            ..Default::default()
                        })))
                        .await;
                    events.cancelled().cancelled().await;
                    Resp::Ack(v1::Ack {})
                }
                Req::AgentMessages(r) => {
                    self.requests.lock().unwrap().push(r.clone());
                    if self.hold.load(Ordering::SeqCst) {
                        self.entered.notify_one();
                        self.release.notified().await;
                    }
                    let page = muster_proto::history::page(&self.messages.lock().unwrap(), &r);
                    self.deliveries.lock().unwrap().push(page.messages.len());
                    Resp::AgentMessages(page)
                }
                _ => Resp::Ack(v1::Ack {}),
            }
        })
    }
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn cached_reopen_fetches_only_delta_and_jump_to_start_loads_all_pages() {
    let dir = tempfile::tempdir().unwrap();
    let socket = dir.path().join("rpc.sock");
    let listener = tokio::net::UnixListener::bind(&socket).unwrap();
    let server = Arc::new(HistoryServer {
        messages: Mutex::new((0..1300).map(message).collect()),
        ..Default::default()
    });
    let handler = server.clone();
    let task = tokio::spawn(async move {
        let (stream, _) = listener.accept().await.unwrap();
        muster_rpc::serve(stream, Principal::Local, handler).await;
    });
    let shell = Arc::new(TestShell::default());
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
    shell.wait(|v| v.screen == app::Screen::Agents as i32).await;
    let open = || {
        core.intent(app::Intent {
            it: Some(app::intent::It::OpenAgent(app::OpenAgent {
                session: SESSION.into(),
                ..Default::default()
            })),
        })
    };
    open();
    shell
        .wait(|v| {
            v.chat
                .as_ref()
                .is_some_and(|c| !c.loading && c.message_count == 500 && c.has_earlier)
        })
        .await;
    core.intent(app::Intent {
        it: Some(app::intent::It::AgentLoadEarlier(app::AgentLoadEarlier {
            all: true,
        })),
    });
    let view = shell
        .wait(|v| {
            v.chat
                .as_ref()
                .is_some_and(|c| !c.loading && c.message_count == 1300 && !c.has_earlier)
        })
        .await;
    assert_eq!(
        common::messages(&view.chat.unwrap()).first().unwrap().id,
        "m0"
    );
    core.intent(app::Intent {
        it: Some(app::intent::It::Navigate(app::Navigate {
            screen: 0,
            back: true,
            ..Default::default()
        })),
    });
    shell.wait(|v| v.screen == app::Screen::Agents as i32).await;
    {
        let mut messages = server.messages.lock().unwrap();
        messages.last_mut().unwrap().text = "stream finished".into();
        messages.push(message(1300));
    }
    server.hold.store(true, Ordering::SeqCst);
    open();
    server.entered.notified().await;
    let cached = shell
        .wait(|v| {
            v.chat
                .as_ref()
                .is_some_and(|c| c.loading && c.message_count == 1300)
        })
        .await;
    assert_eq!(common::messages(&cached.chat.unwrap())[0].id, "m0");
    server.hold.store(false, Ordering::SeqCst);
    server.release.notify_one();
    let view = shell
        .wait(|v| {
            v.chat
                .as_ref()
                .is_some_and(|c| !c.loading && c.message_count == 1301)
        })
        .await;
    let messages = common::messages(&view.chat.unwrap());
    assert_eq!(messages[1299].text, "stream finished");
    assert_eq!(server.deliveries.lock().unwrap().last(), Some(&2));
    assert_eq!(
        server.requests.lock().unwrap().last().unwrap().after_id,
        "m1299"
    );
    assert_eq!(
        server.requests.lock().unwrap().last().unwrap().first_id,
        "m0"
    );
    core.backend().local().close();
    core.shutdown();
    task.abort();
}
