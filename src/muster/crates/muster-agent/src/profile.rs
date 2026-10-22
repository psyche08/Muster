//! Desktop app profiles as data (ARCHITECTURE §3.6.4): how an app's
//! controls are recognized, where its sessions come from and what it
//! offers, written once per app in `profiles/<app>.toml` and compiled by
//! `build.rs` into the statics of this module (`CLAUDE`, `CLAUDE_CHAT`,
//! `CLAUDE_APP`, …). The schema is `docs/APP_PROFILES.md`. One matcher
//! evaluates the selectors over either a live Accessibility element
//! (muster-platform) or a recorded snapshot (tests, and the Chat-mode
//! parser). A profile only says which element is which and which of the
//! engine's strategies apply; the controller keeps every check on process
//! identity, drafts, input, enabled state and presses, and a profile can
//! only narrow what the engine allows.
//!
//! Two surfaces are described: an app's agent conversation ([`Controls`]:
//! Claude Code, Codex, CatDesk) and the Claude and ChatGPT apps' Chat mode
//! ([`ChatControls`]). [`App`] ties them to the app's identity
//! (`muster_core::apps`), its process, its session source and its declared
//! [`Capabilities`]. Selectors are proven against trees recorded from the
//! real apps (`tests/fixtures/ax`, redacted).

use serde_json::Value;

use crate::desktop_chat::AxNode;

/// Where a selector reads a control's label.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Attr {
    /// AXTitle.
    Title,
    /// AXDescription (Chromium reports `aria-label` here).
    Description,
    /// AXPlaceholderValue.
    Placeholder,
    /// What the control is called: its title, else its description. A
    /// button labelled only by `aria-label` has an empty title.
    Name,
}

/// What a selector reads from an element. Empty attributes are `None`.
pub trait AxQuery {
    fn role(&self) -> String;
    fn title(&self) -> Option<String>;
    fn description(&self) -> Option<String>;
    fn placeholder(&self) -> Option<String>;
}

impl Attr {
    fn read(self, node: &impl AxQuery) -> Option<String> {
        match self {
            Attr::Title => node.title(),
            Attr::Description => node.description(),
            Attr::Placeholder => node.placeholder(),
            Attr::Name => node.title().or_else(|| node.description()),
        }
    }
}

/// One kind of control: its roles, and the labels it carries in any of the
/// given attributes, matched exactly.
#[derive(Debug)]
pub struct Selector {
    pub roles: &'static [&'static str],
    pub attrs: &'static [Attr],
    pub labels: &'static [&'static str],
}

impl Selector {
    pub fn matches(&self, node: &impl AxQuery) -> bool {
        self.roles.contains(&node.role().as_str())
            && self.attrs.iter().any(|&attr| {
                attr.read(node)
                    .is_some_and(|label| self.labels.contains(&label.as_str()))
            })
    }
}

/// How a prompt is put into the composer; the engine implements both and
/// then waits for the exact echo before any press.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Draft {
    /// Set the composer's value: Electron editors that re-read their DOM
    /// (ProseMirror).
    Value,
    /// Focus, select the whole draft and replace the selection: editors that
    /// revert DOM edits they did not make (Lexical).
    Selection,
}

/// Where the sessions a user can switch to are listed.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SessionList {
    /// Rows in the app shell's sidebar (`app-shell-sidebar`).
    Sidebar {
        /// Navigation and complementary landmarks are lists too, also
        /// inside the shell sidebar.
        landmarks: bool,
        /// A row's name can carry its status ("Idle"); its one standalone
        /// text is then its title.
        standalone_title: bool,
        /// The control that lists older sessions, pressed when the session
        /// is not among the rows shown.
        more: Option<More>,
    },
    /// Pressable rows of the main window's page, which has no landmark,
    /// found from the text reading the title.
    PageRows,
}

/// A control named `prefix`, a count, then `suffix` ("Show 20 more in
/// Recents").
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct More {
    pub prefix: &'static str,
    pub suffix: &'static str,
}

impl More {
    pub fn matches(&self, name: &str) -> bool {
        name.strip_prefix(self.prefix)
            .and_then(|rest| rest.strip_suffix(self.suffix))
            .is_some_and(|count| !count.is_empty() && count.bytes().all(|b| b.is_ascii_digit()))
    }
}

/// An app's own queue of prompts sent while a turn runs.
#[derive(Debug)]
pub struct Queue {
    /// Controls only a queued row carries. A submit control beside them
    /// belongs to the row, never to the composer.
    pub row: Selector,
    /// The row's control that sends it now.
    pub send_now: Selector,
    /// The row's control that deletes it.
    pub delete: Selector,
}

/// The question cards an app shows in its conversation. Labels are matched
/// on a control or the text inside it (Claude's cards).
#[derive(Debug)]
pub struct Questions {
    /// Reopen a collapsed optional question reply panel.
    pub open: &'static [&'static str],
    pub submit: &'static [&'static str],
    /// Activate the exact focused Submit button with Space instead of AXPress.
    pub submit_via_focused_space: bool,
    pub next: &'static [&'static str],
    pub back: &'static [&'static str],
    /// The option that opens a free-text answer.
    pub other: &'static [&'static str],
    /// One page per question with Back and Next: answering starts from the
    /// first page.
    pub paged: bool,
    /// Polling attributes can return Chromium's stale browser-side cache;
    /// ask the renderer for a fresh read while waiting.
    pub refresh_renderer: bool,
    /// A single-select card of plain buttons answers when one is pressed.
    pub button_choices: bool,
    /// Optional single-select suggestion cards answer on press.
    pub immediate_suggestions: bool,
}

/// Which native surface may offer a permission decision. The platform supplies
/// ancestry evidence; the shared profile owns its interpretation.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ApprovalScope {
    WebArea,
    MainConversation,
}

impl ApprovalScope {
    pub fn admits(self, ancestry: &[String], reaches_web_area: bool) -> bool {
        if !reaches_web_area {
            return false;
        }
        match self {
            Self::WebArea => true,
            Self::MainConversation => {
                ancestry.iter().any(|s| s == "AXLandmarkMain")
                    && !ancestry.iter().any(|s| {
                        matches!(
                            s.as_str(),
                            "AXLandmarkComplementary" | "AXLandmarkNavigation" | "AXTabPanel"
                        )
                    })
            }
        }
    }
}

