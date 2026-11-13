//! State → ViewModel. Pure projection: nothing here decides anything.

use muster_proto::Message as _;
use muster_proto::{app, v1};
use std::collections::{HashMap, HashSet};

use crate::Ctx;
use crate::state::State;

#[cfg(test)]
pub fn view_model(state: &State) -> app::ViewModel {
    project(state, true)
}

/// Only the list badge combines descendant activity. Stored sessions and the
/// chat's send/interrupt decisions keep the parent's own state.
fn session_row(session: &muster_proto::v1::AgentSession) -> muster_proto::v1::AgentSession {
    let mut row = session.clone();
    row.state = match (session.state.as_str(), session.descendant_activity.as_str()) {
        ("waiting_approval", _) | (_, "waiting_approval") => "waiting_approval",
        ("running", _) | (_, "running") => "running",
        (state, _) => state,
    }
    .into();
    row
}

/// A send this client made that the conversation does not account for yet,
/// shown in the reading flow so nothing the user wrote is invisible. The title
/// states exactly what is known; it never claims delivery.
fn admitted_block(send: &crate::state::AdmittedSend) -> app::ChatBlock {
    app::ChatBlock {
        id: format!("admitted:{}", send.op_id),
        kind: "admitted_send".into(),
        title: if send.unknown {
            "送达未确认，请勿重复发送".into()
        } else if send.user_confirmed {
            "已由你确认送达（非提供方回执），未重发".into()
        } else if !send.error.is_empty() {
            format!("未发送：{}", send.error)
        } else if send.sending {
            "发送中".into()
        } else if send.queued {
            "已排队，本轮结束后发送".into()
        } else {
            "已提交，尚未出现在会话中".into()
        },
        message: Some(muster_proto::v1::AgentMessage {
            id: format!("admitted:{}", send.op_id),
            role: "user".into(),
            kind: "text".into(),
            text: send.text.clone(),
            ..Default::default()
        }),
        completed: false,
        ..Default::default()
    }
}

fn session_timeline(
    chat: &crate::state::Chat,
    admitted: &[crate::state::AdmittedSend],
) -> Vec<app::ChatBlock> {
    let mut blocks = chat.timeline_index.borrow_mut().window(
        &chat.messages,
        chat.history_start_index(),
        chat.history_end_index(),
        matches!(chat.session.state.as_str(), "running" | "waiting_approval"),
        chat.session.mode == "orca",
        &chat.work,
    );
    crate::timeline::activity_indicator(
        &mut blocks,
        &chat.session.state,
        &chat.session.descendant_activity,
    );
    fn decorate(blocks: &mut [app::ChatBlock], allowed: bool) {
        for b in blocks {
            if let Some(m) = &b.message {
                b.parts = muster_terminal::commands::parts(m, allowed);
            }
            decorate(&mut b.children, allowed);
        }
    }
    decorate(&mut blocks, crate::update::code_runs::eligible(chat));
    blocks.extend(admitted.iter().map(admitted_block));
    blocks
}

