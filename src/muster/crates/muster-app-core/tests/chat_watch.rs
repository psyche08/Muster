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
struct RetryServer {
    watches: AtomicUsize,
    snapshots: AtomicUsize,
    disconnect: Notify,
    recovering: Notify,
    release: Notify,
}

impl Handler for RetryServer {
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
                    let index = self.watches.fetch_add(1, Ordering::SeqCst);
                    events
                        .send(event(v1::agent_event::Ev::State(v1::AgentSession {
                            id: SESSION.into(),
                            ..Default::default()
                        })))
                        .await
                        .unwrap();
                    if index == 0 {
                        self.disconnect.notified().await;
                        Resp::Error(v1::Error {
                            code: v1::error::Code::Unavailable as i32,
                            message: "synthetic disconnect".into(),
                        })
                    } else {
                        events.cancelled().cancelled().await;
                        Resp::Ack(v1::Ack {})
                    }
                }
                Req::AgentMessages(_) => {
                    let index = self.snapshots.fetch_add(1, Ordering::SeqCst);
                    if index > 0 {
                        self.recovering.notify_one();
                        self.release.notified().await;
                    }
                    Resp::AgentMessages(v1::AgentMessagePage {
                        session: SESSION.into(),
                        messages: vec![message(index)],
                        ..Default::default()
                    })
                }
                _ => Resp::Ack(v1::Ack {}),
            }
        })
    }
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn reconnect_gets_fresh_readiness_and_native_application_evidence() {
    let dir = tempfile::tempdir().unwrap();
    let socket = dir.path().join("rpc.sock");
    let listener = tokio::net::UnixListener::bind(&socket).unwrap();
    let server = Arc::new(RetryServer::default());
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
    core.intent(app::Intent {
        it: Some(app::intent::It::OpenAgent(app::OpenAgent {
            session: SESSION.into(),
            ..Default::default()
        })),
    });
    let first = shell
        .wait(|v| v.session_load.as_ref().is_some_and(|m| m.history_ready))
        .await;
    let first = first.session_load.unwrap();
    let acknowledge = |marker: &app::SessionLoadMarker, apply_us| {
        core.intent(app::Intent {
            it: Some(app::intent::It::SessionViewApplied(
                app::SessionViewApplied {
                    trace_id: marker.trace_id.clone(),
                    history_ready: marker.history_ready,
                    apply_us,
                    ..Default::default()
                },
            )),
        });
    };
    acknowledge(&first, 11);
    // Intents and diagnostic records are serialized by the core loop. Observe
    // the acknowledgement before inducing the failure instead of sleeping.
    shell
        .wait(|_| {
            shell
                .diagnostics
                .lock()
                .unwrap()
                .iter()
                .any(|r| r["event"] == "native_applied" && r["apply_us"] == 11)
        })
        .await;
    server.disconnect.notify_one();
    tokio::time::timeout(Duration::from_secs(5), server.recovering.notified())
        .await
        .unwrap();
    let pending = shell.latest.lock().unwrap().session_load.clone().unwrap();
    assert_ne!(pending.trace_id, first.trace_id);
    assert!(
        !pending.history_ready,
        "cached history must not complete recovery"
    );
    acknowledge(&first, 99); // Must be rejected after the generation changed.
    acknowledge(&pending, 22);
    shell
        .wait(|_| {
            shell
                .diagnostics
                .lock()
                .unwrap()
                .iter()
                .any(|r| r["event"] == "native_applied" && r["apply_us"] == 22)
        })
        .await;
    server.release.notify_one();
    let recovered = shell
        .wait(|v| {
            v.session_load
                .as_ref()
                .is_some_and(|m| m.trace_id == pending.trace_id && m.history_ready)
        })
        .await;
    assert_eq!(
        common::messages(recovered.chat.as_ref().unwrap())[0].id,
        "m1"
    );
    acknowledge(recovered.session_load.as_ref().unwrap(), 33);
    shell
        .wait(|_| {
            shell
                .diagnostics
                .lock()
                .unwrap()
                .iter()
                .any(|r| r["event"] == "native_applied" && r["apply_us"] == 33)
        })
        .await;
    let records = shell.diagnostics.lock().unwrap().clone();
    assert!(!records.iter().any(|r| r["apply_us"] == 99));
    assert!(records.iter().any(|r| r["trace_id"] == pending.trace_id
        && r["event"] == "retry_of"
        && r["entry_trace_id"] == first.trace_id));
    assert_eq!(server.watches.load(Ordering::SeqCst), 2);
    core.backend().local().close();
    core.shutdown();
    task.abort();
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
                            ..Default::default()
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
    let marker = view.session_load.as_ref().expect("correlated load marker");
    assert!(marker.history_ready);
    let records = shell.diagnostics.lock().unwrap().clone();
    for event in [
        "entered",
        "watch_first_event",
        "history_requested",
        "history_received",
        "history_reconciled",
        "history_applied",
    ] {
        assert!(
            records
                .iter()
                .any(|r| r["event"] == event && r["trace_id"] == marker.trace_id),
            "missing correlated {event}"
        );
    }
    let messages = common::messages(&view.chat.unwrap());
    // Every other record is a prompt, so the latest ten turns are 20 records.
    assert_eq!(
        messages.iter().map(|m| m.id.clone()).collect::<Vec<_>>(),
        (280..300).map(|i| format!("m{i}")).collect::<Vec<_>>()
    );
    // The watch must drain all 300 events even though the UI projects only
    // its recent window. Reveal cached history without another watch; the
    // window grows up to the rows it showed.
    core.intent(app::Intent {
        it: Some(app::intent::It::AgentLoadEarlier(app::AgentLoadEarlier {
            all: true,
            ..Default::default()
        })),
    });
    let view = shell
        .wait(|v| v.chat.as_ref().is_some_and(|c| !c.has_earlier))
        .await;
    assert_eq!(
        common::messages(&view.chat.unwrap())
            .iter()
            .map(|m| m.id.clone())
            .collect::<Vec<_>>(),
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
    // The client's own history snapshot deadline is longer than the default
    // patience; wait past it rather than shortening the product timeout.
    let view = shell
        .wait_within(Duration::from_secs(25), "a stalled history error", |v| {
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
    stale_event: bool,
    same_id_update: bool,
    race: std::sync::atomic::AtomicBool,
    watch_count: AtomicUsize,
    snapshot_captured: Notify,
    race_release: Notify,
    messages: Mutex<Vec<v1::AgentMessage>>,
    requests: Mutex<Vec<v1::AgentMessagesRequest>>,
    deliveries: Mutex<Vec<usize>>,
    hold: std::sync::atomic::AtomicBool,
    entered: Notify,
    release: Notify,
    hold_watch: std::sync::atomic::AtomicBool,
    deny_watch: std::sync::atomic::AtomicBool,
    watch_entered: Notify,
    release_watch: Notify,
    /// Pages outlines when asked, as a current daemon does (ADR 0160).
    outline: bool,
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
                    let index = self.watch_count.fetch_add(1, Ordering::SeqCst);
                    if self.hold_watch.load(Ordering::SeqCst) {
                        self.watch_entered.notify_one();
                        self.release_watch.notified().await;
                    }
                    if self.deny_watch.load(Ordering::SeqCst) {
                        return Resp::Error(v1::Error {
                            code: v1::error::Code::Unauthorized as i32,
                            message: "Session belongs to Multica".into(),
                        });
                    }
                    let race_watch = self.race.load(Ordering::SeqCst) && index == 0;
                    let _ = events
                        .send(event(v1::agent_event::Ev::State(v1::AgentSession {
                            id: SESSION.into(),
                            ..Default::default()
                        })))
                        .await;
                    if race_watch {
                        self.snapshot_captured.notified().await;
                        let mut live = message(if self.same_id_update { 0 } else { 1 });
                        if self.same_id_update {
                            live.text = "completed response".into();
                            self.messages.lock().unwrap()[0] = live.clone();
                        } else if !self.stale_event {
                            self.messages.lock().unwrap().push(live.clone());
                        }
                        events
                            .send(event(v1::agent_event::Ev::Message(live)))
                            .await
                            .unwrap();
                        events
                            .send(event(v1::agent_event::Ev::State(v1::AgentSession {
                                id: SESSION.into(),
                                title: "queued-live-message".into(),
                                ..Default::default()
                            })))
                            .await
                            .unwrap();
                    }
                    events.cancelled().cancelled().await;
                    Resp::Ack(v1::Ack {})
                }
                Req::AgentMessages(r) => {
                    self.requests.lock().unwrap().push(r.clone());
                    if self.hold.load(Ordering::SeqCst) {
                        self.entered.notify_one();
                        self.release.notified().await;
                    }
                    let page = {
                        let messages = self.messages.lock().unwrap();
                        if self.outline && (r.turns > 0 || !r.work_id.is_empty()) {
                            muster_proto::outline::Outline::default().page(
                                &messages,
                                &r,
                                Clone::clone,
                            )
                        } else {
                            muster_proto::history::page(&messages, &r)
                        }
                    };
                    if self.race.swap(false, Ordering::SeqCst) {
                        self.snapshot_captured.notify_one();
                        self.race_release.notified().await;
                    }
                    self.deliveries.lock().unwrap().push(page.messages.len());
                    Resp::AgentMessages(page)
                }
                _ => Resp::Ack(v1::Ack {}),
            }
        })
    }
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn newer_live_event_before_history_response_keeps_the_same_subscription() {
    history_event_race(false, false).await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn unorderable_live_event_reconnects_after_one_bounded_reconciliation() {
    history_event_race(true, false).await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn same_id_update_before_old_snapshot_preserves_new_content() {
    history_event_race(false, true).await;
}

async fn history_event_race(stale_event: bool, same_id_update: bool) {
    let dir = tempfile::tempdir().unwrap();
    let socket = dir.path().join("rpc.sock");
    let listener = tokio::net::UnixListener::bind(&socket).unwrap();
    let server = Arc::new(HistoryServer {
        messages: Mutex::new(vec![message(if stale_event { 10 } else { 0 })]),
        stale_event,
        same_id_update,
        race: std::sync::atomic::AtomicBool::new(true),
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
    core.intent(app::Intent {
        it: Some(app::intent::It::OpenAgent(app::OpenAgent {
            session: SESSION.into(),
            ..Default::default()
        })),
    });
    // The metadata marker follows the live message on the event stream, so
    // observing it proves that message was queued before releasing the snapshot.
    shell
        .wait(|v| {
            v.chat
                .as_ref()
                .and_then(|c| c.session.as_ref())
                .is_some_and(|s| s.title == "queued-live-message")
        })
        .await;
    server.race_release.notify_one();
    let view = shell
        .wait(|v| {
            v.chat.as_ref().is_some_and(|c| {
                !c.loading && c.message_count == if stale_event || same_id_update { 1 } else { 2 }
            })
        })
        .await;
    let messages = common::messages(&view.chat.unwrap());
    if same_id_update {
        assert_eq!(messages[0].text, "completed response");
    }
    assert_eq!(
        messages.iter().map(|m| m.id.as_str()).collect::<Vec<_>>(),
        if stale_event {
            vec!["m10"]
        } else if same_id_update {
            vec!["m0"]
        } else {
            vec!["m0", "m1"]
        }
    );
    assert_eq!(
        server.watch_count.load(Ordering::SeqCst),
        if stale_event { 2 } else { 1 },
        "a stale snapshot must not replace the live subscription"
    );
    assert_eq!(
        server.requests.lock().unwrap().len(),
        if stale_event { 3 } else { 2 }
    );
    core.backend().local().close();
    core.shutdown();
    task.abort();
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn cached_reopen_requires_current_access_then_fetches_only_delta_and_loads_all_pages() {
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
                .is_some_and(|c| !c.loading && c.message_count == 50 && c.has_earlier)
        })
        .await;
    core.intent(app::Intent {
        it: Some(app::intent::It::AgentLoadEarlier(app::AgentLoadEarlier {
            all: true,
            ..Default::default()
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
    server.hold_watch.store(true, Ordering::SeqCst);
    let requests_before_reopen = server.requests.lock().unwrap().len();
    open();
    server.watch_entered.notified().await;
    shell
        .wait(|v| {
            v.chat
                .as_ref()
                .is_some_and(|c| c.loading && c.message_count == 0)
        })
        .await;
    assert_eq!(
        server.requests.lock().unwrap().len(),
        requests_before_reopen
    );
    server.hold_watch.store(false, Ordering::SeqCst);
    server.release_watch.notify_one();
    server.entered.notified().await;
    let cached = shell
        .wait(|v| {
            v.chat
                .as_ref()
                .is_some_and(|c| c.loading && c.message_count == 200)
        })
        .await;
    // The cache holds 200 records; the window shows their latest ten turns.
    let cached_messages = common::messages(&cached.chat.unwrap());
    assert_eq!(cached_messages.len(), 20);
    assert_eq!(cached_messages[0].id, "m1280");
    server.hold.store(false, Ordering::SeqCst);
    server.release.notify_one();
    let view = shell
        .wait(|v| {
            v.chat
                .as_ref()
                .is_some_and(|c| !c.loading && c.message_count == 201)
        })
        .await;
    let messages = common::messages(&view.chat.unwrap());
    assert_eq!(
        messages.first().unwrap().id,
        "m1282",
        "Latest window keeps ten turns while streaming"
    );
    assert_eq!(
        messages.iter().find(|m| m.id == "m1299").unwrap().text,
        "stream finished"
    );
    assert_eq!(messages.last().unwrap().id, "m1300");
    assert_eq!(server.deliveries.lock().unwrap().last(), Some(&2));
    assert_eq!(
        server.requests.lock().unwrap().last().unwrap().after_id,
        "m1299"
    );
    assert_eq!(
        server.requests.lock().unwrap().last().unwrap().first_id,
        "m1100"
    );
    core.intent(app::Intent {
        it: Some(app::intent::It::Navigate(app::Navigate {
            back: true,
            ..Default::default()
        })),
    });
    shell.wait(|v| v.screen == app::Screen::Agents as i32).await;
    server.deny_watch.store(true, Ordering::SeqCst);
    let requests_before_denial = server.requests.lock().unwrap().len();
    open();
    shell
        .wait(|v| {
            v.chat
                .as_ref()
                .is_some_and(|c| !c.load_error.is_empty() && c.message_count == 0)
        })
        .await;
    assert_eq!(
        server.requests.lock().unwrap().len(),
        requests_before_denial
    );
    core.backend().local().close();
    core.shutdown();
    task.abort();
}

#[derive(Default)]
struct PushCatalog {
    changes: Notify,
    revision: AtomicUsize,
    lists: AtomicUsize,
    watches: AtomicUsize,
    cancelled: AtomicUsize,
}
impl Handler for PushCatalog {
    fn call(&self, _: Context, request: v1::request::Req, events: EventSink) -> HandlerFuture<'_> {
        use v1::{request::Req, response::Resp};
        Box::pin(async move {
            match request {
                Req::DeviceInfo(_) => Resp::DeviceInfo(v1::DeviceInfo {
                    device_id: "local".into(),
                    ..Default::default()
                }),
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
                    self.lists.fetch_add(1, Ordering::SeqCst);
                    Resp::Ack(v1::Ack {})
                }
                Req::AgentWatch(r) => {
                    assert!(r.catalog_only);
                    // One page of main sessions; later pages are requested.
                    assert_eq!(r.limit, muster_agent::catalog::PAGE_SIZE as u32);
                    assert!(r.session.is_empty());
                    self.watches.fetch_add(1, Ordering::SeqCst);
                    loop {
                        let revision = self.revision.load(Ordering::SeqCst);
                        events
                            .send(v1::event::Ev::AgentEvent(v1::AgentEvent {
                                ev: Some(v1::agent_event::Ev::ListChanged(v1::AgentSessionPage {
                                    sessions: if revision == 2 {
                                        vec![]
                                    } else {
                                        vec![v1::AgentSession {
                                            id: "pushed".into(),
                                            title: format!("Revision {revision}"),
                                            ..Default::default()
                                        }]
                                    },
                                    ..Default::default()
                                })),
                            }))
                            .await
                            .unwrap();
                        tokio::select! {
                            _ = events.cancelled().cancelled() => break,
                            _ = self.changes.notified() => {},
                        }
                    }
                    self.cancelled.fetch_add(1, Ordering::SeqCst);
                    Resp::Ack(v1::Ack {})
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
async fn catalog_push_replaces_renamed_and_removed_rows_without_polling() {
    let dir = tempfile::tempdir().unwrap();
    let socket = dir.path().join("push.sock");
    let listener = tokio::net::UnixListener::bind(&socket).unwrap();
    let server = Arc::new(PushCatalog::default());
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
        .wait(|v| {
            v.agents
                .as_ref()
                .is_some_and(|a| a.sessions.first().is_some_and(|s| s.title == "Revision 0"))
        })
        .await;
    server.revision.store(1, Ordering::SeqCst);
    server.changes.notify_one();
    shell
        .wait(|v| {
            v.agents
                .as_ref()
                .is_some_and(|a| a.sessions.first().is_some_and(|s| s.title == "Revision 1"))
        })
        .await;
    server.revision.store(2, Ordering::SeqCst);
    server.changes.notify_one();
    shell
        .wait(|v| {
            v.agents
                .as_ref()
                .is_some_and(|a| !a.loading && a.sessions.is_empty())
        })
        .await;
    assert_eq!(server.lists.load(Ordering::SeqCst), 0);
    core.lifecycle(app::Lifecycle::Background);
    tokio::time::timeout(Duration::from_secs(3), async {
        while server.cancelled.load(Ordering::SeqCst) < server.watches.load(Ordering::SeqCst) {
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
    })
    .await
    .unwrap();
    server.revision.store(3, Ordering::SeqCst);
    core.lifecycle(app::Lifecycle::Foreground);
    shell
        .wait(|v| {
            v.agents
                .as_ref()
                .is_some_and(|a| a.sessions.first().is_some_and(|s| s.title == "Revision 3"))
        })
        .await;
    assert_eq!(server.lists.load(Ordering::SeqCst), 0);
    core.backend().local().close();
    core.shutdown();
    task.abort();
}

fn outline_turn(turn: usize) -> Vec<v1::AgentMessage> {
    let record = |id: String, role: &str, kind: &str| v1::AgentMessage {
        text: id.clone(),
        id,
        role: role.into(),
        kind: kind.into(),
        ..Default::default()
    };
    let mut out = vec![record(format!("t{turn}-prompt"), "user", "text")];
    for step in 0..5 {
        out.push(v1::AgentMessage {
            tool_name: "Bash".into(),
            tool_call_id: format!("c{turn}-{step}"),
            ..record(format!("t{turn}-call{step}"), "assistant", "tool_call")
        });
    }
    out.push(record(format!("t{turn}-answer"), "assistant", "text"));
    out
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn outline_pages_load_a_turns_work_on_expand_and_cache_it() {
    let dir = tempfile::tempdir().unwrap();
    let socket = dir.path().join("rpc.sock");
    let listener = tokio::net::UnixListener::bind(&socket).unwrap();
    let server = Arc::new(HistoryServer {
        messages: Mutex::new((0..30).flat_map(outline_turn).collect()),
        outline: true,
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
    let group = "working:activity:t25-call0";
    let block = |v: &app::ViewModel| {
        v.chat
            .as_ref()
            .and_then(|c| c.timeline.iter().find(|b| b.id == group).cloned())
    };
    open();
    // Ten turns of outline: nine collapsed, the latest whole.
    let view = shell
        .wait(|v| {
            v.chat
                .as_ref()
                .is_some_and(|c| !c.loading && c.message_count == 34)
        })
        .await;
    assert!(view.chat.as_ref().unwrap().has_earlier);
    assert_eq!(server.requests.lock().unwrap()[0].turns, 10);
    let unloaded = block(&view).unwrap();
    assert_eq!(unloaded.details, "unloaded");
    assert!(unloaded.children.is_empty());
    let expand = || {
        core.intent(app::Intent {
            it: Some(app::intent::It::AgentLoadWork(app::AgentLoadWork {
                block: group.into(),
            })),
        })
    };
    expand();
    let view = shell
        .wait(|v| block(v).is_some_and(|b| b.details.is_empty()))
        .await;
    let ids: Vec<_> = common::messages(&app::ChatView {
        timeline: vec![block(&view).unwrap()],
        ..Default::default()
    })
    .into_iter()
    .map(|m| m.id)
    .collect();
    assert_eq!(
        ids,
        (0..5).map(|i| format!("t25-call{i}")).collect::<Vec<_>>()
    );
    let work_requests = || {
        server
            .requests
            .lock()
            .unwrap()
            .iter()
            .filter(|r| !r.work_id.is_empty())
            .count()
    };
    assert_eq!(work_requests(), 1);
    // Reopened, the work comes from the disk cache without asking the Mac.
    core.intent(app::Intent {
        it: Some(app::intent::It::Navigate(app::Navigate {
            screen: 0,
            back: true,
            ..Default::default()
        })),
    });
    shell.wait(|v| v.screen == app::Screen::Agents as i32).await;
    open();
    let view = shell
        .wait(|v| v.chat.as_ref().is_some_and(|c| !c.loading) && block(v).is_some())
        .await;
    assert_eq!(block(&view).unwrap().details, "unloaded");
    expand();
    shell
        .wait(|v| block(v).is_some_and(|b| b.details.is_empty() && !b.children.is_empty()))
        .await;
    assert_eq!(work_requests(), 1);
    core.backend().local().close();
    core.shutdown();
    task.abort();
}
