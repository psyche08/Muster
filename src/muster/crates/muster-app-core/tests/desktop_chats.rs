//! Desktop Chat mode through the core: the chat list, opening a listed
//! conversation on the desktop, a reply growing in place, a new chat
//! following its first send into the conversation it became, and a send
//! refused behind an unconfirmed one released from the conversation.
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use muster_app_core::{AppConfig, AppCore, Mode, app};
use muster_proto::v1;
use muster_rpc::{Context, EventSink, Handler, HandlerFuture, Principal};
use tokio::sync::Notify;

mod common;
use common::TestShell;

const LISTED: &str = "claude-chat-0f0e0d0c-1111-4222-8333-444455556666";
const CREATED: &str = "claude-chat-aaaaaaaa-bbbb-4ccc-8ddd-eeeeffff0000";
const CODING: &str = "coding-session";
const NEW: &str = "claude-chat-new";

fn session(id: &str, title: &str) -> v1::AgentSession {
    v1::AgentSession {
        id: id.into(),
        provider: "claude".into(),
        title: title.into(),
        state: "idle".into(),
        controller: "native_app".into(),
        mode: if id == CODING {
            String::new()
        } else {
            "chat".into()
        },
        ..Default::default()
    }
}

fn message(id: &str, role: &str, text: &str) -> v1::AgentMessage {
    v1::AgentMessage {
        id: id.into(),
        role: role.into(),
        kind: "text".into(),
        text: text.into(),
        ..Default::default()
    }
}

#[derive(Default)]
struct Server {
    chat_service: bool,
    legacy_catalog: bool,
    legacy_locked: bool,
    locked_load: bool,
    prepares: AtomicUsize,
    coding_options: bool,
    catalogs: AtomicUsize,
    full_catalogs: AtomicUsize,
    opens: AtomicUsize,
    sends: Mutex<Vec<String>>,
    grow: Notify,
    /// An agent operation holds Claude on this Mac.
    agent_busy: std::sync::atomic::AtomicBool,
    /// An earlier send has no provider receipt: sends are refused until a
    /// send releases this token.
    unconfirmed: Mutex<Option<String>>,
    releases: Mutex<Vec<String>>,
}