/// The controls of an app's agent conversation, and how the engine drives
/// them.
#[derive(Debug)]
pub struct Controls {
    /// The text area a prompt is drafted in.
    pub composer: Selector,
    /// What the composer reports as its value while empty.
    pub placeholders: &'static [&'static str],
    pub draft: Draft,
    /// The control that submits it. It is looked up again when sending: in
    /// the ChatGPT app it exists only once the composer holds text, and it
    /// gives way to Stop while a turn runs.
    pub send: Selector,
    /// The control that stops the running turn. An enabled one means a turn
    /// is running; it is what an interrupt presses.
    pub stop: Selector,
    /// The options of a pending permission prompt.
    pub approve: Selector,
    pub approval_scope: ApprovalScope,
    pub sessions: SessionList,
    /// `None`: the app has no queued send.
    pub queue: Option<Queue>,
    /// `None`: none that Muster reads (CatDesk's store holds no questions).
    pub questions: Option<Questions>,
}

/// The controls of the Claude or ChatGPT app's Chat mode, read from a
/// snapshot by `desktop_chat`.
#[derive(Debug)]
pub struct ChatControls {
    /// The chat provider (`chats:<provider>`), as `muster_core::apps`
    /// names it.
    pub provider: &'static str,
    /// The conversation's send button.
    pub send: Selector,
    /// The control that stops the running reply.
    pub stop: Selector,
    /// What opens the empty new-conversation page.
    pub new_chat: Selector,
    /// What dismisses a dialog without choosing anything; looked for only
    /// inside a dialog. A dialog without one is left alone.
    pub close_dialog: Selector,
    /// What leaves the app's settings page, if it covers the conversation.
    pub leave_settings: Option<Selector>,
    /// Settings navigation landmark; enables the bounded shell-Home fallback.
    pub settings_navigation: Option<Selector>,
    /// Home in the same web area's app-shell-sidebar, outside settings.
    pub settings_home: Option<Selector>,
    /// A plain new-chat control that counts only in the sidebar.
    pub sidebar_new: Option<Selector>,
    pub area: ChatArea,
    pub url: ChatUrl,
    pub modes: Modes,
    pub history: HistoryRows,
}

/// Which web area shows a Chat-mode conversation.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ChatArea {
    /// The one web area whose URL starts with this: the Claude app wraps
    /// the claude.ai page in its own shell page.
    Url(&'static str),
    /// The web area with a main landmark, else the only web area.
    Main,
}

/// How a page URL names a conversation.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ChatUrl {
    /// The path segment before a conversation's id.
    pub segment: &'static str,
    /// Ids are UUIDs; otherwise at least eight letters, digits, `-` or `_`.
    pub uuid: bool,
    /// The empty new-conversation page.
    pub new: Option<&'static str>,
    /// Only the URL names a conversation; otherwise the sidebar link marked
    /// current or the page title can.
    pub only: bool,
}

/// The app's modes: Chat, and the agent mode beside it.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Modes {
    /// Names of Chat mode.
    pub chat: &'static [&'static str],
    /// Names of the other mode.
    pub other: &'static [&'static str],
    pub switch: ModeSwitch,
}

/// How the app switches modes.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ModeSwitch {
    /// A radio group labelled `group`, one radio per mode (a status may
    /// follow its name after a comma), and the same commands in the
    /// menu-bar menu `menu`. The page URL says the mode: a path containing
    /// `other_path` is the other mode, one under `settings` neither.
    Radio {
        group: &'static str,
        menu: &'static str,
        other_path: &'static str,
        settings: &'static str,
    },
    /// A pop-up button titled `prefix` and the current mode opens a menu
    /// whose entries start with a mode's name.
    Menu { prefix: &'static str },
}

/// How the history lists conversations.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum HistoryRows {
    /// Title buttons in the sidebar, each beside its own `options` + title
    /// menu; navigation buttons have none. The full history is a page at
    /// one of `pages`.
    TitleButtons {
        options: &'static str,
        pages: &'static [&'static str],
    },
    /// Buttons in the lists of the navigation landmark. A conversation row
    /// carries one of `markers`; `actions` are a row's own commands, never
    /// its title; a row with a `project` action is a project.
    Lists {
        actions: &'static [&'static str],
        markers: &'static [&'static str],
        project: &'static str,
    },
}

/// Which of an app's processes shows its windows.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Gui {
    /// Every process running its main executable.
    Executable,
    /// The one its owner-only command socket reports: CatDesk runs node
    /// helpers from its main executable.
    CommandSocket,
}

/// How a web area the app shows is mapped to the session id the service
/// (and the phone) knows. The engine implements each strategy once; the
/// daemon builds the app's resolver from it.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SessionKey {
    /// The page URL names the desktop session (the Claude app's
    /// `/epitaxy/<desktop id>`), mapped to the CLI session file it records.
    Url,
    /// The web area's title names the session (a Codex thread); it resolves
    /// only while exactly one session carries that title.
    Title,
    /// A popout names its session in its URL; the main window shows the
    /// session the running app reports as current (CatDesk's command socket).
    Runtime,
}

/// Where an app's sessions and their history are read.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Source {
    /// One append-only JSONL transcript per session (Codex rollouts, Claude
    /// projects): file offsets, tails and backwards scans mean something.
    Jsonl,
    /// The app's own SQLite store, read only (CatDesk).
    Sqlite,
}

/// What an app offers. Shared Rust rejects an operation the profile does
/// not declare and the ViewModel shows it unavailable; a declared one is
/// still subject to every engine check.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Capabilities {
    /// Prompts can be sent to a session shown in the app.
    pub send: bool,
    /// Permission prompts can be answered.
    pub respond: bool,
    /// A running turn can be interrupted.
    pub interrupt: bool,
    /// Question cards are read from the session source and answered
    /// through the app (`Controls::questions` is present).
    pub questions: bool,
    /// Prompts the app queued can be sent now or deleted (`Controls::queue`
    /// is present).
    pub queue: bool,
    /// The session's working tree, files and git history can be browsed.
    pub files: bool,
    /// The app has a Chat mode beside its agent (`App::chat` is present).
    pub chat: bool,
    /// Native agent and Chat modes offer model and effort choices.
    pub model_options: bool,
}

/// A condition on one field of a JSON record, addressed by a dotted path.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Cond {
    /// The field is this string.
    Is(&'static str),
    /// The field is one of these strings.
    In(&'static [&'static str]),
    /// The field is `true`.
    True,
}

/// One field a record must satisfy.
#[derive(Debug, PartialEq, Eq)]
pub struct Field {
    pub path: &'static str,
    pub cond: Cond,
}

