//! Everything the core knows, owned by the reducer task alone.

use std::collections::HashMap;

use muster_proto::app::{self, Screen};
use muster_proto::v1;
use muster_terminal::grid::Grid;
use tokio::sync::mpsc;
use tokio_util::sync::CancellationToken;

/// A running watch (device, agent, terminal) that ends when its token does.
pub struct Watch {
    pub token: CancellationToken,
}

impl Drop for Watch {
    fn drop(&mut self) {
        self.token.cancel();
    }
}

pub(crate) fn next_content_revision() -> u64 {
    use std::sync::atomic::{AtomicU64, Ordering};
    static NEXT: AtomicU64 = AtomicU64::new(1);
    NEXT.fetch_add(1, Ordering::Relaxed)
}

#[derive(Default)]
pub struct Chat {
    pub content_revision: u64,
    pub cache_dirty: bool,
    pub device: String,
    pub cache_key: String,
    pub session: v1::AgentSession,
    pub messages: Vec<v1::AgentMessage>,
    /// First visible transcript ID; full history stays cached for offline paging.
    pub history_start: Option<String>,
    pub interactions: Vec<v1::PendingInteraction>,
    pub has_earlier: bool,
    pub sending: bool,
    pub responding_interaction: String,
    /// Interaction owning the current submission status (including a late RPC reply).
    pub response_interaction: String,
    pub response_error: String,
    pub response_unknown: bool,
    pub pending_draft_id: String,
    pub sent_draft_id: String,
    /// The last send was refused behind an earlier unconfirmed one.
    pub unconfirmed: Option<app::UnconfirmedSend>,
    pub draft: app::ChatDraft,
    pub loading: bool,
    pub load_error: String,
    pub watch: Option<Watch>,
    /// Desktop chats: the model / effort choices, once read from the Mac.
    pub options: Option<v1::AgentChatOptions>,
    pub options_busy: bool,
    pub options_error: String,
    pub completing: bool,
    pub completion: Option<(u64, String)>,
    pub completion_error: String,
}

/// Presentation paging is shared across shells; transport history is unchanged.
pub(crate) const CHAT_HISTORY_PAGE: usize = 200;

impl Chat {
    pub(crate) fn history_start_index(&self) -> usize {
        self.history_start
            .as_ref()
            .and_then(|id| self.messages.iter().position(|m| &m.id == id))
            .unwrap_or_else(|| self.messages.len().saturating_sub(CHAT_HISTORY_PAGE))
    }

    pub(crate) fn visible_messages(&self) -> &[v1::AgentMessage] {
        &self.messages[self.history_start_index()..]
    }

    pub(crate) fn reconcile_history_start(&mut self) {
        self.history_start = self
            .messages
            .get(self.history_start_index())
            .map(|m| m.id.clone());
    }

    /// Reveal already cached history without an RPC, including while offline.
    pub(crate) fn reveal_history(&mut self, all: bool) -> bool {
        let start = self.history_start_index();
        if start == 0 {
            return false;
        }
        let next = if all {
            0
        } else {
            start.saturating_sub(CHAT_HISTORY_PAGE)
        };
        self.history_start = Some(self.messages[next].id.clone());
        self.content_revision = next_content_revision();
        true
    }
}

pub enum TerminalKind {
    /// An iTerm2 or PTY session on another device, frames over muster-rpc.
    /// The watch ends the attach stream when the terminal is dropped.
    Remote {
        _attach: Option<Watch>,
        /// A Locked Use terminal session keeping the locked Mac unlocked
        /// under its cover (Ghostty); dropping it ends the session.
        lease: Option<Watch>,
        /// A session was asked for this terminal: a second refusal is shown,
        /// not retried.
        leased: bool,
    },
    /// An in-app SSH session over a forward stream.
    Ssh {
        grid: Box<Grid>,
        input: mpsc::UnboundedSender<SshCommand>,
        _reader: Option<Watch>,
    },
}

impl Terminal {
    /// Whether the shell's viewport sets this grid's size. An SSH session
    /// is ours, a PTY shell was spawned for us; an iTerm2 session is a
    /// window somebody is using on the device and keeps its own size.
    pub fn resizable(&self) -> bool {
        match self.kind {
            TerminalKind::Ssh { .. } => true,
            TerminalKind::Remote { .. } => matches!(self.source.as_str(), "pty" | "tmux"),
        }
    }
}

pub enum SshCommand {
    Write(Vec<u8>),
    Resize(u16, u16),
    Close,
}

