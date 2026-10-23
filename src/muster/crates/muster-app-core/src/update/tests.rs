//! The reducer's chat flows against a quiet shell.

use super::*;

#[test]
fn tab_completion_is_available_for_native_claude_and_codex_only() {
    let mut state = State::new(false, Some("local".into()));
    state.screen = app::Screen::Chat;
    state.chat = Some(crate::state::Chat {
        session: v1::AgentSession {
            mode: "code".into(),
            state: "idle".into(),
            ..Default::default()
        },
        ..Default::default()
    });
    for (provider, allowed) in [("claude", true), ("codex", true), ("catdesk", false)] {
        state.chat.as_mut().unwrap().session.provider = provider.into();
        assert_eq!(
            crate::render::view_model(&state).chat.unwrap().can_complete,
            allowed
        );
    }
    for status in ["running", "waiting_approval"] {
        let session = &mut state.chat.as_mut().unwrap().session;
        session.provider = "codex".into();
        session.state = status.into();
        assert!(!crate::render::view_model(&state).chat.unwrap().can_complete);
    }
}

#[test]
fn native_completion_is_optional_and_cannot_follow_a_new_turn_or_another_chat() {
    let mut state = State::new(false, Some("local".into()));
    state.screen = app::Screen::Chat;
    state.chat = Some(crate::state::Chat {
        device: "mac".into(),
        content_revision: 7,
        session: v1::AgentSession {
            id: "s".into(),
            provider: "claude".into(),
            mode: "code".into(),
            updated_unix: 7,
            state: "idle".into(),
            ..Default::default()
        },
        ..Default::default()
    });
    desktop_chats::msg_completion(
        &mut state,
        "other".into(),
        "s".into(),
        7,
        Ok("wrong".into()),
    );
    assert!(state.chat.as_ref().unwrap().completion.is_none());
    desktop_chats::msg_completion(
        &mut state,
        "mac".into(),
        "s".into(),
        7,
        Ok("next step".into()),
    );
    assert_eq!(
        crate::render::view_model(&state)
            .chat
            .unwrap()
            .composer_suggestion,
        "next step"
    );
    state.chat.as_mut().unwrap().content_revision = 8;
    assert!(
        crate::render::view_model(&state)
            .chat
            .unwrap()
            .composer_suggestion
            .is_empty()
    );
    desktop_chats::msg_completion(
        &mut state,
        "mac".into(),
        "s".into(),
        7,
        Ok("late result".into()),
    );
    assert!(
        crate::render::view_model(&state)
            .chat
            .unwrap()
            .composer_suggestion
            .is_empty()
    );
}

struct QuietShell;
impl crate::Shell for QuietShell {
    fn on_view(&self, _: &[u8]) {}
    fn on_effect(&self, _: u64, _: &[u8]) {}
    fn hw_public_key(&self) -> Option<Vec<u8>> {
        None
    }
    fn hw_sign(&self, _: &[u8]) -> Option<Vec<u8>> {
        None
    }
}

#[tokio::test]
async fn background_phone_cancels_display_watches_and_network_change_keeps_them_stopped() {
    let dir = tempfile::tempdir().unwrap();
    let shell = std::sync::Arc::new(QuietShell);
    let config = crate::AppConfig {
        dir: dir.path().into(),
        mode: crate::Mode::Embedded {
            relay: None,
            name: "phone".into(),
            kind: "phone".into(),
            bind: "127.0.0.1:0".parse().unwrap(),
            version: String::new(),
        },
        allow_loopback: true,
    };
    let backend = std::sync::Arc::new(
        crate::backend::Backend::connect(&config, shell.clone())
            .await
            .unwrap(),
    );
    let (tx, _rx) = tokio::sync::mpsc::unbounded_channel();
    let ctx = Ctx {
        shell,
        backend: backend.clone(),
        tx,
        cache: std::sync::Arc::new(crate::cache::Cache::new(dir.path().join("cache"))),
    };
    let mut state = State::new(true, backend.device_id());
    let (device_watch, device_token) = state.new_watch();
    let (chat_watch, chat_token) = state.new_watch();
    state.device_watch = Some(device_watch);
    state.chat = Some(Chat {
        watch: Some(chat_watch),
        ..Default::default()
    });
    lifecycle(&mut state, &ctx, app::Lifecycle::Background);
    assert!(device_token.is_cancelled() && chat_token.is_cancelled());
    assert!(
        state.chat.is_some(),
        "background preserves the draft and transcript"
    );
    lifecycle(&mut state, &ctx, app::Lifecycle::NetworkChanged);
    assert!(state.device_watch.is_none() && state.chat.as_ref().unwrap().watch.is_none());

    // A target-issued challenge reaches the foreground phone's Rust signing
    // path with no Authenticate effect. Admin requests still request it.
    use muster_core::locked_use::{Console, Gate, MAX_TTL, Observation, Screen as LockScreen};
    use muster_core::{Author, DeviceKind, DeviceRecord, HwSigner, Roster, Scope, SoftSigner};
    let target = SoftSigner::generate();
    let phone = backend.signer().unwrap();
    let mut roster = Roster::new();
    let genesis = Author {
        roster: &roster,
        signer: &target,
    }
    .genesis(
        "test",
        ("192.168.14.0".parse().unwrap(), 24),
        DeviceRecord::new(target.public_key(), "mac", DeviceKind::Desktop),
    )
    .unwrap();
    roster.insert(genesis).unwrap();
    let mut record = DeviceRecord::new(phone.public_key(), "phone", DeviceKind::Phone);
    record
        .scopes
        .extend([Scope::AgentControl, Scope::LockedControl]);
    let entry = Author {
        roster: &roster,
        signer: &target,
    }
    .add(record)
    .unwrap();
    roster.insert(entry).unwrap();
    let mut gate = Gate::new(target.public_key().device_id()).unwrap();
    let now = std::time::Instant::now();
    let challenge = gate
        .issue(
            roster.state(),
            &muster_core::LocalPolicy {
                allow_locked_control: true,
                ..Default::default()
            },
            phone.public_key().device_id(),
            "claude",
            "test-turn",
            [7; 32],
            MAX_TTL,
            now,
            Observation {
                at: now,
                screen: LockScreen::Locked,
                console: Some(Console {
                    uid: 501,
                    audit_session: 1,
                }),
                input_generation: Some(1),
                cover_nonce: None,
                watchdog_nonce: None,
                connected: true,
            },
        )
        .unwrap();
    state.foreground = true;
    state.self_info = Some(v1::DeviceInfo {
        kind: "phone".into(),
        ..Default::default()
    });
    let device = state.self_id.clone(); // Test-local RPC; never reaches another device.
    msg(
        &mut state,
        &ctx,
        Msg::PresenceNeeded {
            response_origin: None,
            what: "send",
            device: device.clone(),
            call_id: 1,
            request: Box::new(Req::LockedUseExecute(v1::LockedUseExecuteRequest {
                challenge: challenge.encode(),
                signature: vec![],
            })),
            message: challenge.signing_message(),
        },
    );
    assert!(state.locked.is_some());
    assert!(state.presence.is_none() && state.effects.is_empty());
    // A loading continuation cancels with the view and cannot reopen an
    // authentication prompt when its delayed challenge arrives.
    state.locked = None;
    let (reply, result) = tokio::sync::oneshot::channel();
    state.chat_calls.insert("load-1".into(), reply);
    lifecycle(&mut state, &ctx, app::Lifecycle::Background);
    assert!(result.await.unwrap().is_err());
    assert!(state.chat_calls.is_empty());
    state.foreground = true;
    msg(
        &mut state,
        &ctx,
        Msg::PresenceNeeded {
            response_origin: Some(("claude".into(), "load-1".into())),
            what: "chat",
            device: device.clone(),
            call_id: 3,
            request: Box::new(Req::LockedUseExecute(v1::LockedUseExecuteRequest {
                challenge: challenge.encode(),
                signature: vec![],
            })),
            message: challenge.signing_message(),
        },
    );
    assert!(state.locked.is_none() && state.presence.is_none());
    let (reply, result) = tokio::sync::oneshot::channel();
    msg(
        &mut state,
        &ctx,
        Msg::ChatControl {
            epoch: 0,
            device: device.clone(),
            provider: "claude".into(),
            request: Req::AgentChatOptions(v1::AgentChatOptionsRequest::default()),
            reply,
        },
    );
    assert!(
        result.await.unwrap().is_err(),
        "a load started before background/navigation cannot dispatch afterwards"
    );
    let (reply, result) = tokio::sync::oneshot::channel();
    state.chat_calls.insert("load-2".into(), reply);
    state.locked = Some((device.clone(), challenge.nonce().to_vec()));
    msg(
        &mut state,
        &ctx,
        Msg::ChatControlReply {
            device: device.clone(),
            id: "load-2".into(),
            nonce: Some(challenge.nonce().to_vec()),
            result: Ok(Resp::AgentChatOptions(v1::AgentChatOptions {
                session: "s".into(),
                ..Default::default()
            })),
        },
    );
    assert!(matches!(
        result.await.unwrap(),
        Ok(Resp::AgentChatOptions(_))
    ));
    assert!(state.locked.is_none());
    // Desktop manual leases must wait for a native Presence effect even when
    // this test backend has a working device signer. Expiration must release
    // busy UI state and ignore a late successful authentication result.
    state.embedded = false;
    state.self_info.as_mut().unwrap().kind = "desktop".into();
    for cancelled in [false, true] {
        state.manual_busy.insert(device.clone());
        msg(
            &mut state,
            &ctx,
            Msg::PresenceNeeded {
                response_origin: None,
                what: "manual",
                device: device.clone(),
                call_id: 4,
                request: Box::new(Req::LockedUseExecute(v1::LockedUseExecuteRequest {
                    challenge: challenge.encode(),
                    signature: vec![],
                })),
                message: challenge.signing_message(),
            },
        );
        let effect_id = state.presence.as_ref().unwrap().effect_id;
        assert_eq!(
            state.effects.get(&effect_id),
            Some(&EffectKind::Authenticate)
        );
        assert!(state.manual_busy.contains(&device));
        if cancelled {
            effect_result(&mut state, &ctx, effect_id, app::EffectResult::default());
        } else {
            msg(
                &mut state,
                &ctx,
                Msg::LockedExpired(challenge.nonce().to_vec()),
            );
        }
        assert!(!state.manual_busy.contains(&device));
        assert!(state.presence.is_none() && state.locked.is_none());
        effect_result(
            &mut state,
            &ctx,
            effect_id,
            app::EffectResult {
                ok: true,
                data: vec![1],
                ..Default::default()
            },
        );
        assert!(state.presence.is_none() && state.locked.is_none());
    }
    // A successful manual lease reply clears only the client's pending
    // authorization; the target retains the established owner.
    state.manual_busy.insert(device.clone());
    state.locked = Some((device.clone(), challenge.nonce().to_vec()));
    msg(
        &mut state,
        &ctx,
        Msg::OpDone {
            what: "manual",
            device: device.clone(),
            locked_nonce: Some(challenge.nonce().to_vec()),
            response_origin: None,
            result: Ok(v1::OpResult {
                status: v1::op_result::Status::Applied as i32,
                ..Default::default()
            }),
        },
    );
    assert!(!state.manual_busy.contains(&device));
    assert!(state.locked.is_none());
    msg(
        &mut state,
        &ctx,
        Msg::PresenceNeeded {
            response_origin: None,
            what: "policy",
            device,
            call_id: 2,
            request: Box::new(Req::PolicyGet(v1::PolicyGetRequest {})),
            message: vec![1],
        },
    );
    assert!(state.presence.is_some() && !state.effects.is_empty());
    backend.shutdown().await;
}
#[tokio::test]
async fn archived_sessions_stay_hidden_when_restoring_a_cached_catalog() {
    let dir = tempfile::tempdir().unwrap();
    let cache = crate::cache::Cache::new(dir.path().into());
    let sessions = ["claude", "codex"]
        .into_iter()
        .flat_map(|provider| {
            [false, true]
                .into_iter()
                .map(move |archived| v1::AgentSession {
                    id: format!("{provider}-{archived}"),
                    provider: provider.into(),
                    archived,
                    state: "ended".into(),
                    ..Default::default()
                })
        })
        .collect();
    // A cache produced by an older client may still include archived rows.
    cache.put("catalog".into(), &v1::AgentSessionPage { sessions });
    let page = cache.get::<v1::AgentSessionPage>("catalog").unwrap();
    let visible = visible_agents(page.sessions);
    assert_eq!(visible.len(), 2);
    assert!(visible.iter().all(|s| !s.archived && s.state == "ended"));
}

