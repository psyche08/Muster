//! Compiles the desktop app profiles (`profiles/*.toml`, ARCHITECTURE
//! §3.6.4) into the Rust statics of `muster_agent::profile`. The TOML files
//! are the one source: the signed binary carries the compiled data and never
//! reads a profile from disk at runtime. This script checks structure (known
//! tables and fields, known strategies, resolvable vocabulary); the fixture
//! tests in `profile.rs` prove that the compiled selectors find the real
//! apps' controls. The schema is documented in `docs/APP_PROFILES.md`.

use std::collections::BTreeMap;
use std::fmt::Write as _;
use std::path::{Path, PathBuf};

use serde::Deserialize;

/// The apps, in the order `profile::APPS` lists them: the order the daemon
/// scans them in.
const APPS: [&str; 4] = ["claude", "codex", "catdesk", "deepseek"];

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Vocabulary {
    labels: BTreeMap<String, Vec<String>>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct AppFile {
    app: AppSection,
    capabilities: CapabilitiesSection,
    source: Option<SourceSection>,
    /// Absent for an app whose controls are not recorded: its sessions are
    /// viewed, never driven through Accessibility.
    controls: Option<ControlsSection>,
    /// The app's sessions are driven through Muster's own plugin inside it.
    plugin: Option<PluginSection>,
    chat: Option<ChatSection>,
    cloud: Option<CloudSection>,
}

/// `[plugin]`: Muster's plugin inside the app serves its sessions over a
/// local socket (DeepSeek Harness, ADR 0074).
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct PluginSection {
    socket: String,
    protocol: u32,
    questions: bool,
}

/// `[cloud]`: the app's cloud sessions, read and written through the
/// provider's own API with the credentials its CLI keeps.
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct CloudSection {
    api: String,
    base_url: String,
    label: String,
    poll_secs: u64,
    credentials: CloudCredentialsSpec,
    capabilities: CloudCapabilitiesSpec,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct CloudCredentialsSpec {
    file: String,
    token: String,
    expires_at: Option<String>,
    account: Option<String>,
    keychain: Option<String>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct CloudCapabilitiesSpec {
    send: bool,
}

/// A record condition: `path = "value"`, `path = ["a", "b"]`,
/// `path = true` or `path = { not = … }`.
type MatchSpec = BTreeMap<String, toml::Value>;

/// One path, or several of which the first present wins.
#[derive(Deserialize)]
#[serde(untagged)]
enum Paths {
    One(String),
    Many(Vec<String>),
}

impl Paths {
    fn list(&self) -> Vec<String> {
        match self {
            Paths::One(path) => vec![path.clone()],
            Paths::Many(paths) => paths.clone(),
        }
    }
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct SourceSection {
    home: HomeSpec,
    timestamp: String,
    cwd: Vec<String>,
    #[serde(default)]
    skip: Vec<MatchSpec>,
    bootstrap_until: Option<MatchSpec>,
    summary_before: Option<MatchSpec>,
    message: MessageSpec,
    tool_call: ToolCallSpec,
    tool_result: ToolResultSpec,
    turn: TurnSpec,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct HomeSpec {
    env: String,
    dir: String,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct MessageSpec {
    when: MatchSpec,
    envelope: Paths,
    role: String,
    role_fallback: Option<String>,
    other_roles: String,
    content: String,
    text_blocks: Vec<String>,
    #[serde(default)]
    media_blocks: Vec<String>,
    block_ids: String,
    phase: Option<String>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct ToolCallSpec {
    record: Option<MatchSpec>,
    block: Option<String>,
    name: String,
    id: String,
    arguments: Vec<String>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct ToolResultSpec {
    record: Option<MatchSpec>,
    block: Option<String>,
    id: String,
    output: String,
    error: Option<String>,
    #[serde(default)]
    interaction_tools: Vec<String>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct TurnSpec {
    started: Option<MatchSpec>,
    ended: MatchSpec,
    #[serde(default)]
    ended_also: Vec<MatchSpec>,
    #[serde(default)]
    user_text_starts: bool,
    #[serde(default)]
    mark_final: bool,
    #[serde(default)]
    activity_on_messages: bool,
    final_phase: Option<String>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct AppSection {
    provider: String,
    doc: Option<String>,
    executable: String,
    gui: Option<String>,
    session_key: Option<String>,
    source: String,
    lock: String,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct CapabilitiesSection {
    send: bool,
    respond: bool,
    interrupt: bool,
    files: bool,
    #[serde(default)]
    model_options: bool,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct ControlsSection {
    draft: String,
    #[serde(default)]
    placeholders: Vec<String>,
    composer: SelectorSpec,
    send: SelectorSpec,
    stop: SelectorSpec,
    approve: SelectorSpec,
    #[serde(default)]
    approval_scope: Option<String>,
    #[serde(default)]
    composer_scope: Option<String>,
    sessions: SessionsSpec,
    queue: Option<QueueSpec>,
    questions: Option<QuestionsSpec>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct SelectorSpec {
    roles: Vec<String>,
    attrs: Vec<String>,
    labels: Labels,
}

/// A list of labels, or `"$name"` for a list in `vocabulary.toml`.
#[derive(Deserialize)]
#[serde(untagged)]
enum Labels {
    List(Vec<String>),
    Named(String),
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct SessionsSpec {
    kind: String,
    landmarks: Option<bool>,
    standalone_title: Option<bool>,
    more: Option<MoreSpec>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct MoreSpec {
    prefix: String,
    suffix: String,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct QueueSpec {
    row: SelectorSpec,
    send_now: SelectorSpec,
    delete: SelectorSpec,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct QuestionsSpec {
    open: Option<Labels>,
    submit: Labels,
    #[serde(default)]
    submit_via_focused_space: bool,
    next: Labels,
    back: Labels,
    other: Labels,
    paged: bool,
    refresh_renderer: bool,
    button_choices: bool,
    immediate_suggestions: bool,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct ChatSection {
    provider: String,
    doc: Option<String>,
    area: AreaSpec,
    send: SelectorSpec,
    stop: SelectorSpec,
    new_chat: SelectorSpec,
    close_dialog: SelectorSpec,
    leave_settings: Option<SelectorSpec>,
    settings_navigation: Option<SelectorSpec>,
    settings_home: Option<SelectorSpec>,
    sidebar_new: Option<SelectorSpec>,
    url: UrlSpec,
    modes: ModesSpec,
    history: HistorySpec,
}

/// `"main"`, or `{ url = "https://…" }`.
#[derive(Deserialize)]
#[serde(untagged)]
enum AreaSpec {
    Main(String),
    Url { url: String },
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct UrlSpec {
    segment: String,
    uuid: bool,
    new: Option<String>,
    only: bool,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct ModesSpec {
    chat: Vec<String>,
    other: Vec<String>,
    switch: SwitchSpec,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct SwitchSpec {
    kind: String,
    group: Option<String>,
    menu: Option<String>,
    other_path: Option<String>,
    settings: Option<String>,
    prefix: Option<String>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct HistorySpec {
    kind: String,
    options: Option<String>,
    pages: Option<Vec<String>>,
    actions: Option<Vec<String>>,
    markers: Option<Vec<String>>,
    project: Option<String>,
}

/// Where the generator is: which file and table a complaint is about.
struct At(String);

impl At {
    fn fail(&self, message: impl std::fmt::Display) -> ! {
        panic!("profile {}: {message}", self.0)
    }
    fn in_table(&self, table: &str) -> At {
        At(format!("{}: [{table}]", self.0))
    }
}

fn read<T: for<'de> Deserialize<'de>>(path: &Path) -> T {
    let text =
        std::fs::read_to_string(path).unwrap_or_else(|e| panic!("read {}: {e}", path.display()));
    toml::from_str(&text).unwrap_or_else(|e| panic!("parse {}: {e}", path.display()))
}

/// A Rust string literal.
fn lit(s: &str) -> String {
    format!("{s:?}")
}

fn list(items: &[String]) -> String {
    let items: Vec<String> = items.iter().map(|s| lit(s)).collect();
    format!("&[{}]", items.join(", "))
}

/// The constant name of a vocabulary list or an app (`stop` → `STOP`,
/// `catdesk` → `CATDESK`).
fn ident(at: &At, name: &str) -> String {
    let ok = !name.is_empty()
        && name
            .bytes()
            .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'_' || b == b'-')
        && name.as_bytes()[0].is_ascii_lowercase();
    if !ok {
        at.fail(format!("{name:?} is not a lower-case identifier"));
    }
    name.to_ascii_uppercase().replace('-', "_")
}

struct Generator<'a> {
    vocabulary: &'a Vocabulary,
    out: String,
}

impl Generator<'_> {
    fn labels(&self, at: &At, labels: &Labels) -> String {
        match labels {
            Labels::List(items) if items.is_empty() => at.fail("labels must not be empty"),
            Labels::List(items) => list(items),
            Labels::Named(name) => match name.strip_prefix('$') {
                Some(key) if self.vocabulary.labels.contains_key(key) => {
                    format!("vocabulary::{}", ident(at, key))
                }
                Some(key) => at.fail(format!("no vocabulary list named {key:?}")),
                None => at.fail(format!(
                    "labels must be a list or \"$name\" naming a vocabulary list, not {name:?}"
                )),
            },
        }
    }

    fn selector(&self, at: &At, spec: &SelectorSpec) -> String {
        if spec.roles.is_empty() || spec.attrs.is_empty() {
            at.fail("a selector needs roles and attrs");
        }
        for role in &spec.roles {
            if !role.starts_with("AX") {
                at.fail(format!("role {role:?} is not an Accessibility role"));
            }
        }
        let attrs: Vec<&str> = spec
            .attrs
            .iter()
            .map(|attr| match attr.as_str() {
                "title" => "Attr::Title",
                "description" => "Attr::Description",
                "placeholder" => "Attr::Placeholder",
                "name" => "Attr::Name",
                other => at.fail(format!(
                    "attr {other:?}: expected title, description, placeholder or name"
                )),
            })
            .collect();
        format!(
            "Selector {{ roles: {}, attrs: &[{}], labels: {} }}",
            list(&spec.roles),
            attrs.join(", "),
            self.labels(at, &spec.labels)
        )
    }

    /// A `Match` literal.
    fn matcher(&self, at: &At, spec: &MatchSpec) -> String {
        let fields: Vec<String> = spec
            .iter()
            .map(|(path, value)| {
                if path.is_empty() {
                    at.fail("a match field needs a path");
                }
                let cond = match value {
                    toml::Value::String(s) => format!("Cond::Is({})", lit(s)),
                    toml::Value::Array(items) => {
                        let strings: Vec<String> = items
                            .iter()
                            .map(|v| match v {
                                toml::Value::String(s) => s.clone(),
                                _ => at.fail(format!("match {path}: a list holds strings only")),
                            })
                            .collect();
                        if strings.is_empty() {
                            at.fail(format!("match {path}: an empty list matches nothing"));
                        }
                        format!("Cond::In({})", list(&strings))
                    }
                    toml::Value::Boolean(true) => "Cond::True".to_string(),
                    toml::Value::Table(table) if table.len() == 1 && table.contains_key("not") => {
                        let strings: Vec<String> = match &table["not"] {
                            toml::Value::String(s) => vec![s.clone()],
                            toml::Value::Array(items) => items
                                .iter()
                                .map(|v| match v {
                                    toml::Value::String(s) => s.clone(),
                                    _ => at.fail(format!("match {path}: not holds strings only")),
                                })
                                .collect(),
                            _ => at.fail(format!("match {path}: not takes a string or a list")),
                        };
                        if strings.is_empty() {
                            at.fail(format!("match {path}: an empty not matches everything"));
                        }
                        format!("Cond::Not({})", list(&strings))
                    }
                    _ => at.fail(format!(
                        "match {path}: expected a string, a list of strings, true or {{ not = … }}"
                    )),
                };
                format!("Field {{ path: {}, cond: {cond} }}", lit(path))
            })
            .collect();
        format!("Match(&[{}])", fields.join(", "))
    }

    fn option_match(&self, at: &At, spec: &Option<MatchSpec>) -> String {
        match spec {
            Some(m) => format!("Some({})", self.matcher(at, m)),
            None => "None".into(),
        }
    }

    fn location(&self, at: &At, record: &Option<MatchSpec>, block: &Option<String>) -> String {
        match (record, block) {
            (Some(m), None) => format!("Where::Record({})", self.matcher(at, m)),
            (None, Some(t)) if !t.is_empty() => format!("Where::Block({})", lit(t)),
            _ => at.fail("give either record = { … } or block = \"type\""),
        }
    }

    fn records(&mut self, at: &At, name: &str, spec: &SourceSection) {
        let at = at.in_table("source");
        if spec.timestamp.is_empty() || spec.cwd.is_empty() {
            at.fail("timestamp and cwd are required");
        }
        let m = &spec.message;
        let envelope = m.envelope.list();
        if envelope.is_empty()
            || envelope.iter().any(String::is_empty)
            || m.role.is_empty()
            || m.content.is_empty()
        {
            at.fail("message.envelope, message.role and message.content are required");
        }
        if m.text_blocks.is_empty() {
            at.fail("message.text_blocks must name at least one block type");
        }
        let other_roles = match m.other_roles.as_str() {
            "skip" => "OtherRoles::Skip",
            "assistant" => "OtherRoles::Assistant",
            other => at.fail(format!(
                "message.other_roles {other:?}: expected skip or assistant"
            )),
        };
        let block_ids = match m.block_ids.as_str() {
            "text" => "BlockIds::Text",
            "all" => "BlockIds::All",
            other => at.fail(format!("message.block_ids {other:?}: expected text or all")),
        };
        let string_option = |s: &Option<String>| match s {
            Some(s) => format!("Some({})", lit(s)),
            None => "None".into(),
        };
        let skip: Vec<String> = spec.skip.iter().map(|m| self.matcher(&at, m)).collect();
        if spec.tool_call.arguments.is_empty() {
            at.fail("tool_call.arguments must name at least one field");
        }
        let _ = write!(
            self.out,
            "/// The record grammar of the {name} transcripts.\npub static {name}_RECORDS: Records = Records {{\n    home: Home {{ env: {}, dir: {} }},\n    timestamp: {},\n    cwd: {},\n    skip: &[{}],\n    bootstrap_until: {},\n    summary_before: {},\n    message: MessageRule {{\n        when: {},\n        envelope: {},\n        role: {},\n        role_fallback: {},\n        other_roles: {other_roles},\n        content: {},\n        text_blocks: {},\n        media_blocks: {},\n        block_ids: {block_ids},\n        phase: {},\n    }},\n    tool_call: ToolCallRule {{ at: {}, name: {}, id: {}, arguments: {} }},\n    tool_result: ToolResultRule {{ at: {}, id: {}, output: {}, error: {}, interaction_tools: {} }},\n    turn: TurnRule {{\n        started: {},\n        ended: {},\n        ended_also: &[{}],\n        user_text_starts: {},\n        mark_final: {},\n        activity_on_messages: {},\n        final_phase: {},\n    }},\n}};\n\n",
            lit(&spec.home.env),
            lit(&spec.home.dir),
            lit(&spec.timestamp),
            list(&spec.cwd),
            skip.join(", "),
            self.option_match(&at, &spec.bootstrap_until),
            self.option_match(&at, &spec.summary_before),
            self.matcher(&at, &m.when),
            list(&envelope),
            lit(&m.role),
            string_option(&m.role_fallback),
            lit(&m.content),
            list(&m.text_blocks),
            list(&m.media_blocks),
            string_option(&m.phase),
            self.location(&at, &spec.tool_call.record, &spec.tool_call.block),
            lit(&spec.tool_call.name),
            lit(&spec.tool_call.id),
            list(&spec.tool_call.arguments),
            self.location(&at, &spec.tool_result.record, &spec.tool_result.block),
            lit(&spec.tool_result.id),
            lit(&spec.tool_result.output),
            string_option(&spec.tool_result.error),
            list(&spec.tool_result.interaction_tools),
            self.option_match(&at, &spec.turn.started),
            self.matcher(&at, &spec.turn.ended),
            spec.turn
                .ended_also
                .iter()
                .map(|m| self.matcher(&at, m))
                .collect::<Vec<_>>()
                .join(", "),
            spec.turn.user_text_starts,
            spec.turn.mark_final,
            spec.turn.activity_on_messages,
            string_option(&spec.turn.final_phase),
        );
    }

    fn cloud(&mut self, at: &At, name: &str, spec: &CloudSection) {
        let at = at.in_table("cloud");
        let api = match spec.api.as_str() {
            "claude-code" => "CloudApi::ClaudeCode",
            "codex-tasks" => "CloudApi::CodexTasks",
            other => at.fail(format!(
                "api {other:?}: expected claude-code or codex-tasks"
            )),
        };
        if !spec.base_url.starts_with("https://") || spec.base_url.ends_with('/') {
            at.fail("base_url must be an https URL without a trailing slash");
        }
        if spec.label.is_empty() {
            at.fail("label is required");
        }
        if spec.poll_secs < 5 {
            at.fail("poll_secs must be at least 5");
        }
        let c = &spec.credentials;
        if c.file.is_empty() || c.token.is_empty() {
            at.in_table("credentials")
                .fail("file and token are required");
        }
        if spec.api == "codex-tasks" && c.account.is_none() {
            at.in_table("credentials")
                .fail("codex-tasks needs account (the ChatGPT account id)");
        }
        let string_option = |s: &Option<String>| match s {
            Some(s) if !s.is_empty() => format!("Some({})", lit(s)),
            Some(_) => at.fail("an optional credentials field must not be empty"),
            None => "None".into(),
        };
        let _ = write!(
            self.out,
            "/// The {name} cloud sessions.
pub static {name}_CLOUD: Cloud = Cloud {{
    api: {api},
    base_url: {},
    label: {},
    poll_secs: {},
    credentials: CloudCredentials {{
        file: {},
        token: {},
        expires_at: {},
        account: {},
        keychain: {},
    }},
    capabilities: CloudCapabilities {{ send: {} }},
}};

",
            lit(&spec.base_url),
            lit(&spec.label),
            spec.poll_secs,
            lit(&c.file),
            lit(&c.token),
            string_option(&c.expires_at),
            string_option(&c.account),
            string_option(&c.keychain),
            spec.capabilities.send,
        );
    }

    fn vocabulary(&mut self, at: &At) {
        let out = &mut self.out;
        out.push_str("/// Label lists shared by several profiles (`profiles/vocabulary.toml`).\n");
        out.push_str("pub mod vocabulary {\n");
        for (name, labels) in &self.vocabulary.labels {
            if labels.is_empty() {
                at.fail(format!("vocabulary list {name:?} is empty"));
            }
            let _ = writeln!(
                out,
                "    pub const {}: &[&str] = {};",
                ident(at, name),
                list(labels)
            );
        }
        out.push_str("}\n\n");
    }

    fn controls(&mut self, at: &At, name: &str, doc: &str, spec: &ControlsSection) {
        let at = at.in_table("controls");
        let draft = match spec.draft.as_str() {
            "value" => "Draft::Value",
            "selection" => "Draft::Selection",
            other => at.fail(format!("draft {other:?}: expected value or selection")),
        };
        let sessions = match spec.sessions.kind.as_str() {
            "sidebar" => {
                let more = match &spec.sessions.more {
                    Some(more) => format!(
                        "Some(More {{ prefix: {}, suffix: {} }})",
                        lit(&more.prefix),
                        lit(&more.suffix)
                    ),
                    None => "None".into(),
                };
                format!(
                    "SessionList::Sidebar {{ landmarks: {}, standalone_title: {}, more: {more} }}",
                    spec.sessions
                        .landmarks
                        .unwrap_or_else(|| at.fail("sessions.landmarks is required for a sidebar")),
                    spec.sessions.standalone_title.unwrap_or_else(
                        || at.fail("sessions.standalone_title is required for a sidebar")
                    ),
                )
            }
            "page-rows" => {
                if spec.sessions.landmarks.is_some()
                    || spec.sessions.standalone_title.is_some()
                    || spec.sessions.more.is_some()
                {
                    at.fail("page-rows sessions take no landmarks, standalone_title or more");
                }
                "SessionList::PageRows".into()
            }
            other => at.fail(format!(
                "sessions.kind {other:?}: expected sidebar or page-rows"
            )),
        };
        let queue = match &spec.queue {
            Some(queue) => format!(
                "Some(Queue {{ row: {}, send_now: {}, delete: {} }})",
                self.selector(&at, &queue.row),
                self.selector(&at, &queue.send_now),
                self.selector(&at, &queue.delete)
            ),
            None => "None".into(),
        };
        let questions = match &spec.questions {
            Some(q) => format!(
                "Some(Questions {{ open: {}, submit: {}, submit_via_focused_space: {}, next: {}, back: {}, other: {}, paged: {}, refresh_renderer: {}, button_choices: {}, immediate_suggestions: {} }})",
                q.open
                    .as_ref()
                    .map_or_else(|| "&[]".into(), |labels| self.labels(&at, labels)),
                self.labels(&at, &q.submit),
                q.submit_via_focused_space,
                self.labels(&at, &q.next),
                self.labels(&at, &q.back),
                self.labels(&at, &q.other),
                q.paged,
                q.refresh_renderer,
                q.button_choices,
                q.immediate_suggestions
            ),
            None => "None".into(),
        };
        let composer = self.selector(&at, &spec.composer);
        let send = self.selector(&at, &spec.send);
        let stop = self.selector(&at, &spec.stop);
        let approve = self.selector(&at, &spec.approve);
        let composer_scope = match spec.composer_scope.as_deref() {
            None | Some("web-area") => "crate::native_scan::ComposerScope::WebArea",
            Some("main-conversation") => "crate::native_scan::ComposerScope::MainConversation",
            Some(other) => at.fail(format!("invalid composer_scope {other:?}")),
        };
        let approval_scope = match spec.approval_scope.as_deref() {
            None | Some("web-area") => "ApprovalScope::WebArea",
            Some("main-conversation") => "ApprovalScope::MainConversation",
            Some(other) => at.fail(format!("invalid approval_scope {other:?}")),
        };
        let _ = write!(
            self.out,
            "/// {doc}\npub static {name}: Controls = Controls {{\n    composer: {composer},\n    composer_scope: {composer_scope},\n    placeholders: {},\n    draft: {draft},\n    send: {send},\n    stop: {stop},\n    approve: {approve},\n    approval_scope: {approval_scope},\n    sessions: {sessions},\n    queue: {queue},\n    questions: {questions},\n}};\n\n",
            list(&spec.placeholders),
        );
    }

    fn chat(&mut self, at: &At, name: &str, spec: &ChatSection) {
        let at = at.in_table("chat");
        let area = match &spec.area {
            AreaSpec::Main(kind) if kind == "main" => "ChatArea::Main".to_string(),
            AreaSpec::Main(other) => at.fail(format!(
                "area {other:?}: expected \"main\" or {{ url = … }}"
            )),
            AreaSpec::Url { url } => format!("ChatArea::Url({})", lit(url)),
        };
        let option = |s: &Option<SelectorSpec>| match s {
            Some(spec) => format!("Some({})", self.selector(&at, spec)),
            None => "None".into(),
        };
        let string_option = |s: &Option<String>| match s {
            Some(s) => format!("Some({})", lit(s)),
            None => "None".into(),
        };
        let send = self.selector(&at, &spec.send);
        let stop = self.selector(&at, &spec.stop);
        let new_chat = self.selector(&at, &spec.new_chat);
        let close_dialog = self.selector(&at, &spec.close_dialog);
        let leave_settings = option(&spec.leave_settings);
        if spec.settings_navigation.is_some() != spec.settings_home.is_some() {
            at.fail("settings_navigation and settings_home must be declared together");
        }
        let settings_navigation = option(&spec.settings_navigation);
        let settings_home = option(&spec.settings_home);
        let sidebar_new = option(&spec.sidebar_new);
        let url_new = string_option(&spec.url.new);
        let switch = match spec.modes.switch.kind.as_str() {
            "radio" => {
                let s = &spec.modes.switch;
                let field = |name: &str, value: &Option<String>| {
                    value.as_deref().map(lit).unwrap_or_else(|| {
                        at.fail(format!(
                            "modes.switch.{name} is required for a radio switch"
                        ))
                    })
                };
                if s.prefix.is_some() {
                    at.fail("a radio switch has no prefix");
                }
                format!(
                    "ModeSwitch::Radio {{ group: {}, menu: {}, other_path: {}, settings: {} }}",
                    field("group", &s.group),
                    field("menu", &s.menu),
                    field("other_path", &s.other_path),
                    field("settings", &s.settings)
                )
            }
            "menu" => {
                let s = &spec.modes.switch;
                if s.group.is_some()
                    || s.menu.is_some()
                    || s.other_path.is_some()
                    || s.settings.is_some()
                {
                    at.fail("a menu switch takes only a prefix");
                }
                format!(
                    "ModeSwitch::Menu {{ prefix: {} }}",
                    s.prefix.as_deref().map(lit).unwrap_or_else(
                        || at.fail("modes.switch.prefix is required for a menu switch")
                    )
                )
            }
            other => at.fail(format!(
                "modes.switch.kind {other:?}: expected radio or menu"
            )),
        };
        let history = match spec.history.kind.as_str() {
            "title-buttons" => {
                let h = &spec.history;
                if h.actions.is_some() || h.markers.is_some() || h.project.is_some() {
                    at.fail("title-buttons history takes options and pages only");
                }
                format!(
                    "HistoryRows::TitleButtons {{ options: {}, pages: {} }}",
                    h.options
                        .as_deref()
                        .map(lit)
                        .unwrap_or_else(|| at.fail("history.options is required")),
                    list(
                        h.pages
                            .as_deref()
                            .unwrap_or_else(|| at.fail("history.pages is required"))
                    )
                )
            }
            "lists" => {
                let h = &spec.history;
                if h.options.is_some() || h.pages.is_some() {
                    at.fail("lists history takes actions, markers and project only");
                }
                format!(
                    "HistoryRows::Lists {{ actions: {}, markers: {}, project: {} }}",
                    list(
                        h.actions
                            .as_deref()
                            .unwrap_or_else(|| at.fail("history.actions is required"))
                    ),
                    list(
                        h.markers
                            .as_deref()
                            .unwrap_or_else(|| at.fail("history.markers is required"))
                    ),
                    h.project
                        .as_deref()
                        .map(lit)
                        .unwrap_or_else(|| at.fail("history.project is required"))
                )
            }
            other => at.fail(format!(
                "history.kind {other:?}: expected title-buttons or lists"
            )),
        };
        if spec.modes.chat.is_empty() || spec.modes.other.is_empty() {
            at.fail("modes.chat and modes.other name each mode at least once");
        }
        let doc = spec
            .doc
            .clone()
            .unwrap_or_else(|| format!("Chat mode of the {} app.", spec.provider));
        let _ = write!(
            self.out,
            "/// {doc}\npub static {name}: ChatControls = ChatControls {{\n    provider: {},\n    send: {},\n    stop: {},\n    new_chat: {},\n    close_dialog: {},\n    leave_settings: {},\n    settings_navigation: {},\n    settings_home: {},\n    sidebar_new: {},\n    area: {area},\n    url: ChatUrl {{ segment: {}, uuid: {}, new: {}, only: {} }},\n    modes: Modes {{ chat: {}, other: {}, switch: {switch} }},\n    history: {history},\n}};\n\n",
            lit(&spec.provider),
            send,
            stop,
            new_chat,
            close_dialog,
            leave_settings,
            settings_navigation,
            settings_home,
            sidebar_new,
            lit(&spec.url.segment),
            spec.url.uuid,
            url_new,
            spec.url.only,
            list(&spec.modes.chat),
            list(&spec.modes.other),
        );
    }

    fn app(&mut self, at: &At, file: &AppFile) -> String {
        let app = &file.app;
        let name = ident(at, &app.provider);
        let doc = app
            .doc
            .clone()
            .unwrap_or_else(|| format!("The {} desktop app.", app.provider));
        if let Some(controls) = &file.controls {
            self.controls(at, &name, &doc, controls);
        }
        let chat = match &file.chat {
            Some(chat) => {
                let chat_name = format!("{}_CHAT", ident(&at.in_table("chat"), &chat.provider));
                self.chat(at, &chat_name, chat);
                format!("Some(&{chat_name})")
            }
            None => "None".into(),
        };
        let root = At(at.0.clone());
        let cloud = match &file.cloud {
            Some(spec) => {
                if file.source.is_none() {
                    root.fail(
                        "a [cloud] table needs a [source] table: the CLI's credentials live under the source's home",
                    );
                }
                self.cloud(&root, &name, spec);
                format!("Some(&{name}_CLOUD)")
            }
            None => "None".into(),
        };
        let caps = &file.capabilities;
        // An app is driven through controls its profile records, or through
        // Muster's plugin inside it; one with neither is viewed and declares
        // nothing that drives it.
        if file.controls.is_some() && file.plugin.is_some() {
            root.fail("an app is driven through [controls] or through [plugin], not both");
        }
        if file.controls.is_none() {
            if app.gui.is_some() || app.session_key.is_some() {
                at.in_table("app").fail(
                    "gui and session_key find windows to drive: they need a [controls] table",
                );
            }
            if caps.model_options || file.chat.is_some() {
                at.in_table("capabilities")
                    .fail("model options and Chat mode need a [controls] table");
            }
            if file.plugin.is_none() && (caps.send || caps.respond || caps.interrupt) {
                at.in_table("capabilities")
                    .fail("send, respond and interrupt need a [controls] or [plugin] table");
            }
        }
        let plugin = match &file.plugin {
            Some(plugin) => {
                if file.source.is_none() {
                    root.fail(
                        "a [plugin] table needs a [source] table: its socket lives under the source's home",
                    );
                }
                if plugin.socket.is_empty()
                    || plugin.socket.starts_with('/')
                    || plugin.socket.contains("..")
                {
                    root.in_table("plugin")
                        .fail("socket is a relative path under the source's home");
                }
                if plugin.protocol == 0 {
                    root.in_table("plugin").fail("protocol starts at 1");
                }
                format!(
                    "Some(&Plugin {{ socket: {}, protocol: {}, questions: {} }})",
                    lit(&plugin.socket),
                    plugin.protocol,
                    plugin.questions
                )
            }
            None => "None".into(),
        };
        let at = at.in_table("app");
        let gui = match (app.gui.as_deref(), &file.controls) {
            (Some("executable"), _) => "Some(Gui::Executable)",
            (Some("command-socket"), _) => "Some(Gui::CommandSocket)",
            (Some(other), _) => at.fail(format!(
                "gui {other:?}: expected executable or command-socket"
            )),
            (None, Some(_)) => at.fail("gui is required with a [controls] table"),
            (None, None) => "None",
        };
        let session_key = match (app.session_key.as_deref(), &file.controls) {
            (Some("url"), _) => "Some(SessionKey::Url)",
            (Some("title"), _) => "Some(SessionKey::Title)",
            (Some("runtime"), _) => "Some(SessionKey::Runtime)",
            (Some(other), _) => at.fail(format!(
                "session_key {other:?}: expected url, title or runtime"
            )),
            (None, Some(_)) => at.fail("session_key is required with a [controls] table"),
            (None, None) => "None",
        };
        let source = match app.source.as_str() {
            "jsonl" => "Source::Jsonl",
            "jsonl-zstd" => "Source::JsonlZstd",
            "sqlite" => "Source::Sqlite",
            other => at.fail(format!(
                "source {other:?}: expected jsonl, jsonl-zstd or sqlite"
            )),
        };
        let records = match (&file.source, app.source.as_str()) {
            (Some(spec), "jsonl" | "jsonl-zstd") => {
                self.records(&root, &name, spec);
                format!("Some(&{name}_RECORDS)")
            }
            (None, "jsonl" | "jsonl-zstd") => at.fail("a JSONL source needs a [source] table"),
            (Some(_), _) => at.fail("only a JSONL source takes a [source] table"),
            (None, _) => "None".into(),
        };
        if app.executable.is_empty() || app.lock.is_empty() {
            at.fail("executable and lock are required");
        }
        let controls = match &file.controls {
            Some(_) => format!("Some(&{name})"),
            None => "None".into(),
        };
        let questions = file
            .controls
            .as_ref()
            .is_some_and(|c| c.questions.is_some())
            || file.plugin.as_ref().is_some_and(|p| p.questions);
        let queue = file.controls.as_ref().is_some_and(|c| c.queue.is_some());
        let app_name = format!("{name}_APP");
        let _ = write!(
            self.out,
            "/// {doc}\npub static {app_name}: App = App {{\n    app: &muster_core::apps::{name},\n    executable: {},\n    gui: {gui},\n    session_key: {session_key},\n    source: {source},\n    records: {records},\n    lock: {},\n    capabilities: Capabilities {{\n        send: {},\n        respond: {},\n        interrupt: {},\n        questions: {},\n        queue: {},\n        files: {},\n        chat: {},\n        model_options: {},\n    }},\n    controls: {controls},\n    plugin: {plugin},\n    chat: {chat},\n    cloud: {cloud},\n}};\n\n",
            lit(&app.executable),
            lit(&app.lock),
            caps.send,
            caps.respond,
            caps.interrupt,
            questions,
            queue,
            caps.files,
            file.chat.is_some(),
            caps.model_options,
        );
        app_name
    }
}

fn main() {
    let dir = PathBuf::from("profiles");
    println!("cargo:rerun-if-changed=build.rs");
    println!("cargo:rerun-if-changed={}", dir.display());
    let vocabulary_path = dir.join("vocabulary.toml");
    println!("cargo:rerun-if-changed={}", vocabulary_path.display());
    let vocabulary: Vocabulary = read(&vocabulary_path);
    let mut generator = Generator {
        vocabulary: &vocabulary,
        out: String::from(
            "// Generated by build.rs from profiles/*.toml (docs/APP_PROFILES.md). Do not edit.\n\n",
        ),
    };
    generator.vocabulary(&At("vocabulary.toml".into()));
    let mut names = Vec::new();
    for provider in APPS {
        let path = dir.join(format!("{provider}.toml"));
        println!("cargo:rerun-if-changed={}", path.display());
        let file: AppFile = read(&path);
        let at = At(format!("{provider}.toml"));
        if file.app.provider != provider {
            at.fail(format!(
                "app.provider is {:?}, the file says {provider:?}",
                file.app.provider
            ));
        }
        names.push(generator.app(&at, &file));
    }
    let refs: Vec<String> = names.iter().map(|n| format!("&{n}")).collect();
    let _ = write!(
        generator.out,
        "/// Every app, in the order the daemon scans them.\npub static APPS: [&App; {}] = [{}];\n",
        names.len(),
        refs.join(", ")
    );
    let out = Path::new(&std::env::var("OUT_DIR").expect("OUT_DIR")).join("profiles.rs");
    std::fs::write(&out, generator.out).unwrap_or_else(|e| panic!("write {}: {e}", out.display()));
}
