//! Strict AX selectors for native organization. The caller binds the window to
//! the requested UUID before using these selectors; labels never bind identity.
use crate::{Error, desktop_chat::AxTree};

pub fn unique(tree: &AxTree, root: usize, labels: &[&str], roles: &[&str]) -> Result<usize, Error> {
    if !tree.complete {
        return Err(Error::Unavailable("Incomplete native controls".into()));
    }
    let matches: Vec<_> = tree
        .preorder(root)
        .into_iter()
        .filter(|&i| {
            let n = &tree.nodes[i];
            n.enabled && roles.contains(&n.role.as_str()) && labels.contains(&n.label())
        })
        .collect();
    match matches.as_slice() {
        [one] => Ok(*one),
        [] => Err(Error::Unavailable(
            "The native app does not expose this AX control".into(),
        )),
        _ => Err(Error::Conflict(
            "Ambiguous native controls; nothing was changed".into(),
        )),
    }
}
pub fn action(tree: &AxTree, root: usize, labels: &[&str]) -> Result<usize, Error> {
    unique(
        tree,
        root,
        labels,
        &[
            "AXButton",
            "AXPopUpButton",
            "AXMenuItem",
            "AXMenuItemCheckBox",
            "AXMenuItemRadio",
        ],
    )
}
fn token(value: &str, id: &str) -> bool {
    value
        .split(|c: char| !c.is_ascii_alphanumeric() && c != '-' && c != '_')
        .any(|s| s == id)
}
/// Only explicit UUID-bearing rows, or the uniquely selected title row in the
/// already verified window. Never select an unselected row by its title.
pub fn row(tree: &AxTree, id: &str, title: &str) -> Result<usize, Error> {
    if !tree.complete {
        return Err(Error::Unavailable("Incomplete sidebar".into()));
    }
    let candidates: Vec<_> = tree
        .preorder(0)
        .into_iter()
        .filter(|&i| {
            let n = &tree.nodes[i];
            n.enabled
                && matches!(n.role.as_str(), "AXButton" | "AXLink" | "AXRow")
                && (token(&n.url, id) || token(&n.identifier, id))
        })
        .collect();
    match candidates.as_slice() {
        [one] => return Ok(*one),
        [] => {}
        _ => return Err(Error::Conflict("Multiple native rows for this UUID".into())),
    }
    if title.is_empty() {
        return Err(Error::Unavailable(
            "The native sidebar exposes no UUID row".into(),
        ));
    }
    let candidates: Vec<_> = tree
        .preorder(0)
        .into_iter()
        .filter(|&i| {
            let n = &tree.nodes[i];
            n.enabled
                && matches!(n.role.as_str(), "AXButton" | "AXLink" | "AXRow")
                && n.label() == title
        })
        .collect();
    match candidates.as_slice() {
        [one] if tree.nodes[*one].selected => Ok(*one),
        _ => Err(Error::Unavailable(
            "The native sidebar does not identify the selected UUID row".into(),
        )),
    }
}
/// A label is only a navigation candidate. The platform must press it, then
/// re-resolve the window UUID before opening its row controls.
pub fn navigation_candidate(tree: &AxTree, title: &str) -> Result<usize, Error> {
    if title.is_empty() {
        return Err(Error::Unavailable("No native navigation label".into()));
    }
    if let Ok(sidebar) = sidebar(tree) {
        let label = format!("More options for {title}");
        let popups: Vec<_> = tree
            .preorder(sidebar)
            .into_iter()
            .filter(|&i| tree.nodes[i].role == "AXPopUpButton" && tree.nodes[i].label() == label)
            .collect();
        if !popups.is_empty() {
            let [popup] = popups.as_slice() else {
                return Err(Error::Conflict("Ambiguous native sidebar rows".into()));
            };
            let parent = tree.nodes[*popup]
                .parent
                .ok_or_else(|| Error::Unavailable("No containing sidebar row".into()))?;
            let rows: Vec<_> = tree.nodes[parent]
                .children
                .iter()
                .copied()
                .filter(|&i| {
                    let n = &tree.nodes[i];
                    n.enabled && matches!(n.role.as_str(), "AXButton" | "AXLink" | "AXRow")
                })
                .collect();
            return match rows.as_slice() {
                [one] if tree.complete => Ok(*one),
                _ => Err(Error::Unavailable(
                    "No unique native row beside the title popup".into(),
                )),
            };
        }
        return unique(tree, sidebar, &[title], &["AXButton", "AXLink", "AXRow"]);
    }
    unique(tree, 0, &[title], &["AXButton", "AXLink", "AXRow"])
}