#[test]
fn file_links_are_relative_to_the_session_and_cannot_escape_it() {
    assert_eq!(
        session_file_link("/repo", "src/a%20b.rs#L12"),
        Ok("src/a b.rs".into())
    );
    assert_eq!(
        session_file_link("/repo", "file:///repo/src/a.rs"),
        Ok("src/a.rs".into())
    );
    assert!(session_file_link("/repo", "../private/key").is_err());
    assert!(session_file_link("/repo", "file:///outside/key").is_err());
    assert!(session_file_link("/repo", "https://example.com/a").is_err());
}
#[test]
fn change_preview_requires_a_loaded_summary_and_keeps_deleted_diff() {
    let mut state = State::new(false, None);
    state.chat = Some(crate::state::Chat {
        content_revision: crate::state::next_content_revision(),
        cache_dirty: false,
        device: "remote".into(),
        cache_key: String::new(),
        session: v1::AgentSession {
            id: "session".into(),
            ..Default::default()
        },
        messages: vec![v1::AgentMessage {
            id: "summary".into(),
            kind: "turn_summary".into(),
            usage: Some(v1::TurnUsage {
                changes: vec![v1::TurnFileChange {
                    path: "deleted.rs".into(),
                    kind: "delete".into(),
                    diff: "-old\n".into(),
                    ..Default::default()
                }],
                ..Default::default()
            }),
            ..Default::default()
        }],
        history_start: None,
        interactions: Vec::new(),
        has_earlier: false,
        sending: false,
        responding_interaction: String::new(),
        response_interaction: String::new(),
        response_error: String::new(),
        response_unknown: false,
        pending_draft_id: String::new(),
        sent_draft_id: String::new(),
        unconfirmed: None,
        draft: Default::default(),
        loading: false,
        load_error: String::new(),
        watch: None,
        options: None,
        options_busy: false,
        options_error: String::new(),
        completing: false,
        completion: None,
        completion_error: String::new(),
    });
    open_change(
        &mut state,
        app::OpenChange {
            summary_id: "missing".into(),
            path: "deleted.rs".into(),
        },
    );
    assert!(state.file.is_none());
    open_change(
        &mut state,
        app::OpenChange {
            summary_id: "summary".into(),
            path: "deleted.rs".into(),
        },
    );
    let file = state.file.unwrap();
    assert_eq!(file.device_id, "remote");
    assert_eq!(file.content, b"-old\n");
    assert!(file.is_diff);
    assert!(file.current_path.is_empty());
}
fn message(id: &str, role: &str) -> v1::AgentMessage {
    v1::AgentMessage {
        id: id.into(),
        role: role.into(),
        text: "repeat".into(),
        ..Default::default()
    }
}
#[test]
fn queued_messages_before_a_shorter_snapshot_are_not_appended_after_it() {
    let messages: Vec<_> = (0..601).map(|i| message(&i.to_string(), "user")).collect();
    let mut queued = messages
        .iter()
        .cloned()
        .map(|m| {
            v1::event::Ev::AgentEvent(v1::AgentEvent {
                ev: Some(v1::agent_event::Ev::Message(m)),
            })
        })
        .collect();
    assert!(reconcile_queued(&mut queued, &messages[100..600]));
    assert_eq!(queued.len(), 1);
    assert!(!reconcile_queued(&mut queued, &messages[..100]));
}

#[test]
fn disjoint_tail_does_not_hide_an_unloaded_gap() {
    let mut current = vec![message("old", "user")];
    merge_chat_tail(&mut current, vec![message("new", "assistant")]);
    assert_eq!(
        current.iter().map(|m| m.id.as_str()).collect::<Vec<_>>(),
        ["new"]
    );
}

#[test]
fn catch_up_restores_missing_user_in_order_and_keeps_earlier_pages() {
    let mut current = vec![message("old", "user"), message("a1", "assistant")];
    let page = vec![
        message("u1", "user"),
        message("a1", "assistant"),
        message("u2", "user"),
        message("a2", "assistant"),
    ];
    merge_chat_tail(&mut current, page.clone());
    merge_chat_tail(&mut current, page);
    assert_eq!(
        current.iter().map(|m| m.id.as_str()).collect::<Vec<_>>(),
        ["old", "u1", "a1", "u2", "a2"]
    );
}