/// A record matches when every field holds (an empty match holds always).
#[derive(Debug, PartialEq, Eq)]
pub struct Match(pub &'static [Field]);

impl Match {
    pub fn matches(&self, record: &Value) -> bool {
        self.0.iter().all(|f| match f.cond {
            Cond::Is(s) => field(record, f.path).as_str() == Some(s),
            Cond::In(list) => field(record, f.path)
                .as_str()
                .is_some_and(|s| list.contains(&s)),
            Cond::True => field(record, f.path).as_bool() == Some(true),
        })
    }
}

/// The value at a dotted path (`payload.type`); `Null` where the path
/// leaves the object.
pub fn field<'a>(value: &'a Value, path: &str) -> &'a Value {
    path.split('.').fold(value, |v, key| &v[key])
}

/// Where a source keeps its state: an environment override, else a
/// directory under `$HOME`.
#[derive(Debug, PartialEq, Eq)]
pub struct Home {
    pub env: &'static str,
    pub dir: &'static str,
}

/// What a message with a role that is neither `user` nor `assistant` is.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum OtherRoles {
    /// Not a message.
    Skip,
    /// The assistant's.
    Assistant,
}

/// How the blocks of one message are numbered in message ids
/// (`<offset>:<index>`), which must not change between releases.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum BlockIds {
    /// Text blocks count; tool calls and results are records of their own.
    Text,
    /// Every content block counts, whatever its type.
    All,
}

/// Where a tool call or result is found.
#[derive(Debug, PartialEq, Eq)]
pub enum Where {
    /// A record of its own; its fields are read from the message envelope.
    Record(Match),
    /// A content block of this type inside a message.
    Block(&'static str),
}

/// Which records are messages and where their parts are.
#[derive(Debug, PartialEq, Eq)]
pub struct MessageRule {
    pub when: Match,
    /// The object holding role and content (`payload`, `message`).
    pub envelope: &'static str,
    /// The role, within the envelope.
    pub role: &'static str,
    /// A record field that names the role when the envelope has none.
    pub role_fallback: Option<&'static str>,
    pub other_roles: OtherRoles,
    /// The content (a string or blocks), within the envelope.
    pub content: &'static str,
    /// Block types whose `text` a person reads.
    pub text_blocks: &'static [&'static str],
    /// Block types that are content without text (images).
    pub media_blocks: &'static [&'static str],
    pub block_ids: BlockIds,
    /// The turn phase of the message, within the envelope (`final_answer`).
    pub phase: Option<&'static str>,
}

/// Where a tool call is and how it is named.
#[derive(Debug, PartialEq, Eq)]
pub struct ToolCallRule {
    pub at: Where,
    pub name: &'static str,
    pub id: &'static str,
    /// The first present of these holds the arguments.
    pub arguments: &'static [&'static str],
}

/// Where a tool result is and what it says.
#[derive(Debug, PartialEq, Eq)]
pub struct ToolResultRule {
    pub at: Where,
    /// The id of the call it answers.
    pub id: &'static str,
    pub output: &'static str,
    /// A field that is `true` when the tool failed.
    pub error: Option<&'static str>,
    /// Tools whose results are the agent asking the person something.
    pub interaction_tools: &'static [&'static str],
}

/// How turns begin and end.
#[derive(Debug, PartialEq, Eq)]
pub struct TurnRule {
    /// A record that starts a turn (the app's own event).
    pub started: Option<Match>,
    /// A record that ends a turn.
    pub ended: Match,
    pub ended_also: &'static [Match],
    /// A user message with text starts a turn.
    pub user_text_starts: bool,
    /// The text of the record that ends a turn is the final answer.
    pub mark_final: bool,
    /// Any message record counts as activity (the source has no events).
    pub activity_on_messages: bool,
    /// A message in this phase ends the turn (recency only).
    pub final_phase: Option<&'static str>,
}

impl TurnRule {
    pub fn ends(&self, record: &Value) -> bool {
        self.ended.matches(record) || self.ended_also.iter().any(|m| m.matches(record))
    }
}

/// The record grammar of a JSONL session source: what the engine
/// (`crate::jsonl`) needs to turn appended records into messages.
#[derive(Debug, PartialEq, Eq)]
pub struct Records {
    pub home: Home,
    /// The record field holding an RFC 3339 time.
    pub timestamp: &'static str,
    /// Record fields that name the working directory, first present wins.
    pub cwd: &'static [&'static str],
    /// Records to ignore entirely.
    pub skip: &'static [Match],
    /// Everything before the first such record is the app's bootstrap.
    pub bootstrap_until: Option<Match>,
    /// A record whose turn summary belongs before its own messages.
    pub summary_before: Option<Match>,
    pub message: MessageRule,
    pub tool_call: ToolCallRule,
    pub tool_result: ToolResultRule,
    pub turn: TurnRule,
}

/// Which cloud API the engine speaks for an app's cloud sessions
/// (`crate::cloud`), named by the profile's `cloud.api`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CloudApi {
    /// Claude Code on the web: `GET /v1/code/sessions`, `GET
    /// /v1/code/sessions/{id}/events` and `POST /v1/code/sessions/{id}/events`
    /// on the Anthropic API with the CLI's OAuth token. Events are the same
    /// records as the local transcript, read by the one JSONL engine.
    ClaudeCode,
    /// Codex cloud tasks: `GET /wham/tasks/list` and `GET /wham/tasks/{id}`
    /// on the ChatGPT backend with the CLI's ChatGPT token; a task's prompt,
    /// replies and diff are read, nothing is written.
    CodexTasks,
}

/// Where the provider's CLI keeps the credentials the cloud API takes:
/// a JSON file under the source's home, with dotted paths into it.
#[derive(Debug, PartialEq, Eq)]
pub struct CloudCredentials {
    /// The file under the source's home (`.credentials.json`, `auth.json`).
    pub file: &'static str,
    /// The path of the bearer token.
    pub token: &'static str,
    /// The path of the token's expiry, milliseconds since the epoch, if the
    /// file records one; otherwise the token's own `exp` claim counts.
    pub expires_at: Option<&'static str>,
    /// The path of the account id sent beside the token
    /// (`ChatGPT-Account-Id`).
    pub account: Option<&'static str>,
    /// On macOS, the generic password holding the same JSON when the CLI
    /// keeps it in the Keychain instead of the file (`Claude
    /// Code-credentials`). Read with the user's consent prompt, never by
    /// unlocking a keychain.
    pub keychain: Option<&'static str>,
}

/// What a cloud session offers; everything else (approvals, interrupts,
/// files, question cards, queues) stays with the desktop app.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct CloudCapabilities {
    /// A prompt can be posted into a session.
    pub send: bool,
}

