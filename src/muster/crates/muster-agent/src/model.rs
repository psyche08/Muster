//! Provider-neutral session model. The proto types are the wire form; the
//! service keeps these and projects them when asked.

use std::path::PathBuf;

pub use muster_proto::v1::{AgentMessage, AgentSession, PendingInteraction};

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Provider {
    Codex,
    Claude,
    /// CatDesk (bundle com.catpaw.cowork): sessions live in the app's own
    /// SQLite store, not in a transcript file per session.
    Catdesk,
    /// DeepSeek Harness (bundle com.deepseek.dsh): one event log per
    /// session, written as Zstandard frames; viewed, not driven.
    Deepseek,
}

impl Provider {
    pub const ALL: [Provider; 4] = [
        Provider::Codex,
        Provider::Claude,
        Provider::Catdesk,
        Provider::Deepseek,
    ];

    pub fn as_str(self) -> &'static str {
        match self {
            Provider::Codex => "codex",
            Provider::Claude => "claude",
            Provider::Catdesk => "catdesk",
            Provider::Deepseek => "deepseek",
        }
    }

    pub fn parse(s: &str) -> Option<Self> {
        match s {
            "codex" => Some(Provider::Codex),
            "claude" => Some(Provider::Claude),
            "catdesk" => Some(Provider::Catdesk),
            "deepseek" => Some(Provider::Deepseek),
            _ => None,
        }
    }

    /// Whether each session is one append-only JSONL transcript file, so
    /// file offsets, tails and backwards scans mean something (the
    /// profile's [`crate::profile::Source`]).
    pub fn file_transcripts(self) -> bool {
        crate::profile::app(self.as_str())
            .is_none_or(|app| app.source == crate::profile::Source::Jsonl)
    }

    /// The record grammar and the hooks of a JSONL source; `None` for a
    /// store-backed provider.
    pub fn jsonl(
        self,
    ) -> Option<(
        &'static crate::profile::Records,
        &'static dyn crate::jsonl::Hooks,
    )> {
        let hooks: &'static dyn crate::jsonl::Hooks = match self {
            Provider::Codex => &crate::codex::HOOKS,
            Provider::Claude => &crate::claude::HOOKS,
            Provider::Deepseek => &crate::deepseek::HOOKS,
            Provider::Catdesk => return None,
        };
        Some((self.profile().records?, hooks))
    }

    /// The app's profile (every provider has one).
    pub fn profile(self) -> &'static crate::profile::App {
        match self {
            Provider::Codex => &crate::profile::CODEX_APP,
            Provider::Claude => &crate::profile::CLAUDE_APP,
            Provider::Catdesk => &crate::profile::CATDESK_APP,
            Provider::Deepseek => &crate::profile::DEEPSEEK_APP,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub enum SessionState {
    #[default]
    Idle,
    Running,
    WaitingApproval,
    Ended,
    Unknown,
}

impl SessionState {
    pub fn as_str(self) -> &'static str {
        match self {
            SessionState::Idle => "idle",
            SessionState::Running => "running",
            SessionState::WaitingApproval => "waiting_approval",
            SessionState::Ended => "ended",
            SessionState::Unknown => "unknown",
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub enum ControllerKind {
    #[default]
    None,
    NativeApp,
    /// The provider's cloud API drives the session (`crate::cloud`).
    Cloud,
    /// Muster's plugin inside the desktop app drives the session
    /// (`crate::deepseek_app`).
    Plugin,
}

impl ControllerKind {
    pub fn as_str(self) -> &'static str {
        match self {
            ControllerKind::None => "none",
            ControllerKind::NativeApp => "native_app",
            ControllerKind::Cloud => "cloud",
            ControllerKind::Plugin => "plugin",
        }
    }
}

/// What a transcript reader knows about its session.
#[derive(Clone, Debug, Default)]
pub struct SessionMeta {
    pub title: String,
    pub cwd: PathBuf,
    pub updated_unix: u64,
    pub archived: bool,
    /// The provider's own view of activity, derived from the transcript.
    pub state: SessionState,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Role {
    User,
    Assistant,
    Tool,
    System,
}

impl Role {
    pub fn as_str(self) -> &'static str {
        match self {
            Role::User => "user",
            Role::Assistant => "assistant",
            Role::Tool => "tool",
            Role::System => "system",
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum MessageKind {
    Text,
    ToolCall,
    ToolResult,
    Interaction,
    TurnSummary,
    ProviderCard,
}

impl MessageKind {
    pub fn as_str(self) -> &'static str {
        match self {
            MessageKind::ProviderCard => "provider_card",
            MessageKind::Text => "text",
            MessageKind::ToolCall => "tool_call",
            MessageKind::ToolResult => "tool_result",
            MessageKind::Interaction => "interaction",
            MessageKind::TurnSummary => "turn_summary",
        }
    }
}

/// One transcript message. `id` is stable across reads: the byte offset of
/// the record in its file, so paging by id survives restarts.
#[derive(Clone, Debug, PartialEq)]
pub struct Message {
    pub id: String,
    pub role: Role,
    pub kind: MessageKind,
    pub text: String,
    pub ts_unix: u64,
    pub usage: Option<muster_proto::v1::TurnUsage>,
    pub phase: String,
    pub tool_name: String,
    pub tool_call_id: String,
}

impl Message {
    pub fn to_proto(&self) -> AgentMessage {
        AgentMessage {
            id: self.id.clone(),
            role: self.role.as_str().into(),
            text: self.text.clone(),
            ts_unix: self.ts_unix,
            kind: self.kind.as_str().into(),
            usage: self.usage.clone(),
            phase: self.phase.clone(),
            tool_name: self.tool_name.clone(),
            tool_call_id: self.tool_call_id.clone(),
            tool_status: String::new(),
        }
    }
}

/// RFC 3339 → unix seconds; 0 when unparsable.
pub fn parse_ts(s: &str) -> u64 {
    time::OffsetDateTime::parse(s, &time::format_description::well_known::Rfc3339)
        .map(|t| t.unix_timestamp().max(0) as u64)
        .unwrap_or(0)
}

pub fn now_unix() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0)
}

/// Clips a title to one line of bounded length.
pub fn title_from(text: &str) -> String {
    let line = text
        .lines()
        .map(str::trim)
        .find(|l| !l.is_empty())
        .unwrap_or("");
    let mut out: String = line.chars().take(80).collect();
    if line.chars().count() > 80 {
        out.push('…');
    }
    out
}
