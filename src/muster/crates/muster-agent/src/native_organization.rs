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
        &["AXButton", "AXPopUpButton", "AXMenuItem"],
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
    unique(tree, 0, &[title], &["AXButton", "AXLink", "AXRow"])
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
        || text.contains("will not delete");
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
        &["Delete section", "Remove section", "Remove project"],
    )
}
pub fn menu(tree: &AxTree) -> Result<usize, Error> {
    let menus = crate::desktop_chat::open_menus(tree);
    match menus.as_slice() {
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