#[tokio::test]
async fn pasted_text_reaches_ssh_bracketed_when_asked_and_oversized_pastes_do_not() {
    let dir = tempfile::tempdir().unwrap();
    let shell = std::sync::Arc::new(QuietShell);
    let config = crate::AppConfig {
        dir: dir.path().into(),
        mode: crate::Mode::Embedded {
            relay: None,
            name: "phone".into(),
            kind: "phone".into(),
            bind: "127.0.0.1:0".parse().unwrap(),
            version: String::new(),
        },
        allow_loopback: true,
    };
    let backend = std::sync::Arc::new(
        crate::backend::Backend::connect(&config, shell.clone())
            .await
            .unwrap(),
    );
    let (tx, _rx) = tokio::sync::mpsc::unbounded_channel();
    let ctx = Ctx {
        shell,
        backend: backend.clone(),
        tx,
        cache: std::sync::Arc::new(crate::cache::Cache::new(dir.path().join("cache"))),
    };
    let mut state = State::new(true, backend.device_id());
    let (input, mut sent) = tokio::sync::mpsc::unbounded_channel();
    state.terminal = Some(Terminal {
        device: "mac".into(),
        session: "ssh-1".into(),
        kind: TerminalKind::Ssh {
            grid: Box::new(muster_terminal::grid::Grid::new(80, 24)),
            input,
            _reader: None,
        },
        source: "ssh".into(),
        cols: 80,
        rows: 24,
        lines: Vec::new(),
        cursor: (0, 0),
        ended: false,
        status: String::new(),
        rev: 0,
        desktop_attach_command: String::new(),
        iterm2_attach_command: String::new(),
        native_sources: Vec::new(),
    });
    let paste = |text: &str| app::TerminalPaste { text: text.into() };
    let written =
        |sent: &mut tokio::sync::mpsc::UnboundedReceiver<SshCommand>| match sent.try_recv() {
            Ok(SshCommand::Write(bytes)) => Some(bytes),
            _ => None,
        };
    state.screen = Screen::Terminal;
    assert!(
        crate::render::view_model(&state)
            .terminal
            .unwrap()
            .can_paste
    );

    terminals::intent_terminal_paste(&mut state, &ctx, paste("ls\n"));
    assert_eq!(written(&mut sent).as_deref(), Some(&b"ls\r"[..]));

    // The program enabled bracketed paste (DECSET 2004).
    if let Some(Terminal {
        kind: TerminalKind::Ssh { grid, .. },
        ..
    }) = &mut state.terminal
    {
        grid.feed(b"\x1b[?2004h");
    }
    terminals::intent_terminal_paste(&mut state, &ctx, paste("ls\n"));
    assert_eq!(
        written(&mut sent).as_deref(),
        Some(&b"\x1b[200~ls\r\x1b[201~"[..])
    );

    let big = "x".repeat(muster_terminal::MAX_PASTE + 1);
    terminals::intent_terminal_paste(&mut state, &ctx, paste(&big));
    assert_eq!(written(&mut sent), None);
    assert!(
        state
            .toasts
            .last()
            .is_some_and(|t| t.error && t.text.starts_with("Not pasted"))
    );

    state.terminal.as_mut().unwrap().ended = true;
    assert!(
        !crate::render::view_model(&state)
            .terminal
            .unwrap()
            .can_paste
    );
}

async fn phone() -> (tempfile::TempDir, Ctx, State) {
    let dir = tempfile::tempdir().unwrap();
    let shell = std::sync::Arc::new(QuietShell);
    let config = crate::AppConfig {
        dir: dir.path().into(),
        mode: crate::Mode::Embedded {
            relay: None,
            name: "phone".into(),
            kind: "phone".into(),
            bind: "127.0.0.1:0".parse().unwrap(),
            version: String::new(),
        },
        allow_loopback: true,
    };
    let backend = std::sync::Arc::new(
        crate::backend::Backend::connect(&config, shell.clone())
            .await
            .unwrap(),
    );
    let (tx, _rx) = tokio::sync::mpsc::unbounded_channel();
    let ctx = Ctx {
        shell,
        backend: backend.clone(),
        tx,
        cache: std::sync::Arc::new(crate::cache::Cache::new(dir.path().join("cache"))),
    };
    let state = State::new(true, backend.device_id());
    (dir, ctx, state)
}

fn remote_terminal() -> Terminal {
    Terminal {
        device: "mac".into(),
        session: "s".into(),
        kind: TerminalKind::Remote {
            _attach: None,
            lease: None,
            leased: false,
            input: Default::default(),
        },
        source: "iterm2".into(),
        cols: 80,
        rows: 24,
        lines: Vec::new(),
        cursor: (0, 0),
        ended: false,
        status: String::new(),
        rev: 0,
        desktop_attach_command: String::new(),
        iterm2_attach_command: String::new(),
        native_sources: Vec::new(),
    }
}

/// Back to the list stops what only the left screen shows: the chat's live
/// watch and an attached remote terminal. A foreground return does not
/// watch a chat the user has left.
#[tokio::test]
async fn back_releases_the_chat_watch_and_a_remote_terminal() {
    let (_dir, ctx, mut state) = phone().await;
    let back = app::Navigate {
        back: true,
        ..Default::default()
    };
    let (watch, token) = state.new_watch();
    state.chat = Some(Chat {
        watch: Some(watch),
        ..Default::default()
    });
    state.screen = Screen::Chat;
    state.stack = vec![Screen::Agents];
    intent_navigate(&mut state, &ctx, back.clone());
    assert_eq!(state.screen, Screen::Agents);
    assert!(token.is_cancelled());
    assert!(state.chat.as_ref().unwrap().watch.is_none());
    lifecycle(&mut state, &ctx, app::Lifecycle::Foreground);
    assert!(state.chat.as_ref().unwrap().watch.is_none());

    state.terminal = Some(remote_terminal());
    state.screen = Screen::Terminal;
    state.stack = vec![Screen::Agents];
    intent_navigate(&mut state, &ctx, back.clone());
    assert!(state.terminal.is_none());

    // An in-app SSH session keeps its connection off screen.
    let (input, _sent) = tokio::sync::mpsc::unbounded_channel();
    let mut ssh = remote_terminal();
    ssh.kind = TerminalKind::Ssh {
        grid: Box::new(muster_terminal::grid::Grid::new(80, 24)),
        input,
        _reader: None,
    };
    state.terminal = Some(ssh);
    state.screen = Screen::Terminal;
    state.stack = vec![Screen::Agents];
    intent_navigate(&mut state, &ctx, back);
    assert!(state.terminal.is_some());
}

#[tokio::test]
async fn native_back_menu_validates_the_displayed_path_and_releases_hidden_chat() {
    let (_dir, ctx, mut state) = phone().await;
    state.screen = Screen::Agents;
    state.go(Screen::Chat);
    state.go(Screen::Files);
    state.go(Screen::File);
    let (watch, token) = state.new_watch();
    state.chat = Some(Chat {
        watch: Some(watch),
        ..Default::default()
    });
    let pop = app::Navigate {
        back: true,
        back_count: 3,
        expected_path: state.navigation_path(),
        ..Default::default()
    };
    let mut stale = pop.clone();
    stale.expected_path.pop();
    intent_navigate(&mut state, &ctx, stale);
    assert_eq!(state.screen, Screen::File);
    assert!(!token.is_cancelled());
    let mut too_far = pop.clone();
    too_far.back_count = 4;
    intent_navigate(&mut state, &ctx, too_far);
    assert_eq!(state.screen, Screen::File);
    intent_navigate(&mut state, &ctx, pop.clone());
    assert_eq!(state.screen, Screen::Agents);
    assert!(token.is_cancelled());
    intent_navigate(&mut state, &ctx, pop);
    assert_eq!(state.screen, Screen::Agents);
}

/// A refresh of the session list neither shows loading over a list it
/// already has nor blinks its errors away; the completed refresh replaces
/// them.
#[tokio::test]
async fn a_list_refresh_keeps_the_view_steady_until_it_completes() {
    let (_dir, ctx, mut state) = phone().await;
    state.screen = Screen::Agents;
    state.agents_epoch = 7;
    state.agents_pending = 2;
    state.agents.loading = false;
    state.agents.errors = vec!["old".into()];
    let failure = || Err(crate::Error::Unavailable("mac offline".into()));
    agents::msg_agents(&mut state, &ctx, 7, "mac".into(), failure());
    assert!(!state.agents.loading);
    assert_eq!(state.agents.errors, vec!["old".to_string()]);
    agents::msg_agents(&mut state, &ctx, 7, "mini".into(), Ok(Vec::new()));
    assert!(!state.agents.loading);
    assert_eq!(
        state.agents.errors,
        vec!["unavailable: mac offline".to_string()]
    );

    // A first load shows loading until every device answered.
    state.agents_epoch = 8;
    state.agents_pending = 2;
    state.agents.loading = true;
    agents::msg_agents(&mut state, &ctx, 8, "mac".into(), Ok(Vec::new()));
    assert!(state.agents.loading);
    agents::msg_agents(&mut state, &ctx, 8, "mini".into(), Ok(Vec::new()));
    assert!(!state.agents.loading);
    assert!(state.agents.errors.is_empty());
}

