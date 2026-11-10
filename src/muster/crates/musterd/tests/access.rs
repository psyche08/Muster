//! Per-provider Agents access over a real link: a phone granted Agents for
//! Codex only sees, reads and drives Codex sessions on the desktop, is
//! refused Claude sessions and terminals, and its open watch ends once the
//! admin switches Agents off.

use std::io::Write;
use std::time::{Duration, Instant};

use muster_core::roster::DeviceKind;
use muster_proto::v1::{self, request::Req, response::Resp};
use muster_rpc::Client;
use musterd::{Daemon, DaemonConfig};

const CODEX: &str = "0f8d6b3a-2f0e-4c53-9d1a-3a4b5c6d7e8f";
const CLAUDE: &str = "a1b2c3d4-e5f6-4a7b-8c9d-0e1f2a3b4c5d";

async fn relay() -> String {
    let server = muster_relay::RelayServer::bind(
        "127.0.0.1:0".parse().unwrap(),
        muster_relay::RelayOptions::default(),
    )
    .await
    .unwrap();
    let addr = server.udp_addr().unwrap();
    tokio::spawn(async move {
        let _ = server.serve().await;
    });
    addr.to_string()
}

fn config(dir: &std::path::Path, relay: Option<String>, name: &str) -> DaemonConfig {
    let mut config = DaemonConfig::new(dir);
    config.bind = "127.0.0.1:0".parse().unwrap();
    config.relay = relay;
    config.name = name.into();
    config.kind = DeviceKind::Desktop;
    config.allow_loopback = true;
    config.agent = Some(muster_agent::AgentConfig {
        codex_home: Some(dir.join("codex")),
        claude_home: Some(dir.join("claude")),
        catdesk_home: None,
        deepseek_home: None,
        claude_desktop: None,
        poll: Duration::from_millis(50),
        rescan: Duration::from_millis(200),
        ..Default::default()
    });
    config.terminal = Some(muster_terminal::TerminalKind::Pty(
        muster_terminal::pty::PtyConfig {
            shell: "/bin/sh".into(),
            cols: 40,
            rows: 8,
            ..Default::default()
        },
    ));
    config
}

