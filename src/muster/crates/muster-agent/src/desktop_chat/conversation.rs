//! The conversation on screen: which web area shows it, what names it,
//! and its messages read from the page.

use super::*;
use crate::model::Role;
use crate::profile::ChatArea;

/// The conversation the app shows.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Conversation {
    pub key: ChatKey,
    pub title: String,
    pub web_area: usize,
    pub lines: Vec<ChatLine>,
    /// Prompt fields in the conversation's main region (exactly one is
    /// required for a write).
    pub composer: Vec<usize>,
    /// Enabled-or-not send buttons next to it.
    pub send: Vec<usize>,
    /// An enabled Stop button: a reply is being generated.
    pub stop: Option<usize>,
}

/// A successful press alone is not delivery: the same conversation must append
/// the requested user message after its previously observed history.
pub fn confirms_send(before: &Conversation, after: &Conversation, text: &str) -> bool {
    before.key == after.key
        && after.lines.starts_with(&before.lines)
        && after.lines[before.lines.len()..].iter().any(|line| {
            line.role == Role::User && line.text.split_whitespace().eq(text.split_whitespace())
        })
}

/// Whether a prompt field's value means nothing was typed: blank, or its
/// own label / placeholder echoed back as the value.
pub fn is_empty_draft(field: &AxNode, value: &str) -> bool {
    let value = value.trim();
    value.is_empty()
        || [&field.title, &field.description, &field.placeholder]
            .iter()
            .any(|t| !t.trim().is_empty() && t.trim() == value)
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ChatLine {
    pub role: Role,
    pub text: String,
}

pub(super) const TITLE_SUFFIXES: [&str; 2] = [" - Claude", " | Claude"];
/// Window titles that name the app rather than a conversation.
pub(super) const GENERIC_TITLES: [&str; 6] =
    ["", "Claude", "ChatGPT", "New chat", "New session", "Codex"];
/// Words in a prompt field's label or placeholder. Needed only when the page
/// has more than one text field (the ChatGPT app can show a Markdown editor).
pub(super) const PROMPT_HINTS: [&str; 7] = [
    "anything", "prompt", "message", "reply", "claude", "chatgpt", "help you",
];

/// The page conversations are rendered in. Claude.app wraps the claude.ai
/// page in its own shell page; ChatGPT.app shows one page.
pub(super) fn conversation_area(app: ChatApp, tree: &AxTree) -> Option<usize> {
    match app.controls().area {
        ChatArea::Url(prefix) => url_area(tree, prefix),
        ChatArea::Main => {
            let areas: Vec<usize> = tree
                .preorder(0)
                .into_iter()
                .filter(|&i| tree.nodes[i].role == "AXWebArea")
                .collect();
            // Prefer the page that has a main landmark (the app shell may
            // host more than one web area).
            let with_main: Vec<usize> = areas
                .iter()
                .copied()
                .filter(|&a| main_region(tree, a).is_some())
                .collect();
            match (with_main.len(), areas.len()) {
                (1, _) => Some(with_main[0]),
                (0, 1) => Some(areas[0]),
                _ => None,
            }
        }
    }
}

pub(super) fn url_area(tree: &AxTree, prefix: &str) -> Option<usize> {
    let mut found = tree.preorder(0).into_iter().filter(|&i| {
        let n = &tree.nodes[i];
        n.role == "AXWebArea" && n.url.starts_with(prefix)
    });
    let first = found.next()?;
    found.next().is_none().then_some(first)
}

pub(super) fn main_region(tree: &AxTree, web: usize) -> Option<usize> {
    tree.preorder(web)
        .into_iter()
        .find(|&i| tree.nodes[i].subrole == "AXLandmarkMain")
}

pub(super) fn url_path(url: &str) -> Option<&str> {
    let rest = url.split_once("://")?.1;
    let path = &rest[rest.find('/').unwrap_or(rest.len())..];
    Some(path.split(['?', '#']).next().unwrap_or(path))
}

/// Claude: /chat/<uuid>, /new. ChatGPT: /c/<id> (also behind a `#`).
pub fn key_from_url(app: ChatApp, url: &str) -> Option<ChatKey> {
    let rule = app.controls().url;
    if rule
        .new
        .is_some_and(|new| url_path(url).is_some_and(|p| p == new))
    {
        return Some(ChatKey::New);
    }
    let rest = url.split(rule.segment).nth(1)?;
    let id: String = rest
        .chars()
        .take_while(|c| c.is_ascii_alphanumeric() || *c == '-' || *c == '_')
        .collect();
    let valid = if rule.uuid {
        crate::viewer::is_uuid(&id)
    } else {
        id.len() >= 8
    };
    valid.then(|| ChatKey::Id(id.to_ascii_lowercase()))
}

pub fn conversation(app: ChatApp, tree: &AxTree) -> Option<Conversation> {
    let web = conversation_area(app, tree)?;
    let area = &tree.nodes[web];
    let mut title = area.title.trim().to_string();
    for suffix in TITLE_SUFFIXES {
        if let Some(stripped) = title.strip_suffix(suffix) {
            title = stripped.trim().to_string();
        }
    }
    let region = main_region(tree, web).unwrap_or(web);
    let lines = parse_lines(app, tree, region);
    // Without an id in the page URL, the sidebar link marked current names
    // the conversation the same way the list does.
    let current_link = || {
        let mut current = sidebar(app, tree)
            .into_iter()
            .filter(|e| matches!(e.key, ChatKey::Id(_)) && tree.nodes[e.node].selected);
        let first = current.next()?;
        current.next().is_none().then_some(first.key)
    };
    let key = match key_from_url(app, &area.url) {
        Some(key) => key,
        None if app.controls().url.only => return None,
        None if let Some(key) = current_link() => key,
        None if GENERIC_TITLES.contains(&title.as_str()) => {
            if !lines.is_empty() {
                // A conversation that has not been named yet: not
                // addressable until it is.
                return None;
            }
            ChatKey::New
        }
        None => ChatKey::Title(title.clone()),
    };
    if GENERIC_TITLES.contains(&title.as_str()) {
        title.clear();
    }
    let controls = app.controls();
    let (mut fields, mut send, mut stop) = (Vec::new(), Vec::new(), None);
    for i in tree.preorder(region) {
        let node = &tree.nodes[i];
        match node.role.as_str() {
            "AXTextArea" | "AXTextField" => fields.push(i),
            _ if controls.send.matches(node) => send.push(i),
            _ if controls.stop.matches(node) && node.enabled => stop = Some(i),
            _ => {}
        }
    }
    let composer = if fields.len() <= 1 {
        fields
    } else {
        fields
            .into_iter()
            .filter(|&i| {
                let n = &tree.nodes[i];
                [&n.title, &n.description, &n.placeholder].iter().any(|t| {
                    let t = t.to_lowercase();
                    PROMPT_HINTS.iter().any(|hint| t.contains(hint))
                })
            })
            .collect()
    };
    Some(Conversation {
        key,
        title,
        web_area: web,
        lines,
        composer,
        send,
        stop,
    })
}

pub(super) const USER_MARKERS: [&str; 1] = ["You said:"];
pub(super) const ASSISTANT_MARKERS: [&str; 5] = [
    "ChatGPT said:",
    "Claude said:",
    "Claude responded:",
    "Claude replied:",
    "Assistant said:",
];

/// Page text that belongs to the app, not to a message: the disclaimer
/// under the conversation and screen-reader landmarks before the prompt.
pub(super) const CHROME_TEXT: [&str; 4] = [
    "ChatGPT can make mistakes",
    "Claude can make mistakes",
    "Claude is AI and can make mistakes",
    "Latest response",
];

pub(super) fn chrome(text: &str) -> bool {
    let text = text.trim();
    CHROME_TEXT.iter().any(|c| text.starts_with(c))
}

/// Subtrees that are controls or live status, never message text.
pub(super) fn skipped(node: &AxNode) -> bool {
    matches!(
        node.role.as_str(),
        "AXButton"
            | "AXPopUpButton"
            | "AXMenuButton"
            | "AXTextArea"
            | "AXTextField"
            | "AXSplitter"
            | "AXImage"
            | "AXCheckBox"
            | "AXRadioButton"
            | "AXRadioGroup"
            | "AXToolbar"
            | "AXTabGroup"
            | "AXScrollBar"
            | "AXMenu"
    ) || node.subrole == "AXApplicationStatus"
}

pub(super) fn inline(node: &AxNode) -> bool {
    node.role == "AXLink"
        || matches!(
            node.subrole.as_str(),
            "AXStrongStyleGroup"
                | "AXEmphasisStyleGroup"
                | "AXCodeStyleGroup"
                | "AXDeleteStyleGroup"
                | "AXInsertStyleGroup"
                | "AXSubscriptStyleGroup"
                | "AXSuperscriptStyleGroup"
                | "AXMarkStyleGroup"
        )
}

pub(super) fn is_clock(text: &str) -> bool {
    let t = text.trim();
    let t = t
        .strip_suffix("AM")
        .or_else(|| t.strip_suffix("PM"))
        .unwrap_or(t)
        .trim();
    let mut parts = t.split(':');
    let (Some(h), Some(m), None) = (parts.next(), parts.next(), parts.next()) else {
        return false;
    };
    (1..=2).contains(&h.len())
        && m.len() == 2
        && h.chars().all(|c| c.is_ascii_digit())
        && m.chars().all(|c| c.is_ascii_digit())
}

pub(super) fn marker(label: &str) -> Option<Role> {
    let label = label.trim();
    if USER_MARKERS.iter().any(|m| label.starts_with(m)) {
        Some(Role::User)
    } else if ASSISTANT_MARKERS.iter().any(|m| label.starts_with(m)) {
        Some(Role::Assistant)
    } else {
        None
    }
}

pub(super) struct Lines<'a> {
    tree: &'a AxTree,
    out: Vec<ChatLine>,
    /// The block the last text was appended to, and whether it is a list item.
    block: Option<(usize, bool)>,
}

