//! The model and effort pickers beside the composer, and what their
//! menus say.

use super::*;

/// A composer control that opens a menu of models or reasoning efforts.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Picker {
    pub node: usize,
    pub label: String,
}

/// Labels (or label prefixes) of the composer's model and effort pickers.
pub(super) const PICKER_LABELS: [&str; 6] = [
    "Select ChatGPT model",
    // The ChatGPT app's Work / Codex composer.
    "Select effort",
    "Model",
    "Effort",
    "Select model",
    "Model selector",
];

/// The pickers next to the prompt field, in page order.
pub fn composer_pickers(app: ChatApp, tree: &AxTree) -> Vec<Picker> {
    let Some(web) = conversation_area(app, tree) else {
        return Vec::new();
    };
    let region = main_region(tree, web).unwrap_or(web);
    tree.preorder(region)
        .into_iter()
        .filter(|&i| {
            let n = &tree.nodes[i];
            n.role == "AXPopUpButton"
                && (PICKER_LABELS.iter().any(|p| n.label().starts_with(p))
                    // The ChatGPT app's Work page names the current model.
                    || n.label().starts_with("GPT-"))
        })
        .map(|i| Picker {
            node: i,
            label: tree.nodes[i].label().to_string(),
        })
        .collect()
}

/// The picker labelled like `picker` in this snapshot (indices change
/// between snapshots).
pub fn composer_pickers_like(tree: &AxTree, picker: &Picker) -> Option<usize> {
    tree.preorder(0).into_iter().find(|&i| {
        let n = &tree.nodes[i];
        n.role == "AXPopUpButton" && n.label() == picker.label
    })
}

/// Menus open in the page (never the menu bar's).
pub fn open_menus(tree: &AxTree) -> Vec<usize> {
    tree.preorder(0)
        .into_iter()
        .filter(|&i| {
            tree.nodes[i].role == "AXMenu"
                && !tree.ancestors(i).any(|a| tree.nodes[a].role == "AXMenuBar")
        })
        .collect()
}

/// The panel a picker opened: a menu, or (ChatGPT) a group titled like the
/// picker holding "Select model" and the reasoning "Power" slider.
pub fn picker_panel(tree: &AxTree, picker: &Picker) -> Option<usize> {
    tree.preorder(0).into_iter().find(|&i| {
        let n = &tree.nodes[i];
        i != picker.node
            && matches!(n.role.as_str(), "AXGroup" | "AXMenu")
            && n.label() == picker.label
            && !tree.ancestors(i).any(|a| tree.nodes[a].role == "AXMenuBar")
    })
}

/// Live status texts inside a panel (ChatGPT: "Medium, 2 of 5.").
pub fn panel_status(tree: &AxTree, panel: usize) -> Vec<String> {
    tree.preorder(panel)
        .into_iter()
        .filter(|&i| tree.nodes[i].subrole == "AXApplicationStatus")
        .map(|i| tree.text_of(i).trim().to_string())
        .filter(|t| !t.is_empty())
        .collect()
}

/// One model or effort a picker offers.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ChatChoice {
    pub node: usize,
    pub name: String,
    pub detail: String,
    pub selected: bool,
}

/// An entry's name and description: menu entries render them as separate
/// texts ("Opus 5.5" / "For complex work and everyday tasks") under one
/// combined title; an entry with one text is all name.
pub fn choice(tree: &AxTree, entry: &MenuEntry) -> ChatChoice {
    let texts: Vec<String> = tree
        .preorder(entry.node)
        .into_iter()
        .filter(|&j| tree.nodes[j].role == "AXStaticText")
        .map(|j| tree.text_of(j).trim().to_string())
        .filter(|t| !t.is_empty())
        .collect();
    let (name, detail) = match texts.as_slice() {
        [name, rest @ ..] if !rest.is_empty() && entry.label.starts_with(name.as_str()) => {
            (name.clone(), rest.join(" "))
        }
        [name] if entry.label.is_empty() => (name.clone(), String::new()),
        _ => split_versioned(&entry.label),
    };
    ChatChoice {
        node: entry.node,
        name,
        detail,
        selected: entry.selected,
    }
}