impl Handler for Server {
    fn call(&self, _: Context, request: v1::request::Req, events: EventSink) -> HandlerFuture<'_> {
        use v1::{request::Req, response::Resp};
        Box::pin(async move {
            match request {
                Req::DeviceInfo(_) => Resp::DeviceInfo(v1::DeviceInfo {
                    device_id: "local".into(),
                    services: if self.chat_service {
                        let mut services = vec![
                            "agent".into(),
                            "agent.chat".into(),
                            "agent.chat.options".into(),
                        ];
                        if !self.legacy_locked {
                            services.push("agent.chat.locked.v1".into());
                        }
                        if self.coding_options {
                            services.push("agent.options.v1".into());
                        }
                        if !self.legacy_catalog {
                            services.push("agent.chat.catalog.v1".into());
                        }
                        services
                    } else {
                        vec!["agent".into()]
                    },
                    ..Default::default()
                }),
                Req::NetworkStatus(_) => Resp::NetworkStatus(v1::NetworkStatus {
                    has_network: true,
                    self_id: "local".into(),
                    members: vec![v1::MemberView {
                        device_id: "local".into(),
                        name: "Studio".into(),
                        kind: "desktop".into(),
                        is_self: true,
                        ..Default::default()
                    }],
                    ..Default::default()
                }),
                Req::AgentList(_) => Resp::AgentSessions(v1::AgentSessionPage {
                    sessions: vec![session(CODING, "Coding")],
                }),
                Req::PolicyGet(_) => Resp::Policy(v1::Policy::default()),
                Req::NetworkPending(_) => Resp::PendingJoins(v1::PendingJoins::default()),
                Req::DeviceWatch(_) => {
                    events.cancelled().cancelled().await;
                    Resp::Ack(v1::Ack {})
                }
                Req::AgentChats(r) => {
                    assert!(matches!(r.provider.as_str(), "claude" | "codex"));
                    self.catalogs.fetch_add(1, Ordering::SeqCst);
                    if r.full_history {
                        self.full_catalogs.fetch_add(1, Ordering::SeqCst);
                        if self.locked_load {
                            return Resp::Error(v1::Error {
                                code: v1::error::Code::LockedUseRequired as i32,
                                message: "Mac locked".into(),
                            });
                        }
                    }
                    Resp::AgentChats(v1::AgentChatCatalog {
                        provider: r.provider,
                        app_running: true,
                        chat_mode: true,
                        chats: vec![
                            v1::AgentChat {
                                id: LISTED.into(),
                                title: "Kyoto trip".into(),
                                current: true,
                            },
                            v1::AgentChat {
                                id: "claude-chat-11111111-2222-4333-8444-555555555555".into(),
                                title: "Borrow checker".into(),
                                current: false,
                            },
                        ],
                        current: LISTED.into(),
                        new_session: NEW.into(),
                        busy: if self.agent_busy.load(Ordering::SeqCst) {
                            "agent".into()
                        } else {
                            String::new()
                        },
                        ..Default::default()
                    })
                }
                Req::LockedUsePrepare(r) => {
                    self.prepares.fetch_add(1, Ordering::SeqCst);
                    let operation = r.operation.unwrap();
                    assert_eq!(operation.provider_queue, "claude");
                    assert!(muster_proto::locked_use::turn(&operation).is_some());
                    assert!(matches!(
                        operation.operation,
                        Some(v1::locked_use_operation::Operation::ChatLoad(_))
                    ));
                    Resp::Error(v1::Error {
                        code: v1::error::Code::Unauthorized as i32,
                        message: "Locked Use denied".into(),
                    })
                }
                Req::AgentChatOpen(_) if self.agent_busy.load(Ordering::SeqCst) => {
                    self.opens.fetch_add(1, Ordering::SeqCst);
                    Resp::Error(v1::Error {
                        code: v1::error::Code::Busy as i32,
                        message: "Studio: Claude is in use by an agent operation (send 1234)"
                            .into(),
                    })
                }
                Req::AgentChatOpen(r) => {
                    self.opens.fetch_add(1, Ordering::SeqCst);
                    if r.chat.is_empty() {
                        Resp::AgentSession(session(NEW, ""))
                    } else {
                        assert_eq!(r.chat, LISTED);
                        Resp::AgentSession(session(LISTED, "Kyoto trip"))
                    }
                }
                Req::AgentWatch(r) => {
                    let title = if r.session == LISTED {
                        "Kyoto trip"
                    } else {
                        ""
                    };
                    let _ = events
                        .send(v1::event::Ev::AgentEvent(v1::AgentEvent {
                            ev: Some(v1::agent_event::Ev::State(session(&r.session, title))),
                        }))
                        .await;
                    if r.session == LISTED {
                        self.grow.notified().await;
                        let _ = events
                            .send(v1::event::Ev::AgentEvent(v1::AgentEvent {
                                ev: Some(v1::agent_event::Ev::Message(message(
                                    "ax0-00001",
                                    "assistant",
                                    "Day one: Fushimi Inari.",
                                ))),
                            }))
                            .await;
                    }
                    events.cancelled().cancelled().await;
                    Resp::Ack(v1::Ack {})
                }
                Req::AgentMessages(r) => Resp::AgentMessages(v1::AgentMessagePage {
                    messages: if r.session == LISTED {
                        vec![
                            message("ax0-00000", "user", "Plan Kyoto"),
                            message("ax0-00001", "assistant", "Day"),
                        ]
                    } else if r.session == CREATED {
                        vec![message("ax0-00000", "user", "hello")]
                    } else {
                        Vec::new()
                    },
                    session: r.session,
                    has_more: false,
                    ..Default::default()
                }),
                Req::AgentChatOptions(r) => Resp::AgentChatOptions(options(&r.session, "Opus 5.5")),
                Req::AgentChatSetOptions(r) if r.model == "not-offered" => Resp::Error(v1::Error {
                    code: v1::error::Code::Invalid as i32,
                    message: "refresh choices".into(),
                }),
                Req::AgentChatSetOptions(r) => {
                    let mut result = options(&r.session, &r.model);
                    if !r.effort.is_empty() {
                        result.effort = r.effort;
                        for e in &mut result.efforts {
                            e.selected = e.name == result.effort;
                        }
                    }
                    Resp::AgentChatOptions(result)
                }
                Req::AgentSend(r) => {
                    self.sends.lock().unwrap().push(r.session.clone());
                    let mut unconfirmed = self.unconfirmed.lock().unwrap();
                    if !r.release_unconfirmed.is_empty() {
                        self.releases
                            .lock()
                            .unwrap()
                            .push(r.release_unconfirmed.clone());
                        if unconfirmed.as_ref() == Some(&r.release_unconfirmed) {
                            *unconfirmed = None;
                        }
                    }
                    if let Some(token) = unconfirmed.as_ref() {
                        return Resp::OpResult(v1::OpResult {
                            status: v1::op_result::Status::Rejected as i32,
                            message: "conflict: Not sent: the earlier send app-1-4 at 2026-10-04T03:39:14Z to this session has no provider receipt yet.".into(),
                            unconfirmed: token.clone(),
                            ..Default::default()
                        });
                    }
                    Resp::OpResult(v1::OpResult {
                        status: v1::op_result::Status::Applied as i32,
                        result_id: if r.session == NEW {
                            CREATED.into()
                        } else {
                            r.session
                        },
                        ..Default::default()
                    })
                }
                _ => Resp::Ack(v1::Ack {}),
            }
        })
    }
}