fn project(state: &State, content: bool) -> app::ViewModel {
    let service_devices = state.service_devices();
    let mut vm = app::ViewModel {
        session_load: state.chat.as_ref().and_then(|chat| {
            chat.load_trace
                .as_ref()
                .map(|trace| app::SessionLoadMarker {
                    trace_id: trace.id().into(),
                    history_ready: trace.history_ready(),
                })
        }),
        browser: Some(state.browser.clone()),
        desktop_setup: Some(desktop_setup(state)),
        desktop_update: Some(state.desktop_update.clone()),
        multica: Some(state.multica.clone()),
        multica_workspace: Some(crate::update::multica::view(state)),
        multica_page: Some(state.multica_page.clone()),
        vision: Some(state.vision.clone()),
        selected_device_has_services: service_devices
            .iter()
            .any(|m| m.device_id == state.target()),
        local_device_has_services: service_devices.iter().any(|m| m.is_self),
        service_devices,
        screen: state.screen as i32,
        can_go_back: state.can_go_back(),
        navigation_path: state.navigation_path(),
        workspace: state.workspace() as i32,
        toasts: state.toasts.clone(),
        busy: state.busy > 0,
        self_: state.self_info.clone(),
        network: state.network.clone(),
        pending: state.pending.clone(),
        selected_device: state.target(),
        can_invite: state.can_invite(),
        invite: state.invite.clone().filter(|_| state.can_invite()),
        join: Some(app::JoinView {
            message: join_text(&state.join),
            ..state.join.clone()
        }),
        // Presence may have changed since the list loaded.
        agents: Some(app::AgentsView {
            create_hosts: state
                .reachable_service_devices()
                .into_iter()
                .map(|m| muster_proto::v1::MemberView {
                    device_id: m.device_id,
                    name: m.name,
                    kind: m.kind,
                    is_self: m.is_self,
                    ..Default::default()
                })
                .collect(),
            create_agents: vec!["codex".into(), "claude".into()],
            native_create_hosts: crate::update::native_create::hosts(state)
                .into_iter()
                .map(|m| muster_proto::v1::MemberView {
                    device_id: m.device_id,
                    name: m.name,
                    kind: m.kind,
                    is_self: m.is_self,
                    ..Default::default()
                })
                .collect(),
            native_create_apps: crate::update::native_create::APPS
                .iter()
                .map(|app| (*app).into())
                .collect(),
            device_id: state.agents.device_id.clone(),
            native_sections: state.agents.native_sections.clone(),
            errors: state.agents.errors.clone(),
            loading: state.agents.loading,
            creating: state.agents.creating,
            query: state.agents.query.clone(),
            searching: state.agents.searching,
            search_has_more: state.agents.search_has_more,
            search_notices: state.agents.search_notices.clone(),
            native_creating: state.agents.native_creating,
            native_create_draft: state.agents.native_create_draft.clone(),
            sessions: Vec::new(),
            sections: Vec::new(),
        }),
        desktop_chats: Some(app::DesktopChatsView {
            choices: muster_agent::desktop_chat::ChatApp::ALL
                .iter()
                .map(|app| app::ChatProvider {
                    id: app.provider().as_str().into(),
                    label: app.name().into(),
                })
                .collect(),
            ..state.desktop_chats.clone()
        }),
        chat: None,
        terminals: Some(app::TerminalsView {
            sessions: state
                .terminals
                .sessions
                .iter()
                .filter(|s| !state.offline(&s.device_id))
                .cloned()
                .collect(),
            authorization: terminal_authorization(state),
            ..state.terminals.clone()
        }),
        terminal: None,
        policy: state.policy.clone(),
        embedded: state.embedded,
        require_locked_use_auth: state.locked_use_auth.as_ref().is_some_and(|p| p.required()),
        files: state.files.clone(),
        file: state.file.clone(),
        git: state.git.clone(),
        device_access: state
            .network
            .as_ref()
            .and_then(|n| n.members.iter().find(|m| m.device_id == state.target()))
            .map(|m| crate::access::view(&m.scopes)),
        join_access: Some(crate::access::join_defaults()),
        device_actions: Some(device_actions(state)),
        device_locked_use: crate::update::manual_locked_use::views(state),
    };
    if content {
        let agents = vm.agents.as_mut().unwrap();
        (agents.sessions, agents.sections) = catalog_rows(state);
    }
    if state.screen == app::Screen::Chat
        && let Some(chat) = &state.chat
    {
        // Desktop chats take text only: no attachments offered.
        let attachments = !matches!(chat.session.mode.as_str(), "chat" | "orca");
        let (queued, interactions): (Vec<_>, Vec<_>) =
            muster_agent::queue::ordered_interactions(&chat.interactions)
                .into_iter()
                .filter(|_| !chat.session.read_only)
                .partition(|i| i.kind == muster_agent::queue::KIND);
        vm.chat = Some(app::ChatView {
            device_id: chat.device.clone(),
            session: Some({
                let mut session = chat.session.clone();
                for child in session
                    .subagents
                    .iter_mut()
                    .chain(session.subsessions.iter_mut())
                {
                    *child = session_row(child);
                }
                session
            }),
            message_count: chat.messages.len() as u32,
            content_revision: chat.content_revision,
            timeline: if content {
                session_timeline(chat, crate::update::admitted::open(state))
            } else {
                Vec::new()
            },
            trace: if content {
                crate::trace::spans(chat.visible_messages(), &chat.session.state)
            } else {
                Vec::new()
            },
            compact_wrap_up_ids: interactions
                .iter()
                .filter(|i| {
                    chat.session.provider == "claude"
                        && matches!(chat.session.mode.as_str(), "" | "code")
                        && i.kind == "question"
                        && i.optional
                        && muster_agent::claude_cards::is_wrap(i)
                        && !i.questions[0].multi_select
                        && !i.questions[0].options.is_empty()
                })
                .map(|i| i.id.clone())
                .collect(),
            interactions,
            queue: Some(app::QueuedMessagesView {
                messages: queued,
                submitting: chat.sending || chat.loading || !chat.responding_interaction.is_empty(),
                error: if chat.response_interaction.starts_with("queued:") {
                    chat.response_error.clone()
                } else {
                    String::new()
                },
                outcome_unknown: chat.response_unknown,
            }),
            has_earlier: chat.has_earlier || chat.history_start_index() > 0,
            has_newer: chat.history_end_index() < chat.messages.len(),
            sending: chat.sending || chat.loading,
            responding_interaction: chat.responding_interaction.clone(),
            response_error: chat.response_error.clone(),
            response_unknown: chat.response_unknown,
            loading: chat.loading,
            load_error: chat.load_error.clone(),
            sent_draft_id: chat.sent_draft_id.clone(),
            unconfirmed: chat.unconfirmed.clone(),
            draft: Some(chat.draft.clone()),
            attachment_bytes_limit: if attachments {
                muster_proto::attachments::MAX_BYTES as u64
            } else {
                0
            },
            attachment_count_limit: if attachments {
                muster_proto::attachments::MAX_COUNT as u32
            } else {
                0
            },
            locked_use_status: if state.locked.is_some() {
                if state.presence.is_some() {
                    "Confirm this operation on your device"
                } else {
                    "Mac operation in progress; waiting for confirmed relock"
                }
            } else {
                ""
            }
            .into(),
            can_cancel_locked_use: state.locked.is_some(),
            chat_options: chat.options.clone(),
            chat_options_busy: chat.options_busy,
            chat_options_error: chat.options_error.clone(),
            // A cloud session (`mode = "cloud"`) takes prompts only: its
            // approvals, interrupts and files stay with the provider's web app.
            can_interrupt: matches!(chat.session.state.as_str(), "running" | "waiting_approval")
                && chat.session.mode != "cloud"
                && offers(&chat.session, |c| c.interrupt),
            can_choose_model: !matches!(chat.session.mode.as_str(), "cloud" | "orca")
                && offers(&chat.session, |c| c.model_options),
            can_browse_files: chat.session.mode != "orca"
                && chat.session.mode != "chat"
                && chat.session.mode != "cloud"
                && offers(&chat.session, |c| c.files),
            provider_label: provider_label(&chat.session),
            composer_suggestion: if chat.session.read_only {
                String::new()
            } else {
                chat.completion
                    .as_ref()
                    .filter(|(turn, _)| {
                        *turn == chat.content_revision
                            && !matches!(
                                chat.session.state.as_str(),
                                "running" | "waiting_approval"
                            )
                    })
                    .map(|(_, text)| text.clone())
                    .unwrap_or_else(|| chat.session.composer_suggestion.clone())
            },
            can_complete: matches!(chat.session.provider.as_str(), "claude" | "codex")
                && chat.session.mode == "code"
                && !chat.session.read_only
                && !chat.sending
                && !matches!(chat.session.state.as_str(), "running" | "waiting_approval"),
            terminal: Some(app::SessionTerminalView {
                available: !chat.session.cwd.is_empty() && chat.session.mode != "chat",
                cwd: chat.session.cwd.clone(),
                busy: state.session_terminal.busy,
                error: state.session_terminal.error.clone(),
                attached: state.session_terminal.active
                    && state.terminal.as_ref().is_some_and(|t| !t.ended),
                can_end: state.session_terminal.active && !state.session_terminal.busy,
                can_restart: !chat.session.cwd.is_empty()
                    && chat.session.mode != "chat"
                    && !state.session_terminal.busy,
            }),
            code_preview: crate::update::code_runs::preview(state),
            code_runs: crate::update::code_runs::views(state, &chat.device, &chat.session.id),
            code_error: state
                .code
                .errors
                .get(&crate::update::code_runs::origin(
                    &chat.device,
                    &chat.session.id,
                ))
                .cloned()
                .unwrap_or_default(),
            completing: chat.completing,
            completion_error: chat.completion_error.clone(),
            composer_placeholder: composer_placeholder(
                &chat.session,
                chat.session.message_count > 0 || !chat.messages.is_empty(),
            ),
        });
    }
    // Frames stream at up to 60 Hz: only the terminal's own screen shows them.
    if let Some(terminal) = state.terminal.as_ref().filter(|_| {
        state.screen == app::Screen::Terminal
            || (state.screen == app::Screen::Chat && state.session_terminal.active)
    }) {
        vm.terminal = Some(app::TerminalView {
            device_id: terminal.device.clone(),
            session: terminal.session.clone(),
            source: terminal.source.clone(),
            resizable: terminal.resizable(),
            cols: terminal.cols,
            rows: terminal.rows,
            lines: terminal.lines.clone(),
            cursor_x: terminal.cursor.0,
            cursor_y: terminal.cursor.1,
            ended: terminal.ended,
            status: terminal.status.clone(),
            can_paste: !terminal.ended,
            desktop_attach_command: terminal.desktop_attach_command.clone(),
            iterm2_attach_command: terminal.iterm2_attach_command.clone(),
            native_sources: terminal.native_sources.clone(),
        });
    }
    vm
}

/// Compare small presentation metadata before cloning, rebuilding or encoding
/// the transcript. Shells continue to receive complete, authoritative snapshots.
/// A member's link label without its RTT ("relay · 47 ms" → "relay"). The
/// daemon refreshes the RTT with every keepalive; the path is what a list
/// or chat screen shows changing.
fn strip_rtt(link: &mut String) {
    if let Some((path, rtt)) = link.rsplit_once(" · ")
        && rtt
            .strip_suffix(" ms")
            .is_some_and(|n| !n.is_empty() && n.bytes().all(|b| b.is_ascii_digit()))
    {
        link.truncate(path.len());
    }
}

fn catalog_rows(state: &State) -> (Vec<v1::AgentSession>, Vec<app::AgentCatalogSection>) {
    let visible = |rows: &[v1::AgentSession]| {
        rows.iter()
            .filter(|s| !state.offline(&s.device_id))
            .map(session_row)
            .collect::<Vec<_>>()
    };
    let sessions = visible(&state.agents.sessions);
    let sections = if state.agents.query.trim().is_empty() {
        crate::catalog::project(state, &sessions)
    } else {
        crate::catalog::project(state, &visible(&state.agents_search_sessions))
    };
    (sessions, sections)
}

#[derive(Default)]
struct CatalogCache {
    sessions: Vec<v1::AgentSession>,
    search: Vec<v1::AgentSession>,
    native_sections: Vec<v1::AgentSection>,
    groups: Vec<v1::AgentCatalogGroup>,
    device: String,
    searching: bool,
    offline: Vec<String>,
    disclosures: HashMap<String, bool>,
    limits: HashMap<String, usize>,
    pending: HashMap<String, usize>,
    organizing: HashSet<String>,
    rows: Option<(Vec<v1::AgentSession>, Vec<app::AgentCatalogSection>)>,
}