/// One Codex and one Claude transcript under `dir`; returns the Codex file.
fn transcripts(dir: &std::path::Path) -> std::path::PathBuf {
    let day = dir.join("codex/sessions/2026/09/29");
    std::fs::create_dir_all(&day).unwrap();
    let codex = day.join(format!("rollout-2026-09-29T10-00-00-{CODEX}.jsonl"));
    let mut file = std::fs::File::create(&codex).unwrap();
    writeln!(
        file,
        r#"{{"type":"session_meta","payload":{{"id":"{CODEX}","cwd":"{}"}}}}"#,
        dir.display()
    )
    .unwrap();
    writeln!(file, r#"{{"type":"turn_context","payload":{{}}}}"#).unwrap();
    append_codex(&codex, "hello codex");
    let project = dir.join("claude/projects/-repo");
    std::fs::create_dir_all(&project).unwrap();
    std::fs::write(
        project.join(format!("{CLAUDE}.jsonl")),
        format!(
            "{{\"type\":\"user\",\"cwd\":\"{}\",\"message\":{{\"role\":\"user\",\"content\":\"hello claude\"}}}}\n",
            dir.display()
        ),
    )
    .unwrap();
    codex
}

fn append_codex(path: &std::path::Path, text: &str) {
    let mut file = std::fs::OpenOptions::new().append(true).open(path).unwrap();
    writeln!(
        file,
        r#"{{"type":"response_item","payload":{{"type":"message","role":"user","content":[{{"type":"input_text","text":"{text}"}}]}}}}"#
    )
    .unwrap();
}

async fn client(daemon: &Daemon) -> Client {
    Client::new(
        tokio::net::UnixStream::connect(daemon.socket_path())
            .await
            .unwrap(),
    )
}

fn unauthorized<T: std::fmt::Debug>(result: &Result<T, muster_rpc::Error>) -> bool {
    matches!(
        result,
        Err(muster_rpc::Error::Remote {
            code: v1::error::Code::Unauthorized,
            ..
        })
    )
}

async fn sessions(remote: &Client, provider: &str) -> Result<Vec<String>, muster_rpc::Error> {
    match remote
        .call(Req::AgentList(v1::AgentListRequest {
            limit: 0,
            provider: provider.into(),
            ..Default::default()
        }))
        .await?
    {
        Resp::AgentSessions(page) => Ok(page.sessions.into_iter().map(|s| s.id).collect()),
        other => panic!("{other:?}"),
    }
}

async fn watch_page(watch: &mut muster_rpc::Call, timeout: Duration) -> v1::AgentSessionPage {
    let event = tokio::time::timeout(timeout, watch.events.recv())
        .await
        .expect("catalog deadline")
        .expect("catalog stream");
    match event {
        v1::event::Ev::AgentEvent(v1::AgentEvent {
            ev: Some(v1::agent_event::Ev::ListChanged(page)),
        }) => page,
        _ => panic!("expected a catalog page"),
    }
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn agents_are_granted_per_provider_and_withdrawn_from_open_watches() {
    let _ = env_logger::Builder::from_env(env_logger::Env::default().default_filter_or("info"))
        .is_test(true)
        .try_init();
    let relay = relay().await;
    let dir_a = tempfile::tempdir().unwrap();
    let dir_b = tempfile::tempdir().unwrap();
    let codex_file = transcripts(dir_a.path());
    let mut alpha = config(dir_a.path(), Some(relay.clone()), "alpha");
    alpha.agent.as_mut().unwrap().cli_runtime = Some(
        muster_agent::cli::runtime::NativeRuntime::with_programs(
            &dir_a.path().join("orca"),
            [("codex".into(), std::path::PathBuf::from("/bin/true"))]
                .into_iter()
                .collect(),
        )
        .unwrap(),
    );
    let a = Daemon::start_with(alpha, std::sync::Arc::new(musterd::DefaultPlatform), None)
        .await
        .unwrap();
    let b = Daemon::start_with(
        config(dir_b.path(), None, "bravo"),
        std::sync::Arc::new(musterd::DefaultPlatform),
        None,
    )
    .await
    .unwrap();
    let ca = client(&a).await;
    let cb = client(&b).await;

    // A creates the network and invites; B joins as a phone.
    ca.call(Req::NetworkCreate(v1::NetworkCreateRequest {
        name: "home".into(),
        vnet_cidr: String::new(),
    }))
    .await
    .unwrap();
    let Resp::InviteText(invite) = ca
        .call(Req::NetworkInvite(v1::NetworkInviteRequest {
            ttl_secs: 120,
        }))
        .await
        .unwrap()
    else {
        panic!()
    };
    let join = tokio::spawn(async move {
        let call = cb
            .start(Req::NetworkJoin(v1::NetworkJoinRequest {
                invite_text: invite.text,
                device_name: "bravo".into(),
                device_kind: "phone".into(),
            }))
            .await
            .unwrap();
        call.finish().await
    });
    let started = Instant::now();
    let pending = loop {
        let Resp::PendingJoins(p) = ca
            .call(Req::NetworkPending(v1::NetworkPendingRequest {}))
            .await
            .unwrap()
        else {
            panic!()
        };
        if let Some(join) = p.joins.into_iter().next() {
            break join;
        }
        assert!(started.elapsed() < Duration::from_secs(20), "no join");
        tokio::time::sleep(Duration::from_millis(100)).await;
    };
    // Agents on with Codex only; Chats' provider without its switch; no terminals.
    ca.call(Req::NetworkDecide(v1::NetworkDecideRequest {
        device_id: pending.device_id.clone(),
        approve: true,
        scopes: vec![
            "agents".into(),
            "agents:codex".into(),
            "chats:claude".into(),
        ],
        reason: String::new(),
    }))
    .await
    .unwrap();
    tokio::time::timeout(Duration::from_secs(30), join)
        .await
        .unwrap()
        .unwrap()
        .unwrap();

    let (send, recv) = tokio::time::timeout(
        Duration::from_secs(15),
        b.node()
            .open(a.device_id(), musterd::SERVICE, musterd::SERVICE_VERSION),
    )
    .await
    .unwrap()
    .unwrap();
    let remote = Client::new(tokio::io::join(recv, send));
    for action in [
        v1::multica_request::Action::Check,
        v1::multica_request::Action::Install,
        v1::multica_request::Action::Save,
        v1::multica_request::Action::Progress,
    ] {
        assert!(unauthorized(
            &remote
                .call(Req::Multica(v1::MulticaRequest {
                    action: action as i32,
                    ..Default::default()
                }))
                .await
        ));
    }

    assert!(unauthorized(
        &remote
            .call(Req::MulticaMutation(v1::MulticaMutation::default()))
            .await
    ));
    assert!(unauthorized(
        &remote
            .call(Req::MulticaBrowse(v1::MulticaBrowseRequest::default()))
            .await
    ));

    assert!(unauthorized(
        &remote
            .call(Req::DesktopUpdateLease(v1::DesktopUpdateLeaseRequest {}))
            .await
    ));

    // A itself sees both providers once its scanner has found them.
    let started = Instant::now();
    while sessions(&ca, "").await.unwrap().len() < 2 {
        assert!(started.elapsed() < Duration::from_secs(10), "no sessions");
        tokio::time::sleep(Duration::from_millis(100)).await;
    }

    let orca_id = a
        .agent()
        .unwrap()
        .cli_source()
        .unwrap()
        .create(
            dir_a.path().to_str().unwrap(),
            "codex",
            "create-local",
            &Default::default(),
        )
        .await
        .unwrap();
    assert!(unauthorized(&sessions(&remote, "orca").await));
    assert!(unauthorized(
        &remote
            .call(Req::OrcaCreate(v1::OrcaCreateRequest {
                op_id: "denied-create".into(),
                directory: dir_a.path().display().to_string(),
                agent: "codex".into(),
                env: Default::default(),
            }))
            .await
    ));
    // A desktop session needs Agent control of its provider (ADR 0150):
    // Claude is refused before the request is read; Codex passes
    // authorization and is refused only for its (empty) first message.
    let create = |provider: &str| {
        Req::AgentNativeCreate(v1::AgentNativeCreateRequest {
            op_id: format!("remote-create-{provider}"),
            provider: provider.into(),
            directory: dir_a.path().display().to_string(),
            prompt: String::new(),
        })
    };
    assert!(unauthorized(&remote.call(create("claude")).await));
    let codex = remote.call(create("codex")).await;
    assert!(codex.is_err() && !unauthorized(&codex), "{codex:?}");
    assert!(unauthorized(
        &remote
            .call(Req::AgentSend(v1::AgentSendRequest {
                op_id: "denied-orca-send".into(),
                session: orca_id,
                text: "must not run".into(),
                ..Default::default()
            }))
            .await
    ));
    assert_eq!(a.agent().unwrap().cli_source().unwrap().list().len(), 1);

    // Listing: Codex only, and asking for Claude by name is refused.
    assert_eq!(sessions(&remote, "").await.unwrap(), [CODEX]);
    assert_eq!(sessions(&remote, "codex").await.unwrap(), [CODEX]);
    // Search is the same provider-scoped read, including body results.
    let Resp::AgentSessions(search) = remote
        .call(Req::AgentList(v1::AgentListRequest {
            query: "hello".into(),
            limit: 100,
            ..Default::default()
        }))
        .await
        .unwrap()
    else {
        panic!("search response")
    };
    assert_eq!(search.query, "hello");
    assert_eq!(
        search
            .sessions
            .iter()
            .map(|s| s.id.as_str())
            .collect::<Vec<_>>(),
        [CODEX]
    );
    assert!(unauthorized(
        &remote
            .call(Req::AgentList(v1::AgentListRequest {
                provider: "claude".into(),
                query: "hello".into(),
                ..Default::default()
            }))
            .await
    ));

    assert!(unauthorized(&sessions(&remote, "claude").await));

    // The merged desktop-chat RPCs also honor their named provider before
    // any catalog enumeration, history traversal or application selection.
    for full_history in [false, true] {
        assert!(unauthorized(
            &remote
                .call(Req::AgentChats(v1::AgentChatsRequest {
                    provider: "claude".into(),
                    full_history,
                    ..Default::default()
                }))
                .await
        ));
    }
    assert!(unauthorized(
        &remote
            .call(Req::AgentChatOpen(v1::AgentChatOpenRequest {
                provider: "claude".into(),
                ..Default::default()
            }))
            .await
    ));

    // Reading follows the session's provider.
    let read = |session: &str| {
        remote.call(Req::AgentMessages(v1::AgentMessagesRequest {
            session: session.into(),
            tail: 10,
            before_id: String::new(),
            ..Default::default()
        }))
    };
    assert!(matches!(read(CODEX).await, Ok(Resp::AgentMessages(_))));
    assert!(unauthorized(&read(CLAUDE).await));
    assert!(unauthorized(
        &remote
            .call(Req::AgentFiles(v1::AgentFilesRequest {
                session: CLAUDE.into(),
                path: String::new(),
            }))
            .await
    ));

    // Control too: Claude is refused before anything runs; Codex passes
    // authorization and then fails for lack of a desktop app here.
    let send_to = |session: &str, op: &str| {
        remote.call(Req::AgentSend(v1::AgentSendRequest {
            op_id: op.into(),
            session: session.into(),
            text: "hi".into(),
            attachments: Vec::new(),
            release_unconfirmed: String::new(),
        }))
    };
    assert!(unauthorized(&send_to(CLAUDE, "op-claude").await));
    // An unlocked desktop uses ordinary provider authorization even when
    // temporary unlocking is disabled. No Locked Use switch is required.
    let codex_send = send_to(CODEX, "op-unlocked-policy-off").await;
    assert!(!unauthorized(&codex_send), "{codex_send:?}");

    // Terminals were not granted.
    assert!(unauthorized(
        &remote
            .call(Req::TerminalSessions(v1::TerminalSessionsRequest::default()))
            .await
    ));
    for request in [
        Req::TerminalCreate(v1::TerminalCreateRequest {
            source: "ghostty".into(),
            op_id: "denied-native-create".into(),
            ..Default::default()
        }),
        Req::TerminalAttach(v1::TerminalAttachRequest {
            session: "ghostty:fixture".into(),
            since_rev: 0,
        }),
        Req::TerminalHistory(v1::TerminalHistoryRequest {
            session: "ghostty:fixture".into(),
            lines: 10,
        }),
        Req::TerminalInput(v1::TerminalInputRequest {
            session: "ghostty:fixture".into(),
            bytes: b"never".to_vec(),
            op_id: "denied-native-input".into(),
        }),
        Req::TerminalResize(v1::TerminalResizeRequest {
            session: "ghostty:fixture".into(),
            cols: 80,
            rows: 24,
        }),
        Req::TerminalClose(v1::TerminalCloseRequest {
            session: "ghostty:fixture".into(),
        }),
    ] {
        assert!(
            unauthorized(&remote.call(request).await),
            "authorization must precede source lookup or native work"
        );
    }

    ca.call(Req::NetworkSetScopes(v1::NetworkSetScopesRequest {
        device_id: pending.device_id.clone(),
        scopes: vec![
            "agents".into(),
            "agents:codex".into(),
            "agents:claude".into(),
            "agents:catdesk".into(),
        ],
        role: "member".into(),
    }))
    .await
    .unwrap();
    let all_desktop = sessions(&remote, "").await.unwrap();
    assert!(all_desktop.contains(&CODEX.to_string()) && all_desktop.contains(&CLAUDE.to_string()));
    assert!(
        !all_desktop.iter().any(|id| id.starts_with("orca:")),
        "all Desktop scopes must not leak CLI sessions"
    );
    let mut catalog = remote
        .start(Req::AgentWatch(v1::AgentWatchRequest {
            catalog_only: true,
            ..Default::default()
        }))
        .await
        .unwrap();
    let initial = watch_page(&mut catalog, Duration::from_secs(3)).await;
    assert!(initial.sessions.iter().any(|s| s.id == CLAUDE));
    ca.call(Req::NetworkSetScopes(v1::NetworkSetScopesRequest {
        device_id: pending.device_id.clone(),
        scopes: vec!["agents".into(), "agents:codex".into()],
        role: "member".into(),
    }))
    .await
    .unwrap();

    // A narrower provider grant bypasses the ordinary five-second interval.
    let restricted = watch_page(&mut catalog, Duration::from_secs(2)).await;
    assert_eq!(restricted.sessions.len(), 1);
    assert_eq!(restricted.sessions[0].id, CODEX);
    catalog.cancel().await;

    // A watch of every session shows Codex only.
    let mut watch = remote
        .start(Req::AgentWatch(v1::AgentWatchRequest {
            session: String::new(),
            ..Default::default()
        }))
        .await
        .unwrap();
    let Some(v1::event::Ev::AgentEvent(v1::AgentEvent {
        ev: Some(v1::agent_event::Ev::ListChanged(page)),
    })) = tokio::time::timeout(Duration::from_secs(5), watch.events.recv())
        .await
        .unwrap()
    else {
        panic!("the watch starts with the list")
    };
    assert_eq!(
        page.sessions
            .iter()
            .map(|s| s.id.as_str())
            .collect::<Vec<_>>(),
        [CODEX]
    );

    // The admin switches Agents off but keeps the provider choice.
    ca.call(Req::NetworkSetScopes(v1::NetworkSetScopesRequest {
        device_id: pending.device_id.clone(),
        scopes: vec!["agents:codex".into()],
        role: String::new(),
    }))
    .await
    .unwrap();
    // The next change on A ends the open watch instead of reaching B.
    append_codex(&codex_file, "after the switch");
    let ended = tokio::time::timeout(Duration::from_secs(10), async {
        while let Some(ev) = watch.events.recv().await {
            assert!(
                !matches!(
                    ev,
                    v1::event::Ev::AgentEvent(v1::AgentEvent {
                        ev: Some(v1::agent_event::Ev::Message(_)),
                    })
                ),
                "a message crossed a withdrawn grant: {ev:?}"
            );
        }
        watch.finish().await
    })
    .await
    .expect("the watch ends");
    assert!(unauthorized(&ended), "{ended:?}");
    assert!(unauthorized(&sessions(&remote, "").await));
    assert!(unauthorized(&read(CODEX).await));

    // Ordinary Agent permission and network connectivity never imply Multica.
    let Resp::MulticaService(off) = remote
        .call(Req::MulticaService(v1::MulticaServiceRequest::default()))
        .await
        .unwrap()
    else {
        panic!("service")
    };
    assert!(!off.can_view && off.workspaces.is_empty() && !off.can_manage);
    let grants = vec![
        "multica.view".into(),
        "agents".into(),
        "agents:codex".into(),
    ];
    assert!(
        a.set_scopes(&pending.device_id, &grants, "").is_err(),
        "unchecked issuance is rejected"
    );
    ca.call(Req::NetworkSetScopes(v1::NetworkSetScopesRequest {
        device_id: pending.device_id.clone(),
        scopes: grants.clone(),
        role: String::new(),
    }))
    .await
    .unwrap();
    let Resp::MulticaService(off) = remote
        .call(Req::MulticaService(v1::MulticaServiceRequest::default()))
        .await
        .unwrap()
    else {
        panic!("service")
    };
    assert!(!off.can_view, "a grant does not turn on the host service");
    {
        let mut policy = a.policy().write().unwrap();
        policy.offer_multica = true;
        policy.multica_workspaces.insert("workspace-a".into());
    }
    let Resp::MulticaService(on) = remote
        .call(Req::MulticaService(v1::MulticaServiceRequest::default()))
        .await
        .unwrap()
    else {
        panic!("service")
    };
    assert!(on.can_view && !on.can_manage && !on.can_control);
    assert_eq!(on.workspaces, ["workspace-a"]);
    assert!(unauthorized(
        &remote
            .call(Req::MulticaService(v1::MulticaServiceRequest {
                action: 1,
                offered: true,
                workspaces: vec!["other".into()]
            }))
            .await
    ));
    assert!(unauthorized(
        &remote
            .call(Req::MulticaBrowse(v1::MulticaBrowseRequest {
                workspace_id: "not-shared".into(),
                ..Default::default()
            }))
            .await
    ));
    reserve_multica(&a, CODEX);
    assert!(sessions(&remote, "").await.unwrap().is_empty());
    assert!(unauthorized(&read(CODEX).await));
    a.policy().write().unwrap().offer_multica = false;
    let Resp::MulticaService(off) = remote
        .call(Req::MulticaService(v1::MulticaServiceRequest::default()))
        .await
        .unwrap()
    else {
        panic!("service")
    };
    assert!(!off.can_view && off.workspaces.is_empty());
    #[cfg(target_os = "macos")]
    {
        // Exercise the release command without covering the test machine.
        let release = Req::DeviceSetBlackout(v1::DeviceSetCoverRequest { on: false });
        assert!(unauthorized(&remote.call(release.clone()).await));
        a.set_scopes(&pending.device_id, &["locked.control".into()], "")
            .unwrap();
        a.policy().write().unwrap().allow_locked_control = false;
        assert!(unauthorized(&remote.call(release.clone()).await));
        a.policy().write().unwrap().allow_locked_control = true;
        assert!(matches!(remote.call(release).await.unwrap(), Resp::Ack(_)));
    }
    b.shutdown().await;
    assert!(
        a.set_scopes_checked(&pending.device_id, &grants, "")
            .await
            .is_err(),
        "an offline active member blocks new-scope issuance"
    );
}

fn reserve_multica(daemon: &Daemon, session: &str) {
    use muster_store::multica::{NativeSession, Reservation, Task};
    let agent = daemon.agent().unwrap();
    let identity = agent.native_lineage(session).unwrap().remove(0);
    let device = daemon.device_id().to_string();
    daemon
        .store()
        .multica_reserve(&Reservation {
            task: Task {
                device: device.clone(),
                service: "test-service".into(),
                workspace: "test-workspace".into(),
                task: "test-task".into(),
            },
            run: "run-1".into(),
            role: "executor".into(),
            profile: "desktop".into(),
            generation: 1,
            native: NativeSession {
                device,
                provider: identity.provider,
                store: identity.store,
                session: identity.session,
            },
            canonical_cwd: agent.session(session).unwrap().cwd,
            requester: "local-test-owner".into(),
            operation: "reserve-1".into(),
            provider_request: "native-1".into(),
        })
        .unwrap();
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn multica_reservation_removes_catalog_search_and_child_access_and_ends_idle_watch() {
    let dir = tempfile::tempdir().unwrap();
    let parent = transcripts(dir.path());
    let child = "00000000-0000-0000-0000-000000000001";
    std::fs::write(parent.parent().unwrap().join(format!("rollout-child-{child}.jsonl")),
        format!("{{\"type\":\"session_meta\",\"payload\":{{\"id\":\"{child}\",\"parent_thread_id\":\"{CODEX}\",\"cwd\":\"{}\"}}}}\n",dir.path().display())).unwrap();
    let daemon = Daemon::start_with(
        config(dir.path(), None, "test-host"),
        std::sync::Arc::new(musterd::DefaultPlatform),
        None,
    )
    .await
    .unwrap();
    daemon.agent().unwrap().rescan().await;
    let local = client(&daemon).await;
    assert!(sessions(&local, "").await.unwrap().contains(&CODEX.into()));
    let mut watch = local
        .start(Req::AgentWatch(v1::AgentWatchRequest {
            session: CODEX.into(),
            ..Default::default()
        }))
        .await
        .unwrap();
    assert!(
        tokio::time::timeout(Duration::from_secs(5), watch.events.recv())
            .await
            .unwrap()
            .is_some()
    );
    // No provider event or message follows: the durable ownership revision must
    // end this already-open watch, including for the trusted local client.
    reserve_multica(&daemon, CODEX);
    let ended = tokio::time::timeout(Duration::from_secs(3), async {
        let mut invalidated = false;
        while let Some(event) = watch.events.recv().await {
            if let v1::event::Ev::AgentEvent(v1::AgentEvent {
                ev: Some(v1::agent_event::Ev::Unavailable(removed)),
            }) = event
            {
                invalidated |= removed.sessions.iter().any(|id| id == CODEX);
            }
        }
        assert!(
            invalidated,
            "the client must clear content before the stream ends"
        );
        watch.finish().await
    })
    .await
    .expect("ownership alone ends the watch");
    assert!(unauthorized(&ended), "{ended:?}");
    assert_eq!(sessions(&local, "").await.unwrap(), [CLAUDE]);
    for grouped in [false, true] {
        let Resp::AgentSessions(page) = local
            .call(Req::AgentList(v1::AgentListRequest {
                grouped,
                query: if grouped { "" } else { "hello" }.into(),
                ..Default::default()
            }))
            .await
            .unwrap()
        else {
            panic!("catalog response")
        };
        assert!(page.sessions.iter().all(|s| s.id != CODEX && s.id != child));
    }
    for id in [CODEX, child] {
        for request in [
            Req::AgentMessages(v1::AgentMessagesRequest {
                session: id.into(),
                ..Default::default()
            }),
            Req::AgentFiles(v1::AgentFilesRequest {
                session: id.into(),
                ..Default::default()
            }),
            Req::AgentFile(v1::AgentFileRequest {
                session: id.into(),
                path: "file.txt".into(),
                ..Default::default()
            }),
            Req::AgentGit(v1::AgentGitRequest {
                session: id.into(),
                ..Default::default()
            }),
            Req::AgentSend(v1::AgentSendRequest {
                session: id.into(),
                op_id: "denied".into(),
                text: "not sent".into(),
                ..Default::default()
            }),
            Req::AgentSendStatus(v1::AgentSendRequest {
                session: id.into(),
                ..Default::default()
            }),
            Req::AgentOrganize(v1::AgentOrganizeRequest {
                session: id.into(),
                ..Default::default()
            }),
            Req::AgentInspect(v1::AgentInspectRequest {
                session: id.into(),
                ..Default::default()
            }),
        ] {
            let result = local.call(request).await;
            assert!(unauthorized(&result), "{id}: {result:?}");
        }
    }
    // Sharing a directory and provider never transfers personal ownership.
    assert!(
        local
            .call(Req::AgentMessages(v1::AgentMessagesRequest {
                session: CLAUDE.into(),
                ..Default::default()
            }))
            .await
            .is_ok()
    );
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn catalog_watch_coalesces_changes_and_invalidates_ownership_immediately() {
    let dir = tempfile::tempdir().unwrap();
    transcripts(dir.path());
    let daemon = Daemon::start_with(
        config(dir.path(), None, "test-host"),
        std::sync::Arc::new(musterd::DefaultPlatform),
        None,
    )
    .await
    .unwrap();
    let agent = daemon.agent().unwrap();
    let rename = |title: &str| {
        std::fs::write(
            dir.path().join("codex/session_index.jsonl"),
            format!(r#"{{"id":"{CODEX}","thread_name":"{title}"}}"#),
        )
        .unwrap();
    };
    rename("Initial");
    agent.rescan().await;
    let local = client(&daemon).await;
    let mut watch = local
        .start(Req::AgentWatch(v1::AgentWatchRequest {
            catalog_only: true,
            grouped: true,
            provider_pages: true,
            ..Default::default()
        }))
        .await
        .unwrap();
    let has_title = |page: v1::AgentSessionPage, title: &str| {
        page.sessions
            .iter()
            .any(|s| s.id == CODEX && s.title == title)
    };
    assert!(has_title(
        watch_page(&mut watch, Duration::from_secs(7)).await,
        "Initial"
    ));
    rename("Intermediate");
    agent.rescan().await;
    rename("Latest");
    agent.rescan().await;
    assert!(
        tokio::time::timeout(Duration::from_millis(600), watch.events.recv())
            .await
            .is_err()
    );
    assert!(has_title(
        watch_page(&mut watch, Duration::from_secs(7)).await,
        "Latest"
    ));

    // A burst returning to the published page should produce no delivery.
    rename("Transient");
    agent.rescan().await;
    rename("Latest");
    agent.rescan().await;
    assert!(
        tokio::time::timeout(Duration::from_secs(6), watch.events.recv())
            .await
            .is_err()
    );

    rename("Pending");
    agent.rescan().await;
    reserve_multica(&daemon, CODEX);
    let event = tokio::time::timeout(Duration::from_secs(2), watch.events.recv())
        .await
        .unwrap();
    assert!(
        matches!(event, Some(v1::event::Ev::AgentEvent(v1::AgentEvent {
        ev: Some(v1::agent_event::Ev::Unavailable(removed)),
    })) if removed.sessions.iter().any(|id| id == CODEX))
    );
    assert!(
        watch_page(&mut watch, Duration::from_secs(2))
            .await
            .sessions
            .iter()
            .all(|s| s.id != CODEX)
    );
    watch.cancel().await;
    daemon.shutdown().await;
}