fn options(session: &str, selected: &str) -> v1::AgentChatOptions {
    v1::AgentChatOptions {
        session: session.into(),
        models: ["Opus 5.5", "Sonnet 5.5"]
            .into_iter()
            .map(|name| v1::AgentChatOption {
                name: name.into(),
                detail: String::new(),
                selected: name == selected,
            })
            .collect(),
        effort: "Medium".into(),
        efforts: ["Low", "Medium", "High"]
            .into_iter()
            .map(|name| v1::AgentChatOption {
                name: name.into(),
                selected: name == "Medium",
                ..Default::default()
            })
            .collect(),
        note: String::new(),
    }
}

async fn start(server: Arc<Server>) -> (AppCore, Arc<TestShell>, tempfile::TempDir) {
    let dir = tempfile::tempdir().unwrap();
    let socket = dir.path().join("rpc.sock");
    let listener = tokio::net::UnixListener::bind(&socket).unwrap();
    tokio::spawn(async move {
        loop {
            let Ok((stream, _)) = listener.accept().await else {
                return;
            };
            let handler = server.clone();
            tokio::spawn(muster_rpc::serve(stream, Principal::Local, handler));
        }
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
        .wait_for("the local desktop", |v| !v.service_devices.is_empty())
        .await;
    (core, shell, dir)
}

fn intent(core: &AppCore, it: app::intent::It) {
    core.intent(app::Intent { it: Some(it) });
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn list_open_grow_and_follow_a_new_chat() {
    let server = Arc::new(Server {
        chat_service: true,
        ..Default::default()
    });
    let (core, shell, _dir) = start(server.clone()).await;

    intent(
        &core,
        app::intent::It::OpenDesktopChats(app::OpenDesktopChats {
            provider: "claude".into(),
        }),
    );
    let view = shell
        .wait_for("the chat list", |v| {
            v.screen == app::Screen::DesktopChats as i32
                && v.desktop_chats
                    .as_ref()
                    .and_then(|d| d.catalog.as_ref())
                    .is_some_and(|c| c.chats.len() == 2)
        })
        .await;
    // The core picked the Mac; the Chat tab is a root like Agents.
    let chats = view.desktop_chats.unwrap();
    assert_eq!(chats.device_name, "Studio");
    assert_eq!(chats.device_id, "local");
    assert!(!chats.loading);
    assert_eq!(server.full_catalogs.load(Ordering::SeqCst), 1);
    assert!(!view.can_go_back);
    assert_eq!(view.workspace, app::Screen::DesktopChats as i32);

    // A listed conversation is shown on the desktop first, then followed.
    intent(
        &core,
        app::intent::It::DesktopChatOpen(app::DesktopChatOpen {
            chat: LISTED.into(),
            desktop: false,
        }),
    );
    shell
        .wait_for("the listed conversation", |v| {
            v.screen == app::Screen::Chat as i32
                && v.chat.as_ref().is_some_and(|c| {
                    c.session.as_ref().is_some_and(|s| s.id == LISTED) && c.message_count == 2
                })
        })
        .await;
    assert_eq!(server.opens.load(Ordering::SeqCst), 1);
    server.grow.notify_one();
    let view = shell
        .wait_for("the reply growing in place", |v| {
            v.chat.as_ref().is_some_and(|c| {
                c.message_count == 2 && common::messages(c)[1].text == "Day one: Fushimi Inari."
            })
        })
        .await;
    // A chat opened from the Chat tab still belongs to it (a Mac shows the
    // list beside it).
    assert_eq!(view.workspace, app::Screen::DesktopChats as i32);
    let chat = view.chat.unwrap();
    assert_eq!(chat.session.unwrap().mode, "chat");
    // Desktop chats take text only.
    assert_eq!(chat.attachment_count_limit, 0);

    // Model choices are read from the Mac on demand, then chosen by name.
    intent(
        &core,
        app::intent::It::ChatOptionsLoad(app::ChatOptionsLoad {}),
    );
    shell
        .wait_for("the model choices", |v| {
            v.chat.as_ref().is_some_and(|c| {
                !c.chat_options_busy
                    && c.chat_options
                        .as_ref()
                        .is_some_and(|o| o.models.len() == 2 && o.models[0].selected)
            })
        })
        .await;
    intent(
        &core,
        app::intent::It::ChatSetOptions(app::ChatSetOptions {
            model: "Sonnet 5.5".into(),
            effort: String::new(),
        }),
    );
    let view = shell
        .wait_for("the chosen model", |v| {
            v.chat.as_ref().is_some_and(|c| {
                c.chat_options.as_ref().is_some_and(|o| {
                    o.models
                        .iter()
                        .any(|m| m.selected && m.name == "Sonnet 5.5")
                })
            })
        })
        .await;
    assert_eq!(view.chat.unwrap().chat_options.unwrap().effort, "Medium");

    // Back to the list reloads it.
    let before = server.catalogs.load(Ordering::SeqCst);
    intent(
        &core,
        app::intent::It::Navigate(app::Navigate {
            screen: 0,
            back: true,
            ..Default::default()
        }),
    );
    shell
        .wait_for("the list again", |v| {
            v.screen == app::Screen::DesktopChats as i32
        })
        .await;
    tokio::time::timeout(Duration::from_secs(5), async {
        while server.catalogs.load(Ordering::SeqCst) == before {
            tokio::time::sleep(Duration::from_millis(20)).await;
        }
    })
    .await
    .expect("catalog reloaded");

    // A new chat opens locally; the desktop changes only on the first send.
    intent(
        &core,
        app::intent::It::DesktopChatOpen(app::DesktopChatOpen {
            chat: String::new(),
            desktop: false,
        }),
    );
    shell
        .wait_for("the new chat", |v| {
            v.chat
                .as_ref()
                .is_some_and(|c| c.session.as_ref().is_some_and(|s| s.id == NEW))
        })
        .await;
    assert_eq!(server.opens.load(Ordering::SeqCst), 1);
    intent(
        &core,
        app::intent::It::AgentSend(app::AgentSend {
            text: "hello".into(),
            attachments: Vec::new(),
            draft_id: "draft-1".into(),
            release_unconfirmed: String::new(),
        }),
    );
    shell
        .wait_for("the conversation the new chat became", |v| {
            v.chat.as_ref().is_some_and(|c| {
                c.session.as_ref().is_some_and(|s| s.id == CREATED) && c.message_count == 1
            })
        })
        .await;
    assert_eq!(server.sends.lock().unwrap().as_slice(), [NEW]);

    // Switching the Mac to Chat mode right away asks the desktop.
    intent(
        &core,
        app::intent::It::OpenDesktopChats(app::OpenDesktopChats {
            provider: "claude".into(),
        }),
    );
    shell
        .wait_for("the list", |v| v.screen == app::Screen::DesktopChats as i32)
        .await;
    intent(
        &core,
        app::intent::It::DesktopChatOpen(app::DesktopChatOpen {
            chat: String::new(),
            desktop: true,
        }),
    );
    shell
        .wait_for("the new chat shown on the desktop", |v| {
            v.screen == app::Screen::Chat as i32
                && v.chat
                    .as_ref()
                    .is_some_and(|c| c.session.as_ref().is_some_and(|s| s.id == NEW))
        })
        .await;
    assert_eq!(server.opens.load(Ordering::SeqCst), 2);
    core.backend().local().close();
    core.shutdown();
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn a_desktop_without_chat_support_says_to_update() {
    let server = Arc::new(Server::default());
    let (core, shell, _dir) = start(server.clone()).await;
    intent(
        &core,
        app::intent::It::OpenDesktopChats(app::OpenDesktopChats {
            provider: "codex".into(),
        }),
    );
    let view = shell
        .wait_for("the update hint", |v| {
            v.desktop_chats
                .as_ref()
                .is_some_and(|d| !d.loading && !d.error.is_empty())
        })
        .await;
    assert!(view.desktop_chats.unwrap().error.contains("update Muster"));
    assert_eq!(server.catalogs.load(Ordering::SeqCst), 0);
    core.backend().local().close();
    core.shutdown();
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn a_mac_an_agent_holds_is_left_and_reported_when_no_other_can_help() {
    let server = Arc::new(Server {
        chat_service: true,
        ..Default::default()
    });
    let (core, shell, _dir) = start(server.clone()).await;
    intent(
        &core,
        app::intent::It::OpenDesktopChats(app::OpenDesktopChats {
            provider: "claude".into(),
        }),
    );
    let view = shell
        .wait_for("the chat list", |v| {
            v.desktop_chats
                .as_ref()
                .is_some_and(|d| d.catalog.as_ref().is_some_and(|c| c.chats.len() == 2))
        })
        .await;
    // An explicit provider choice stays selected.
    assert_eq!(view.desktop_chats.unwrap().provider, "claude");

    // An agent operation takes Claude between the list and the tap: the
    // open fails at once, the core looks for another Mac, and with none
    // left says why instead of waiting.
    server.agent_busy.store(true, Ordering::SeqCst);
    intent(
        &core,
        app::intent::It::DesktopChatOpen(app::DesktopChatOpen {
            chat: LISTED.into(),
            desktop: false,
        }),
    );
    let view = shell
        .wait_for("no Mac left", |v| {
            v.desktop_chats
                .as_ref()
                .is_some_and(|d| !d.loading && !d.error.is_empty())
        })
        .await;
    let chats = view.desktop_chats.unwrap();
    assert!(
        chats
            .error
            .contains("Studio: Claude is in use by an agent operation"),
        "{}",
        chats.error
    );
    assert!(chats.device_id.is_empty());
    assert_eq!(view.screen, app::Screen::DesktopChats as i32);
    assert_eq!(server.opens.load(Ordering::SeqCst), 1);

    // Once the agent is done, the next refresh picks the Mac again.
    server.agent_busy.store(false, Ordering::SeqCst);
    intent(&core, app::intent::It::Refresh(app::Refresh {}));
    shell
        .wait_for("the Mac picked again", |v| {
            v.desktop_chats
                .as_ref()
                .is_some_and(|d| d.device_id == "local" && d.error.is_empty())
        })
        .await;
    core.backend().local().close();
    core.shutdown();
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn switching_to_chat_uses_local_source_without_full_history_navigation() {
    let server = Arc::new(Server {
        chat_service: true,
        ..Default::default()
    });
    let (core, shell, _dir) = start(server.clone()).await;
    for expected in 1..=2 {
        intent(
            &core,
            app::intent::It::Navigate(app::Navigate {
                screen: app::Screen::DesktopChats as i32,
                back: false,
                ..Default::default()
            }),
        );
        shell
            .wait_for("local catalog refreshed", |v| {
                v.screen == app::Screen::DesktopChats as i32
                    && v.desktop_chats
                        .as_ref()
                        .is_some_and(|d| !d.loading && d.catalog.is_some())
                    && server.catalogs.load(Ordering::SeqCst) >= expected
                    && server.full_catalogs.load(Ordering::SeqCst) == 0
            })
            .await;
        intent(
            &core,
            app::intent::It::Navigate(app::Navigate {
                screen: app::Screen::Agents as i32,
                back: false,
                ..Default::default()
            }),
        );
        shell
            .wait_for("agent tab", |v| v.screen == app::Screen::Agents as i32)
            .await;
    }
    core.backend().local().close();
    core.shutdown();
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn chatgpt_is_first_and_default_and_uses_the_probe_catalog() {
    let server = Arc::new(Server {
        chat_service: true,
        ..Default::default()
    });
    let (core, shell, _dir) = start(server.clone()).await;
    intent(
        &core,
        app::intent::It::OpenDesktopChats(app::OpenDesktopChats {
            provider: String::new(),
        }),
    );
    let view = shell
        .wait_for("default ChatGPT catalog", |v| {
            v.desktop_chats
                .as_ref()
                .is_some_and(|d| !d.loading && d.catalog.is_some())
        })
        .await;
    let chats = view.desktop_chats.unwrap();
    assert_eq!(chats.provider, "codex");
    let choices: Vec<_> = chats
        .choices
        .iter()
        .map(|c| (c.id.as_str(), c.label.as_str()))
        .collect();
    assert_eq!(choices, [("codex", "ChatGPT"), ("claude", "Claude")]);
    assert_eq!(server.catalogs.load(Ordering::SeqCst), 1);
    assert_eq!(server.full_catalogs.load(Ordering::SeqCst), 0);
    core.backend().local().close();
    core.shutdown();
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn old_daemon_cannot_leak_untyped_codex_rows_into_chatgpt() {
    let server = Arc::new(Server {
        chat_service: true,
        legacy_catalog: true,
        ..Default::default()
    });
    let (core, shell, _dir) = start(server.clone()).await;
    intent(
        &core,
        app::intent::It::OpenDesktopChats(app::OpenDesktopChats {
            provider: String::new(),
        }),
    );
    let view = shell
        .wait_for("typed catalog update hint", |v| {
            v.desktop_chats
                .as_ref()
                .is_some_and(|d| !d.loading && !d.error.is_empty())
        })
        .await;
    assert!(
        view.desktop_chats
            .unwrap()
            .error
            .contains("typed ChatGPT catalog")
    );
    assert_eq!(server.catalogs.load(Ordering::SeqCst), 0);
    core.backend().local().close();
    core.shutdown();
}

/// A send refused behind an unconfirmed earlier one shows that refusal in
/// the conversation with its token, not as a toast. The send that releases
/// it names that token; a stale release sends nothing.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn an_unconfirmed_earlier_send_is_released_from_the_conversation() {
    let server = Arc::new(Server {
        chat_service: true,
        unconfirmed: Mutex::new(Some("app-1-4".into())),
        ..Default::default()
    });
    let (core, shell, _dir) = start(server.clone()).await;
    intent(
        &core,
        app::intent::It::OpenDesktopChats(app::OpenDesktopChats {
            provider: "claude".into(),
        }),
    );
    shell
        .wait_for("the chat list", |v| {
            v.screen == app::Screen::DesktopChats as i32
        })
        .await;
    intent(
        &core,
        app::intent::It::DesktopChatOpen(app::DesktopChatOpen {
            chat: LISTED.into(),
            desktop: false,
        }),
    );
    shell
        .wait_for("the conversation", |v| {
            v.chat
                .as_ref()
                .is_some_and(|c| c.session.as_ref().is_some_and(|s| s.id == LISTED))
        })
        .await;
    let send = |draft: &str, release: &str| {
        intent(
            &core,
            app::intent::It::AgentSend(app::AgentSend {
                text: "hello".into(),
                attachments: Vec::new(),
                draft_id: draft.into(),
                release_unconfirmed: release.into(),
            }),
        )
    };
    send("draft-1", "");
    let view = shell
        .wait_for("the refusal", |v| {
            v.chat
                .as_ref()
                .is_some_and(|c| !c.sending && c.unconfirmed.is_some())
        })
        .await;
    let refusal = view.chat.unwrap().unconfirmed.unwrap();
    assert_eq!(refusal.token, "app-1-4");
    assert!(refusal.message.contains("no provider receipt"));
    assert!(view.toasts.iter().all(|t| !t.text.contains("Not sent")));

    // Only the refusal shown is released; then the send goes through.
    send("draft-2", "app-1-3");
    send("draft-3", "app-1-4");
    let view = shell
        .wait_for("the released send", |v| {
            v.chat
                .as_ref()
                .is_some_and(|c| c.sent_draft_id == "draft-3")
        })
        .await;
    assert!(view.chat.unwrap().unconfirmed.is_none());
    assert_eq!(server.releases.lock().unwrap().as_slice(), ["app-1-4"]);
    assert_eq!(server.sends.lock().unwrap().len(), 2);
    core.backend().local().close();
    core.shutdown();
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn coding_session_changes_model_and_effort_and_requires_new_service() {
    for supported in [false, true] {
        let (core, shell, _dir) = start(Arc::new(Server {
            chat_service: true,
            coding_options: supported,
            ..Default::default()
        }))
        .await;
        intent(
            &core,
            app::intent::It::OpenAgents(app::OpenAgents {
                device_id: "local".into(),
            }),
        );
        shell
            .wait_for("coding session catalog", |v| {
                v.agents
                    .as_ref()
                    .is_some_and(|a| a.sessions.iter().any(|s| s.id == CODING))
            })
            .await;
        intent(
            &core,
            app::intent::It::OpenAgent(app::OpenAgent {
                session: CODING.into(),
                device_id: "local".into(),
            }),
        );
        shell
            .wait_for("coding session options entry", |v| {
                v.chat.as_ref().is_some_and(|c| {
                    c.can_choose_model
                        && c.session
                            .as_ref()
                            .is_some_and(|s| s.id == CODING && s.mode.is_empty())
                })
            })
            .await;
        intent(
            &core,
            app::intent::It::ChatOptionsLoad(app::ChatOptionsLoad {}),
        );
        if !supported {
            shell
                .wait_for("older host asks for update", |v| {
                    v.chat.as_ref().is_some_and(|c| {
                        !c.chat_options_busy && c.chat_options_error.contains("update Muster")
                    })
                })
                .await;
            continue;
        }
        shell
            .wait_for("coding options from host", |v| {
                v.chat.as_ref().is_some_and(|c| {
                    c.chat_options
                        .as_ref()
                        .is_some_and(|o| o.session == CODING && o.efforts.len() == 3)
                })
            })
            .await;
        intent(
            &core,
            app::intent::It::ChatSetOptions(app::ChatSetOptions {
                model: "Sonnet 5.5".into(),
                effort: "High".into(),
            }),
        );
        shell
            .wait_for("confirmed coding model and effort", |v| {
                v.chat.as_ref().is_some_and(|c| {
                    !c.chat_options_busy
                        && c.chat_options.as_ref().is_some_and(|o| {
                            o.models
                                .iter()
                                .any(|m| m.selected && m.name == "Sonnet 5.5")
                                && o.efforts.iter().any(|e| e.selected && e.name == "High")
                        })
                })
            })
            .await;
        intent(
            &core,
            app::intent::It::ChatSetOptions(app::ChatSetOptions {
                model: "not-offered".into(),
                effort: String::new(),
            }),
        );
        shell
            .wait_for("failed write clears stale selections", |v| {
                v.chat.as_ref().is_some_and(|c| {
                    !c.chat_options_busy
                        && c.chat_options.is_none()
                        && c.chat_options_error.contains("refresh choices")
                })
            })
            .await;
    }
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn chat_loading_rejects_old_hosts_and_finishes_after_locked_use_denial() {
    for legacy in [true, false] {
        let server = Arc::new(Server {
            chat_service: true,
            legacy_locked: legacy,
            locked_load: true,
            ..Default::default()
        });
        let (core, shell, _dir) = start(server.clone()).await;
        intent(
            &core,
            app::intent::It::OpenDesktopChats(app::OpenDesktopChats {
                provider: "claude".into(),
            }),
        );
        shell
            .wait_for("loading stopped with a useful authorization error", |v| {
                v.desktop_chats.as_ref().is_some_and(|d| {
                    !d.loading
                        && d.error.contains(if legacy {
                            "update Muster"
                        } else {
                            "Locked Use denied"
                        })
                })
            })
            .await;
        assert_eq!(
            server.full_catalogs.load(Ordering::SeqCst),
            usize::from(!legacy)
        );
        assert_eq!(server.prepares.load(Ordering::SeqCst), usize::from(!legacy));
        assert_eq!(server.opens.load(Ordering::SeqCst), 0);
        core.backend().local().close();
        core.shutdown();
    }
}