impl CatalogCache {
    fn refresh(&mut self, state: &State) -> bool {
        let offline: Vec<_> = state
            .network
            .iter()
            .flat_map(|n| &n.members)
            .filter(|m| crate::state::member_offline(m))
            .map(|m| m.device_id.clone())
            .collect();
        let searching = !state.agents.query.trim().is_empty();
        let search = if searching {
            state.agents_search_sessions.as_slice()
        } else {
            &[]
        };
        if self.rows.is_some()
            && self.sessions == state.agents.sessions
            && self.search == search
            && self.native_sections == state.agents.native_sections
            && self.groups == state.agent_catalog_groups
            && self.device == state.agents.device_id
            && self.searching == searching
            && self.offline == offline
            && self.disclosures == state.agent_disclosures
            && self.limits == state.agent_catalog_limits
            && self.pending == state.agent_catalog_pending
            && self.organizing == state.agent_organizing
        {
            return false;
        }
        self.sessions.clone_from(&state.agents.sessions);
        self.search = search.to_vec();
        self.native_sections
            .clone_from(&state.agents.native_sections);
        self.groups.clone_from(&state.agent_catalog_groups);
        self.device.clone_from(&state.agents.device_id);
        self.searching = searching;
        self.offline = offline;
        self.disclosures.clone_from(&state.agent_disclosures);
        self.limits.clone_from(&state.agent_catalog_limits);
        self.pending.clone_from(&state.agent_catalog_pending);
        self.organizing.clone_from(&state.agent_organizing);
        let rows = catalog_rows(state);
        if self.rows.as_ref() == Some(&rows) {
            return false;
        }
        self.rows = Some(rows);
        true
    }
}

