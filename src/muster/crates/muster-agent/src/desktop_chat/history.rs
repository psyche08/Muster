//! The conversations the sidebar and the full history page list.

use super::*;
use crate::profile::HistoryRows;

/// One conversation the sidebar lists.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SidebarChat {
    pub node: usize,
    pub key: ChatKey,
    pub title: String,
}

/// Explicit history navigation, never a conversation row with the same title.
pub fn history_opener(app: ChatApp, tree: &AxTree) -> Option<usize> {
    let entries = sidebar(app, tree);
    let candidates: Vec<_> = tree
        .nodes
        .iter()
        .enumerate()
        .filter(|(i, n)| {
            in_sidebar(tree, *i)
                && !entries.iter().any(|e| e.node == *i)
                && matches!(n.role.as_str(), "AXButton" | "AXLink")
                && matches!(
                    n.label(),
                    "View all"
                        | "All chats"
                        | "See all"
                        | "Chats"
                        | "History"
                        | "所有聊天"
                        | "全部会话"
                )
        })
        .map(|(i, _)| i)
        .collect();
    // Claude has a header icon and a footer button, both View all; prefer
    // the visible title button whose role/label does not depend on a tooltip.
    candidates
        .iter()
        .copied()
        .find(|&i| tree.nodes[i].title == "View all")
        .or_else(|| (candidates.len() == 1).then(|| candidates[0]))
}
/// Full Claude history has a distinct URL; transcript links in the main
/// conversation must never be mistaken for history rows.
pub fn history_page(app: ChatApp, tree: &AxTree) -> bool {
    let HistoryRows::TitleButtons { pages, .. } = app.controls().history else {
        return false;
    };
    conversation_area(app, tree)
        .is_some_and(|i| url_path(&tree.nodes[i].url).is_some_and(|p| pages.contains(&p)))
}
pub(super) fn history_scope(app: ChatApp, tree: &AxTree, i: usize) -> bool {
    in_sidebar(tree, i)
        || in_navigation(tree, i)
        || (history_page(app, tree)
            && conversation_area(app, tree)
                .and_then(|w| main_region(tree, w))
                .is_some_and(|m| tree.inside(i, m)))
}
pub(super) fn more_label(label: &str) -> bool {
    matches!(
        label,
        "Load more" | "Show more" | "More conversations" | "Next page" | "加载更多" | "下一页"
    ) || (label.starts_with("Show ") && label.ends_with(" more in Recents"))
}
pub fn history_more(app: ChatApp, tree: &AxTree) -> Option<usize> {
    // Multiple project/recents lists each have their own Show more. Drain
    // them in document order, taking a fresh snapshot after every press.
    tree.nodes
        .iter()
        .enumerate()
        .find(|(i, n)| {
            n.enabled
                && history_scope(app, tree, *i)
                && matches!(n.role.as_str(), "AXButton" | "AXLink")
                && more_label(n.label())
        })
        .map(|(i, _)| i)
}
pub fn history_loading(app: ChatApp, tree: &AxTree) -> bool {
    tree.nodes.iter().enumerate().any(|(i, n)| {
        history_scope(app, tree, i)
            && (n.role == "AXProgressIndicator"
                || (!n.enabled && more_label(n.label()))
                || matches!(
                    n.label(),
                    "Loading…" | "Loading..." | "Loading chats" | "加载中"
                ))
    })
}
/// This UI uses explicit paging: completion requires the history page (Claude)
/// or the ChatGPT navigation lists, no pending page control and no loader.
pub fn history_exhausted(app: ChatApp, tree: &AxTree) -> bool {
    tree.complete
        && !history_loading(app, tree)
        && history_more(app, tree).is_none()
        && (history_page(app, tree)
            || (matches!(app.controls().history, HistoryRows::Lists { .. })
                && mode(app, tree).chat == Some(true)
                && tree
                    .nodes
                    .iter()
                    .enumerate()
                    .any(|(i, n)| n.role == "AXList" && in_navigation(tree, i))))
}
/// Ancestors of list rows are the only eligible scroll surfaces.
pub fn history_scroll_roots(tree: &AxTree, entries: &[SidebarChat]) -> Vec<usize> {
    let mut roots = Vec::new();
    for entry in entries {
        for i in tree.ancestors(entry.node) {
            if tree.nodes[i].role == "AXScrollArea" && !roots.contains(&i) {
                roots.push(i);
            }
        }
    }
    roots
}