/// A combined title without separate texts: the name runs through its first
/// versioned word ("Opus 5.5 For complex work…" → "Opus 5.5"), when a
/// description of two words or more follows ("GPT-5.6 Sol" stays whole).
pub(super) fn split_versioned(label: &str) -> (String, String) {
    let words: Vec<&str> = label.split_whitespace().collect();
    match words
        .iter()
        .position(|w| w.chars().any(|c| c.is_ascii_digit()))
    {
        Some(k) if words.len() - k > 2 => (words[..=k].join(" "), words[k + 1..].join(" ")),
        _ => (label.trim().to_string(), String::new()),
    }
}

/// Claude's model menu carries an "Effort <level>" entry that opens the
/// effort levels, and "More models".
pub const MORE_MODELS: &str = "More models";

pub fn effort_entry(entries: &[MenuEntry]) -> Option<&MenuEntry> {
    entries.iter().find(|e| e.label.starts_with("Effort"))
}

/// Model entries of a model menu: everything but effort and More models.
pub fn model_entries(entries: &[MenuEntry]) -> Vec<&MenuEntry> {
    entries
        .iter()
        .filter(|e| !e.label.starts_with("Effort") && e.label != MORE_MODELS)
        .collect()
}

/// The ChatGPT app's panel names the power level first: "Medium, 2 of 5.".
pub fn stated_effort(status: &str) -> String {
    status
        .split(',')
        .next()
        .unwrap_or("")
        .trim()
        .trim_end_matches('.')
        .to_string()
}

/// A ChatGPT Power level as its panel states it: "Medium, 2 of 5.".
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PowerLevel {
    pub name: String,
    pub position: u32,
    pub total: u32,
}

pub fn power_level(status: &str) -> Option<PowerLevel> {
    let (name, rest) = status.trim().trim_end_matches('.').rsplit_once(',')?;
    let (position, total) = rest.trim().split_once(" of ")?;
    let (position, total) = (position.trim().parse().ok()?, total.trim().parse().ok()?);
    (position >= 1 && position <= total).then(|| PowerLevel {
        name: name.trim().to_string(),
        position,
        total,
    })
}

/// The one level Accessibility can choose: AXPress on the Power track
/// clicks its centre, which Radix rounds to this level (3 of 5).
pub fn middle_position(total: u32) -> u32 {
    (f64::from(total.saturating_sub(1)) / 2.0).round() as u32 + 1
}

/// The effort choice Muster offers for ChatGPT while Power is below its
/// middle level.
pub const RAISE_TO_MIDDLE: &str = "Middle";

/// The part of the Power control that takes a centre click: its track, the
/// first empty group under the "Power" item.
pub fn power_track(tree: &AxTree, power: usize) -> Option<usize> {
    tree.preorder(power)
        .into_iter()
        .skip(1)
        .find(|&i| tree.nodes[i].subrole == "AXEmptyGroup")
}

/// One entry of an open menu.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct MenuEntry {
    pub node: usize,
    pub label: String,
    pub selected: bool,
    pub enabled: bool,
}

pub fn menu_entries(tree: &AxTree, menu: usize) -> Vec<MenuEntry> {
    tree.preorder(menu)
        .into_iter()
        .filter(|&i| {
            matches!(
                tree.nodes[i].role.as_str(),
                "AXMenuItem" | "AXMenuItemRadio" | "AXMenuItemCheckBox"
            )
        })
        .map(|i| MenuEntry {
            node: i,
            label: tree.text_of(i).trim().to_string(),
            selected: tree.nodes[i].selected,
            enabled: tree.nodes[i].enabled,
        })
        .collect()
}