pub fn sidebar(tree: &AxTree) -> Result<usize, Error> {
    unique(tree, 0, &["Sidebar"], &["AXGroup", "AXOutline"])
}

/// Group controls have their own heading context, not the session header.
pub fn section_heading(tree: &AxTree, title: &str) -> Result<usize, Error> {
    let sidebar = sidebar(tree)?;
    let heading = unique(tree, sidebar, &[title], &["AXHeading"])?;
    action(tree, heading, &[title])
}

/// Claude repeats Group and sort on every section. The Recents heading's
/// sibling is the global creation control; never arbitrarily pick a group.
pub fn create_trigger(tree: &AxTree) -> Result<usize, Error> {
    let sidebar = sidebar(tree)?;
    let heading = unique(tree, sidebar, &["Recents"], &["AXHeading"])?;
    let parent = tree.nodes[heading]
        .parent
        .ok_or_else(|| Error::Unavailable("No Recents group".into()))?;
    let controls: Vec<_> = tree.nodes[parent]
        .children
        .iter()
        .copied()
        .filter(|&i| {
            let n = &tree.nodes[i];
            n.enabled && n.role == "AXPopUpButton" && n.label() == "Group and sort"
        })
        .collect();
    match controls.as_slice() {
        [one] => Ok(*one),
        _ => Err(Error::Unavailable(
            "No unique Recents grouping control".into(),
        )),
    }
}
pub fn deletion_confirmation(tree: &AxTree) -> Result<usize, Error> {
    let dialogs: Vec<_> = tree
        .preorder(0)
        .into_iter()
        .filter(|&i| {
            matches!(tree.nodes[i].role.as_str(), "AXDialog" | "AXSheet")
                || tree.nodes[i].subrole == "AXApplicationDialog"
        })
        .collect();
    let [dialog] = dialogs.as_slice() else {
        return Err(Error::Unavailable("No unique removal confirmation".into()));
    };
    let text = tree
        .preorder(*dialog)
        .into_iter()
        .map(|i| tree.text_of(i))
        .collect::<Vec<_>>()
        .join(" ")
        .to_lowercase();
    let preserves = (text.contains("moved to")
        && (text.contains("recent") || text.contains("ungrouped")))
        || text.contains("won't delete")
        || text.contains("won’t delete")
        || text.contains("will not delete")
        || text.contains("nothing is in it");
    if !preserves
        || ["delete files", "discard", "worktree", "permanently"]
            .iter()
            .any(|word| text.contains(word))
    {
        return Err(Error::Conflict(
            "Native removal dialog does not promise to preserve conversations and files".into(),
        ));
    }
    action(
        tree,
        *dialog,
        &[
            "Delete section",
            "Remove section",
            "Remove project",
            "Delete",
        ],
    )
}
pub fn menu(tree: &AxTree) -> Result<usize, Error> {
    let menus = crate::desktop_chat::open_menus(tree);
    let leaves: Vec<_> = menus
        .iter()
        .copied()
        .filter(|&i| {
            !menus
                .iter()
                .any(|&other| other != i && tree.preorder(i).contains(&other))
        })
        .collect();
    match leaves.as_slice() {
        [one] => Ok(*one),
        _ => Err(Error::Unavailable("No unique native menu is open".into())),
    }
}
pub fn safe_dialog(tree: &AxTree) -> Result<usize, Error> {
    let dialogs: Vec<_> = tree
        .preorder(0)
        .into_iter()
        .filter(|&i| {
            matches!(tree.nodes[i].role.as_str(), "AXDialog" | "AXSheet")
                || tree.nodes[i].subrole == "AXApplicationDialog"
        })
        .collect();
    let [one] = dialogs.as_slice() else {
        return Err(Error::Unavailable("No unique section dialog".into()));
    };
    let text = tree
        .preorder(*one)
        .into_iter()
        .map(|i| tree.text_of(i))
        .collect::<Vec<_>>()
        .join(" ")
        .to_lowercase();
    if [
        "delete files",
        "delete conversations",
        "delete chats",
        "delete sessions",
        "discard",
        "worktree",
        "permanently",
    ]
    .iter()
    .any(|s| text.contains(s))
    {
        return Err(Error::Conflict(
            "Complete this destructive native dialog manually; no confirmation was pressed".into(),
        ));
    }
    Ok(*one)
}
#[cfg(test)]
mod tests {
    use super::*;
    use crate::desktop_chat::AxNode;
    fn tree(nodes: Vec<AxNode>) -> AxTree {
        AxTree {
            nodes,
            complete: true,
        }
    }
    #[test]
    fn title_is_never_an_identity() {
        let mut t = tree(vec![AxNode {
            role: "AXButton".into(),
            title: "Same".into(),
            enabled: true,
            ..Default::default()
        }]);
        assert!(row(&t, "uuid", "Same").is_err());
        t.nodes[0].selected = true;
        assert_eq!(row(&t, "uuid", "Same").unwrap(), 0);
        t.nodes[0].children = vec![1];
        t.nodes.push(t.nodes[0].clone());
        t.nodes[1].children.clear();
        assert!(row(&t, "uuid", "Same").is_err());
    }
    #[test]
    fn uuid_requires_token_boundary_and_complete_snapshot() {
        let mut t = tree(vec![AxNode {
            role: "AXLink".into(),
            url: "app://chat/uuid-other".into(),
            enabled: true,
            ..Default::default()
        }]);
        assert!(row(&t, "uuid", "").is_err());
        t.nodes[0].url = "app://chat/uuid".into();
        assert!(row(&t, "uuid", "").is_ok());
        t.complete = false;
        assert!(row(&t, "uuid", "").is_err());
    }
}