/// A log over the upload limit is refused locally, with a toast.
#[tokio::test]
async fn an_oversized_diagnostics_upload_is_refused_before_sending() {
    let (_dir, ctx, mut state) = phone().await;
    network::intent_upload_diagnostics(
        &mut state,
        &ctx,
        app::UploadDiagnostics {
            device_id: "mac".into(),
            kind: "performance".into(),
            data: vec![b'x'; muster_proto::MAX_DIAGNOSTICS + 1],
        },
    );
    assert!(
        state
            .toasts
            .last()
            .is_some_and(|t| t.error && t.text.starts_with("Log not sent"))
    );
}

/// A locked Mac's Ghostty terminal needs a terminal session, which only a
/// phone keeps open; anywhere else the terminal says why it cannot open.
#[tokio::test]
async fn a_locked_ghostty_terminal_off_a_phone_says_why() {
    let (_dir, ctx, mut state) = phone().await;
    state.terminal = Some(Terminal {
        source: "ghostty".into(),
        ..remote_terminal()
    });
    terminals::msg_terminal_locked(&mut state, &ctx, "mac".into(), "s".into());
    let terminal = state.terminal.as_ref().unwrap();
    assert!(terminal.ended && terminal.status.contains("from a phone"));
    let TerminalKind::Remote { lease, leased, .. } = &terminal.kind else {
        panic!()
    };
    assert!(lease.is_none() && !leased);
    // Another terminal's refusal changes nothing here.
    terminals::msg_terminal_locked(&mut state, &ctx, "mac".into(), "other".into());
}

/// The background ends a terminal session at once: the phone cannot keep
/// renewing it. The session's watch is dropped, which withdraws it.
#[tokio::test]
async fn the_background_ends_a_terminal_session() {
    let (_dir, ctx, mut state) = phone().await;
    let (watch, token) = state.new_watch();
    state.terminal = Some(Terminal {
        source: "ghostty".into(),
        kind: TerminalKind::Remote {
            _attach: None,
            lease: Some(watch),
            leased: true,
            input: Default::default(),
        },
        ..remote_terminal()
    });
    lifecycle(&mut state, &ctx, app::Lifecycle::Background);
    assert!(token.is_cancelled());
    let terminal = state.terminal.as_ref().unwrap();
    assert!(terminal.ended);
    let TerminalKind::Remote { lease, leased, .. } = &terminal.kind else {
        panic!()
    };
    assert!(
        lease.is_none() && !leased,
        "the foreground may open a new one"
    );
}

/// Once the session is open the terminal attaches again, under it.
#[tokio::test]
async fn an_open_terminal_session_attaches_again() {
    let (_dir, ctx, mut state) = phone().await;
    state.terminal = Some(Terminal {
        ended: true,
        ..remote_terminal()
    });
    terminals::msg_terminal_lease_opened(&mut state, &ctx, "mac".into(), "s".into());
    let terminal = state.terminal.as_ref().unwrap();
    assert!(!terminal.ended && terminal.status.contains("遮罩"));
    assert!(matches!(
        terminal.kind,
        TerminalKind::Remote {
            _attach: Some(_),
            ..
        }
    ));
    terminals::msg_terminal_lease_ended(&mut state, "mac".into(), "s".into(), "expired".into());
    let terminal = state.terminal.as_ref().unwrap();
    assert!(terminal.ended && terminal.status.ends_with("expired"));
}

#[test]
fn subagent_navigation_is_bound_to_the_open_device_and_provider_relationships() {
    let mut state = State::new(false, None);
    state.chat = Some(Chat {
        device: "mac-a".into(),
        session: v1::AgentSession {
            id: "parent".into(),
            subagents: vec![v1::AgentSession {
                id: "child".into(),
                read_only: true,
                parent_session_id: "parent".into(),
                ..Default::default()
            }],
            ..Default::default()
        },
        ..Default::default()
    });
    let (device, child) = agents::related_agent(&state, "mac-a", "child").unwrap();
    assert_eq!(device, "mac-a");
    assert!(child.read_only);
    assert!(agents::related_agent(&state, "mac-b", "child").is_none());
    assert!(agents::related_agent(&state, "mac-a", "unrelated").is_none());
    state.chat.as_mut().unwrap().session = child;
    // An archived parent can be absent from the main catalog; it stays read-only
    // until the watch supplies authoritative state from the same device.
    let (device, parent) = agents::related_agent(&state, "", "parent").unwrap();
    assert_eq!(device, "mac-a");
    assert_eq!(parent.id, "parent");
    assert!(parent.read_only);
}

#[test]
fn reopening_a_large_gap_reaches_latest_in_two_requests_then_streams_in_order() {
    let all: Vec<_> = (0..10_000)
        .map(|i| message(&i.to_string(), "user"))
        .collect();
    let mut catch_up = ChatCatchUp::new(all[..2].to_vec());
    assert_eq!(
        catch_up
            .apply(muster_proto::history::page(&all, &catch_up.request("s")))
            .unwrap(),
        None
    );
    assert_eq!(
        catch_up
            .apply(muster_proto::history::page(&all, &catch_up.request("s")))
            .unwrap(),
        Some(true)
    );
    assert!(catch_up.reset);
    assert_eq!(catch_up.messages, all[9_500..]);
    // Events that overlapped the catch-up must not duplicate the tail; later
    // messages remain incremental and preserve the stream's order.
    let mut queued = [all[9_999].clone(), message("10000", "assistant")]
        .into_iter()
        .map(|m| {
            v1::event::Ev::AgentEvent(v1::AgentEvent {
                ev: Some(v1::agent_event::Ev::Message(m)),
            })
        })
        .collect();
    assert!(reconcile_queued(&mut queued, &catch_up.messages));
    assert_eq!(queued.len(), 1);
    // The skipped gap remains available through existing earlier navigation.
    let earlier = muster_proto::history::page(
        &all,
        &v1::AgentMessagesRequest {
            session: "s".into(),
            tail: 500,
            before_id: "9500".into(),
            ..Default::default()
        },
    );
    assert_eq!(earlier.messages, all[9_000..9_500]);
}

#[test]
fn reopening_small_deltas_keeps_validated_cache_and_rewritten_history_resets() {
    let mut all: Vec<_> = (0..50).map(|i| message(&i.to_string(), "user")).collect();
    let mut catch_up = ChatCatchUp::new(all[..40].to_vec());
    assert_eq!(
        catch_up
            .apply(muster_proto::history::page(&all, &catch_up.request("s")))
            .unwrap(),
        Some(false)
    );
    assert!(!catch_up.reset);
    assert_eq!(catch_up.messages, all);
    all[0].text = "edited".into();
    assert_eq!(
        catch_up
            .apply(muster_proto::history::page(&all, &catch_up.request("s")))
            .unwrap(),
        Some(false)
    );
    assert!(catch_up.reset);
    assert_eq!(catch_up.messages, all);
}

fn mac(id: &str, presence: &str) -> v1::MemberView {
    v1::MemberView {
        device_id: id.into(),
        name: id.into(),
        kind: "desktop".into(),
        connected: matches!(presence, "online" | "reconnecting"),
        presence: presence.into(),
        ..Default::default()
    }
}

fn session_on(device: &str, id: &str) -> v1::AgentSession {
    v1::AgentSession {
        id: id.into(),
        device_id: device.into(),
        provider: "claude".into(),
        ..Default::default()
    }
}

fn network_of(members: Vec<v1::MemberView>) -> v1::NetworkStatus {
    v1::NetworkStatus {
        has_network: true,
        members,
        ..Default::default()
    }
}

/// An offline (or unreachable) Mac is not asked for its sessions and none
/// are shown, cached or late; its cache stays on disk for when it is back.
#[tokio::test]
async fn an_offline_macs_sessions_are_neither_polled_nor_shown() {
    let (_dir, ctx, mut state) = phone().await;
    state.network = Some(network_of(vec![
        mac("mini", "online"),
        mac("pro", "offline"),
        mac("max", "unreachable"),
    ]));
    ctx.cache.put(
        agents::cache_key(&state, "agents", "pro", ""),
        &v1::AgentSessionPage {
            sessions: vec![session_on("pro", "cached")],
        },
    );
    state.agents.sessions = vec![session_on("mini", "a"), session_on("pro", "b")];
    agents::open_agents(&mut state, &ctx);
    assert_eq!(state.agents_pending, 1, "only the online Mac is asked");
    assert!(state.agents.sessions.iter().all(|s| s.device_id == "mini"));
    // A late answer from the offline Mac changes nothing.
    let epoch = state.agents_epoch;
    agents::msg_agents(
        &mut state,
        &ctx,
        epoch,
        "pro".into(),
        Ok(vec![session_on("pro", "late")]),
    );
    assert!(state.agents.sessions.iter().all(|s| s.device_id == "mini"));
    // The cache was not touched.
    let cached = ctx
        .cache
        .get::<v1::AgentSessionPage>(&agents::cache_key(&state, "agents", "pro", ""))
        .unwrap();
    assert_eq!(cached.sessions[0].id, "cached");
    // Going offline after the list loaded hides it at once.
    state.agents.sessions = vec![session_on("mini", "a"), session_on("pro", "b")];
    let view = crate::render::view_model(&state);
    let shown: Vec<_> = view
        .agents
        .unwrap()
        .sessions
        .into_iter()
        .map(|s| s.id)
        .collect();
    assert_eq!(shown, ["a"]);
}