#[derive(Default)]
pub struct SessionTerminalState {
    pub epoch: u64,
    pub busy: bool,
    pub active: bool,
    pub error: String,
}

pub struct Terminal {
    pub device: String,
    pub session: String,
    pub kind: TerminalKind,
    /// iterm2 | pty | ssh, as the device reported the session.
    pub source: String,
    pub cols: u32,
    pub rows: u32,
    pub lines: Vec<v1::TerminalLine>,
    pub cursor: (u32, u32),
    pub ended: bool,
    pub status: String,
    pub rev: u64,
    pub desktop_attach_command: String,
    pub iterm2_attach_command: String,
    pub native_sources: Vec<v1::TerminalSource>,
}

/// An intent that waits for local authentication.
pub struct PendingPresence {
    pub response_origin: Option<(String, String)>,
    pub action_id: String,
    pub call_id: u64,
    pub device: String,
    pub request: v1::request::Req,
    pub effect_id: u64,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum EffectKind {
    ScanQr,
    Authenticate,
    /// Local authentication before dropping this desktop's privacy cover.
    Uncover,
    Other,
    DesktopHelper,
    /// Only desktops update themselves.
    #[cfg_attr(not(any(target_os = "macos", target_os = "windows")), allow(dead_code))]
    PrepareUpdate,
    ActivateNetwork,
}

/// A desktop-chat request that met a busy Mac, retried on the next one.
#[derive(Clone, Debug)]
pub enum ChatRetry {
    /// Open this conversation (empty: a new one; `desktop`: show it there).
    Open { chat: String, desktop: bool },
    /// Show the open chat through another Mac; its draft stays unsent.
    Move {
        session: Box<v1::AgentSession>,
        from: String,
    },
}

pub struct State {
    pub desktop_update: app::DesktopUpdateView,
    #[cfg(any(target_os = "macos", target_os = "windows"))]
    pub update_candidate: Option<crate::desktop_update::Candidate>,
    #[cfg(any(target_os = "macos", target_os = "windows"))]
    pub update_prepared: Option<std::sync::Arc<crate::desktop_update::Prepared>>,
    #[cfg(any(target_os = "macos", target_os = "windows"))]
    pub update_lease: Option<crate::desktop_update::Lease>,
    pub desktop_restart_required: bool,
    pub desktop_permissions: Option<[Option<bool>; 3]>,
    pub desktop_setup_dismissed: bool,
    pub desktop_setup_error: String,
    pub browser: app::BrowserView,
    pub manual_devices: HashMap<String, v1::DeviceInfo>,
    pub manual_busy: std::collections::HashSet<String>,
    pub manual_watches: HashMap<String, Watch>,
    pub embedded: bool,
    pub self_id: String,
    /// What the shell was last told about the cover (desktops).
    pub cover_shown: Option<bool>,
    pub screen: Screen,
    pub stack: Vec<Screen>,
    pub toasts: Vec<app::Toast>,
    pub next_toast: u64,
    pub busy: u32,
    pub self_info: Option<v1::DeviceInfo>,
    pub network: Option<v1::NetworkStatus>,
    pub pending: Vec<v1::PendingJoin>,
    pub policy: Option<v1::Policy>,
    pub selected: String,
    pub invite: Option<app::InviteView>,
    pub join: app::JoinView,
    pub agents: app::AgentsView,
    pub agents_epoch: u64,
    pub agents_pending: usize,
    pub agents_retry: u64,
    /// Errors of the list refresh in flight; they replace the shown ones
    /// when it completes, so a refresh does not blink them away.
    pub agents_errors: Vec<String>,
    pub chat: Option<Chat>,
    /// Desktop Chat mode: one device's Claude or ChatGPT conversations.
    pub desktop_chats: app::DesktopChatsView,
    /// Bumped when the chat list is opened for another device/app; late
    /// open results of an earlier list are ignored.
    pub desktop_chats_view: u64,
    /// Bumped per catalog load; late catalogs are ignored.
    pub desktop_chats_load: u64,
    /// What to do once another Mac has been picked because the last one
    /// was busy (at most once per request).
    pub desktop_chat_retry: Option<ChatRetry>,
    pub terminals: app::TerminalsView,
    pub terminals_epoch: u64,
    pub terminals_pending: usize,
    pub terminal: Option<Terminal>,
    pub session_terminal: SessionTerminalState,
    pub code: crate::update::code_runs::Workbench,
    pub files: Option<app::FilesView>,
    pub file: Option<app::FileView>,
    pub git: Option<app::GitView>,
    pub presence: Option<PendingPresence>,
    pub locked: Option<(String, Vec<u8>)>,
    pub effects: HashMap<u64, EffectKind>,
    pub last_network_activation: Option<std::time::Instant>,
    pub next_effect: u64,
    pub next_op: u64,
    pub device_watch: Option<Watch>,
    pub foreground: bool,
    pub pending_drafts: HashMap<String, app::ChatDraft>,
    pub chat_call_epoch: u64,
    pub chat_calls: HashMap<
        String,
        tokio::sync::oneshot::Sender<Result<muster_proto::v1::response::Resp, crate::Error>>,
    >,
}

impl State {
    /// Ignore a late completion from another operation while a lease is active.
    /// In particular, a cancelled lease's reply cannot clear a newer lease.
    pub fn finish_locked(&mut self, device: &str, nonce: Option<&[u8]>) -> bool {
        if let Some((target, active)) = &self.locked {
            if target != device || nonce != Some(active.as_slice()) {
                return false;
            }
            self.locked = None;
        }
        true
    }