#[derive(Default)]
pub(crate) struct Renderer {
    previous: Option<app::ViewModel>,
    catalog: CatalogCache,
    content_key: Option<(u64, String, String, bool)>,
    timeline: Vec<app::ChatBlock>,
    trace: Vec<app::TraceSpan>,
    #[cfg(test)]
    timeline_builds: usize,
}
impl Renderer {
    fn prepare(&mut self, state: &State) -> Option<Vec<u8>> {
        if state.embedded && !state.foreground {
            return None;
        }
        let mut vm = project(state, false);
        let key = state.chat.as_ref().filter(|_| vm.chat.is_some()).map(|c| {
            (
                c.content_revision,
                c.session.state.clone(),
                c.session.descendant_activity.clone(),
                c.session.mode == "orca",
            )
        });
        let mut comparison = vm.clone();
        // RTT and uptime aren't displayed in the agent/chat workspaces. Keep
        // authoritative values in State and emit them when opening details.
        // Lock, cover, permissions, membership and connection changes always render.
        if matches!(
            state.screen,
            app::Screen::Agents | app::Screen::Home | app::Screen::Chat | app::Screen::DesktopChats
        ) {
            if let Some(info) = &mut comparison.self_
                && let Some(device) = &mut info.state
            {
                device.uptime_secs = 0;
            }
            if let Some(network) = &mut comparison.network {
                for member in &mut network.members {
                    member.rtt_ms = 0;
                    strip_rtt(&mut member.link);
                }
            }
            for member in &mut comparison.service_devices {
                member.rtt_ms = 0;
                strip_rtt(&mut member.link);
            }
        }
        let catalog_changed = self.catalog.refresh(state);
        if !catalog_changed
            && self.content_key == key
            && self.previous.as_ref() == Some(&comparison)
        {
            return None;
        }
        if self.content_key != key {
            self.timeline.clear();
            self.trace.clear();
            if let Some(chat) = state.chat.as_ref().filter(|_| key.is_some()) {
                self.timeline = session_timeline(chat, crate::update::admitted::open(state));
                self.trace = crate::trace::spans(chat.visible_messages(), &chat.session.state);
                #[cfg(test)]
                {
                    self.timeline_builds += 1;
                }
            }
            self.content_key = key;
        }
        self.previous = Some(comparison);
        if let Some(chat) = &mut vm.chat {
            chat.timeline = self.timeline.clone();
            chat.trace = self.trace.clone();
        }
        let agents = vm.agents.as_mut().unwrap();
        (agents.sessions, agents.sections) = self.catalog.rows.as_ref().unwrap().clone();
        Some(vm.encode_to_vec())
    }
    pub fn push(&mut self, state: &State, ctx: &Ctx) {
        if let Some(bytes) = self.prepare(state) {
            if let Some(chat) = &state.chat
                && let Some(trace) = &chat.load_trace
            {
                trace.published(trace.history_ready(), bytes.len());
            }
            ctx.shell.on_view(&bytes);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use muster_proto::v1;

    #[test]
    fn session_list_reflects_descendants_without_changing_chat_control_state() {
        let mut state = State::new(false, Some("local".into()));
        state.screen = app::Screen::Chat;
        for (own, descendant, expected) in [
            ("idle", "running", "running"),
            ("unknown", "running", "running"),
            ("ended", "running", "running"),
            ("running", "waiting_approval", "waiting_approval"),
            ("waiting_approval", "running", "waiting_approval"),
            ("idle", "", "idle"),
            ("running", "", "running"),
            ("idle", "unknown", "idle"),
        ] {
            let session = v1::AgentSession {
                id: "parent".into(),
                provider: "codex".into(),
                state: own.into(),
                descendant_activity: descendant.into(),
                ..Default::default()
            };
            state.agents.sessions = vec![session.clone()];
            state.chat = Some(crate::state::Chat {
                session,
                ..Default::default()
            });
            let vm = view_model(&state);
            assert_eq!(vm.agents.unwrap().sessions[0].state, expected);
            let chat = vm.chat.unwrap();
            assert_eq!(chat.session.unwrap().state, own);
            assert_eq!(
                chat.can_interrupt,
                matches!(own, "running" | "waiting_approval")
            );
            assert_eq!(state.agents.sessions[0].state, own);
        }
    }

    #[test]
    fn child_completion_clears_list_and_content_in_the_same_projection() {
        let mut state = State::new(false, None);
        state.screen = app::Screen::Chat;
        let mut renderer = Renderer::default();
        for descendant in ["running", "", "running", "waiting_approval", ""] {
            let session = v1::AgentSession {
                id: "parent".into(),
                state: "idle".into(),
                descendant_activity: descendant.into(),
                ..Default::default()
            };
            state.agents.sessions = vec![session.clone()];
            if let Some(chat) = state.chat.as_mut() {
                chat.session = session;
            } else {
                state.chat = Some(crate::state::Chat {
                    session,
                    ..Default::default()
                });
            }
            let bytes = renderer.prepare(&state).unwrap();
            let vm = app::ViewModel::decode(bytes.as_slice()).unwrap();
            assert_eq!(
                vm.agents.unwrap().sessions[0].state,
                if descendant.is_empty() {
                    "idle"
                } else {
                    descendant
                }
            );
            let chat = vm.chat.unwrap();
            assert!(!chat.can_interrupt);
            assert_eq!(
                chat.timeline.iter().any(|b| b.id == "session-activity"),
                !descendant.is_empty()
            );
        }
    }

    #[test]
    fn completed_reply_keeps_its_history_while_worker_or_children_continue() {
        let mut state = State::new(false, None);
        state.screen = app::Screen::Chat;
        let message = |id: &str, kind: &str, phase: &str| v1::AgentMessage {
            id: id.into(),
            role: "assistant".into(),
            kind: kind.into(),
            text: id.into(),
            phase: phase.into(),
            ..Default::default()
        };
        state.chat = Some(crate::state::Chat {
            session: v1::AgentSession {
                provider: "claude".into(),
                state: "running".into(),
                ..Default::default()
            },
            messages: vec![
                message("tool", "tool_call", ""),
                message("reply", "text", "final_answer"),
                message("summary", "turn_summary", ""),
            ],
            ..Default::default()
        });
        let mut renderer = Renderer::default();
        let mut render = |state: &State| {
            let bytes = renderer.prepare(state).unwrap();
            app::ViewModel::decode(bytes.as_slice())
                .unwrap()
                .chat
                .unwrap()
                .timeline
        };
        let running = render(&state);
        assert_eq!(running[0].title, "Worked");
        assert!(running[0].completed);
        assert_eq!(running[1].message.as_ref().unwrap().id, "reply");
        assert_eq!(running.last().unwrap().title, "Working");
        assert!(running.last().unwrap().children.is_empty());
        let session = &mut state.chat.as_mut().unwrap().session;
        session.state = "idle".into();
        session.descendant_activity = "running".into();
        let children = render(&state);
        assert_eq!(children.last().unwrap().title, "Subagents working");
        assert_eq!(children[..children.len() - 1], running[..running.len() - 1]);
        state.chat.as_mut().unwrap().session.descendant_activity = "waiting_approval".into();
        assert_eq!(
            render(&state).last().unwrap().title,
            "Subagents need attention"
        );
        state
            .chat
            .as_mut()
            .unwrap()
            .session
            .descendant_activity
            .clear();
        let finished = render(&state);
        assert!(finished.iter().all(|b| b.id != "session-activity"));
        assert_eq!(finished, running[..running.len() - 1]);
        let chat = state.chat.as_mut().unwrap();
        chat.session.state = "running".into();
        chat.messages = vec![message("live", "tool_call", "")];
        chat.content_revision += 1;
        let live = render(&state);
        assert_eq!(
            live.len(),
            1,
            "ongoing work groups need no duplicate marker"
        );
        assert_eq!(live[0].title, "Working");
        let chat = state.chat.as_mut().unwrap();
        chat.messages.clear();
        chat.content_revision += 1;
        assert_eq!(
            render(&state)[0].title,
            "Working",
            "waiting for the first message is still active"
        );
    }

    /// The 2026-10-04 iPhone heat finding: the daemon refreshes a member's
    /// RTT, and its link label with it, about once a second. On a list or
    /// chat screen that alone must not re-render; a path change must.
    #[test]
    fn rtt_in_link_labels_does_not_rerender_lists_but_a_path_change_does() {
        let mut state = State::new(true, Some("phone".into()));
        state.screen = app::Screen::Agents;
        let member = |link: &str, rtt: u32| v1::MemberView {
            device_id: "mac".into(),
            link: link.into(),
            rtt_ms: rtt,
            ..Default::default()
        };
        state.network = Some(v1::NetworkStatus {
            members: vec![member("relay · 47 ms", 47)],
            ..Default::default()
        });
        let mut renderer = Renderer::default();
        assert!(renderer.prepare(&state).is_some());
        state.network.as_mut().unwrap().members[0] = member("relay · 52 ms", 52);
        assert!(renderer.prepare(&state).is_none());
        state.network.as_mut().unwrap().members[0] = member("direct · 3 ms", 3);
        assert!(renderer.prepare(&state).is_some());
    }

    #[test]
    fn only_the_rtt_is_stripped_from_a_link_label() {
        for (label, path) in [
            ("relay · 47 ms", "relay"),
            ("direct · 3 ms", "direct"),
            ("connected", "connected"),
            ("relay · fast ms", "relay · fast ms"),
            ("relay ·  ms", "relay ·  ms"),
        ] {
            let mut link = label.to_string();
            strip_rtt(&mut link);
            assert_eq!(link, path);
        }
    }

    /// Terminal frames arrive at up to 60 Hz: they are part of the view only
    /// on the terminal's own screen.
    #[test]
    fn a_terminal_is_shown_only_on_its_screen() {
        let mut state = State::new(true, Some("phone".into()));
        state.terminal = Some(crate::state::Terminal {
            device: "mac".into(),
            session: "s".into(),
            kind: crate::state::TerminalKind::Remote {
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
        });
        state.screen = app::Screen::Agents;
        assert!(project(&state, false).terminal.is_none());
        state.screen = app::Screen::Terminal;
        assert!(project(&state, false).terminal.is_some());
    }

    #[test]
    fn hidden_phone_defers_views_until_foreground_without_losing_changes() {
        let mut state = State::new(true, None);
        let mut renderer = Renderer::default();
        assert!(renderer.prepare(&state).is_some());
        state.foreground = false;
        state.screen = app::Screen::Settings;
        assert!(renderer.prepare(&state).is_none());
        state.foreground = true;
        let bytes = renderer.prepare(&state).unwrap();
        assert_eq!(
            app::ViewModel::decode(bytes.as_slice()).unwrap().screen,
            app::Screen::Settings as i32
        );
        assert!(renderer.prepare(&state).is_none());
    }

    #[test]
    fn chat_noops_skip_encoding_and_reuse_timeline_without_hiding_permission_changes() {
        let mut state = State::new(false, Some("local".into()));
        state.screen = app::Screen::Chat;
        state.stack = vec![app::Screen::Agents];
        state.self_info = Some(v1::DeviceInfo {
            state: Some(v1::DeviceState {
                accessibility: "granted".into(),
                ..Default::default()
            }),
            ..Default::default()
        });
        state.network = Some(v1::NetworkStatus {
            has_network: true,
            members: vec![v1::MemberView {
                device_id: "local".into(),
                kind: "desktop".into(),
                ..Default::default()
            }],
            ..Default::default()
        });
        state.chat = Some(crate::state::Chat {
            content_revision: crate::state::next_content_revision(),
            session: v1::AgentSession {
                id: "s".into(),
                state: "idle".into(),
                ..Default::default()
            },
            messages: (0..1000)
                .map(|i| v1::AgentMessage {
                    id: i.to_string(),
                    kind: "text".into(),
                    role: "assistant".into(),
                    text: "long message ".repeat(128),
                    ..Default::default()
                })
                .collect(),
            ..Default::default()
        });
        let mut renderer = Renderer::default();
        let first = renderer.prepare(&state).unwrap();
        let chat = app::ViewModel::decode(first.as_slice())
            .unwrap()
            .chat
            .unwrap();
        let initial_revision = chat.content_revision;
        assert_eq!(chat.message_count, 1000);
        assert_eq!(
            chat.timeline.len(),
            200,
            "each visible reply is its own block"
        );
        for i in 1..=100 {
            state
                .self_info
                .as_mut()
                .unwrap()
                .state
                .as_mut()
                .unwrap()
                .uptime_secs = i;
            state.network.as_mut().unwrap().members[0].rtt_ms = i as u32;
            assert!(renderer.prepare(&state).is_none());
        }
        assert_eq!(renderer.timeline_builds, 1);
        eprintln!(
            "100 unchanged chat updates: 0 encodes/callbacks; avoided {} bytes of full snapshots",
            first.len() * 100
        );
        state
            .self_info
            .as_mut()
            .unwrap()
            .state
            .as_mut()
            .unwrap()
            .accessibility = "missing".into();
        let permissions = renderer.prepare(&state).unwrap();
        let shown = app::ViewModel::decode(permissions.as_slice()).unwrap();
        assert_eq!(shown.self_.unwrap().state.unwrap().accessibility, "missing");
        assert_eq!(renderer.timeline_builds, 1);
        let chat = state.chat.as_mut().unwrap();
        chat.messages.last_mut().unwrap().text = "updated streaming reply".into();
        chat.content_revision = crate::state::next_content_revision();
        let updated = renderer.prepare(&state).unwrap();
        let updated_chat = app::ViewModel::decode(updated.as_slice())
            .unwrap()
            .chat
            .unwrap();
        assert_eq!(updated_chat.message_count, 1000);
        assert_ne!(updated_chat.content_revision, initial_revision);
        let timeline = updated_chat.timeline;
        assert_eq!(
            timeline.last().unwrap().message.as_ref().unwrap().text,
            "updated streaming reply"
        );
        assert_eq!(renderer.timeline_builds, 2);
        state.screen = app::Screen::Agents;
        let hidden = renderer.prepare(&state).unwrap();
        assert!(
            app::ViewModel::decode(hidden.as_slice())
                .unwrap()
                .chat
                .is_none()
        );
        state.screen = app::Screen::Settings;
        renderer.prepare(&state).unwrap();
        state
            .self_info
            .as_mut()
            .unwrap()
            .state
            .as_mut()
            .unwrap()
            .uptime_secs += 1;
        assert!(
            renderer.prepare(&state).is_some(),
            "details keep live metrics"
        );
    }

    #[test]
    fn legacy_home_routes_to_agents_and_chat_keeps_the_agents_workspace() {
        let mut state = State::new(false, Some("local".into()));
        state.go(app::Screen::Home);
        assert_eq!(state.screen, app::Screen::Agents);
        assert_eq!(state.workspace(), app::Screen::Agents);
        state.go(app::Screen::Chat);
        assert_eq!(view_model(&state).workspace, app::Screen::Agents as i32);
        state.back();
        assert_eq!(state.screen, app::Screen::Agents);
        assert!(!state.can_go_back());
    }

    #[test]
    fn back_availability_and_target_come_from_the_core_stack() {
        use app::Screen;
        let mut state = State::new(true, Some("phone".into()));
        assert!(!view_model(&state).can_go_back);
        state.screen = Screen::Agents;
        state.go(Screen::Chat);
        assert!(view_model(&state).can_go_back);
        state.go(Screen::Files);
        state.go(Screen::File);
        assert_eq!(
            view_model(&state).navigation_path,
            vec![
                Screen::Agents as i32,
                Screen::Chat as i32,
                Screen::Files as i32,
                Screen::File as i32
            ]
        );
        state.back();
        assert_eq!(state.screen, Screen::Files);
        state.back();
        assert_eq!(state.screen, Screen::Chat);
        state.back();
        assert_eq!(state.screen, Screen::Agents);
        assert!(!view_model(&state).can_go_back);
        state.back();
        assert_eq!(state.screen, Screen::Agents);
    }

    #[test]
    fn mobile_members_stay_in_network_settings_but_are_not_service_targets() {
        let mut state = State::new(true, Some("phone".into()));
        state.network = Some(v1::NetworkStatus {
            has_network: true,
            members: ["desktop", "server", "phone", "tablet", "unknown"]
                .into_iter()
                .map(|kind| v1::MemberView {
                    device_id: kind.into(),
                    kind: kind.into(),
                    is_self: kind == "phone",
                    ..Default::default()
                })
                .collect(),
            ..Default::default()
        });
        let vm = view_model(&state);
        assert_eq!(vm.network.unwrap().members.len(), 5);
        assert_eq!(
            vm.service_devices
                .iter()
                .map(|m| m.device_id.as_str())
                .collect::<Vec<_>>(),
            ["desktop", "server"]
        );
        assert!(!vm.local_device_has_services);
        assert!(!vm.selected_device_has_services);
        state.selected = "desktop".into();
        assert!(view_model(&state).selected_device_has_services);
    }

    fn member(id: &str, kind: &str, role: &str, is_self: bool) -> v1::MemberView {
        v1::MemberView {
            device_id: id.into(),
            kind: kind.into(),
            role: role.into(),
            is_self,
            ..Default::default()
        }
    }

    #[test]
    fn blackout_switch_requires_live_status_from_the_selected_mac() {
        let mut phone = State::new(true, Some("phone".into()));
        let mut mac = member("mac", "desktop", "member", false);
        mac.connected = true;
        phone.network = Some(v1::NetworkStatus {
            members: vec![mac],
            ..Default::default()
        });
        phone.selected = "mac".into();
        assert!(device_actions(&phone).blackout.is_none());
        let mode = v1::DisplayBlackout {
            enabled: true,
            status: "active".into(),
        };
        phone.manual_devices.insert(
            "mac".into(),
            v1::DeviceInfo {
                device_id: "mac".into(),
                platform: "macos".into(),
                state: Some(v1::DeviceState {
                    blackout: Some(mode.clone()),
                    ..Default::default()
                }),
                ..Default::default()
            },
        );
        assert_eq!(device_actions(&phone).blackout, Some(mode));
        phone.embedded = false;
        assert!(device_actions(&phone).blackout.is_none());
        phone.embedded = true;
        phone.network.as_mut().unwrap().members[0].connected = false;
        assert!(device_actions(&phone).blackout.is_none());
    }

    #[test]
    fn device_page_actions_follow_role_kind_and_mode() {
        let mut phone = State::new(true, Some("phone".into()));
        phone.network = Some(v1::NetworkStatus {
            has_network: true,
            members: vec![
                member("phone", "phone", "admin", true),
                member("mac", "desktop", "member", false),
            ],
            ..Default::default()
        });
        phone.selected = "mac".into();
        let mac = view_model(&phone).device_actions.unwrap();
        assert!(mac.can_cover && mac.can_ssh && mac.show_access);
        assert!(mac.can_edit_access && mac.can_remove);
        phone.selected = "phone".into();
        let own = view_model(&phone).device_actions.unwrap();
        assert!(!own.can_cover && !own.can_ssh && !own.can_edit_access && !own.can_remove);
        assert!(own.show_access, "an admin's own page summarises access");

        let mut desktop = State::new(false, Some("mac".into()));
        desktop.network = Some(v1::NetworkStatus {
            has_network: true,
            members: vec![
                member("mac", "desktop", "member", true),
                member("phone", "phone", "admin", false),
            ],
            ..Default::default()
        });
        desktop.selected = "phone".into();
        let phone_page = view_model(&desktop).device_actions.unwrap();
        assert!(!phone_page.can_cover, "only desktops are covered");
        assert!(!phone_page.can_ssh, "in-app SSH is a phone core's");
        assert!(!phone_page.can_edit_access && !phone_page.can_remove);
        assert!(
            !phone_page.show_access,
            "a member without grants shows nothing"
        );
    }

    #[test]
    fn compact_wrap_ups_require_an_optional_claude_choice() {
        let mut state = State::new(false, Some("local".into()));
        state.screen = app::Screen::Chat;
        let args = serde_json::json!({
            "recap": "Checks passed.", "ask": "Continue?",
            "actions": [{"label": "Continue"}, {"label": "Later"}]
        });
        let wrap = muster_agent::claude_cards::wrap_up(&args, "wrap", "s").unwrap();
        let mut blocked = wrap.clone();
        blocked.id = "blocked".into();
        blocked.optional = false;
        let mut generic = wrap.clone();
        generic.id = "generic".into();
        generic.questions[0].id = "ordinary".into();
        state.chat = Some(crate::state::Chat {
            session: v1::AgentSession {
                provider: "claude".into(),
                ..Default::default()
            },
            interactions: vec![generic, wrap.clone(), blocked],
            ..Default::default()
        });
        assert_eq!(
            view_model(&state).chat.unwrap().compact_wrap_up_ids,
            [wrap.id]
        );
        state.chat.as_mut().unwrap().session.provider = "codex".into();
        assert!(
            view_model(&state)
                .chat
                .unwrap()
                .compact_wrap_up_ids
                .is_empty()
        );
    }

    #[test]
    fn chat_view_carries_the_decisions_and_lists_queued_messages_once() {
        let mut state = State::new(false, Some("local".into()));
        state.screen = app::Screen::Chat;
        let interaction = |id: &str, kind: &str| v1::PendingInteraction {
            id: id.into(),
            kind: kind.into(),
            ..Default::default()
        };
        state.chat = Some(crate::state::Chat {
            session: v1::AgentSession {
                provider: "codex".into(),
                mode: "chat".into(),
                state: "running".into(),
                ..Default::default()
            },
            interactions: vec![
                interaction("q", muster_agent::queue::KIND),
                interaction("a", "approval"),
            ],
            ..Default::default()
        });
        let chat = view_model(&state).chat.unwrap();
        assert_eq!(chat.interactions.len(), 1);
        assert_eq!(chat.interactions[0].id, "a");
        assert_eq!(chat.queue.unwrap().messages.len(), 1);
        assert!(chat.can_interrupt && chat.can_choose_model && !chat.can_browse_files);
        assert_eq!(chat.provider_label, "ChatGPT");
        assert_eq!(chat.composer_placeholder, "Ask ChatGPT");

        let session = &mut state.chat.as_mut().unwrap().session;
        session.mode = String::new();
        session.state = "idle".into();
        let chat = view_model(&state).chat.unwrap();
        assert!(!chat.can_interrupt && chat.can_choose_model && chat.can_browse_files);
        assert_eq!(chat.provider_label, "Codex");
        assert_eq!(chat.composer_placeholder, "Do anything");
        let session = &mut state.chat.as_mut().unwrap().session;
        session.message_count = 3;
        assert_eq!(
            view_model(&state).chat.unwrap().composer_placeholder,
            "Work with Codex"
        );
        let session = &mut state.chat.as_mut().unwrap().session;
        session.composer_suggestion = "Commit and install it".into();
        assert_eq!(
            view_model(&state).chat.unwrap().composer_suggestion,
            "Commit and install it"
        );
        assert_eq!(
            view_model(&state).chat.unwrap().composer_placeholder,
            "Work with Codex"
        );
        state.chat.as_mut().unwrap().session.read_only = true;
        assert!(
            view_model(&state)
                .chat
                .unwrap()
                .composer_suggestion
                .is_empty()
        );
        state.chat.as_mut().unwrap().session.read_only = false;
        let session = &mut state.chat.as_mut().unwrap().session;
        session.composer_suggestion.clear();
        session.provider = "claude".into();
        assert_eq!(
            view_model(&state).chat.unwrap().composer_placeholder,
            "How can I help you today?"
        );
        state.chat.as_mut().unwrap().session.provider = "codex".into();

        state.chat.as_mut().unwrap().session.read_only = true;
        state.chat.as_mut().unwrap().session.state = "running".into();
        let readonly = view_model(&state).chat.unwrap();
        assert!(
            !readonly.can_interrupt && !readonly.can_choose_model && !readonly.can_browse_files
        );
        assert!(readonly.interactions.is_empty());
        assert!(readonly.queue.unwrap().messages.is_empty());
        state.chat.as_mut().unwrap().session.read_only = false;

        // A cloud session: the provider's cloud label, prompts only.
        let session = &mut state.chat.as_mut().unwrap().session;
        session.mode = "cloud".into();
        session.state = "running".into();
        let chat = view_model(&state).chat.unwrap();
        assert!(!chat.can_interrupt && !chat.can_choose_model && !chat.can_browse_files);
        assert_eq!(chat.provider_label, "Codex Cloud");
        assert_eq!(chat.composer_placeholder, "");
        state.chat.as_mut().unwrap().session.provider = "claude".into();
        assert_eq!(
            view_model(&state).chat.unwrap().provider_label,
            "Claude Cloud"
        );
    }

    #[test]
    fn join_text_names_each_phase() {
        let text = |phase: &str, message: &str| {
            join_text(&app::JoinView {
                phase: phase.into(),
                message: message.into(),
            })
        };
        assert_eq!(text("connecting", ""), "Connecting to the admin…");
        assert_eq!(text("failed", ""), "Joining failed.");
        assert_eq!(text("failed", "invite expired"), "invite expired");
        assert_eq!(text("idle", ""), "");
    }
}

/// What the join screen says for its phase.
fn join_text(join: &app::JoinView) -> String {
    match join.phase.as_str() {
        "connecting" => "Connecting to the admin…",
        "waiting_approval" => "Waiting for the admin to approve this device…",
        "joined" => "Joined.",
        "failed" if join.message.is_empty() => "Joining failed.",
        _ => return join.message.clone(),
    }
    .into()
}

/// Whether the session's app declares a capability in its profile
/// (`muster_agent::profile`). A provider this build has no profile for (a
/// newer daemon's app) is not hidden: the target still decides every call.
fn offers(
    session: &muster_proto::v1::AgentSession,
    pick: fn(&muster_agent::profile::Capabilities) -> bool,
) -> bool {
    !session.read_only
        && muster_agent::profile::capabilities(&session.provider).is_none_or(|c| pick(&c))
}

/// The provider as the user knows it: a Chat-mode conversation is the
/// desktop app's (ChatGPT for codex), a cloud session the provider's cloud
/// (Claude Cloud), an agent session the agent's (Claude Code, CatDesk); an
/// unknown provider shows its own name.
fn provider_label(session: &muster_proto::v1::AgentSession) -> String {
    if session.mode == "orca" {
        return "CLI".into();
    }
    let Some(app) = muster_core::apps::app(&session.provider) else {
        return session.provider.clone();
    };
    if session.mode == "cloud"
        && let Some(cloud) = muster_agent::profile::cloud(&session.provider)
    {
        return cloud.label.into();
    }
    match &app.chat {
        Some(chat) if session.mode == "chat" => chat.label,
        _ => app.agent_label,
    }
    .into()
}

/// What the provider's own app shows in an empty composer. Codex names its
/// home and follow-up composers differently (its `homePage.composer.
/// placeholder.askAnything.v2` and `composer.placeholder.workWithCodex`) and
/// its Chat mode `Ask ChatGPT`; Claude is Claude.app's new-chat text. Empty
/// leaves the shell's own.
fn composer_placeholder(session: &muster_proto::v1::AgentSession, has_messages: bool) -> String {
    match (session.provider.as_str(), session.mode.as_str()) {
        (_, "orca" | "cloud") => "",
        ("codex", "chat") => "Ask ChatGPT",
        ("codex", _) if has_messages => "Work with Codex",
        ("codex", _) => "Do anything",
        ("claude", _) => "How can I help you today?",
        _ => "",
    }
    .into()
}

/// What the selected device's page offers (ARCHITECTURE §3.7: decided here,
/// rendered by the shells; the target still authorizes every call).
pub(crate) fn device_actions(state: &State) -> app::DeviceActions {
    let Some(network) = &state.network else {
        return app::DeviceActions::default();
    };
    let target = state.target();
    let Some(member) = network.members.iter().find(|m| m.device_id == target) else {
        return app::DeviceActions::default();
    };
    let admin = network
        .members
        .iter()
        .any(|m| m.is_self && m.role == "admin");
    let other = !member.is_self;
    app::DeviceActions {
        blackout: state
            .manual_devices
            .get(&target)
            .filter(|i| {
                state.embedded
                    && other
                    && member.connected
                    && member.kind == "desktop"
                    && i.device_id == target
                    && i.platform == "macos"
            })
            .and_then(|i| i.state.as_ref()?.blackout.clone()),
        can_cover: other && member.kind == "desktop",
        can_ssh: other && state.embedded,
        show_access: !member.scopes.is_empty() || admin,
        can_edit_access: admin && other,
        can_remove: admin && other,
    }
}

/// This Mac's own state, or `None` wherever the setup dialog does not apply
/// (a phone's embedded daemon, a shell on another platform).
fn local_mac(state: &State) -> Option<muster_proto::v1::DeviceState> {
    let info = state.self_info.as_ref()?;
    if state.embedded || info.platform != "macos" {
        return None;
    }
    Some(info.state.clone().unwrap_or_default())
}

fn desktop_setup(state: &State) -> app::DesktopSetupView {
    let Some(device) = local_mac(state) else {
        return app::DesktopSetupView::default();
    };
    let accessibility_ready = device.accessibility == "granted";
    // Screen Recording belongs to musterd: the daemon is what captures, and
    // the app cannot hold the permission for it. An OS with no such
    // permission reports nothing, which is not something to ask for.
    let screen_recording_ready = device.screen_recording != "missing";
    let plugin_ready = device.locked_use_setup == "ready";
    let checking = device.locked_use_setup == "checking";
    app::DesktopSetupView {
        ready: accessibility_ready && screen_recording_ready && plugin_ready,
        // A new Screen Recording grant reaches only a new process: macOS
        // answers this daemon's preflight from what it cached at launch, in
        // both directions, so the app never sees the grant land and no
        // transition can be waited for. The restart is offered from the
        // moment the grant is missing, and it is what makes one take.
        restart_required: state.desktop_restart_required || !screen_recording_ready,
        can_restart: state.can_restart_desktop(),
        error: state.desktop_setup_error.clone(),
        visible: !state.desktop_setup_dismissed && (state.desktop_restart_required || !accessibility_ready || !screen_recording_ready || (!plugin_ready && !checking)),
        accessibility_ready, screen_recording_ready, plugin_ready, checking,
        screen_recording_message: if screen_recording_ready {
            "musterd 已获准读取窗口画面。"
        } else {
            "在系统设置 → 隐私与安全性 → 屏幕录制中启用 Muster；列表中的 musterd 属于同一项授权，一并启用。macOS 只把新授权交给新进程，所以启用后要用下方的「重启 Muster」才生效。原生发送失败时，Muster 才能看一眼应用窗口并说明原因。"
        }.into(),
        can_install: matches!(device.locked_use_setup.as_str(), "checking" | "needs_installation"),
        plugin_message: match device.locked_use_setup.as_str() {
            "ready" => "Locked Use 插件已由系统 helper 安装并验证。",
            "checking" => "正在检查并安装 Locked Use 插件…",
            "needs_installation" => "需要授权系统 helper 安装或验证 Locked Use 插件。在系统设置 → 通用 → 登录项与扩展中允许 Muster 后台运行，随后将自动安装，最多等待约一分钟。",
            _ => "无法验证 Locked Use 插件。请将最新版 Muster 放入 /Applications 后重新启动。",
        }.into(),
    }
}

/// The terminal Automation this Mac still needs, for the Terminals page.
/// One authorized source is all the Mac offers terminals through, so the
/// other app's Automation is never asked for: a Mac driving iTerm2 has no
/// use for a Ghostty grant, and being asked for one reads as a fault.
/// Ghostty needs Automation only (it has no API to enable); iTerm2 also
/// needs its Python API. `None` means there is no terminal app to
/// authorize, here or on any other platform.
fn terminal_authorization(state: &State) -> Option<app::TerminalAuthorizationView> {
    let device = local_mac(state)?;
    let iterm_installed = device.iterm2_automation != "not_installed";
    let ghostty_installed =
        !device.ghostty_automation.is_empty() && device.ghostty_automation != "not_installed";
    if !iterm_installed && !ghostty_installed {
        return None;
    }
    let iterm_ready = device.iterm2_automation == "granted" && device.iterm2_api_available;
    let ghostty_ready = device.ghostty_automation == "granted";
    let ready = iterm_ready || ghostty_ready;
    // Which app the section speaks about: the authorized one once the Mac
    // has terminals, otherwise the source Rust picked, or whichever of the
    // two is installed at all.
    let ghostty = if ready {
        ghostty_ready && !iterm_ready
    } else if device.terminal_source == "ghostty" {
        ghostty_installed
    } else {
        !iterm_installed
    };
    Some(app::TerminalAuthorizationView {
        app: if ghostty { "Ghostty" } else { "iTerm2" }.into(),
        ready,
        busy: state.busy > 0,
        message: if ghostty {
            match device.ghostty_automation.as_str() {
                "granted" => "自动化权限已授权，可以列出 Ghostty 终端并输入；Mac 锁屏时，手机打开终端会在遮罩下临时解锁读屏。",
                "not_running" => "请先启动 Ghostty，再点击授权。",
                _ => "需要允许 Muster / musterd 自动化控制 Ghostty（列出终端、输入）。",
            }
        } else { match device.iterm2_automation.as_str() {
            "granted" if device.iterm2_api_available => "自动化权限已授权，iTerm2 API 可用。",
            "granted" => "请启动 iTerm2，并在 Settings → General → Magic 中启用 Python API；首次连接时允许 Muster 访问。",
            "not_running" => "请先启动 iTerm2，再点击授权。在 iTerm2 Settings → General → Magic 中启用 Python API。",
            _ => "需要允许 Muster / musterd 自动化控制 iTerm2，并在 iTerm2 Settings → General → Magic 中启用 Python API。首次连接时还需允许 Muster 访问。",
        } }.into(),
    })
}

#[cfg(test)]
mod desktop_setup_tests {
    use super::*;
    use muster_proto::v1;
    fn ready() -> State {
        let mut state = State::new(false, None);
        state.self_info = Some(v1::DeviceInfo {
            platform: "macos".into(),
            state: Some(v1::DeviceState {
                accessibility: "granted".into(),
                screen_recording: "granted".into(),
                locked_use_setup: "ready".into(),
                iterm2_automation: "granted".into(),
                iterm2_api_available: true,
                ..Default::default()
            }),
            ..Default::default()
        });
        state
    }
    fn device(state: &mut State) -> &mut v1::DeviceState {
        state.self_info.as_mut().unwrap().state.as_mut().unwrap()
    }
    #[test]
    fn ready_needs_every_permission_and_installation() {
        let mut state = ready();
        assert!(desktop_setup(&state).ready);
        device(&mut state).locked_use_setup = "needs_installation".into();
        assert!(!desktop_setup(&state).ready);
        device(&mut state).locked_use_setup = "ready".into();
        device(&mut state).screen_recording = "missing".into();
        assert!(!desktop_setup(&state).ready);
    }

    #[test]
    fn startup_prompt_requires_current_local_permission_observations() {
        let mut state = ready();
        assert!(!desktop_setup(&state).visible);
        device(&mut state).accessibility = "missing".into();
        assert!(desktop_setup(&state).visible);
        state.desktop_setup_dismissed = true;
        assert!(!desktop_setup(&state).visible);
        state.desktop_setup_dismissed = false;
        state.embedded = true;
        assert!(!desktop_setup(&state).visible);
    }

    /// Screen Recording is musterd's own permission, so a missing one has to
    /// reach the startup prompt the same way Accessibility does. An OS that
    /// has no such permission reports nothing and is never prompted.
    #[test]
    fn missing_screen_recording_prompts_but_an_unreporting_os_does_not() {
        let mut state = ready();
        device(&mut state).screen_recording = "missing".into();
        let view = desktop_setup(&state);
        assert!(view.visible && !view.screen_recording_ready);
        assert!(view.screen_recording_message.contains("屏幕录制"));
        // The grant only reaches a new process, so the restart that makes it
        // take is offered straight away rather than waited for.
        assert!(view.restart_required);
        device(&mut state).screen_recording = String::new();
        let view = desktop_setup(&state);
        assert!(view.screen_recording_ready && !view.visible && !view.restart_required);
    }

    /// Terminal Automation left the setup dialog: it no longer decides
    /// whether the dialog appears, and `ready` no longer waits on it.
    #[test]
    fn terminal_automation_no_longer_drives_the_setup_dialog() {
        let mut state = ready();
        device(&mut state).iterm2_automation = "missing".into();
        device(&mut state).iterm2_api_available = false;
        let view = desktop_setup(&state);
        assert!(view.ready && !view.visible);
        let authorization = terminal_authorization(&state).unwrap();
        assert_eq!(authorization.app, "iTerm2");
        assert!(!authorization.ready);
    }

    #[test]
    fn absent_terminal_app_has_nothing_to_authorize_but_a_stopped_one_does() {
        let mut state = ready();
        device(&mut state).iterm2_automation = "not_installed".into();
        device(&mut state).iterm2_api_available = false;
        assert!(terminal_authorization(&state).is_none());
        device(&mut state).iterm2_automation = "not_running".into();
        let authorization = terminal_authorization(&state).unwrap();
        assert!(!authorization.ready && !authorization.message.is_empty());
    }

    /// One authorized source is all the Mac needs. A Ghostty that is
    /// installed but not authorized must not be asked for while iTerm2
    /// already drives this Mac's terminals, and the reverse holds too.
    #[test]
    fn an_authorized_source_never_asks_for_the_other_app() {
        let mut state = ready();
        device(&mut state).ghostty_automation = "not_running".into();
        let authorization = terminal_authorization(&state).unwrap();
        assert!(authorization.ready);
        assert_eq!(authorization.app, "iTerm2");
        // The other way round: Ghostty drives this Mac, iTerm2 is installed
        // but unauthorized, and nothing asks for it.
        device(&mut state).ghostty_automation = "granted".into();
        device(&mut state).terminal_source = "ghostty".into();
        device(&mut state).iterm2_automation = "missing".into();
        device(&mut state).iterm2_api_available = false;
        let authorization = terminal_authorization(&state).unwrap();
        assert!(authorization.ready);
        assert_eq!(authorization.app, "Ghostty");
    }

    /// With nothing authorized yet, the section names the source Rust picked,
    /// and falls back to whichever app is installed at all.
    #[test]
    fn an_outstanding_grant_names_the_source_in_use() {
        let mut state = ready();
        device(&mut state).iterm2_automation = "missing".into();
        device(&mut state).iterm2_api_available = false;
        device(&mut state).ghostty_automation = "not_running".into();
        let authorization = terminal_authorization(&state).unwrap();
        assert!(!authorization.ready);
        assert_eq!(authorization.app, "iTerm2"); // terminal_source is iterm2
        device(&mut state).terminal_source = "ghostty".into();
        assert_eq!(terminal_authorization(&state).unwrap().app, "Ghostty");
        // Only Ghostty is installed: it is named whatever the source says.
        device(&mut state).terminal_source = String::new();
        device(&mut state).iterm2_automation = "not_installed".into();
        assert_eq!(terminal_authorization(&state).unwrap().app, "Ghostty");
    }

    /// The Terminals page belongs to the Mac itself; a phone's embedded
    /// daemon and other platforms get no authorization section.
    #[test]
    fn terminal_authorization_is_offered_on_this_mac_only() {
        let mut state = ready();
        assert!(terminal_authorization(&state).is_some());
        state.embedded = true;
        assert!(terminal_authorization(&state).is_none());
        state.embedded = false;
        state.self_info.as_mut().unwrap().platform = "ios".into();
        assert!(terminal_authorization(&state).is_none());
    }

    /// Ghostty needs Automation only; iTerm2 also needs its Python API, so
    /// an iTerm2 that is authorized but has no API does not count as a
    /// working source — and a Ghostty grant then makes the Mac ready.
    #[test]
    fn an_iterm2_without_its_api_is_not_a_working_source() {
        let mut state = ready();
        device(&mut state).iterm2_api_available = false;
        let authorization = terminal_authorization(&state).unwrap();
        assert_eq!(authorization.app, "iTerm2");
        assert!(!authorization.ready);
        device(&mut state).ghostty_automation = "granted".into();
        let authorization = terminal_authorization(&state).unwrap();
        assert!(authorization.ready);
        assert_eq!(authorization.app, "Ghostty");
    }

    #[test]
    fn helper_registration_or_socket_alone_does_not_mean_ready() {
        let mut state = ready();
        device(&mut state).locked_use_setup = "needs_installation".into();
        let view = desktop_setup(&state);
        assert!(view.visible && view.can_install && !view.plugin_ready);
        device(&mut state).locked_use_setup = "ready".into();
        assert!(!desktop_setup(&state).visible);
    }
}

#[cfg(test)]
mod history_projection_tests {
    use super::*;
    use crate::state::Chat;
    use muster_proto::v1;

    #[test]
    fn catalog_reuse_tracks_edits_search_controls_and_reachability() {
        let mut state = State::new(false, None);
        state.agents.sessions.push(v1::AgentSession {
            id: "a".into(),
            device_id: "device-a".into(),
            provider: "codex".into(),
            title: "old".into(),
            ..Default::default()
        });
        let mut renderer = Renderer::default();
        let check = |renderer: &mut Renderer, state: &State| {
            let emitted = renderer.prepare(state).unwrap();
            assert_eq!(
                app::ViewModel::decode(emitted.as_slice()).unwrap().agents,
                view_model(state).agents
            );
            assert!(!renderer.catalog.refresh(state));
        };
        check(&mut renderer, &state);
        state
            .agent_disclosures
            .insert("missing-group".into(), false);
        assert!(renderer.prepare(&state).is_none());
        state.busy += 1;
        assert!(!renderer.catalog.refresh(&state));
        check(&mut renderer, &state);
        state.agents.sessions[0].title = "new".into();
        check(&mut renderer, &state);
        state
            .agent_disclosures
            .insert("catalog:recent".into(), false);
        check(&mut renderer, &state);
        state.agents.query = "term".into();
        state.agents_search_sessions = state.agents.sessions.clone();
        check(&mut renderer, &state);
        state.network = Some(v1::NetworkStatus {
            members: vec![v1::MemberView {
                device_id: "device-a".into(),
                presence: "offline".into(),
                ..Default::default()
            }],
            ..Default::default()
        });
        check(&mut renderer, &state);
        assert!(renderer.catalog.rows.as_ref().unwrap().0.is_empty());
    }

    #[test]
    fn large_cached_transcript_projects_the_latest_turns_without_losing_history_or_cards() {
        let mut state = State::new(true, Some("phone".into()));
        state.screen = app::Screen::Chat;
        state.chat = Some(Chat {
            messages: (0..1500)
                .map(|i| v1::AgentMessage {
                    id: format!("message-{i}"),
                    role: "user".into(),
                    kind: "text".into(),
                    text: format!("message {i}: {}", "synthetic content ".repeat(100)),
                    ..Default::default()
                })
                .collect(),
            interactions: vec![v1::PendingInteraction {
                id: "question:active".into(),
                kind: "question".into(),
                ..Default::default()
            }],
            ..Default::default()
        });
        state.chat.as_mut().unwrap().reconcile_history_start();
        let mut renderer = Renderer::default();
        let small = renderer.prepare(&state).unwrap();
        let projected = app::ViewModel::decode(small.as_slice())
            .unwrap()
            .chat
            .unwrap();
        // Every record is a prompt, so each is a turn: the window holds ten.
        assert_eq!(projected.timeline.len(), crate::state::CHAT_HISTORY_TURNS);
        assert_eq!(
            projected.timeline[0].message.as_ref().unwrap().id,
            "message-1490"
        );
        assert_eq!(projected.message_count, 1500);
        assert!(projected.has_earlier);
        assert_eq!(projected.interactions.len(), 1);
        let chat = state.chat.as_mut().unwrap();
        assert!(chat.reveal_history(false));
        assert_eq!(chat.visible_messages().len(), 20);
        let middle = renderer
            .prepare(&state)
            .expect("local reveal invalidates renderer");
        assert_eq!(
            app::ViewModel::decode(middle.as_slice())
                .unwrap()
                .chat
                .unwrap()
                .timeline
                .len(),
            20
        );
        let chat = state.chat.as_mut().unwrap();
        assert!(chat.reveal_history(true));
        let full = renderer.prepare(&state).unwrap();
        assert!(
            !app::ViewModel::decode(full.as_slice())
                .unwrap()
                .chat
                .unwrap()
                .has_earlier
        );
        // The limit is 100 times the ten-turn window; all 1,500 would be 150.
        assert!(
            full.len() < small.len() * 120,
            "oldest window remains bounded"
        );
        let view = app::ViewModel::decode(full.as_slice())
            .unwrap()
            .chat
            .unwrap();
        assert!(view.has_newer);
        assert_eq!(view.timeline.len(), crate::state::CHAT_HISTORY_LIMIT);
        assert_eq!(state.chat.as_ref().unwrap().messages.len(), 1500);
        eprintln!(
            "history snapshot: recent={} bytes full={} bytes",
            small.len(),
            full.len()
        );
    }
}