/// Back online: the list is loaded again, with that Mac.
#[tokio::test]
async fn a_mac_coming_back_online_is_listed_again() {
    let (_dir, ctx, mut state) = phone().await;
    state.screen = Screen::Agents;
    state.network = Some(network_of(vec![
        mac("mini", "online"),
        mac("pro", "offline"),
    ]));
    agents::open_agents(&mut state, &ctx);
    assert_eq!(state.agents_pending, 1);
    let epoch = state.agents_epoch;
    // A reconnect inside its retries is not a change worth a reload.
    network::msg_network(
        &mut state,
        &ctx,
        network_of(vec![mac("mini", "reconnecting"), mac("pro", "offline")]),
    );
    assert_eq!(state.agents_epoch, epoch);
    network::msg_network(
        &mut state,
        &ctx,
        network_of(vec![mac("mini", "online"), mac("pro", "online")]),
    );
    assert_eq!(state.agents_epoch, epoch + 1);
    assert_eq!(state.agents_pending, 2);
}

/// The background suspends presence (a suspended phone's failed dials say
/// nothing about the Macs); the foreground resumes it.
#[tokio::test]
async fn the_background_suspends_presence_and_the_foreground_resumes_it() {
    let (_dir, ctx, mut state) = phone().await;
    let node = ctx.backend.daemon().unwrap().node();
    assert!(!node.suspended());
    lifecycle(&mut state, &ctx, app::Lifecycle::Background);
    assert!(node.suspended());
    lifecycle(&mut state, &ctx, app::Lifecycle::Foreground);
    assert!(!node.suspended());
}

#[tokio::test]
async fn blocked_cards_notify_once_in_background_including_optional_to_blocked_transition() {
    let (_dir, ctx, mut state) = phone().await;
    state.foreground = false;
    state.chat = Some(Chat {
        device: "d".into(),
        session: v1::AgentSession {
            id: "s".into(),
            title: "Task".into(),
            ..Default::default()
        },
        ..Default::default()
    });
    let mut q = muster_agent::claude_cards::wrap_up(
        &serde_json::json!({"recap":"Need input","ask":"Choose","actions":[{"label":"Yes"}]}),
        "w",
        "s",
    )
    .unwrap();
    let send = |state: &mut State, q: v1::PendingInteraction| {
        agents::msg_agent_event(
            state,
            &ctx,
            CancellationToken::new(),
            "d".into(),
            "s".into(),
            v1::agent_event::Ev::Interaction(q),
        )
    };
    send(&mut state, q.clone());
    assert_eq!(state.effects.len(), 0);
    q.optional = false;
    send(&mut state, q.clone());
    assert_eq!(state.effects.len(), 1);
    send(&mut state, q.clone());
    assert_eq!(state.effects.len(), 1);
    q.id = "question:second".into();
    send(&mut state, q.clone());
    assert_eq!(state.effects.len(), 2);
    state.foreground = true;
    q.id = "question:third".into();
    send(&mut state, q);
    assert_eq!(state.effects.len(), 2);
    assert!(session_file_link("/repo", "muster-delivered:toolu_a:0").is_ok());
    assert!(session_file_link("/repo", "muster-delivered:../bad:0").is_err());
}

#[tokio::test]
async fn session_terminal_rejects_late_results_and_detaches_without_closing_shell() {
    let dir = tempfile::tempdir().unwrap();
    let shell = Arc::new(QuietShell);
    let config = crate::AppConfig {
        dir: dir.path().into(),
        mode: crate::Mode::Embedded {
            relay: None,
            name: "phone".into(),
            kind: "phone".into(),
            bind: "127.0.0.1:0".parse().unwrap(),
            version: String::new(),
        },
        allow_loopback: true,
    };
    let backend = Arc::new(
        crate::backend::Backend::connect(&config, shell.clone())
            .await
            .unwrap(),
    );
    let (tx, _rx) = tokio::sync::mpsc::unbounded_channel();
    let ctx = Ctx {
        shell,
        backend: backend.clone(),
        tx,
        cache: Arc::new(crate::cache::Cache::new(dir.path().join("cache"))),
    };
    let mut state = State::new(true, backend.device_id());
    state.screen = Screen::Chat;
    state.chat = Some(Chat {
        device: "mac".into(),
        session: v1::AgentSession {
            id: "agent".into(),
            cwd: "/workspace".into(),
            mode: "code".into(),
            ..Default::default()
        },
        ..Default::default()
    });
    state.session_terminal.epoch = 5;
    state.session_terminal.busy = true;
    for (epoch, device, agent) in [
        (4, "mac", "agent"),
        (5, "other", "agent"),
        (5, "mac", "other"),
    ] {
        terminals::session_terminal_ready(
            &mut state,
            &ctx,
            epoch,
            device.into(),
            agent.into(),
            Ok("tmux-late".into()),
        );
        assert!(state.terminal.is_none());
        assert!(state.session_terminal.busy);
    }
    terminals::session_terminal_ready(
        &mut state,
        &ctx,
        5,
        "mac".into(),
        "agent".into(),
        Err(Error::Unavailable("failed".into())),
    );
    assert!(!state.session_terminal.busy);
    assert!(state.session_terminal.error.contains("failed"));
    let (watch, token) = state.new_watch();
    state.terminal = Some(Terminal {
        device: "mac".into(),
        session: "tmux-owned".into(),
        kind: TerminalKind::Remote {
            _attach: Some(watch),
            lease: None,
            leased: false,
            input: Default::default(),
        },
        source: "tmux".into(),
        cols: 80,
        rows: 24,
        lines: vec![],
        cursor: (0, 0),
        ended: false,
        status: "attached".into(),
        rev: 0,
        desktop_attach_command: String::new(),
        iterm2_attach_command: String::new(),
        native_sources: Vec::new(),
    });
    state.session_terminal.active = true;
    let vm = crate::render::view_model(&state);
    assert!(vm.chat.unwrap().terminal.unwrap().can_end);
    assert!(vm.terminal.unwrap().resizable);
    terminals::background_terminal(&mut state);
    assert!(token.is_cancelled());
    assert!(state.terminal.as_ref().unwrap().ended);
    assert!(
        state.session_terminal.active,
        "keep the shell identity for foreground reconnect"
    );
    terminals::detach_session_terminal(&mut state);
    assert!(state.terminal.is_none());
    assert!(!state.session_terminal.active);
    terminals::session_terminal_ready(
        &mut state,
        &ctx,
        5,
        "mac".into(),
        "agent".into(),
        Ok("tmux-late".into()),
    );
    assert!(
        state.terminal.is_none(),
        "a late start must not reopen a closed tab"
    );
    state.session_terminal.busy = true;
    let epoch = state.session_terminal.epoch;
    state.go(Screen::Agents);
    terminals::session_terminal_ready(
        &mut state,
        &ctx,
        epoch,
        "mac".into(),
        "agent".into(),
        Ok("tmux-late".into()),
    );
    assert!(state.terminal.is_none());
    assert!(!state.session_terminal.busy);
}