pub fn sidebar(app: ChatApp, tree: &AxTree) -> Vec<SidebarChat> {
    history_entries(app, tree, false)
}
pub fn history_entries(app: ChatApp, tree: &AxTree, full: bool) -> Vec<SidebarChat> {
    let Some(web) = conversation_area(app, tree) else {
        return Vec::new();
    };
    let main = main_region(tree, web);
    let mut out: Vec<SidebarChat> = Vec::new();
    let push = |out: &mut Vec<SidebarChat>, node: usize, key: ChatKey, title: String| {
        if !title.trim().is_empty() && !out.iter().any(|c| c.key == key) {
            out.push(SidebarChat {
                node,
                key,
                title: title.trim().to_string(),
            });
        }
    };
    // Conversation links carry their id wherever the app renders anchors.
    for i in tree.preorder(web) {
        if (!full || !history_page(app, tree)) && main.is_some_and(|m| tree.inside(i, m)) {
            continue;
        }
        let node = &tree.nodes[i];
        if node.role != "AXLink" {
            continue;
        }
        if let Some(ChatKey::Id(id)) = key_from_url(app, &node.url) {
            push(&mut out, i, ChatKey::Id(id), tree.text_of(i));
        }
    }
    // During navigation the history URL can appear before its rows. Never
    // fall back to sidebar title IDs in that intermediate snapshot: those
    // become duplicates when stable history IDs arrive on the next page.
    if !out.is_empty() || (full && history_page(app, tree)) {
        return out;
    }
    let (actions, markers, project) = match app.controls().history {
        HistoryRows::Lists {
            actions,
            markers,
            project,
        } => (actions, markers, project),
        HistoryRows::TitleButtons { options, .. } => {
            // The Claude app lists recents as title buttons, each next to its
            // own "More options for <title>" menu; navigation buttons have none.
            let mut seen: Vec<String> = Vec::new();
            let mut twice: Vec<String> = Vec::new();
            for i in tree.preorder(web) {
                let node = &tree.nodes[i];
                if node.role != "AXButton"
                    || node.label().trim().is_empty()
                    || (!in_sidebar(tree, i)
                        && !(full
                            && history_page(app, tree)
                            && main.is_some_and(|m| tree.inside(i, m))))
                {
                    continue;
                }
                let title = node.label().trim().to_string();
                let options = format!("{options}{title}");
                let row = node.parent.unwrap_or(i);
                if !tree
                    .preorder(row)
                    .into_iter()
                    .any(|j| tree.nodes[j].label() == options)
                {
                    continue;
                }
                if seen.contains(&title) {
                    twice.push(title);
                    continue;
                }
                seen.push(title.clone());
                push(&mut out, i, ChatKey::Title(title.clone()), title);
            }
            out.retain(|c| !matches!(&c.key, ChatKey::Title(t) if twice.contains(t)));
            return out;
        }
    };
    // The ChatGPT app lists its history as buttons in content lists inside
    // the navigation landmark (Pinned, Projects with their chats, Recents);
    // only the title identifies a row. A conversation row carries its own
    // chat actions; project rows ("Project actions for …") are not chats. A
    // title twice in one list is ambiguous and not offered; the same title
    // in two lists (a project's chat also under Recents) is one chat.
    let mut ambiguous: Vec<String> = Vec::new();
    for list in tree.preorder(web) {
        let node = &tree.nodes[list];
        if node.role != "AXList" || !in_navigation(tree, list) {
            continue;
        }
        let mut seen: Vec<String> = Vec::new();
        for &row in &node.children {
            let entry = tree.preorder(row).into_iter().find(|&i| {
                let n = &tree.nodes[i];
                matches!(n.role.as_str(), "AXButton" | "AXLink")
                    && !n.label().is_empty()
                    && !actions.contains(&n.label())
                    && !n.label().starts_with("More options")
                    && !n.label().ends_with("options")
            });
            let Some(i) = entry else { continue };
            let labels: Vec<&str> = tree
                .preorder(i)
                .into_iter()
                .map(|j| tree.nodes[j].label())
                .collect();
            let chat_row = labels.iter().any(|l| markers.contains(l))
                && !labels.iter().any(|l| l.starts_with(project));
            if !chat_row {
                continue;
            }
            let title = tree.nodes[i].label().trim().to_string();
            if seen.contains(&title) {
                ambiguous.push(title);
                continue;
            }
            seen.push(title.clone());
            push(&mut out, i, ChatKey::Title(title.clone()), title);
        }
    }
    out.retain(|c| !matches!(&c.key, ChatKey::Title(t) if ambiguous.contains(t)));
    out
}

/// The ChatGPT app's new-chat page offers Chat and Work; Muster's desktop
/// chats are the Chat kind. Returns the Chat toggle and whether it is on.
pub fn composer_chat_toggle(app: ChatApp, tree: &AxTree) -> Option<(usize, bool)> {
    let web = conversation_area(app, tree)?;
    let group = tree.preorder(web).into_iter().find(|&i| {
        let n = &tree.nodes[i];
        n.label() == "Composer mode"
    })?;
    tree.preorder(group).into_iter().find_map(|i| {
        let n = &tree.nodes[i];
        (matches!(n.role.as_str(), "AXCheckBox" | "AXRadioButton" | "AXButton")
            && n.label() == "Chat")
            .then_some((i, n.selected))
    })
}

pub(super) fn in_navigation(tree: &AxTree, i: usize) -> bool {
    tree.ancestors(i).any(|a| {
        let n = &tree.nodes[a];
        n.subrole == "AXLandmarkNavigation" || n.identifier == "app-shell-sidebar"
    })
}