impl Lines<'_> {
    fn start(&mut self, role: Role) {
        self.out.push(ChatLine {
            role,
            text: String::new(),
        });
        self.block = None;
    }

    fn block_of(&self, i: usize) -> (usize, bool) {
        let tree = self.tree;
        let block = std::iter::once(i)
            .chain(tree.ancestors(i))
            .skip(1)
            .find(|&a| !inline(&tree.nodes[a]))
            .unwrap_or(i);
        let item = tree.nodes[block]
            .parent
            .is_some_and(|p| tree.nodes[p].role == "AXList");
        (block, item)
    }

    fn text(&mut self, i: usize, text: &str) {
        if text.is_empty() || self.out.is_empty() {
            // Before the first message: hints such as "Use the arrow keys".
            return;
        }
        let (block, item) = self.block_of(i);
        let previous = self.block;
        let line = self.out.last_mut().expect("checked above");
        if !line.text.is_empty() && previous.map(|(b, _)| b) != Some(block) {
            let both_items = item && previous.is_some_and(|(_, was_item)| was_item);
            line.text.push_str(if both_items { "\n" } else { "\n\n" });
        }
        line.text.push_str(text);
        self.block = Some((block, item));
    }

    fn visit(&mut self, i: usize) {
        let tree = self.tree;
        let node = &tree.nodes[i];
        if skipped(node) {
            return;
        }
        match node.role.as_str() {
            "AXHeading" => {
                let label = tree.text_of(i);
                if let Some(role) = marker(&label) {
                    self.start(role);
                    return;
                }
                if chrome(&label) {
                    return;
                }
                let text = label.trim().to_string();
                if !text.is_empty() {
                    self.text(i, &format!("**{text}**"));
                    // The heading is its own block.
                    self.block = Some((i, false));
                }
                return;
            }
            "AXStaticText" => {
                let text = tree.text_of(i);
                if !is_clock(&text) && !chrome(&text) {
                    self.text(i, &text);
                }
                return;
            }
            "AXListMarker" => {
                let marker = node.value.trim();
                let marker = if marker == "•" || marker == "◦" || marker == "▪" {
                    "- "
                } else {
                    &node.value
                };
                self.text(i, marker);
                return;
            }
            _ => {}
        }
        // Claude renders each turn as an article; a user turn carries a
        // "You said:" heading, anything after it is the reply.
        let article = node.subrole == "AXDocumentArticle";
        let user_article = article
            && tree.preorder(i).into_iter().any(|j| {
                tree.nodes[j].role == "AXHeading" && marker(&tree.text_of(j)) == Some(Role::User)
            });
        if article && !user_article && self.out.last().is_none_or(|l| l.role != Role::Assistant) {
            self.start(Role::Assistant);
        }
        for &child in &node.children {
            self.visit(child);
        }
        if user_article {
            self.start(Role::Assistant);
        }
    }
}

/// The conversation's messages, oldest first, in the order the page shows
/// them. Empty messages (a reply that has not started) are dropped.
pub fn parse_lines(_app: ChatApp, tree: &AxTree, region: usize) -> Vec<ChatLine> {
    let mut lines = Lines {
        tree,
        out: Vec::new(),
        block: None,
    };
    lines.visit(region);
    lines
        .out
        .into_iter()
        .map(|l| ChatLine {
            role: l.role,
            text: l.text.trim().to_string(),
        })
        .filter(|l| !l.text.is_empty())
        .collect()
}