/// The app's cloud sessions: the same account's sessions that run in the
/// provider's cloud, listed, followed and written through its API.
#[derive(Debug)]
pub struct Cloud {
    pub api: CloudApi,
    /// The API origin, no trailing slash.
    pub base_url: &'static str,
    /// How the shells name a cloud session's provider ("Claude Cloud").
    pub label: &'static str,
    /// How often the daemon lists sessions and polls the ones being read
    /// while someone watches; a quarter as often otherwise.
    pub poll_secs: u64,
    pub credentials: CloudCredentials,
    pub capabilities: CloudCapabilities,
}

/// One desktop app, as compiled from `profiles/<provider>.toml`.
#[derive(Debug)]
pub struct App {
    /// Who the app is: provider, labels, bundle id, signing team.
    pub app: &'static muster_core::apps::DesktopApp,
    /// The main executable's path suffix (libproc), e.g.
    /// `Claude.app/Contents/MacOS/Claude`.
    pub executable: &'static str,
    pub gui: Gui,
    pub session_key: SessionKey,
    pub source: Source,
    /// The record grammar of a JSONL source; `None` for a store.
    pub records: Option<&'static Records>,
    /// The name of the cross-process desktop lock every daemon of this user
    /// shares for the app; fixed across releases.
    pub lock: &'static str,
    pub capabilities: Capabilities,
    /// The agent conversation's controls.
    pub controls: &'static Controls,
    /// The Chat mode's controls, if the app has one.
    pub chat: Option<&'static ChatControls>,
    /// The app's cloud sessions, if the profile declares them.
    pub cloud: Option<&'static Cloud>,
}

impl App {
    /// The agent provider (`claude`, `codex`, `catdesk`).
    pub fn provider(&self) -> &'static str {
        self.app.provider
    }

    /// Whether the composer's value means "nothing typed".
    pub fn is_empty(&self, value: &str) -> bool {
        !self.controls.is_draft(value)
    }
}

include!(concat!(env!("OUT_DIR"), "/profiles.rs"));

/// The app whose agent provider is `provider`.
pub fn app(provider: &str) -> Option<&'static App> {
    APPS.iter().copied().find(|a| a.provider() == provider)
}

/// The app whose Chat mode is `provider` (`chatgpt`, `claude`).
pub fn chat_app(provider: &str) -> Option<&'static App> {
    APPS.iter()
        .copied()
        .find(|a| a.chat.is_some_and(|c| c.provider == provider))
}

/// What `provider` declares, if it is a profiled app.
pub fn capabilities(provider: &str) -> Option<Capabilities> {
    app(provider).map(|a| a.capabilities)
}

/// The cloud sessions of the app whose agent provider is `provider`.
pub fn cloud(provider: &str) -> Option<&'static Cloud> {
    app(provider).and_then(|a| a.cloud)
}

impl Controls {
    /// Whether the composer's value is a draft: neither blank nor the
    /// placeholder an empty composer reports as its value.
    pub fn is_draft(&self, value: &str) -> bool {
        let trimmed = value.trim();
        !trimmed.is_empty() && !self.placeholders.contains(&trimmed)
    }
}