#[tokio::test]
async fn code_preview_rejects_edits_and_recovered_runs_keep_device_consent_and_unknown_receipts() {
    let dir = tempfile::tempdir().unwrap();
    let shell = std::sync::Arc::new(QuietShell);
    let config = crate::AppConfig {
        dir: dir.path().into(),
        mode: crate::Mode::Embedded {
            relay: None,
            name: "phone".into(),
            kind: "phone".into(),
            bind: "127.0.0.1:0".parse().unwrap(),
            version: String::new(),
        },
        allow_loopback: true,
    };
    let backend = std::sync::Arc::new(
        crate::backend::Backend::connect(&config, shell.clone())
            .await
            .unwrap(),
    );
    let (tx, mut rx) = tokio::sync::mpsc::unbounded_channel();
    let ctx = Ctx {
        shell,
        backend: backend.clone(),
        tx,
        cache: std::sync::Arc::new(crate::cache::Cache::new(dir.path().join("cache"))),
    };
    let mut state = State::new(true, backend.device_id());
    let message = v1::AgentMessage {
        id: "message".into(),
        role: "assistant".into(),
        kind: "text".into(),
        text: "```sh\nprintf hello\n```".into(),
        ..Default::default()
    };
    let handle = muster_terminal::commands::parts(&message, true)[0]
        .command_id
        .clone();
    state.screen = Screen::Chat;
    state.chat = Some(Chat {
        device: "mac".into(),
        session: v1::AgentSession {
            id: "original".into(),
            provider: "claude".into(),
            cwd: "/tmp".into(),
            ..Default::default()
        },
        messages: vec![message],
        ..Default::default()
    });
    code_runs::action(
        &mut state,
        &ctx,
        app::CodeAction {
            action: 0,
            id: handle.clone(),
            auto_return: true,
        },
    );
    assert!(code_runs::preview(&state).is_some());
    state.chat.as_mut().unwrap().messages[0].text = "```sh\nprintf changed\n```".into();
    code_runs::action(
        &mut state,
        &ctx,
        app::CodeAction {
            action: 1,
            id: handle,
            auto_return: true,
        },
    );
    assert!(rx.try_recv().is_err()); // A stale confirmation never starts an RPC.
    assert!(
        state
            .code
            .errors
            .values()
            .any(|s| s.contains("Code changed"))
    );
    state.foreground = false; // Polling data cannot initiate a send in the background.
    let owner = state.self_id.clone();
    code_runs::received(
        &mut state,
        &ctx,
        "mac".into(),
        "original".into(),
        "claude".into(),
        Ok(vec![
            v1::TerminalRun {
                id: "owned".into(),
                return_ready: true,
                session: "original".into(),
                state: "completed".into(),
                exit_known: true,
                return_owner: owner.clone(),
                ..Default::default()
            },
            v1::TerminalRun {
                id: "unknown".into(),
                session: "original".into(),
                state: "completed".into(),
                exit_known: true,
                return_owner: owner,
                return_state: "unknown".into(),
                ..Default::default()
            },
            v1::TerminalRun {
                id: "foreign".into(),
                session: "wrong-chat".into(),
                state: "completed".into(),
                exit_known: true,
                ..Default::default()
            },
        ]),
    );
    let views = code_runs::views(&state, "mac", "original");
    assert_eq!(views.len(), 2);
    assert!(
        views
            .iter()
            .find(|v| v.run.as_ref().unwrap().id == "owned")
            .unwrap()
            .can_return
    );
    let unknown = views
        .iter()
        .find(|v| v.run.as_ref().unwrap().id == "unknown")
        .unwrap();
    assert_eq!(unknown.delivery, "unknown");
    assert!(!unknown.can_return);
    assert!(rx.try_recv().is_err());
    assert!(
        state
            .code
            .errors
            .values()
            .any(|s| s.contains("Code changed"))
    );
    state.code = Default::default();
    state.foreground = true;
    state.chat = None; // Leaving the conversation must not lose the return intent.
    let mut waiting = v1::TerminalRun {
        id: "wait-for-turn".into(),
        session: "original".into(),
        state: "completed".into(),
        exit_known: true,
        return_owner: state.self_id.clone(),
        return_ready: false,
        ..Default::default()
    };
    code_runs::received(
        &mut state,
        &ctx,
        "mac".into(),
        "original".into(),
        "codex".into(),
        Ok(vec![waiting.clone()]),
    );
    assert_eq!(
        code_runs::views(&state, "mac", "original")[0].delivery,
        "waiting"
    );
    assert!(!code_runs::views(&state, "mac", "original")[0].can_return);
    assert!(rx.try_recv().is_err());
    waiting.return_ready = true;
    code_runs::received(
        &mut state,
        &ctx,
        "mac".into(),
        "original".into(),
        "codex".into(),
        Ok(vec![waiting]),
    );
    assert_eq!(
        code_runs::views(&state, "mac", "original")[0].delivery,
        "sending"
    );
}

#[test]
fn history_window_retains_stream_anchor_and_pages_cached_data_offline() {
    let mut chat = Chat {
        messages: (0..500).map(|i| message(&i.to_string(), "user")).collect(),
        ..Default::default()
    };
    chat.reconcile_history_start();
    assert_eq!(chat.history_start.as_deref(), Some("300"));
    chat.messages.push(message("500", "assistant"));
    chat.reconcile_history_start();
    assert_eq!(chat.visible_messages().first().unwrap().id, "300");
    assert_eq!(chat.visible_messages().len(), 201);
    chat.messages[499].text = "streamed update".into();
    chat.reconcile_history_start();
    assert_eq!(chat.history_start.as_deref(), Some("300"));
    assert!(chat.reveal_history(false));
    assert_eq!(chat.history_start.as_deref(), Some("100"));
    assert!(
        !chat.has_earlier,
        "all messages are cached; no remote fetch needed"
    );
    assert!(chat.reveal_history(true));
    assert_eq!(chat.visible_messages().len(), 501);
    assert!(!chat.reveal_history(false));
}

#[test]
fn authoritative_history_reset_reanchors_but_stale_results_do_not() {
    let mut state = State::new(true, Some("phone".into()));
    state.chat = Some(Chat {
        device: "mac".into(),
        session: v1::AgentSession {
            id: "s".into(),
            ..Default::default()
        },
        messages: (0..300).map(|i| message(&i.to_string(), "user")).collect(),
        ..Default::default()
    });
    state.chat.as_mut().unwrap().reconcile_history_start();
    let cancelled = CancellationToken::new();
    cancelled.cancel();
    agents::msg_chat(
        &mut state,
        cancelled,
        "mac".into(),
        "s".into(),
        false,
        true,
        Ok((vec![], false)),
    );
    assert_eq!(
        state.chat.as_ref().unwrap().history_start.as_deref(),
        Some("100")
    );
    let changed: Vec<_> = (1000..1200)
        .map(|i| message(&i.to_string(), "user"))
        .collect();
    agents::msg_chat(
        &mut state,
        CancellationToken::new(),
        "mac".into(),
        "s".into(),
        false,
        true,
        Ok((changed, true)),
    );
    assert_eq!(
        state.chat.as_ref().unwrap().history_start.as_deref(),
        Some("1000")
    );
    let earlier: Vec<_> = (920..1000)
        .map(|i| message(&i.to_string(), "user"))
        .collect();
    state.chat.as_mut().unwrap().reveal_history(true);
    agents::msg_chat(
        &mut state,
        CancellationToken::new(),
        "mac".into(),
        "s".into(),
        true,
        false,
        Ok((earlier, true)),
    );
    assert_eq!(
        state.chat.as_ref().unwrap().history_start.as_deref(),
        Some("920")
    );
    assert_eq!(state.chat.as_ref().unwrap().messages.len(), 280);
}

#[test]
fn native_inventory_preserves_partial_failures_and_rejects_late_source_choices() {
    let mut state = State::new(false, Some("local".into()));
    state.terminals.device_id = "local".into();
    state.terminals_epoch = 4;
    state.terminals_pending = 1;
    let list = v1::TerminalSessionList {
        sessions: vec![v1::TerminalSession {
            id: "ghostty:fixture".into(),
            source: "ghostty".into(),
            device_id: "local".into(),
            ..Default::default()
        }],
        sources: vec![v1::TerminalSource {
            source: "ghostty".into(),
            label: "Ghostty".into(),
        }],
        errors: vec!["iTerm2 API disabled".into()],
    };
    terminals::msg_terminals(&mut state, 3, "local".into(), Ok(list.clone()));
    assert!(state.terminals.sources.is_empty());
    terminals::msg_terminals(&mut state, 4, "local".into(), Ok(list));
    assert!(state.terminals.can_create);
    assert_eq!(state.terminals.sources[0].label, "Ghostty");
    assert_eq!(state.terminals.sessions[0].id, "ghostty:fixture");
    assert_eq!(state.terminals.errors, ["iTerm2 API disabled"]);
    assert!(!state.terminals.loading);
}