    pub fn new(embedded: bool, self_id: Option<String>) -> Self {
        Self {
            pending_drafts: HashMap::new(),
            chat_calls: HashMap::new(),
            chat_call_epoch: 0,
            desktop_update: app::DesktopUpdateView {
                supported: !embedded && cfg!(any(target_os = "macos", target_os = "windows")),
                status: "idle".into(),
                can_check: true,
                ..Default::default()
            },
            #[cfg(any(target_os = "macos", target_os = "windows"))]
            update_candidate: None,
            #[cfg(any(target_os = "macos", target_os = "windows"))]
            update_prepared: None,
            #[cfg(any(target_os = "macos", target_os = "windows"))]
            update_lease: None,
            desktop_restart_required: false,
            desktop_permissions: None,
            desktop_setup_dismissed: false,
            desktop_setup_error: String::new(),
            browser: app::BrowserView::default(),
            manual_devices: HashMap::new(),
            manual_busy: Default::default(),
            manual_watches: HashMap::new(),
            embedded,
            self_id: self_id.unwrap_or_default(),
            cover_shown: None,
            screen: Screen::Onboarding,
            stack: Vec::new(),
            toasts: Vec::new(),
            next_toast: 1,
            busy: 0,
            self_info: None,
            network: None,
            pending: Vec::new(),
            policy: None,
            selected: String::new(),
            invite: None,
            join: app::JoinView {
                phase: "idle".into(),
                message: String::new(),
            },
            agents: app::AgentsView::default(),
            agents_epoch: 0,
            agents_pending: 0,
            agents_retry: 1,
            agents_errors: Vec::new(),
            chat: None,
            desktop_chats: app::DesktopChatsView::default(),
            desktop_chats_view: 0,
            desktop_chats_load: 0,
            desktop_chat_retry: None,
            terminals: app::TerminalsView::default(),
            terminals_epoch: 0,
            terminals_pending: 0,
            terminal: None,
            session_terminal: SessionTerminalState::default(),
            code: Default::default(),
            files: None,
            file: None,
            git: None,
            presence: None,
            locked: None,
            effects: HashMap::new(),
            last_network_activation: None,
            next_effect: 1,
            next_op: 1,
            device_watch: None,
            foreground: true,
        }
    }

    /// Only a real missing -> granted transition requires a restart. Initial
    /// granted observations after relaunch must not create a restart loop.
    pub fn observe_desktop_permissions(&mut self, device: &v1::DeviceState) {
        if self.embedded {
            return;
        }
        let mut now = [
            match device.accessibility.as_str() {
                "granted" => Some(true),
                "missing" => Some(false),
                _ => None,
            },
            match device.locked_use_setup.as_str() {
                "ready" => Some(true),
                "needs_installation" => Some(false),
                _ => None,
            },
            match device.iterm2_automation.as_str() {
                "granted" => Some(true),
                "missing" => Some(false),
                _ => None,
            },
        ];
        if let Some(before) = self.desktop_permissions {
            if before
                .iter()
                .zip(now)
                .any(|(old, new)| *old == Some(false) && new == Some(true))
            {
                self.desktop_restart_required = true;
                self.desktop_setup_dismissed = false;
            }
            for (value, previous) in now.iter_mut().zip(before) {
                if value.is_none() {
                    *value = previous;
                }
            }
        }
        self.desktop_permissions = Some(now);
    }