#[cfg(test)]
mod regression_tests {
    use super::*;
    use crate::desktop_chat::AxNode;
    fn node(role: &str, title: &str, parent: Option<usize>, children: &[usize]) -> AxNode {
        AxNode {
            role: role.into(),
            title: title.into(),
            parent,
            children: children.into(),
            enabled: true,
            ..Default::default()
        }
    }
    #[test]
    fn sidebar_popup_disambiguates_header_and_status_suffix_without_binding_identity() {
        let mut tree = AxTree {
            complete: true,
            nodes: vec![
                node("AXWindow", "", None, &[1, 5]),
                node("AXGroup", "Sidebar", Some(0), &[2]),
                node("AXGroup", "", Some(1), &[3, 4]),
                node("AXButton", "Example workspace · Connected", Some(2), &[]),
                node("AXPopUpButton", "More options for Example", Some(2), &[]),
                node("AXPopUpButton", "More options for Example", Some(0), &[]),
            ],
        };
        assert_eq!(navigation_candidate(&tree, "Example").unwrap(), 3);
        assert!(row(&tree, "uuid", "Example").is_err());
        tree.nodes[2].children.push(6);
        tree.nodes.push(node(
            "AXPopUpButton",
            "More options for Example",
            Some(2),
            &[],
        ));
        assert!(navigation_candidate(&tree, "Example").is_err());
        tree.nodes.pop();
        tree.nodes[2].children.pop();
        tree.complete = false;
        assert!(navigation_candidate(&tree, "Example").is_err());
    }
    #[test]
    fn create_uses_recents_sibling_not_nested_section_control() {
        let tree = AxTree {
            complete: true,
            nodes: vec![
                node("AXGroup", "Sidebar", None, &[1]),
                node("AXGroup", "", Some(0), &[2, 5, 6]),
                node("AXGroup", "", Some(1), &[3, 4]),
                node("AXHeading", "Example", Some(2), &[]),
                node("AXPopUpButton", "Group and sort", Some(2), &[]),
                node("AXHeading", "Recents", Some(1), &[]),
                node("AXPopUpButton", "Group and sort", Some(1), &[]),
            ],
        };
        assert_eq!(create_trigger(&tree).unwrap(), 6);
    }
    #[test]
    fn nested_menus_select_leaf_but_unrelated_menus_fail_closed() {
        let mut tree = AxTree {
            complete: true,
            nodes: vec![
                node("AXWindow", "", None, &[1]),
                node("AXMenu", "", Some(0), &[2]),
                node("AXMenu", "", Some(1), &[]),
            ],
        };
        assert_eq!(menu(&tree).unwrap(), 2);
        tree.nodes[1].children.clear();
        tree.nodes[0].children.push(2);
        tree.nodes[2].parent = Some(0);
        assert!(menu(&tree).is_err());
    }
    #[test]
    fn empty_section_delete_is_scoped_to_preserving_dialog() {
        let mut tree = AxTree {
            complete: true,
            nodes: vec![
                node("AXDialog", "Delete section?", None, &[1, 2]),
                node(
                    "AXStaticText",
                    "Example will be removed. Nothing is in it.",
                    Some(0),
                    &[],
                ),
                node("AXButton", "Delete", Some(0), &[]),
            ],
        };
        assert_eq!(deletion_confirmation(&tree).unwrap(), 2);
        tree.nodes[1].title = "Delete files permanently".into();
        assert!(deletion_confirmation(&tree).is_err());
    }
}
