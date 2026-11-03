//! Permission-card interpretation over bounded OS facts, never label-only grants.
use crate::{desktop_chat::AxTree, profile::App};

#[derive(Debug, PartialEq, Eq)]
pub struct Card {
    pub prompt: String,
    pub options: Vec<usize>,
    pub named: bool,
}

fn path(tree: &AxTree, mut index: usize) -> Option<Vec<usize>> {
    let mut result = Vec::new();
    for _ in 0..64 {
        if result.contains(&index) {
            return None;
        }
        let node = tree.nodes.get(index)?;
        result.push(index);
        if index == 0 {
            return (node.role == "AXWebArea").then_some(result);
        }
        index = node.parent?;
    }
    None
}

pub fn observe(profile: &App, tree: &AxTree) -> Option<Card> {
    if !tree.complete {
        return None;
    }
    let controls = profile.controls();
    let cards: Vec<_> = tree
        .nodes
        .iter()
        .enumerate()
        .filter_map(|(i, node)| {
            (controls
                .permission_card_prefix
                .is_some_and(|prefix| node.label().starts_with(prefix))
                && matches!(node.role.as_str(), "AXGroup" | "AXUnknown" | "AXDialog")
                && path(tree, i).is_some())
            .then_some(i)
        })
        .collect();
    // Never combine decisions from two commands or a sidebar quick-action copy.
    if cards.len() > 1 {
        return None;
    }
    let card = cards.first().copied();
    let options: Vec<_> = tree
        .nodes
        .iter()
        .enumerate()
        .filter_map(|(i, node)| {
            if !node.enabled || !controls.approve.matches(node) {
                return None;
            }
            let ancestry = path(tree, i)?;
            if card.is_some_and(|card| !ancestry.contains(&card)) {
                return None;
            }
            let subroles: Vec<_> = ancestry
                .iter()
                .take(ancestry.len() - 1)
                .map(|&j| tree.nodes[j].subrole.clone())
                .collect();
            controls.approval_scope.admits(&subroles, true).then_some(i)
        })
        .collect();
    if options.is_empty() {
        return None;
    }
    let mut labels = std::collections::HashSet::new();
    if options
        .iter()
        .any(|&i| !labels.insert(tree.nodes[i].label()))
    {
        return None;
    }
    let prompt = if let Some(card) = card {
        let mut parts = Vec::new();
        for (i, node) in tree.nodes.iter().enumerate() {
            if node.role != "AXStaticText" {
                continue;
            }
            let Some(ancestry) = path(tree, i) else {
                continue;
            };
            if !ancestry.contains(&card) || ancestry.iter().any(|j| options.contains(j)) {
                continue;
            }
            let text = if node.value.is_empty() {
                &node.title
            } else {
                &node.value
            };
            if !text.trim().is_empty() {
                parts.push(text.as_str());
            }
        }
        let text = parts.join("\n");
        // Truncation could conceal the command being authorized.
        if text.len() > 64 * 1024 {
            return None;
        }
        if text.is_empty() {
            tree.nodes[card].label().to_owned()
        } else {
            text
        }
    } else {
        format!("{} asks for permission", profile.app.provider)
    };
    Some(Card {
        prompt,
        options,
        named: card.is_some(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{desktop_chat::AxNode, profile::CLAUDE_APP};
    fn fixture() -> AxTree {
        let mut tree = AxTree {
            complete: true,
            nodes: vec![AxNode {
                role: "AXWebArea".into(),
                ..Default::default()
            }],
        };
        for (role, label, parent, enabled) in [
            ("AXGroup", "Permission request: run this command", 0, true),
            ("AXStaticText", "Allow Claude to run this command?", 1, true),
            ("AXStaticText", "Inspect example source files", 1, true),
            (
                "AXStaticText",
                "A nested command cannot be checked before it runs",
                1,
                true,
            ),
            ("AXStaticText", "wc -l example.rs", 1, true),
            ("AXButton", "Deny", 1, true),
            ("AXButton", "Allow once", 1, true),
            ("AXButton", "Yes", 0, true),
        ] {
            tree.nodes.push(AxNode {
                role: role.into(),
                title: label.into(),
                parent: Some(parent),
                enabled,
                ..Default::default()
            });
        }
        tree
    }
    #[test]
    fn command_card_projects_context_and_only_its_enabled_decisions() {
        let mut tree = fixture();
        let card = observe(&CLAUDE_APP, &tree).unwrap();
        assert_eq!(card.options, vec![6, 7]);
        assert!(card.prompt.contains("wc -l example.rs"));
        assert!(card.prompt.contains("nested command"));
        assert!(!card.prompt.contains("Allow once"));
        tree.nodes[7].enabled = false;
        assert_eq!(observe(&CLAUDE_APP, &tree).unwrap().options, vec![6]);
        tree.nodes[6].enabled = false;
        assert!(observe(&CLAUDE_APP, &tree).is_none());
    }
    #[test]
    fn changed_command_is_a_different_decision_even_with_identical_buttons() {
        let mut tree = fixture();
        let first = observe(&CLAUDE_APP, &tree).unwrap();
        tree.nodes[5].title = "wc -l other.rs".into();
        assert_ne!(observe(&CLAUDE_APP, &tree).unwrap(), first);
        tree.complete = false;
        assert!(observe(&CLAUDE_APP, &tree).is_none());
    }
    #[test]
    fn legacy_cards_keep_exact_vocabulary_and_scope() {
        let mut tree = fixture();
        tree.nodes[1].title.clear();
        tree.nodes[8].title = "Allow once for everything".into();
        assert_eq!(observe(&CLAUDE_APP, &tree).unwrap().options, vec![6, 7]);
        assert!(observe(&crate::profile::CODEX_APP, &tree).is_none());
        tree.nodes[1].subrole = "AXLandmarkMain".into();
        assert_eq!(
            observe(&crate::profile::CODEX_APP, &tree).unwrap().options,
            vec![6, 7]
        );
        tree.nodes[1].subrole = "AXLandmarkComplementary".into();
        assert!(observe(&crate::profile::CODEX_APP, &tree).is_none());
    }

    #[test]
    fn ambiguity_and_detached_or_cyclic_controls_never_offer_a_grant() {
        let mut tree = fixture();
        tree.nodes.push(tree.nodes[1].clone());
        assert!(observe(&CLAUDE_APP, &tree).is_none());
        tree.nodes.pop();
        tree.nodes[6].parent = Some(6);
        tree.nodes[7].parent = None;
        assert!(observe(&CLAUDE_APP, &tree).is_none());
    }
}