    pub fn can_restart_desktop(&self) -> bool {
        !self.embedded
            && self.busy == 0
            && self.presence.is_none()
            && self.locked.is_none()
            && self.terminal.is_none()
            && !self
                .chat
                .as_ref()
                .is_some_and(|c| c.sending || !c.responding_interaction.is_empty())
            && self
                .self_info
                .as_ref()
                .and_then(|i| i.state.as_ref())
                .is_some_and(|s| !s.locked && !s.covered)
    }

    pub fn has_network(&self) -> bool {
        self.network.as_ref().is_some_and(|n| n.has_network)
    }

    pub fn can_invite(&self) -> bool {
        self.network.as_ref().is_some_and(|n| {
            n.has_network && n.members.iter().any(|m| m.is_self && m.role == "admin")
        })
    }

    pub fn is_self(&self, device: &str) -> bool {
        device.is_empty() || device == self.self_id
    }

    /// A member the daemon reports offline (the relay's word) or
    /// unreachable (its bounded reconnects failed): not polled, and its
    /// sessions are not shown. Its cached sessions stay on disk.
    pub fn offline(&self, device: &str) -> bool {
        self.network.as_ref().is_some_and(|n| {
            n.members
                .iter()
                .any(|m| m.device_id == device && member_offline(m))
        })
    }

    /// Desktops and servers worth asking: not known to be offline.
    pub fn reachable_service_devices(&self) -> Vec<v1::MemberView> {
        self.service_devices()
            .into_iter()
            .filter(|m| !member_offline(m))
            .collect()
    }

    pub fn service_devices(&self) -> Vec<v1::MemberView> {
        self.network
            .as_ref()
            .map(|n| {
                n.members
                    .iter()
                    .filter(|m| matches!(m.kind.as_str(), "desktop" | "server"))
                    .cloned()
                    .collect()
            })
            .unwrap_or_default()
    }

    /// The device the agent/terminal screens act on.
    pub fn target(&self) -> String {
        if self.selected.is_empty() {
            self.self_id.clone()
        } else {
            self.selected.clone()
        }
    }

    /// This desktop's cover, as its daemon reports it.
    pub fn covered(&self) -> bool {
        self.self_info
            .as_ref()
            .and_then(|i| i.state.as_ref())
            .is_some_and(|s| s.covered)
    }

    pub fn toast(&mut self, text: impl Into<String>, error: bool) {
        let id = self.next_toast;
        self.next_toast += 1;
        self.toasts.push(app::Toast {
            id: id.to_string(),
            text: text.into(),
            error,
        });
        if self.toasts.len() > 5 {
            self.toasts.remove(0);
        }
    }

    pub fn detach_session_terminal(&mut self) {
        self.session_terminal.epoch += 1;
        self.session_terminal.busy = false;
        self.session_terminal.error.clear();
        if self.session_terminal.active {
            self.terminal = None;
        }
        self.session_terminal.active = false;
    }

    pub fn go(&mut self, screen: Screen) {
        if screen != Screen::Chat {
            self.detach_session_terminal();
        }
        let screen = if screen == Screen::Home {
            Screen::Agents
        } else {
            screen
        };
        if self.screen != screen {
            self.stack.push(self.screen);
            if self.stack.len() > 16 {
                self.stack.remove(0);
            }
            self.screen = screen;
        }
    }

    pub fn can_go_back(&self) -> bool {
        !matches!(
            self.screen,
            Screen::Onboarding
                | Screen::Home
                | Screen::Agents
                | Screen::Terminals
                | Screen::DesktopChats
        )
    }

    /// The native navigation stack stops at the current workspace root.
    /// Includes Back's fallback root and normalizes the legacy Home screen.
    pub fn navigation_path(&self) -> Vec<i32> {
        let mut path = vec![self.screen as i32];
        if !self.can_go_back() {
            return path;
        }
        for screen in self.stack.iter().rev().copied() {
            let screen = if screen == Screen::Home {
                Screen::Agents
            } else {
                screen
            };
            path.push(screen as i32);
            if matches!(
                screen,
                Screen::Onboarding | Screen::Agents | Screen::Terminals | Screen::DesktopChats
            ) {
                path.reverse();
                return path;
            }
        }
        path.push(if self.has_network() {
            Screen::Agents
        } else {
            Screen::Onboarding
        } as i32);
        path.reverse();
        path
    }

