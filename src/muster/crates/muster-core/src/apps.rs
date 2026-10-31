//! The desktop apps Muster drives (ARCHITECTURE §3.6.4): what identifies
//! each one and what people call it, in one table. How an app's sessions are
//! read and its controls found lives in muster-agent and muster-platform;
//! this is only who the app is.

/// One desktop app whose sessions Muster views and controls.
#[derive(Debug, PartialEq, Eq)]
pub struct DesktopApp {
    /// The agent provider: session providers, `agents:<provider>` scopes and
    /// Locked Use provider queues.
    pub provider: &'static str,
    /// How access settings name its agent.
    pub agent_label: &'static str,
    /// Its ordinary Chat mode, if the app has one.
    pub chat: Option<ChatMode>,
    /// The macOS bundle identifier, also the local Accessibility allowlist entry.
    pub bundle_id: &'static str,
    /// The Apple Developer team that signs the app.
    pub team_id: &'static str,
}

/// An app's ordinary (non-agent) chats.
#[derive(Debug, PartialEq, Eq)]
pub struct ChatMode {
    /// `chats:<provider>`.
    pub provider: &'static str,
    /// The app as people know it in Chat mode.
    pub label: &'static str,
}

/// Codex, inside the ChatGPT desktop app, whose ChatGPT mode has chats.
pub const CODEX: DesktopApp = DesktopApp {
    provider: "codex",
    agent_label: "Codex",
    chat: Some(ChatMode {
        provider: "chatgpt",
        label: "ChatGPT",
    }),
    bundle_id: "com.openai.codex",
    team_id: "2DC432GLL2",
};

/// Claude Code in the Claude desktop app, whose Claude mode has chats.
pub const CLAUDE: DesktopApp = DesktopApp {
    provider: "claude",
    agent_label: "Claude Code",
    chat: Some(ChatMode {
        provider: "claude",
        label: "Claude",
    }),
    bundle_id: "com.anthropic.claudefordesktop",
    team_id: "Q6L2SF6YDW",
};

/// CatDesk; it has no separate Chat mode.
pub const CATDESK: DesktopApp = DesktopApp {
    provider: "catdesk",
    agent_label: "CatDesk",
    chat: None,
    bundle_id: "com.catpaw.cowork",
    team_id: "BHWTW6L8X6",
};

/// DeepSeek Harness; it has no separate Chat mode.
pub const DEEPSEEK: DesktopApp = DesktopApp {
    provider: "deepseek",
    agent_label: "DeepSeek Harness",
    chat: None,
    bundle_id: "com.deepseek.dsh",
    team_id: "NAN929V4UM",
};

/// Every app, in the order lists show them.
pub static APPS: [&DesktopApp; 4] = [&CODEX, &CLAUDE, &CATDESK, &DEEPSEEK];

/// The app whose agent provider is `provider`.
pub fn app(provider: &str) -> Option<&'static DesktopApp> {
    APPS.iter().copied().find(|a| a.provider == provider)
}

/// The app with a Chat mode by either of its names (`claude`; `chatgpt` or
/// `codex`), ignoring case: what a person types in a diagnostic.
pub fn chat_app(name: &str) -> Option<&'static DesktopApp> {
    let name = name.to_ascii_lowercase();
    APPS.iter().copied().find(|a| {
        a.chat
            .as_ref()
            .is_some_and(|c| c.provider == name || a.provider == name)
    })
}

/// The label of a chat provider (`chatgpt` → ChatGPT).
pub fn chat_label(provider: &str) -> Option<&'static str> {
    APPS.iter()
        .find_map(|a| a.chat.as_ref().filter(|c| c.provider == provider))
        .map(|c| c.label)
}

/// Agent providers, in list order.
pub fn agent_providers() -> Vec<&'static str> {
    APPS.iter()
        .map(|a| a.provider)
        .chain(std::iter::once("orca"))
        .collect()
}

/// Chat providers, in list order.
pub fn chat_providers() -> Vec<&'static str> {
    APPS.iter()
        .filter_map(|a| a.chat.as_ref().map(|c| c.provider))
        .collect()
}

impl DesktopApp {
    /// The code signing requirement a process must satisfy to be this app.
    pub fn requirement(&self) -> String {
        format!(
            "identifier \"{}\" and anchor apple generic and certificate leaf[subject.OU] = \"{}\"",
            self.bundle_id, self.team_id
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn names_resolve_to_one_app() {
        assert_eq!(app("codex").unwrap().bundle_id, "com.openai.codex");
        assert!(app("chatgpt").is_none(), "a chat provider is not an agent");
        assert_eq!(chat_app("ChatGPT").unwrap().provider, "codex");
        assert_eq!(chat_app("codex").unwrap().provider, "codex");
        assert_eq!(chat_app("Claude").unwrap().provider, "claude");
        assert!(chat_app("catdesk").is_none(), "CatDesk has no Chat mode");
        assert!(
            chat_app("deepseek").is_none(),
            "DeepSeek Harness has no Chat mode"
        );
        assert_eq!(
            agent_providers(),
            ["codex", "claude", "catdesk", "deepseek", "orca"]
        );
        assert_eq!(chat_providers(), ["chatgpt", "claude"]);
        assert_eq!(chat_label("chatgpt"), Some("ChatGPT"));
        assert_eq!(chat_label("codex"), None);
    }

    /// The requirements the controllers enforced before the table existed.
    #[test]
    fn requirements_are_unchanged() {
        assert_eq!(
            app("claude").unwrap().requirement(),
            "identifier \"com.anthropic.claudefordesktop\" and anchor apple generic and certificate leaf[subject.OU] = \"Q6L2SF6YDW\""
        );
        assert_eq!(
            app("codex").unwrap().requirement(),
            "identifier \"com.openai.codex\" and anchor apple generic and certificate leaf[subject.OU] = \"2DC432GLL2\""
        );
        assert_eq!(
            app("catdesk").unwrap().requirement(),
            "identifier \"com.catpaw.cowork\" and anchor apple generic and certificate leaf[subject.OU] = \"BHWTW6L8X6\""
        );
        // The installed DeepSeek Harness 0.2.1-rc.5's signature (codesign -dv).
        assert_eq!(
            app("deepseek").unwrap().requirement(),
            "identifier \"com.deepseek.dsh\" and anchor apple generic and certificate leaf[subject.OU] = \"NAN929V4UM\""
        );
    }
}