#[test]
fn native_frames_preserve_host_choices_across_cursor_deltas_and_clear_shrunken_rows() {
    let mut state = State::new(false, Some("local".into()));
    state.terminal = Some(remote_terminal());
    let full = v1::TerminalFrame {
        session: "s".into(),
        rev: 1,
        full: true,
        cols: 80,
        rows: 2,
        lines: vec![
            v1::TerminalLine {
                index: 0,
                text: "first".into(),
                ..Default::default()
            },
            v1::TerminalLine {
                index: 1,
                text: "second".into(),
                ..Default::default()
            },
        ],
        native_sources: vec![v1::TerminalSource {
            source: "ghostty".into(),
            label: "Ghostty".into(),
        }],
        ..Default::default()
    };
    terminals::msg_terminal_frame(&mut state, "other-device".into(), "s".into(), full.clone());
    terminals::msg_terminal_frame(
        &mut state,
        "mac".into(),
        "other-session".into(),
        full.clone(),
    );
    assert!(state.terminal.as_ref().unwrap().lines.is_empty());
    terminals::msg_terminal_frame(&mut state, "mac".into(), "s".into(), full);
    terminals::msg_terminal_frame(
        &mut state,
        "mac".into(),
        "s".into(),
        v1::TerminalFrame {
            session: "s".into(),
            rev: 2,
            cursor_x: 4,
            cursor_y: 1,
            ..Default::default()
        },
    );
    let terminal = state.terminal.as_ref().unwrap();
    assert_eq!(terminal.cursor, (4, 1));
    assert_eq!(terminal.lines.len(), 2);
    assert_eq!(terminal.native_sources[0].source, "ghostty");
    terminals::msg_terminal_frame(
        &mut state,
        "mac".into(),
        "s".into(),
        v1::TerminalFrame {
            session: "s".into(),
            rev: 3,
            full: true,
            rows: 1,
            cols: 80,
            lines: vec![v1::TerminalLine {
                text: "short".into(),
                ..Default::default()
            }],
            ..Default::default()
        },
    );
    assert_eq!(state.terminal.as_ref().unwrap().lines.len(), 1);
    terminals::msg_terminal_frame(
        &mut state,
        "mac".into(),
        "s".into(),
        v1::TerminalFrame {
            session: "s".into(),
            rev: 4,
            ended: true,
            ..Default::default()
        },
    );
    assert!(state.terminal.as_ref().unwrap().ended);
    assert_eq!(state.terminal.as_ref().unwrap().status, "ended");
}

#[tokio::test]
async fn native_creation_requires_an_advertised_host_and_a_live_managed_handle() {
    let (_dir, ctx, mut state) = phone().await;
    state.terminals.device_id = "*".into();
    terminals::intent_terminal_create(
        &mut state,
        &ctx,
        app::TerminalCreate {
            source: "ghostty".into(),
            ..Default::default()
        },
    );
    assert_eq!(state.busy, 0);
    state.terminals.device_id = "mac".into();
    terminals::intent_terminal_create(
        &mut state,
        &ctx,
        app::TerminalCreate {
            source: "unadvertised".into(),
            ..Default::default()
        },
    );
    assert_eq!(state.busy, 0);
    for (source, ended, requested) in [
        ("iterm2", false, "ghostty"),
        ("tmux", true, "ghostty"),
        ("tmux", false, "unadvertised"),
    ] {
        state.terminal = Some(Terminal {
            source: source.into(),
            ended,
            native_sources: vec![v1::TerminalSource {
                source: "ghostty".into(),
                label: "Ghostty".into(),
            }],
            ..remote_terminal()
        });
        terminals::intent_terminal_create(
            &mut state,
            &ctx,
            app::TerminalCreate {
                source: requested.into(),
                attach_managed: true,
                ..Default::default()
            },
        );
        assert_eq!(
            state.busy, 0,
            "invalid creation must never start a network operation"
        );
    }
}

#[tokio::test]
async fn native_creation_targets_the_terminal_device_and_sends_only_the_managed_handle() {
    use std::sync::{Arc, Mutex};
    struct Server(
        Mutex<Vec<v1::TerminalCreateRequest>>,
        std::sync::atomic::AtomicBool,
    );
    impl muster_rpc::Handler for Server {
        fn call(
            &self,
            _: muster_rpc::Context,
            request: Req,
            _: muster_rpc::EventSink,
        ) -> muster_rpc::HandlerFuture<'_> {
            Box::pin(async move {
                match request {
                    Req::DeviceInfo(_) => Resp::DeviceInfo(v1::DeviceInfo {
                        device_id: "local".into(),
                        services: if self.1.load(std::sync::atomic::Ordering::SeqCst) {
                            vec!["terminal.create.locked.v1".into()]
                        } else {
                            vec![]
                        },
                        ..Default::default()
                    }),
                    Req::TerminalCreate(r) => {
                        let session = v1::TerminalSession {
                            id: format!("{}:new", r.source),
                            source: r.source.clone(),
                            ..Default::default()
                        };
                        self.0.lock().unwrap().push(r);
                        Resp::TerminalSession(session)
                    }
                    _ => Resp::Ack(v1::Ack {}),
                }
            })
        }
    }
    let dir = tempfile::tempdir().unwrap();
    let socket = dir.path().join("fixture.sock");
    let listener = tokio::net::UnixListener::bind(&socket).unwrap();
    let server = Arc::new(Server(Mutex::new(Vec::new()), false.into()));
    let handler = server.clone();
    let accept = tokio::spawn(async move {
        while let Ok((stream, _)) = listener.accept().await {
            tokio::spawn(muster_rpc::serve(
                stream,
                muster_rpc::Principal::Local,
                handler.clone(),
            ));
        }
    });
    let shell = Arc::new(QuietShell);
    let backend = Arc::new(
        crate::backend::Backend::connect(
            &crate::AppConfig {
                dir: dir.path().into(),
                mode: crate::Mode::Socket { path: socket },
                allow_loopback: true,
            },
            shell.clone(),
        )
        .await
        .unwrap(),
    );
    let (tx, mut rx) = tokio::sync::mpsc::unbounded_channel();
    let ctx = Ctx {
        shell,
        backend,
        tx,
        cache: Arc::new(crate::cache::Cache::new(dir.path().join("cache"))),
    };
    let mut state = State::new(false, Some("local".into()));
    state.selected = "unrelated-device".into();
    state.screen = Screen::Chat;
    state.terminals.device_id = "local".into();
    state.terminals.sources = ["iterm2", "ghostty"]
        .into_iter()
        .map(|source| v1::TerminalSource {
            source: source.into(),
            label: source.into(),
        })
        .collect();
    state.terminal = Some(Terminal {
        source: "tmux".into(),
        device: "local".into(),
        session: "tmux-exact-generation".into(),
        native_sources: state.terminals.sources.clone(),
        ..remote_terminal()
    });
    terminals::intent_terminal_create(
        &mut state,
        &ctx,
        app::TerminalCreate {
            source: "ghostty".into(),
            ..Default::default()
        },
    );
    let Some(Loop::Msg(message)) = rx.recv().await else {
        panic!()
    };
    assert!(matches!(
        *message,
        Msg::TerminalCreated { result: Err(_), .. }
    ));
    msg(&mut state, &ctx, *message);
    assert!(
        server.0.lock().unwrap().is_empty(),
        "old targets must not create a blank window"
    );
    server.1.store(true, std::sync::atomic::Ordering::SeqCst);
    for attach_managed in [false, true] {
        for source in ["iterm2", "ghostty"] {
            terminals::intent_terminal_create(
                &mut state,
                &ctx,
                app::TerminalCreate {
                    source: source.into(),
                    attach_managed,
                    ..Default::default()
                },
            );
            let Loop::Msg(message) =
                tokio::time::timeout(std::time::Duration::from_secs(2), rx.recv())
                    .await
                    .unwrap()
                    .unwrap()
            else {
                panic!()
            };
            if attach_managed {
                assert!(matches!(
                    *message,
                    Msg::TerminalCreated {
                        attach_managed: true,
                        result: Ok(_),
                        ..
                    }
                ));
                msg(&mut state, &ctx, *message);
                assert_eq!(state.screen, Screen::Chat);
                assert_eq!(
                    state.terminal.as_ref().unwrap().session,
                    "tmux-exact-generation"
                );
            } else {
                assert!(
                    matches!(*message, Msg::TerminalCreated { ref device, result: Ok(ref session), .. } if device == "local" && session.source == source)
                );
            }
            let requests = server.0.lock().unwrap();
            let last = requests.last().unwrap();
            assert_eq!(last.source, source);
            assert_eq!(
                last.managed_session,
                if attach_managed {
                    "tmux-exact-generation"
                } else {
                    ""
                }
            );
            assert!(!last.op_id.is_empty());
        }
    }
    let requests = server.0.lock().unwrap();
    assert_eq!(requests.len(), 4);
    let unique: std::collections::HashSet<_> = requests.iter().map(|r| &r.op_id).collect();
    assert_eq!(unique.len(), 4);
    accept.abort();
}