impl AxQuery for AxNode {
    fn role(&self) -> String {
        self.role.clone()
    }
    fn title(&self) -> Option<String> {
        (!self.title.is_empty()).then(|| self.title.clone())
    }
    fn description(&self) -> Option<String> {
        (!self.description.is_empty()).then(|| self.description.clone())
    }
    fn placeholder(&self) -> Option<String> {
        (!self.placeholder.is_empty()).then(|| self.placeholder.clone())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::desktop_chat::tests::tree;
    use crate::desktop_chat::{
        AxTree, ChatApp, ChatKey, conversation, history_opener, mode, mode_control,
        new_chat_control, obstruction, sidebar,
    };

    const CODEX_DESKTOP: &str = include_str!("../tests/fixtures/ax/codex-desktop.dump");
    const CLAUDE_CODE: &str = include_str!("../tests/fixtures/ax/claude-code.dump");
    const CATDESK_DUMP: &str = include_str!("../tests/fixtures/ax/catdesk.dump");
    const CLAUDE_CODE_RUNNING: &str = include_str!("../tests/fixtures/ax/claude-code-running.dump");
    const CLAUDE_CODE_QUESTION: &str =
        include_str!("../tests/fixtures/ax/claude-code-question.dump");
    const CODEX_DESKTOP_RUNNING: &str =
        include_str!("../tests/fixtures/ax/codex-desktop-running.dump");
    const CLAUDE_CODE_RUNNING_DRAFT: &str =
        include_str!("../tests/fixtures/ax/claude-code-running-draft.dump");
    const CODEX_DESKTOP_RUNNING_DRAFT: &str =
        include_str!("../tests/fixtures/ax/codex-desktop-running-draft.dump");
    const CHATGPT_NEW_DUMP: &str = include_str!("../tests/fixtures/ax/chatgpt-chat-new.dump");
    const CHATGPT_CHAT_DUMP: &str = include_str!("../tests/fixtures/ax/chatgpt-chat.dump");
    const CLAUDE_CHAT_DUMP: &str = include_str!("../tests/fixtures/ax/claude-chat-new.dump");

    fn found(tree: &AxTree, selector: &Selector) -> Vec<usize> {
        (0..tree.nodes.len())
            .filter(|&i| selector.matches(&tree.nodes[i]))
            .collect()
    }

    /// Codex, idle, empty composer (recorded 2026-10-03, Mac-B, ChatGPT app
    /// in Codex mode): one composer showing its placeholder, no send button
    /// until text is typed, and the Markdown editor beside it is not a
    /// composer.
    #[test]
    fn codex_desktop_recording() {
        let tree = tree(CODEX_DESKTOP);
        let composers = found(&tree, &CODEX.composer);
        assert_eq!(composers.len(), 1);
        assert_eq!(tree.nodes[composers[0]].title, "Do anything");
        assert!(found(&tree, &CODEX.send).is_empty());
        idle(&tree, &CODEX);
        let text_areas = tree.nodes.iter().filter(|n| n.role == "AXTextArea").count();
        assert_eq!(text_areas, 2, "the composer and the Markdown editor");
    }

    /// The installed 26.1002.52244 bundle uses Work with Codex for existing
    /// threads (including these zh-CN / zh-HK / zh-TW translations). Apply
    /// those labels to the older recorded layouts: these are regression
    /// variants, not new live AX recordings.
    #[test]
    fn codex_followup_composer_labels_in_recorded_layouts() {
        for label in ["Work with Codex", "与 Codex 协作", "與 Codex 協作"] {
            for recording in [CODEX_DESKTOP, CODEX_DESKTOP_RUNNING] {
                let tree = tree(&recording.replace("Do anything", label));
                let composers = found(&tree, &CODEX.composer);
                assert_eq!(composers.len(), 1, "{label}");
                let value = tree.nodes[composers[0]].value.replace('⏎', "\n");
                assert!(!CODEX.is_draft(&value), "placeholder is not a draft");
                assert!(codex_submits(&tree).is_empty());
            }
            let tree = tree(&CODEX_DESKTOP_RUNNING_DRAFT.replace("Do anything", label));
            let composers = found(&tree, &CODEX.composer);
            assert_eq!(composers.len(), 1, "{label}");
            assert!(CODEX.is_draft(&tree.nodes[composers[0]].value));
            let submits = codex_submits(&tree);
            assert_eq!(submits.len(), 1);
            assert_eq!(tree.nodes[submits[0]].label(), "Queue");
        }
    }

    #[test]
    fn codex_composer_labels_remain_exact_and_role_scoped() {
        for label in [
            "Do anything",
            "Ask anything",
            "Work with Codex",
            "与 Codex 协作",
            "與 Codex 協作",
            "随心输入",
            "輸入任何任務",
            "想做什麼都可以",
        ] {
            for attr in [Attr::Title, Attr::Description, Attr::Placeholder] {
                let mut node = AxNode {
                    role: "AXTextArea".into(),
                    ..Default::default()
                };
                match attr {
                    Attr::Title => node.title = label.into(),
                    Attr::Description => node.description = label.into(),
                    Attr::Placeholder => node.placeholder = label.into(),
                    Attr::Name => unreachable!(),
                }
                assert!(CODEX.composer.matches(&node), "{attr:?}: {label}");
                assert!(!CODEX.is_draft(label));
                node.role = "AXButton".into();
                assert!(!CODEX.composer.matches(&node));
            }
        }
        for label in [
            "Ask ChatGPT",
            "Work with ChatGPT",
            "Work with Codex in a document",
            "Search",
            "",
        ] {
            let node = AxNode {
                role: "AXTextArea".into(),
                title: label.into(),
                ..Default::default()
            };
            assert!(!CODEX.composer.matches(&node), "{label}");
        }
    }

    /// Claude Code, idle (recorded 2026-10-03, Mac-B, Claude app in Code
    /// mode): three web areas each hold a Prompt composer and its Send
    /// button; "Send feedback" and "Run in terminal and send output to
    /// Claude" are not send buttons.
    #[test]
    fn claude_code_recording() {
        let tree = tree(CLAUDE_CODE);
        assert_eq!(found(&tree, &CLAUDE.composer).len(), 3);
        let sends = found(&tree, &CLAUDE.send);
        assert_eq!(sends.len(), 3);
        assert!(sends.iter().all(|&i| tree.nodes[i].description == "Send"));
        idle(&tree, &CLAUDE);
    }

    /// CatDesk, idle (recorded 2026-10-03, Mac-B): the chat input and the
    /// send slot showing its disabled hint.
    #[test]
    fn catdesk_recording() {
        let tree = tree(CATDESK_DUMP);
        let composers = found(&tree, &CATDESK.composer);
        assert_eq!(composers.len(), 1);
        assert_eq!(tree.nodes[composers[0]].description, "聊天输入框");
        let sends = found(&tree, &CATDESK.send);
        assert_eq!(sends.len(), 1);
        assert_eq!(
            tree.nodes[sends[0]].description,
            "Enter a message before sending"
        );
        idle(&tree, &CATDESK);
    }

    /// An idle conversation shows neither Stop nor a permission prompt;
    /// none of the hundreds of recorded controls is mistaken for one.
    fn idle(tree: &AxTree, controls: &Controls) {
        assert!(found(tree, &controls.stop).is_empty());
        assert!(found(tree, &controls.approve).is_empty());
    }

    /// The ChatGPT app's own New chat buttons (recorded in Codex mode, the
    /// same window chrome as Chat mode); no dialog or settings page covers
    /// it.
    #[test]
    fn chatgpt_chat_controls_in_the_recording() {
        let tree = tree(CODEX_DESKTOP);
        let new_chats = found(&tree, &CHATGPT_CHAT.new_chat);
        assert_eq!(new_chats.len(), 2);
        assert!(
            new_chats
                .iter()
                .all(|&i| tree.nodes[i].label() == "New chat")
        );
        assert_eq!(obstruction(ChatApp::ChatGpt, &tree), None);
    }

    /// Claude's sidebar Dismiss button closes a notice, not a dialog: only a
    /// close control inside a dialog is an obstruction.
    #[test]
    fn a_close_control_outside_a_dialog_is_no_obstruction() {
        let tree = tree(CLAUDE_CODE);
        assert_eq!(found(&tree, &CLAUDE_CHAT.close_dialog).len(), 1);
        assert_eq!(obstruction(ChatApp::Claude, &tree), None);
    }

    /// Claude Code with a turn running (recorded 2026-10-03, Mac-C): Send
    /// gives way to Stop, and Stop carries only a description. Live, its
    /// AXTitle is an empty string, not absent: reading the title first and
    /// falling back only when it is missing (Muster up to 1.0.32) never saw
    /// it, so a running Claude turn was neither shown nor interruptible.
    #[test]
    fn claude_code_running_recording() {
        let tree = tree(CLAUDE_CODE_RUNNING);
        assert_eq!(found(&tree, &CLAUDE.composer).len(), 1);
        assert!(found(&tree, &CLAUDE.send).is_empty());
        let stops = found(&tree, &CLAUDE.stop);
        assert_eq!(stops.len(), 1);
        let stop = &tree.nodes[stops[0]];
        assert_eq!(
            (stop.title.as_str(), stop.description.as_str()),
            ("", "Stop")
        );
        assert!(found(&tree, &CLAUDE.approve).is_empty());
    }

    /// Codex with a turn running (recorded 2026-10-03, Mac-C, ChatGPT app):
    /// Stop instead of a send button, and the empty composer reporting its
    /// placeholder as its value. No queued row.
    #[test]
    fn codex_desktop_running_recording() {
        let tree = tree(CODEX_DESKTOP_RUNNING);
        let composers = found(&tree, &CODEX.composer);
        assert_eq!(composers.len(), 1);
        let value = tree.nodes[composers[0]].value.trim_end_matches('⏎');
        assert!(CODEX.placeholders.contains(&value));
        assert!(found(&tree, &CODEX.send).is_empty());
        assert_eq!(found(&tree, &CODEX.stop).len(), 1);
        assert!(found(&tree, &CODEX.queue.as_ref().unwrap().row).is_empty());
        assert!(found(&tree, &CODEX.approve).is_empty());
    }

    /// Codex's submit controls that `composer_submits` keeps: those in the
    /// composer's branch, never a queued row's.
    fn codex_submits(tree: &AxTree) -> Vec<usize> {
        let queue = CODEX.queue.as_ref().unwrap();
        let nodes: Vec<crate::native_send::ControlNode> = tree
            .nodes
            .iter()
            .map(|n| {
                let composer = CODEX.composer.matches(n);
                crate::native_send::ControlNode {
                    parent: n.parent,
                    composer,
                    submit: !composer && CODEX.send.matches(n),
                    queued_delete: queue.row.matches(n),
                }
            })
            .collect();
        crate::native_send::composer_submits(&nodes)
    }

    /// A running turn with a draft in the composer (recorded 2026-10-03,
    /// Mac-C). Neither app shows Stop then: Claude shows Send, which queues
    /// the prompt into the running turn, and Codex shows Queue in the
    /// composer's branch. So a draft hides a running turn from the Stop
    /// check, and an interrupt finds nothing to press until it is cleared.
    #[test]
    fn running_with_a_draft_recordings() {
        let claude = tree(CLAUDE_CODE_RUNNING_DRAFT);
        assert_eq!(found(&claude, &CLAUDE.composer).len(), 1);
        let sends = found(&claude, &CLAUDE.send);
        assert_eq!(sends.len(), 1);
        assert_eq!(claude.nodes[sends[0]].description, "Send");
        assert!(found(&claude, &CLAUDE.stop).is_empty());

        let codex = tree(CODEX_DESKTOP_RUNNING_DRAFT);
        let composers = found(&codex, &CODEX.composer);
        assert_eq!(composers.len(), 1);
        let draft = codex.nodes[composers[0]].value.trim_end_matches('⏎');
        assert!(!draft.is_empty() && !CODEX.placeholders.contains(&draft));
        assert!(found(&codex, &CODEX.stop).is_empty());
        let submits = codex_submits(&codex);
        assert_eq!(submits.len(), 1);
        assert_eq!(codex.nodes[submits[0]].label(), "Queue");
        // With an empty composer there is nothing to submit.
        assert!(codex_submits(&tree(CODEX_DESKTOP_RUNNING)).is_empty());
    }

    /// What the visible controls say about each recorded turn when the
    /// app's runtime status is missing: Stop means running, an idle page is
    /// idle, and a running turn with a draft, whose Stop is hidden, is
    /// unknown rather than idle. (The dump writes a newline as ⏎.)
    #[test]
    fn observed_states_of_the_recordings() {
        use crate::model::SessionState::{Idle, Running, Unknown};
        let state = |dump: &str, controls: &Controls| {
            let tree = tree(dump);
            let stop = found(&tree, &controls.stop)
                .iter()
                .any(|&i| tree.nodes[i].enabled);
            let draft = found(&tree, &controls.composer)
                .iter()
                .any(|&i| controls.is_draft(&tree.nodes[i].value.replace('⏎', "\n")));
            crate::native_send::observed_state(false, stop, draft)
        };
        assert_eq!(state(CLAUDE_CODE, &CLAUDE), Idle);
        assert_eq!(state(CODEX_DESKTOP, &CODEX), Idle);
        assert_eq!(state(CATDESK_DUMP, &CATDESK), Idle);
        assert_eq!(state(CLAUDE_CODE_RUNNING, &CLAUDE), Running);
        assert_eq!(state(CODEX_DESKTOP_RUNNING, &CODEX), Running);
        assert_eq!(state(CLAUDE_CODE_RUNNING_DRAFT, &CLAUDE), Unknown);
        assert_eq!(state(CODEX_DESKTOP_RUNNING_DRAFT, &CODEX), Unknown);
    }

    /// Claude's Recents list older sessions behind "Show <count> more".
    #[test]
    fn more_sessions_control_is_named_with_a_count() {
        let SessionList::Sidebar {
            more: Some(more), ..
        } = CLAUDE.sessions
        else {
            unreachable!()
        };
        assert!(more.matches("Show 20 more in Recents"));
        assert!(more.matches("Show 5 more in Recents"));
        for name in [
            "Show more",
            "Show  more in Recents",
            "Show twenty more in Recents",
            "Show 20 more in Pinned",
            "Show 20 more in Recents now",
        ] {
            assert!(!more.matches(name), "{name}");
        }
    }

    /// A placeholder echo or blank value is no draft.
    #[test]
    fn drafts_exclude_blanks_and_placeholders() {
        assert!(!CODEX.is_draft("Do anything\n"));
        assert!(!CLAUDE.is_draft("\n"));
        assert!(!CLAUDE.is_draft("  "));
        assert!(CODEX.is_draft("test"));
        assert!(CLAUDE.is_draft("Do anything"));
    }

    /// A Claude Code question card (recorded 2026-10-03, Mac-C: the first
    /// of two questions, the turn waiting on it). Options carry their label
    /// on a text child, "Other" among them; the card has its own free-text
    /// field, which is not the composer, and a Next command.
    #[test]
    fn claude_code_question_card_recording() {
        let tree = tree(CLAUDE_CODE_QUESTION);
        let cards = CLAUDE.questions.as_ref().unwrap();
        let buttons = |labels: &[&str]| -> Vec<usize> {
            (0..tree.nodes.len())
                .filter(|&i| {
                    tree.nodes[i].role == "AXButton" && labels.contains(&tree.nodes[i].label())
                })
                .collect()
        };
        let next = buttons(cards.next);
        assert_eq!(next.len(), 1);
        let card = tree.nodes[next[0]].parent.unwrap();
        let inside = tree.preorder(card);
        let other: Vec<usize> = inside
            .iter()
            .copied()
            .filter(|&i| {
                tree.nodes[i].role == "AXButton"
                    && tree.nodes[i].children.iter().any(|&c| {
                        tree.nodes[c].role == "AXStaticText"
                            && cards.other.contains(&tree.nodes[c].value.as_str())
                    })
            })
            .collect();
        assert_eq!(other.len(), 1);
        let fields: Vec<usize> = inside
            .iter()
            .copied()
            .filter(|&i| tree.nodes[i].role == "AXTextArea")
            .collect();
        assert_eq!(fields.len(), 1);
        assert!(!CLAUDE.composer.matches(&tree.nodes[fields[0]]));
        assert!(
            buttons(cards.submit).is_empty(),
            "the first page has no Submit"
        );
        assert_eq!(found(&tree, &CLAUDE.composer).len(), 1);
        assert_eq!(found(&tree, &CLAUDE.stop).len(), 1);
    }

    /// The ChatGPT app's Chat mode (recorded 2026-10-03, Mac-C), on its
    /// new-chat page and in a conversation: Chat mode is read from the mode
    /// pop-up, which must be opened before an entry can be chosen; the new
    /// page is the new conversation; a conversation has its composer and
    /// Send, its text and its row in the navigation lists, where project
    /// rows and row actions are never conversations.
    #[test]
    fn chatgpt_chat_recordings() {
        let app = ChatApp::ChatGpt;
        let new = tree(CHATGPT_NEW_DUMP);
        let view = mode(app, &new);
        assert_eq!(view.chat, Some(true));
        assert!(view.opener.is_some() && view.switch.is_none());
        assert_eq!(conversation(app, &new).unwrap().key, ChatKey::New);
        assert!(new_chat_control(app, &new).is_some());
        assert_eq!(obstruction(app, &new), None);

        let tree = tree(CHATGPT_CHAT_DUMP);
        assert_eq!(mode(app, &tree).chat, Some(true));
        let chat = conversation(app, &tree).unwrap();
        assert!(matches!(chat.key, ChatKey::Title(_)));
        assert_eq!(chat.composer.len(), 1);
        assert_eq!(chat.send.len(), 1);
        assert!(chat.stop.is_none());
        assert!(!chat.lines.is_empty());
        let rows = sidebar(app, &tree);
        assert!(rows.iter().any(|row| row.key == chat.key));
        let HistoryRows::Lists {
            actions, project, ..
        } = CHATGPT_CHAT.history
        else {
            unreachable!()
        };
        assert!(rows.iter().all(|row| {
            !actions.contains(&row.title.as_str()) && !row.title.starts_with(project)
        }));
    }

    /// The Claude app's Chat mode on its new-chat page (recorded
    /// 2026-10-03, Mac-C): the page URL says Chat; the Mode radios are
    /// "Claude" and "Code, <status>"; recents are title buttons beside
    /// their "More options for <title>" menus, with View all and New.
    #[test]
    fn claude_chat_recording() {
        let app = ChatApp::Claude;
        let tree = tree(CLAUDE_CHAT_DUMP);
        let view = mode(app, &tree);
        assert_eq!(view.chat, Some(true));
        assert_eq!(tree.nodes[view.switch.unwrap()].label(), "Claude");
        let code = mode_control(app, &tree, false).unwrap();
        assert!(tree.nodes[code].label().starts_with("Code, "));
        assert_eq!(conversation(app, &tree).unwrap().key, ChatKey::New);
        assert_eq!(
            tree.nodes[new_chat_control(app, &tree).unwrap()].label(),
            "New"
        );
        assert_eq!(
            tree.nodes[history_opener(app, &tree).unwrap()].label(),
            "View all"
        );
        let rows = sidebar(app, &tree);
        assert_eq!(rows.len(), 19);
        assert!(rows.iter().all(|row| {
            let menu = format!("More options for {}", row.title);
            tree.nodes.iter().any(|n| n.label() == menu)
        }));
    }

    /// A profile never matches across roles or by a partial label.
    #[test]
    fn labels_match_exactly_and_only_on_their_role() {
        let node = |role: &str, description: &str| AxNode {
            role: role.into(),
            description: description.into(),
            ..Default::default()
        };
        assert!(CLAUDE.send.matches(&node("AXButton", "Send")));
        assert!(!CLAUDE.send.matches(&node("AXButton", "Send feedback")));
        assert!(!CLAUDE.send.matches(&node("AXLink", "Send")));
        assert!(!CLAUDE.composer.matches(&node("AXTextField", "Prompt")));
    }

    /// A control's name is its title when it has one, else its
    /// description.
    #[test]
    fn a_name_is_the_title_else_the_description() {
        let button = |title: &str, description: &str| AxNode {
            role: "AXButton".into(),
            title: title.into(),
            description: description.into(),
            ..Default::default()
        };
        assert!(CODEX.stop.matches(&button("", "Stop")));
        assert!(CODEX.stop.matches(&button("Stop", "")));
        assert!(!CODEX.stop.matches(&button("Pin chat", "Stop")));
        assert!(CLAUDE.approve.matches(&button("Allow once", "")));
        assert!(!CLAUDE.approve.matches(&button("Allow for this chat", "")));
        assert!(CHATGPT_CHAT.leave_settings.is_some() && CLAUDE_CHAT.leave_settings.is_none());
    }

    #[test]
    fn codex_approval_scope_excludes_observed_review_pane_and_sidebar() {
        let chain = |roles: &[&str]| roles.iter().map(|s| (*s).to_owned()).collect::<Vec<_>>();
        // Mac-C's real Computer Use card is beneath the main landmark.
        let card = chain(&["", "", "AXLandmarkMain", ""]);
        assert!(CODEX.approval_scope.admits(&card, true));
        // The adjacent code review Reject is a genuine enabled button too.
        assert!(CODEX.approve.matches(&AxNode {
            role: "AXButton".into(),
            title: "Reject".into(),
            ..Default::default()
        }));
        for chain in [
            chain(&[
                "",
                "AXTabPanel",
                "AXLandmarkComplementary",
                "AXLandmarkMain",
            ]),
            chain(&["", "AXLandmarkComplementary", "AXLandmarkMain"]),
            chain(&["", "AXLandmarkNavigation", "AXLandmarkMain"]),
            chain(&["", "AXLandmarkComplementary"]),
            chain(&["", ""]),
        ] {
            assert!(!CODEX.approval_scope.admits(&chain, true), "{chain:?}");
        }
        // A detached or truncated ancestry must never inherit a stale scope.
        assert!(!CODEX.approval_scope.admits(&card, false));
        assert!(CLAUDE.approval_scope.admits(&[], true));
        assert!(CATDESK.approval_scope.admits(&[], true));
        assert!(!CLAUDE.approval_scope.admits(&[], false));
    }

    #[test]
    fn codex_approval_card_keeps_exact_conversation_and_persistent_labels() {
        let node = |role: &str, title: &str| AxNode {
            role: role.into(),
            title: title.into(),
            ..Default::default()
        };
        for label in [
            "Allow once",
            "Always allow",
            "Allow this conversation",
            "Deny",
        ] {
            assert!(CODEX.approve.matches(&node("AXButton", label)));
            assert!(!CODEX.approve.matches(&node("AXStaticText", label)));
            assert!(
                !CODEX
                    .approve
                    .matches(&node("AXButton", &format!("{label} all apps")))
            );
        }
        assert!(
            !CLAUDE
                .approve
                .matches(&node("AXButton", "Allow this conversation"))
        );
        assert!(
            !CODEX
                .approve
                .matches(&node("AXButton", "Always allow sending to all chats"))
        );
    }

    /// CatDesk's controls are the app's own zh-CN and en strings; its
    /// sending state and other Stop-like buttons are not them.
    #[test]
    fn catdesk_labels_are_the_apps_own_strings() {
        let node = |role: &str, description: &str| AxNode {
            role: role.into(),
            description: description.into(),
            ..Default::default()
        };
        assert!(CATDESK.composer.matches(&node("AXTextArea", "Chat input")));
        assert!(!CATDESK.composer.matches(&node("AXTextArea", "Editor")));
        assert!(
            CATDESK
                .send
                .matches(&node("AXButton", "请输入内容后再发送"))
        );
        assert!(!CATDESK.send.matches(&node("AXButton", "发送中...")));
        assert!(!CATDESK.send.matches(&node("AXButton", "停止")));
        assert!(CATDESK.stop.matches(&node("AXButton", "停止")));
        assert!(!CATDESK.stop.matches(&node("AXButton", "停止录音")));
    }

    /// The compiled profiles agree with the app registry: every registered
    /// app is profiled once, its Chat mode is the registry's, and what it
    /// declares matches the tables it carries.
    #[test]
    fn profiles_cover_the_registry() {
        let mut mine: Vec<&str> = APPS.iter().map(|a| a.provider()).collect();
        let mut registry: Vec<_> = muster_core::apps::APPS.iter().map(|a| a.provider).collect();
        mine.sort_unstable();
        registry.sort_unstable();
        assert_eq!(mine, registry);
        for a in APPS {
            assert!(std::ptr::eq(app(a.provider()).unwrap(), a));
            assert_eq!(
                a.chat.map(|c| c.provider),
                a.app.chat.as_ref().map(|c| c.provider)
            );
            assert_eq!(a.capabilities.chat, a.chat.is_some());
            assert_eq!(a.capabilities.questions, a.controls.questions.is_some());
            assert_eq!(a.capabilities.queue, a.controls.queue.is_some());
            assert!(!a.executable.is_empty() && !a.lock.is_empty());
        }
        assert_eq!(chat_app("chatgpt").map(App::provider), Some("codex"));
        assert_eq!(chat_app("claude").map(App::provider), Some("claude"));
        assert!(chat_app("catdesk").is_none() && app("chatgpt").is_none());
        assert_eq!(capabilities("nobody"), None);
    }

    /// The strategies each app was accepted with on devices.
    #[test]
    fn strategies_are_the_accepted_ones() {
        assert_eq!(CLAUDE_APP.session_key, SessionKey::Url);
        assert_eq!(CODEX_APP.session_key, SessionKey::Title);
        assert_eq!(CATDESK_APP.session_key, SessionKey::Runtime);
        assert_eq!(CATDESK_APP.gui, Gui::CommandSocket);
        assert_eq!(CATDESK_APP.source, Source::Sqlite);
        assert_eq!(CATDESK_APP.controls.draft, Draft::Selection);
        assert!(CLAUDE_APP.source == Source::Jsonl && CODEX_APP.source == Source::Jsonl);
        assert!(!CATDESK_APP.capabilities.queue && !CATDESK_APP.capabilities.questions);
        assert!(CODEX_APP.capabilities.queue && CLAUDE_APP.capabilities.questions);
        assert!(!CATDESK_APP.capabilities.model_options && CLAUDE_APP.capabilities.model_options);
    }

    /// A record grammar comes with every JSONL source and with no store;
    /// its matcher reads dotted paths and the three conditions.
    #[test]
    fn record_grammars_follow_the_source() {
        assert!(CLAUDE_APP.records.is_some() && CODEX_APP.records.is_some());
        assert!(CATDESK_APP.records.is_none());
        let codex = CODEX_APP.records.unwrap();
        let claude = CLAUDE_APP.records.unwrap();
        let ended = serde_json::json!({"type":"event_msg","payload":{"type":"turn_aborted"}});
        assert!(codex.turn.ended.matches(&ended));
        assert!(!claude.turn.ended.matches(&ended));
        let reply = serde_json::json!({"type":"assistant","message":{"stop_reason":"end_turn"}});
        assert!(claude.turn.ended.matches(&reply));
        let side = serde_json::json!({"type":"user","isSidechain":true});
        assert!(claude.skip.iter().any(|m| m.matches(&side)));
        assert!(!claude.skip.iter().any(|m| m.matches(&reply)));
        assert!(codex.skip.is_empty());
        assert_eq!(field(&ended, "payload.type"), "turn_aborted");
        assert!(field(&ended, "payload.missing.deeper").is_null());
        assert_eq!(codex.home.env, "CODEX_HOME");
        assert_eq!(claude.home.dir, ".claude");
    }

    /// Cloud sessions follow the CLI logins: Claude Code's OAuth token (file
    /// or Keychain) with its recorded expiry, Codex's ChatGPT token with its
    /// account id; only Claude's sessions take a prompt.
    #[test]
    fn cloud_sessions_follow_the_cli_logins() {
        let claude = CLAUDE_APP.cloud.expect("claude cloud");
        assert_eq!(claude.api, CloudApi::ClaudeCode);
        assert_eq!(claude.base_url, "https://api.anthropic.com");
        assert_eq!(claude.credentials.file, ".credentials.json");
        assert!(claude.credentials.expires_at.is_some() && claude.credentials.keychain.is_some());
        assert!(claude.credentials.account.is_none());
        assert!(claude.capabilities.send);
        let codex = CODEX_APP.cloud.expect("codex cloud");
        assert_eq!(codex.api, CloudApi::CodexTasks);
        assert_eq!(codex.base_url, "https://chatgpt.com/backend-api");
        assert_eq!(codex.credentials.file, "auth.json");
        assert!(codex.credentials.account.is_some() && codex.credentials.keychain.is_none());
        assert!(!codex.capabilities.send);
        assert!(CATDESK_APP.cloud.is_none());
        assert_eq!(cloud("claude").map(|c| c.label), Some("Claude Cloud"));
        assert!(cloud("catdesk").is_none() && cloud("chatgpt").is_none());
        for app in APPS.iter().filter_map(|a| a.cloud) {
            assert!(app.poll_secs >= 5 && app.base_url.starts_with("https://"));
        }
    }

    /// Desktop locks are keyed by these names across daemon releases.
    #[test]
    fn lock_names_are_stable() {
        assert_eq!(CLAUDE_APP.lock, "Claude");
        assert_eq!(CODEX_APP.lock, "ChatGPT");
        assert_eq!(CATDESK_APP.lock, "CatDesk");
    }
}
