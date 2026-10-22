//! Which apps have a Chat mode, and how a conversation on screen is
//! named as an agent session (`<provider>-chat-<key>`).

use sha2::{Digest, Sha256};

use crate::model::Provider;

/// The two desktop apps with a Chat mode.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum ChatApp {
    /// Claude.app: the "Claude" entry of its Mode switch (the other is Code).
    Claude,
    /// ChatGPT.app (bundle com.openai.codex): ChatGPT mode; the other is Codex.
    ChatGpt,
}

impl ChatApp {
    pub const ALL: [ChatApp; 2] = [ChatApp::ChatGpt, ChatApp::Claude];

    pub fn provider(self) -> Provider {
        match self {
            ChatApp::Claude => Provider::Claude,
            ChatApp::ChatGpt => Provider::Codex,
        }
    }

    pub fn from_provider(provider: &str) -> Option<ChatApp> {
        match Provider::parse(provider)? {
            Provider::Claude => Some(ChatApp::Claude),
            Provider::Codex => Some(ChatApp::ChatGpt),
            // CatDesk has no separate Chat mode.
            Provider::Catdesk => None,
        }
    }

    /// The app's name as the user knows it in Chat mode. Desktop locks are
    /// keyed by it, so it must not change between releases.
    pub fn name(self) -> &'static str {
        muster_core::apps::app(self.provider().as_str())
            .and_then(|app| app.chat.as_ref())
            .map_or("", |chat| chat.label)
    }

    /// The app's profile (`crate::profile`): identity, process, controls.
    pub fn profile(self) -> &'static crate::profile::App {
        self.provider().profile()
    }

    /// How its Chat-mode controls are recognized (data, see
    /// `crate::profile`).
    pub fn controls(self) -> &'static crate::profile::ChatControls {
        match self {
            ChatApp::Claude => &crate::profile::CLAUDE_CHAT,
            ChatApp::ChatGpt => &crate::profile::CHATGPT_CHAT,
        }
    }

    fn prefix(self) -> String {
        format!("{}-chat-", self.provider().as_str())
    }
}

/// What identifies a conversation the app shows.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub enum ChatKey {
    /// The empty page a new conversation starts from.
    New,
    /// The app's own conversation id (from its URL).
    Id(String),
    /// Only a title is exposed; unique titles select, renames are followed
    /// while the conversation is watched.
    Title(String),
}

/// The agent session id of a conversation: `<provider>-chat-<key>`.
pub fn session_id(app: ChatApp, key: &ChatKey) -> String {
    match key {
        ChatKey::New => format!("{}new", app.prefix()),
        ChatKey::Id(id) => {
            let clean: String = id
                .chars()
                .filter(|c| c.is_ascii_alphanumeric() || *c == '-' || *c == '_')
                .take(64)
                .collect();
            format!("{}{}", app.prefix(), clean.to_ascii_lowercase())
        }
        ChatKey::Title(title) => {
            let digest = Sha256::digest(title.as_bytes());
            format!("{}t{}", app.prefix(), &hex::encode(digest)[..16])
        }
    }
}

/// The session id that starts a new conversation on its first send.
pub fn new_session_id(app: ChatApp) -> String {
    session_id(app, &ChatKey::New)
}

/// The app a chat session id belongs to; `None` for transcript sessions.
pub fn app_of(session: &str) -> Option<ChatApp> {
    ChatApp::ALL
        .into_iter()
        .find(|app| session.starts_with(&app.prefix()) && session.len() > app.prefix().len())
}

pub fn is_new_session(session: &str) -> bool {
    app_of(session).is_some_and(|app| session == new_session_id(app))
}