/// Delay acknowledgements at the RPC boundary: later keys must not reach the
/// host first, and Unicode/paste/Return must share the exact same byte stream.
#[tokio::test]
async fn remote_terminal_bursts_wait_for_ack_and_preserve_input_order() {
    use std::sync::Arc;
    type InputCall = (v1::TerminalInputRequest, tokio::sync::oneshot::Sender<Resp>);
    struct Server(mpsc::UnboundedSender<InputCall>);
    impl muster_rpc::Handler for Server {
        fn call(
            &self,
            _: muster_rpc::Context,
            request: Req,
            _: muster_rpc::EventSink,
        ) -> muster_rpc::HandlerFuture<'_> {
            Box::pin(async move {
                match request {
                    Req::DeviceInfo(_) => Resp::DeviceInfo(v1::DeviceInfo {
                        device_id: "local".into(),
                        ..Default::default()
                    }),
                    Req::TerminalInput(request) => {
                        let (tx, rx) = tokio::sync::oneshot::channel();
                        self.0.send((request, tx)).unwrap();
                        rx.await.unwrap()
                    }
                    _ => Resp::Ack(v1::Ack {}),
                }
            })
        }
    }
    let dir = tempfile::tempdir().unwrap();
    let socket = dir.path().join("input.sock");
    let listener = tokio::net::UnixListener::bind(&socket).unwrap();
    let (calls, mut received) = mpsc::unbounded_channel();
    let server = Arc::new(Server(calls));
    let accept = tokio::spawn(async move {
        while let Ok((stream, _)) = listener.accept().await {
            tokio::spawn(muster_rpc::serve(
                stream,
                muster_rpc::Principal::Local,
                server.clone(),
            ));
        }
    });
    let shell = Arc::new(QuietShell);
    let backend = Arc::new(
        crate::backend::Backend::connect(
            &crate::AppConfig {
                dir: dir.path().into(),
                mode: crate::Mode::Socket { path: socket },
                allow_loopback: true,
            },
            shell.clone(),
        )
        .await
        .unwrap(),
    );
    let (tx, mut rx) = mpsc::unbounded_channel();
    let ctx = Ctx {
        shell,
        backend,
        tx,
        cache: Arc::new(crate::cache::Cache::new(dir.path().join("cache"))),
    };
    let mut state = State::new(false, Some("local".into()));
    state.screen = Screen::Terminal;
    for source in ["iterm2", "ghostty"] {
        state.terminal = Some(Terminal {
            device: "local".into(),
            source: source.into(),
            ..remote_terminal()
        });
        terminal_input(&mut state, &ctx, b"printf '".to_vec());
        let (first, first_ack) = tokio::time::timeout(Duration::from_secs(2), received.recv())
            .await
            .unwrap()
            .unwrap();
        assert_eq!(first.bytes, b"printf '");
        for ch in "ordered ".chars() {
            terminal_input(&mut state, &ctx, ch.to_string().into_bytes());
        }
        terminals::intent_terminal_paste(
            &mut state,
            &ctx,
            app::TerminalPaste {
                text: "中文🙂'".into(),
            },
        );
        terminals::intent_terminal_key(
            &mut state,
            &ctx,
            app::TerminalKey {
                key: "enter".into(),
            },
        );
        assert!(
            tokio::time::timeout(Duration::from_millis(50), received.recv())
                .await
                .is_err()
        );
        first_ack
            .send(Resp::OpResult(v1::OpResult {
                status: v1::op_result::Status::Applied as i32,
                ..Default::default()
            }))
            .unwrap();
        let Loop::Msg(reply) = tokio::time::timeout(Duration::from_secs(2), rx.recv())
            .await
            .unwrap()
            .unwrap()
        else {
            panic!()
        };
        msg(&mut state, &ctx, *reply);
        let (second, second_ack) = tokio::time::timeout(Duration::from_secs(2), received.recv())
            .await
            .unwrap()
            .unwrap();
        assert_eq!(second.bytes, "ordered 中文🙂'\r".as_bytes());
        assert_eq!(second.session, first.session);
        assert_ne!(second.op_id, first.op_id);
        second_ack
            .send(Resp::OpResult(v1::OpResult {
                status: v1::op_result::Status::Duplicate as i32,
                ..Default::default()
            }))
            .unwrap();
        let Loop::Msg(reply) = tokio::time::timeout(Duration::from_secs(2), rx.recv())
            .await
            .unwrap()
            .unwrap()
        else {
            panic!()
        };
        msg(&mut state, &ctx, *reply);
        let TerminalKind::Remote { input, .. } = &state.terminal.as_ref().unwrap().kind else {
            panic!()
        };
        assert!(input.in_flight.is_none() && input.pending.is_empty() && !input.stopped);
    }
    accept.abort();
}

#[tokio::test]
async fn terminal_input_failure_overflow_and_background_discard_queued_return() {
    let (_dir, ctx, mut state) = phone().await;
    state.screen = Screen::Terminal;
    for result in [
        Err(Error::Unavailable("lost acknowledgement".into())),
        Ok(v1::OpResult {
            status: v1::op_result::Status::Unknown as i32,
            ..Default::default()
        }),
        Ok(v1::OpResult {
            status: v1::op_result::Status::Rejected as i32,
            ..Default::default()
        }),
    ] {
        let mut terminal = remote_terminal();
        let TerminalKind::Remote { input, .. } = &mut terminal.kind else {
            panic!()
        };
        input.in_flight = Some("first".into());
        input.pending = b"dangerous suffix\r".to_vec();
        state.terminal = Some(terminal);
        terminals::msg_terminal_input_done(
            &mut state,
            &ctx,
            "mac".into(),
            Some(("s".into(), "first".into())),
            result,
        );
        terminal_input(&mut state, &ctx, b"\r".to_vec());
        let TerminalKind::Remote { input, .. } = &state.terminal.as_ref().unwrap().kind else {
            panic!()
        };
        assert!(input.stopped && input.pending.is_empty() && input.in_flight.is_none());
    }
    for background in [false, true] {
        let mut terminal = remote_terminal();
        let TerminalKind::Remote { input, .. } = &mut terminal.kind else {
            panic!()
        };
        input.in_flight = Some("old".into());
        input.pending = vec![b'x'; muster_terminal::MAX_PASTE];
        state.terminal = Some(terminal);
        if background {
            terminals::background_terminal(&mut state);
        } else {
            terminal_input(&mut state, &ctx, b"\r".to_vec());
        }
        terminals::msg_terminal_input_done(
            &mut state,
            &ctx,
            "mac".into(),
            Some(("s".into(), "old".into())),
            Ok(v1::OpResult::default()),
        );
        let TerminalKind::Remote { input, .. } = &state.terminal.as_ref().unwrap().kind else {
            panic!()
        };
        assert!(input.stopped && input.pending.is_empty() && input.in_flight.is_none());
    }
    // Reopening the same remote session starts a distinct input generation.
    let mut terminal = remote_terminal();
    let TerminalKind::Remote { input, .. } = &mut terminal.kind else {
        panic!()
    };
    input.in_flight = Some("new".into());
    input.pending = b"new input".to_vec();
    state.terminal = Some(terminal);
    terminals::msg_terminal_input_done(
        &mut state,
        &ctx,
        "mac".into(),
        Some(("s".into(), "old".into())),
        Err(Error::Unavailable("late".into())),
    );
    let TerminalKind::Remote { input, .. } = &state.terminal.as_ref().unwrap().kind else {
        panic!()
    };
    assert_eq!(input.in_flight.as_deref(), Some("new"));
    assert_eq!(input.pending, b"new input");
    assert!(!input.stopped);
    state.stack = vec![Screen::Terminals];
    intent_navigate(
        &mut state,
        &ctx,
        app::Navigate {
            back: true,
            ..Default::default()
        },
    );
    assert!(state.terminal.is_none());
}

#[test]
fn pending_terminal_creation_is_cancelled_on_navigation_or_background() {
    for action in 0..4 {
        let mut state = State::new(false, Some("local".into()));
        state.screen = Screen::Terminals;
        let (watch, token) = state.new_watch();
        state.terminal_creation = Some(("create-1".into(), watch));
        match action {
            0 => state.go(Screen::Chat),
            1 => state.back(),
            2 => state.detach_session_terminal(),
            _ => terminals::background_terminal(&mut state),
        }
        assert!(state.terminal_creation.is_none());
        assert!(token.is_cancelled());
    }
}

#[tokio::test]
async fn late_creation_reply_cannot_open_a_window_after_navigation() {
    let (_dir, ctx, mut state) = phone().await;
    state.screen = Screen::Terminals;
    let (watch, token) = state.new_watch();
    state.terminal_creation = Some(("old".into(), watch));
    state.busy = 1;
    state.go(Screen::Agents);
    terminals::msg_terminal_created(
        &mut state,
        &ctx,
        "mac".into(),
        "old".into(),
        false,
        Ok(v1::TerminalSession {
            id: "ghostty:new".into(),
            ..Default::default()
        }),
    );
    assert!(token.is_cancelled());
    assert_eq!(state.screen, Screen::Agents);
    assert!(state.terminal.is_none());
    assert_eq!(state.busy, 0);
}