    /// The tab the screen belongs to: the nearest root below it.
    pub fn workspace(&self) -> Screen {
        std::iter::once(self.screen)
            .chain(self.stack.iter().rev().copied())
            .find(|s| {
                matches!(
                    s,
                    Screen::DesktopChats | Screen::Agents | Screen::Terminals | Screen::Home
                )
            })
            .map(|s| if s == Screen::Home { Screen::Agents } else { s })
            .unwrap_or(Screen::Agents)
    }

    /// Whether `screen` is shown or lies under the shown screen.
    pub fn on_path(&self, screen: Screen) -> bool {
        self.screen == screen || self.stack.contains(&screen)
    }

    pub fn back(&mut self) {
        self.detach_session_terminal();
        if !self.can_go_back() {
            return;
        }
        match self.stack.pop() {
            Some(previous) => {
                self.screen = if previous == Screen::Home {
                    Screen::Agents
                } else {
                    previous
                }
            }
            None => {
                self.screen = if self.has_network() {
                    Screen::Agents
                } else {
                    Screen::Onboarding
                }
            }
        }
    }

    pub fn op_id(&mut self) -> String {
        let n = self.next_op;
        self.next_op += 1;
        let nanos = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_nanos())
            .unwrap_or(0);
        format!("app-{nanos}-{n}")
    }

    pub fn new_watch(&self) -> (Watch, CancellationToken) {
        let token = CancellationToken::new();
        (
            Watch {
                token: token.clone(),
            },
            token,
        )
    }

    /// The device and session the open chat acts on.
    pub fn chat_target(&self) -> Option<(String, String)> {
        self.chat
            .as_ref()
            .map(|chat| (chat.device.clone(), chat.session.id.clone()))
    }

    pub fn cancel_all(&mut self) {
        self.detach_session_terminal();
        self.manual_watches.clear();
        self.device_watch = None;
        self.chat = None;
        self.terminal = None;
        self.files = None;
        self.file = None;
        self.git = None;
    }
}

#[cfg(test)]
mod locked_tests {
    use super::*;

    #[test]
    fn late_completion_preserves_new_lease_and_its_expiry_cancellation() {
        let mut state = State::new(true, None);
        state.locked = Some(("mac".into(), vec![2; 32]));
        assert!(!state.finish_locked("mac", Some(&[1; 32])));
        assert!(!state.finish_locked("mac", None));
        assert!(!state.finish_locked("another-mac", Some(&[2; 32])));
        assert_eq!(state.locked.as_ref().unwrap().1, vec![2; 32]);
        assert!(state.finish_locked("mac", Some(&[2; 32])));
        assert!(state.locked.is_none());
    }
}

#[cfg(test)]
mod desktop_restart_tests {
    use super::*;
    #[test]
    fn authorization_transition_requires_restart_but_initial_grants_do_not() {
        let mut state = State::new(false, None);
        let mut device = v1::DeviceState {
            accessibility: "granted".into(),
            locked_use_setup: "checking".into(),
            ..Default::default()
        };
        state.observe_desktop_permissions(&device);
        device.locked_use_setup = "ready".into();
        state.observe_desktop_permissions(&device);
        assert!(!state.desktop_restart_required);
        device.accessibility = "missing".into();
        state.observe_desktop_permissions(&device);
        device.accessibility = "".into(); // temporary unknown must not erase missing
        state.observe_desktop_permissions(&device);
        state.desktop_setup_dismissed = true;
        device.accessibility = "granted".into();
        state.observe_desktop_permissions(&device);
        assert!(state.desktop_restart_required);
        assert!(!state.desktop_setup_dismissed);
        let mut relaunched = State::new(false, None);
        relaunched.observe_desktop_permissions(&device);
        assert!(!relaunched.desktop_restart_required);
    }
    #[test]
    fn restart_is_blocked_during_locked_use_or_operations() {
        let mut state = State::new(false, None);
        state.self_info = Some(v1::DeviceInfo {
            state: Some(v1::DeviceState::default()),
            ..Default::default()
        });
        assert!(state.can_restart_desktop());
        state.busy = 1;
        assert!(!state.can_restart_desktop());
        state.busy = 0;
        state.locked = Some(("local".into(), vec![1]));
        assert!(!state.can_restart_desktop());
    }
}

/// Offline or unreachable per the daemon's presence (muster_net::presence).
pub fn member_offline(member: &v1::MemberView) -> bool {
    matches!(member.presence.as_str(), "offline" | "unreachable")
}
