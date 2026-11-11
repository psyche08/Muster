//! Interpret native coding-session model/effort menus. No model catalogue is
//! cached or invented: a write must match one enabled choice in the live menu.
use crate::{
    Error,
    desktop_chat::{self as chat, AxTree, ChatChoice},
};
use std::collections::BTreeMap;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Kind {
    Model,
    Effort,
}

/// A combined model trigger is only usable for effort when its opened
/// panel exposes Power; a plain model list must never become effort choices.
pub fn effort_trigger(label: &str) -> bool {
    let label = label.to_ascii_lowercase();
    label.starts_with("select effort")
        || label.starts_with("reasoning effort")
        || label == "effort"
        || label.starts_with("effort:")
        || label.starts_with("effort ")
}

/// A snapshot rooted at the already resolved session's web area.
pub fn picker(tree: &AxTree, kind: Kind) -> Result<usize, Error> {
    if !tree.complete {
        return Err(Error::Unavailable(
            "Incomplete model controls snapshot".into(),
        ));
    }
    let candidates: Vec<_> = tree
        .preorder(0)
        .into_iter()
        .filter(|&i| {
            let n = &tree.nodes[i];
            if !n.enabled || !matches!(n.role.as_str(), "AXPopUpButton" | "AXButton") {
                return false;
            }
            let label = n.label().to_ascii_lowercase();
            match kind {
                Kind::Model => {
                    ["select model", "select chatgpt model", "model selector"]
                        .iter()
                        .any(|p| label.starts_with(p))
                        || label == "model"
                        || label.starts_with("model:")
                        || label.starts_with("model ")
                        || (n.role == "AXPopUpButton"
                            && (label.starts_with("gpt-")
                                || label.starts_with("claude ")
                                || label.starts_with("opus ")
                                || label.starts_with("sonnet ")
                                || label.starts_with("haiku ")))
                }
                Kind::Effort => effort_trigger(n.label()),
            }
        })
        .collect();
    // Newer Codex uses one combined Model/Power trigger labelled "Select
    // effort" after an explicit model choice. Interpretation of the opened panel decides what is
    // actually selectable; the trigger label never supplies option names.
    let candidates = if candidates.is_empty() {
        tree.preorder(0)
            .into_iter()
            .filter(|&i| {
                let n = &tree.nodes[i];
                n.enabled
                    && n.role == "AXPopUpButton"
                    && match kind {
                        Kind::Model => n.label() == "Select effort",
                        Kind::Effort => {
                            n.label().starts_with("GPT-")
                                || n.label().starts_with("Select model")
                                || n.label().starts_with("Select ChatGPT model")
                        }
                    }
            })
            .collect()
    } else {
        candidates
    };
    match candidates.as_slice() {
        [one] => Ok(*one),
        [] => Err(Error::Unavailable(format!(
            "The app exposes no {kind:?} picker in this session"
        ))),
        _ => Err(Error::Conflict(format!(
            "Multiple {kind:?} pickers in this session"
        ))),
    }
}

/// A direct menu or the model/Power panel. Never accept multiple candidate
/// menus, or mistake its navigation rows for actual model choices.
pub fn panel(tree: &AxTree, picker: &chat::Picker) -> Result<Option<usize>, Error> {
    let panels: Vec<_> = tree
        .preorder(0)
        .into_iter()
        .filter(|&i| {
            let n = &tree.nodes[i];
            matches!(n.role.as_str(), "AXGroup" | "AXMenu") && n.label() == picker.label
        })
        .collect();
    match panels.as_slice() {
        [one] => return Ok(Some(*one)),
        [] => {}
        _ => {
            return Err(Error::Conflict(
                "Multiple matching option panels are open".into(),
            ));
        }
    }
    match chat::open_menus(tree).as_slice() {
        [] => Ok(None),
        [one] => Ok(Some(*one)),
        _ => Err(Error::Conflict("Multiple option menus are open".into())),
    }
}

/// The panel `opened` opened, on a fresh read. The ChatGPT app (26.1103)
/// renames its button while the panel is open ("GPT-6 Astra High" becomes
/// "Select effort") and titles the panel so: a panel under the button's
/// current name counts too.
pub fn opened_panel(
    tree: &AxTree,
    opened: &chat::Picker,
    kind: Kind,
) -> Result<Option<usize>, Error> {
    if let Some(found) = panel(tree, opened)? {
        return Ok(Some(found));
    }
    match picker(tree, kind) {
        Ok(node) if tree.nodes[node].label() != opened.label => panel(
            tree,
            &chat::Picker {
                node,
                label: tree.nodes[node].label().to_owned(),
            },
        ),
        _ => Ok(None),
    }
}

pub fn choices(tree: &AxTree, menu: usize, kind: Kind) -> Result<Vec<ChatChoice>, Error> {
    if !tree.complete {
        return Err(Error::Unavailable("Incomplete model menu snapshot".into()));
    }
    if kind == Kind::Effort {
        if let Some((level, track)) = power(tree, menu) {
            let mut choices = vec![ChatChoice {
                node: menu,
                name: level.name.clone(),
                detail: format!("{} of {}", level.position, level.total),
                selected: true,
            }];
            let middle = chat::middle_position(level.total);
            if level.position < middle {
                choices.push(ChatChoice {
                    node: track,
                    name: chat::RAISE_TO_MIDDLE.into(),
                    detail: format!("{middle} of {}", level.total),
                    selected: false,
                });
            }
            return Ok(choices);
        }
    }
    Ok(chat::menu_entries(tree, menu)
        .iter()
        .filter(|e| {
            e.enabled
                && (kind != Kind::Model
                    || (!e.label.starts_with("Effort")
                        && e.label != chat::MORE_MODELS
                        && e.label != "Select model"
                        && e.label != "Power"
                        // Claude's Code-mode panel ends with a paid "Enable
                        // fast mode" toggle; it is never a model.
                        && !e.label.to_ascii_lowercase().contains("fast mode")))
        })
        .map(|e| chat::choice(tree, e))
        .filter(|c| !c.name.is_empty())
        .collect())
}

/// The current Power status and the only AX-pressable part of that control.
/// A centre click may only raise a lower value to the middle; it must never
/// silently lower a stronger setting or pretend to offer arbitrary levels.
pub fn power(tree: &AxTree, panel: usize) -> Option<(chat::PowerLevel, usize)> {
    let entries = chat::menu_entries(tree, panel);
    let power = entries.iter().find(|e| e.label == "Power" && e.enabled)?;
    let level = chat::panel_status(tree, panel)
        .iter()
        .find_map(|s| chat::power_level(s))?;
    Some((level, chat::power_track(tree, power.node)?))
}

pub const POWER_NOTE: &str = "This app exposes Power as a slider: Accessibility can only raise a lower setting to the middle. Other levels must be changed in the desktop app.";

pub fn choose<'a>(choices: &'a [ChatChoice], name: &str) -> Result<&'a ChatChoice, Error> {
    let matches: Vec<_> = choices.iter().filter(|c| c.name == name).collect();
    match matches.as_slice() {
        [one] => Ok(one),
        [] => Err(Error::Invalid(format!(
            "{name:?} is not offered by the current model/effort menu; refresh the choices"
        ))),
        _ => Err(Error::Conflict(
            "The app exposes duplicate option names".into(),
        )),
    }
}

/// Confirm a freshly observed selection, including the app's own name for
/// the middle Power level. Never derive success from the press result.
pub fn confirmed(choices: &[ChatChoice], kind: Kind, name: &str) -> bool {
    if kind == Kind::Effort && name == chat::RAISE_TO_MIDDLE {
        choices.iter().any(|c| {
            c.selected
                && c.detail.split_once(" of ").is_some_and(|(p, total)| {
                    p.parse::<u32>()
                        .ok()
                        .zip(total.parse::<u32>().ok())
                        .is_some_and(|(p, total)| total > 0 && p == chat::middle_position(total))
                })
        })
    } else {
        choose(choices, name).is_ok_and(|c| c.selected)
    }
}

/// The stable start of every refusal of a new session's requested model or
/// effort (ADR 0181). Such a creation sent nothing, and asking again
/// unchanged is refused again, so callers treat it as final.
pub const CREATE_REFUSED: &str = "The new session's model and effort were not set";

/// A refusal of a new session's model or effort: nothing was drafted or sent.
pub fn create_refused(why: impl std::fmt::Display) -> Error {
    Error::Invalid(format!("{CREATE_REFUSED}: {why}; nothing was sent"))
}

/// Whether a requested model or effort name may be sent to a Mac: empty
/// (keep the app's choice), or a short name exactly as the app shows it.
pub fn valid_name(name: &str) -> bool {
    name.len() <= 128 && name.trim() == name && !name.chars().any(char::is_control)
}

/// What a picker shows, part by part: Claude's "Model: Opus 5.5 High · Ultracode"
/// renders its parts as separate texts ("Opus 5.5", "High", "Ultracode").
/// A picker without texts shows its label, less a "Model:" prefix.
pub fn shown(tree: &AxTree, picker: usize) -> Vec<String> {
    let parts: Vec<String> = tree
        .preorder(picker)
        .into_iter()
        .skip(1)
        .filter(|&i| tree.nodes[i].role == "AXStaticText")
        .map(|i| tree.text_of(i).trim().to_string())
        .filter(|t| t.chars().any(char::is_alphanumeric))
        .collect();
    if !parts.is_empty() {
        return parts;
    }
    let label = tree.nodes[picker].label();
    let label = label.strip_prefix("Model:").unwrap_or(label).trim();
    if label.is_empty() {
        Vec::new()
    } else {
        vec![label.to_string()]
    }
}

/// The effort Claude's model panel names on its "Effort <level>" entry, less
/// the toggles it names after a dot ("Effort Extra · Ultracode" is "Extra").
/// Reading it back is how a chosen level is confirmed.
pub fn submenu_effort(tree: &AxTree, panel: usize) -> Option<String> {
    let entries = chat::menu_entries(tree, panel);
    let entry = chat::effort_entry(&entries)?;
    let level = entry.label.strip_prefix("Effort")?;
    let level = level.split(" · ").next().unwrap_or_default().trim();
    (!level.is_empty()).then(|| level.to_string())
}

/// Claude's effort levels, once its focused "Effort <level>" entry
/// (`entry`, its title) opened them: the entries of the group of choices in
/// the submenu titled like that entry. Each is named by its whole title,
/// badge included ("Medium Recommended", "Max 3.5×"). Toggles after that
/// group ("Ultracode") are not levels.
pub fn effort_levels(tree: &AxTree, entry: &str) -> Vec<ChatChoice> {
    let group = tree
        .nodes
        .iter()
        .position(|n| n.role == "AXGroup" && n.title == entry)
        .and_then(|submenu| {
            tree.preorder(submenu)
                .into_iter()
                .find(|&i| tree.nodes[i].subrole == "AXApplicationGroup")
        });
    let Some(group) = group else {
        return Vec::new();
    };
    chat::menu_entries(tree, group)
        .into_iter()
        .filter(|e| e.enabled && !e.label.is_empty())
        .map(|e| ChatChoice {
            node: e.node,
            name: e.label,
            detail: String::new(),
            selected: e.selected,
        })
        .collect()
}

/// The level a name names: the one titled exactly so, else the one level
/// whose title is that name and a badge ("Medium" names "Medium
/// Recommended"). None when no level, or more than one, fits.
pub fn level_named<'a>(levels: &'a [ChatChoice], name: &str) -> Option<&'a ChatChoice> {
    let exact: Vec<_> = levels.iter().filter(|c| c.name == name).collect();
    let fits = if exact.is_empty() {
        levels
            .iter()
            .filter(|c| {
                c.name
                    .strip_prefix(name)
                    .is_some_and(|badge| badge.starts_with(' '))
            })
            .collect()
    } else {
        exact
    };
    match fits.as_slice() {
        [one] => Some(one),
        _ => None,
    }
}

/// The level to press for `name`, or none when it is already chosen.
/// Refuses a name no single level fits, listing the levels.
pub fn create_level<'a>(
    levels: &'a [ChatChoice],
    name: &str,
) -> Result<Option<&'a ChatChoice>, Error> {
    let level = level_named(levels, name)
        .ok_or_else(|| not_offered(Kind::Effort, name, levels.iter().map(|c| c.name.as_str())))?;
    Ok((!level.selected).then_some(level))
}

/// Whether the level the "Effort" entry shows after a press (`shown`) is
/// the one `name` chose from `levels`: the same name, or one that fits the
/// same level ("Medium" for a request of "Medium Recommended").
pub fn level_confirmed(levels: &[ChatChoice], name: &str, shown: &str) -> bool {
    shown == name
        || level_named(levels, shown)
            .zip(level_named(levels, name))
            .is_some_and(|(shown, chosen)| shown.node == chosen.node)
}

/// The ChatGPT app's "Power" item: its slider takes arrow keys while the
/// item itself is focused.
pub fn power_entry(tree: &AxTree, panel: usize) -> Option<usize> {
    chat::menu_entries(tree, panel)
        .iter()
        .find(|e| e.label == "Power" && e.enabled)
        .map(|e| e.node)
}

/// One arrow key on the focused Power item.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PowerStep {
    Lower,
    Higher,
}

/// Walks the ChatGPT app's Power slider to a level known only by name: the
/// app states only the level it shows, as a model-and-reasoning name
/// ("GPT-6 Astra Extended, 3 of 5."). A level's own name ends the walk where
/// it is read. Any other name is looked up only after every level was read,
/// downwards first and then upwards ([`power_named`]), and then walked to.
/// A name no level has walks back to the starting level and refuses the
/// creation. Every key must move exactly one level.
#[derive(Debug)]
pub struct PowerWalk {
    start: chat::PowerLevel,
    target: String,
    seen: BTreeMap<u32, String>,
    last: Option<(u32, PowerStep)>,
    keys: u32,
    /// Once every level was read: the requested level, or the starting
    /// one when no level has the name (`offered` false).
    goal: Option<u32>,
    offered: bool,
}

impl PowerWalk {
    pub fn new(start: &chat::PowerLevel, target: &str) -> Self {
        Self {
            start: start.clone(),
            target: target.to_owned(),
            seen: BTreeMap::new(),
            last: None,
            keys: 0,
            goal: None,
            offered: false,
        }
    }

    /// The next key after reading `level`, the level the app states now;
    /// `None` once it states the requested level, whose name is then
    /// `level.name`.
    pub fn next(&mut self, level: &chat::PowerLevel) -> Result<Option<PowerStep>, Error> {
        if let Some((from, step)) = self.last.take() {
            let expected = match step {
                PowerStep::Lower => from.checked_sub(1),
                PowerStep::Higher => from.checked_add(1),
            };
            if Some(level.position) != expected || level.total != self.start.total {
                return Err(create_unavailable(format!(
                    "the Power slider moved from {from} to {} of {} on one key",
                    level.position, level.total
                )));
            }
        }
        self.seen.insert(level.position, level.name.clone());
        if self.goal.is_none() && level.name == self.target {
            return Ok(None);
        }
        let goal = match self.goal {
            Some(goal) => goal,
            None => {
                let unseen = |mut range: std::ops::RangeInclusive<u32>| {
                    range.any(|p| !self.seen.contains_key(&p))
                };
                if unseen(1..=level.position.saturating_sub(1)) {
                    return self.key(level, PowerStep::Lower);
                }
                if unseen(level.position + 1..=level.total) {
                    return self.key(level, PowerStep::Higher);
                }
                let found = power_named(&self.seen, &self.target);
                self.offered = found.is_some();
                *self.goal.insert(found.unwrap_or(self.start.position))
            }
        };
        match level.position.cmp(&goal) {
            std::cmp::Ordering::Equal if self.offered => Ok(None),
            std::cmp::Ordering::Equal => Err(not_offered(
                Kind::Effort,
                &self.target,
                self.seen.values().map(String::as_str),
            )),
            std::cmp::Ordering::Greater => self.key(level, PowerStep::Lower),
            std::cmp::Ordering::Less => self.key(level, PowerStep::Higher),
        }
    }

    fn key(
        &mut self,
        level: &chat::PowerLevel,
        step: PowerStep,
    ) -> Result<Option<PowerStep>, Error> {
        self.keys += 1;
        if self.keys > 3 * level.total {
            return Err(create_unavailable(
                "the Power slider did not settle on a level",
            ));
        }
        self.last = Some((level.position, step));
        Ok(Some(step))
    }
}

/// The position of the Power level `name` names, from every level's name
/// (`seen`): the app's own name ("GPT-6 Astra Light"), or that name without
/// the leading words every level shares, the model ("Light"). A name that
/// fits no single level is not offered. The app's button names the levels
/// in other words ("GPT-6 Astra High" for "GPT-6 Astra Extended"); those are
/// never matched.
pub fn power_named(seen: &BTreeMap<u32, String>, name: &str) -> Option<u32> {
    let exact: Vec<u32> = seen
        .iter()
        .filter(|(_, n)| *n == name)
        .map(|(p, _)| *p)
        .collect();
    if !exact.is_empty() || seen.len() < 2 {
        return match exact.as_slice() {
            [one] => Some(*one),
            _ => None,
        };
    }
    let words: Vec<Vec<&str>> = seen.values().map(|n| n.split(' ').collect()).collect();
    let shared = (0..)
        .take_while(|&i| words.iter().all(|w| i + 1 < w.len() && w[i] == words[0][i]))
        .count();
    if shared == 0 {
        return None;
    }
    let short: Vec<u32> = seen
        .keys()
        .zip(&words)
        .filter(|(_, w)| w[shared..].join(" ") == name)
        .map(|(p, _)| *p)
        .collect();
    match short.as_slice() {
        [one] => Some(*one),
        _ => None,
    }
}

/// The choice a new session's form must press, or `None` when it already
/// shows `name`. A name the live menu does not offer refuses the creation.
pub fn create_choice<'a>(
    choices: &'a [ChatChoice],
    kind: Kind,
    name: &str,
) -> Result<Option<&'a ChatChoice>, Error> {
    match choose(choices, name) {
        Ok(choice) => Ok((!choice.selected).then_some(choice)),
        Err(Error::Invalid(_)) => Err(not_offered(
            kind,
            name,
            choices.iter().map(|c| c.name.as_str()),
        )),
        Err(other) => Err(other),
    }
}

/// Whether a fresh read of the form shows `name`: the menu marks it
/// selected, or the picker names it first (Claude marks no menu entry).
pub fn create_model_confirmed(choices: &[ChatChoice], shown: &[String], name: &str) -> bool {
    shown.first().is_some_and(|first| first == name) || confirmed(choices, Kind::Model, name)
}

/// The refusal of an effort for a model whose form shows none.
pub fn create_no_effort(name: &str) -> Error {
    create_refused(format!(
        "effort {name:?} cannot be chosen: the app shows no effort for this model"
    ))
}

/// The app did not open Claude's effort levels for its focused "Effort"
/// entry. Nothing was chosen; a later attempt may succeed.
pub fn create_levels_closed(name: &str) -> Error {
    create_unavailable(format!(
        "effort {name:?} could not be chosen: the app did not open its effort levels"
    ))
}

/// Like [`create_refused`], for a step the app did not complete: the same
/// request may succeed later.
pub fn create_unavailable(why: impl std::fmt::Display) -> Error {
    Error::Unavailable(format!("{CREATE_REFUSED}: {why}; nothing was sent"))
}

fn not_offered<'a>(kind: Kind, name: &str, offered: impl Iterator<Item = &'a str>) -> Error {
    let offered: Vec<_> = offered.map(|n| format!("{n:?}")).collect();
    create_refused(format!(
        "{} {name:?} is not offered (the app offers {})",
        match kind {
            Kind::Model => "model",
            Kind::Effort => "effort",
        },
        if offered.is_empty() {
            "none".to_string()
        } else {
            offered.join(", ")
        }
    ))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::desktop_chat::tests::tree;
    #[test]
    fn coding_pickers_are_distinct_and_ambiguity_rejected() {
        let mut t = tree(
            "AXWebArea\n  AXPopUpButton desc=\"GPT-6.1 Sol\"\n  AXPopUpButton desc=\"Select effort\"",
        );
        assert_eq!(picker(&t, Kind::Model).unwrap(), 1);
        assert_eq!(picker(&t, Kind::Effort).unwrap(), 2);
        t.nodes[2].description = "Select model".into();
        assert!(matches!(picker(&t, Kind::Model), Err(Error::Conflict(_))));
        t.complete = false;
        assert!(picker(&t, Kind::Model).is_err());
    }
    #[test]
    fn exact_enabled_runtime_choices_only() {
        let t = tree(
            "AXMenu\n  AXMenuItemRadio title=\"GPT-6.1 Sol\" sel=\"1\"\n  AXMenuItemRadio title=\"Unavailable\" enabled=\"0\"\n  AXMenuItem title=\"Effort High\"\n  AXMenuItem title=\"More models\"",
        );
        let c = choices(&t, 0, Kind::Model).unwrap();
        assert_eq!(c.len(), 1);
        assert!(choose(&c, "GPT-6.1 Sol").unwrap().selected);
        assert!(choose(&c, "GPT-6.1").is_err());
        assert!(choose(&c, "Unavailable").is_err());
        assert!(choose(&[c[0].clone(), c[0].clone()], "GPT-6.1 Sol").is_err());
    }
    #[test]
    fn claude_model_label_and_duplicate_panels_are_checked() {
        let t = tree(
            "AXWebArea\n  AXPopUpButton desc=\"Model: Opus 5.5 Medium\"\n  AXGroup title=\"Model: Opus 5.5 Medium\"\n  AXGroup title=\"Model: Opus 5.5 Medium\"",
        );
        assert_eq!(picker(&t, Kind::Model).unwrap(), 1);
        assert!(
            panel(
                &t,
                &chat::Picker {
                    node: 1,
                    label: "Model: Opus 5.5 Medium".into()
                }
            )
            .is_err()
        );
    }

    #[test]
    fn model_trigger_does_not_authorize_its_menu_as_effort_choices() {
        assert!(!effort_trigger("GPT-6.1 Sol"));
        assert!(!effort_trigger("Select model"));
        assert!(effort_trigger("Select effort"));
        assert!(effort_trigger("Effort: High"));
    }

    #[test]
    fn effort_names_and_selection_come_from_menu() {
        let t = tree(
            "AXMenu\n  AXMenuItemRadio title=\"Low\"\n  AXMenuItemRadio title=\"Ultra\" sel=\"1\"",
        );
        let c = choices(&t, 0, Kind::Effort).unwrap();
        assert!(choose(&c, "Ultra").unwrap().selected);
        assert!(choose(&c, "High").is_err());
    }
    #[test]
    fn combined_power_panel_only_offers_a_safe_middle_step() {
        for (position, expected) in [(2, 2), (3, 1), (4, 1)] {
            let t = tree(&format!(
                "AXWebArea\n  AXPopUpButton desc=\"Select effort\"\n  AXGroup title=\"Select effort\"\n    AXStaticText/AXApplicationStatus value=\"High, {position} of 5.\"\n    AXMenuItem title=\"Select model\"\n    AXMenuItem title=\"Power\"\n      AXGroup/AXEmptyGroup"
            ));
            assert_eq!(picker(&t, Kind::Model).unwrap(), 1);
            let panel = panel(
                &t,
                &chat::Picker {
                    node: 1,
                    label: "Select effort".into(),
                },
            )
            .unwrap()
            .unwrap();
            let options = choices(&t, panel, Kind::Effort).unwrap();
            assert_eq!(options.len(), expected);
            assert_eq!(
                confirmed(&options, Kind::Effort, chat::RAISE_TO_MIDDLE),
                position == 3
            );
            assert!(choices(&t, panel, Kind::Model).unwrap().is_empty());
        }
    }

    /// Claude 4.23118 (Code mode): the composer's "Model:" button names model
    /// and effort; it opens a group titled like it, whose models are plain
    /// menu items (the current one carries AXValue 1 and a check mark, not
    /// AXSelected), then "Effort <level>" (a hover submenu), "More models"
    /// and a paid fast-mode toggle.
    const CLAUDE_FORM: &str = "AXWebArea url=\"https://claude.ai/epitaxy\"
  AXTextArea desc=\"Prompt\"
  AXPopUpButton desc=\"Model: Fable 5.2 High · Ultracode\"
    AXStaticText value=\"Fable 5.2\"
    AXStaticText value=\" \"
    AXStaticText value=\"High\"
    AXStaticText value=\" · \"
    AXStaticText value=\"Ultracode\"
  AXGroup
    AXGroup title=\"Model: Fable 5.2 High · Ultracode\"
      AXGroup
        AXGroup/AXApplicationGroup
          AXMenuItem title=\"Opus 5.5\"
          AXMenuItem title=\"Fable 5.2\"
          AXMenuItem title=\"Sonnet 5.5\"
          AXMenuItem title=\"Haiku 5.5\"
        AXSplitter
        AXMenuItem title=\"Effort High · Ultracode\"
        AXSplitter
        AXMenuItem title=\"More models\"
        AXStaticText value=\"Fast mode\"
        AXGroup
          AXMenuItem title=\"Enable fast mode Requires usage credits Buy credits\"
            AXStaticText value=\"Enable fast mode\"
            AXStaticText value=\"Requires usage credits\"
            AXStaticText value=\"Buy credits\"
  AXPopUpButton title=\"Auto\"";

    #[test]
    fn claude_code_form_names_model_and_effort_and_offers_only_models() {
        let t = tree(CLAUDE_FORM);
        let model = picker(&t, Kind::Model).unwrap();
        assert_eq!(shown(&t, model), ["Fable 5.2", "High", "Ultracode"]);
        // Claude has no effort picker of its own: its effort is read only.
        assert!(matches!(
            picker(&t, Kind::Effort),
            Err(Error::Unavailable(_))
        ));
        let picker_ = chat::Picker {
            node: model,
            label: t.nodes[model].label().into(),
        };
        let panel = panel(&t, &picker_).unwrap().unwrap();
        let models = choices(&t, panel, Kind::Model).unwrap();
        let names: Vec<_> = models.iter().map(|c| c.name.as_str()).collect();
        assert_eq!(names, ["Opus 5.5", "Fable 5.2", "Sonnet 5.5", "Haiku 5.5"]);
        assert!(models.iter().all(|c| !c.selected));
        assert_eq!(submenu_effort(&t, panel).as_deref(), Some("High"));

        // The current model needs no press; another one does; an unknown
        // or paid toggle is refused with the stable wording.
        let shown = shown(&t, model);
        assert!(create_model_confirmed(&models, &shown, "Fable 5.2"));
        assert!(!create_model_confirmed(&models, &shown, "Opus 5.5"));
        assert_eq!(
            create_choice(&models, Kind::Model, "Opus 5.5")
                .unwrap()
                .unwrap()
                .name,
            "Opus 5.5"
        );
        for refused in ["Opus 9", "Enable fast mode", "opus 5.5"] {
            let Err(Error::Invalid(message)) = create_choice(&models, Kind::Model, refused) else {
                panic!("{refused} must be refused");
            };
            assert_eq!(
                message,
                format!(
                    "{CREATE_REFUSED}: model {refused:?} is not offered (the app offers \"Opus 5.5\", \"Fable 5.2\", \"Sonnet 5.5\", \"Haiku 5.5\"); nothing was sent"
                )
            );
        }
    }

    /// Claude 4.23118.1, read on a Mac: the focused "Effort <level>" entry
    /// opens, on the Right arrow key, a group titled like it. Its levels are
    /// one group of menu items whose titles carry badges; "Ultracode" is a
    /// toggle after them.
    #[test]
    fn claude_effort_levels_are_the_choices_in_the_effort_submenu() {
        let closed = tree(CLAUDE_FORM);
        let entry = "Effort High · Ultracode";
        assert!(effort_levels(&closed, entry).is_empty());
        let open = tree(&format!(
            "{CLAUDE_FORM}
  AXGroup title=\"{entry}\"
    AXGroup
      AXStaticText value=\"Higher effort means more thorough responses\"
      AXGroup/AXApplicationGroup
        AXMenuItem title=\"Low\"
        AXMenuItem title=\"Medium Recommended\"
        AXMenuItem title=\"High\"
        AXMenuItem title=\"Extra\"
        AXMenuItem title=\"Max 3.5×\"
        AXMenuItem title=\"Ultra\" enabled=\"0\"
      AXSplitter
      AXMenuItem title=\"Ultracode\""
        ));
        let levels = effort_levels(&open, entry);
        let names: Vec<_> = levels.iter().map(|c| c.name.as_str()).collect();
        assert_eq!(
            names,
            ["Low", "Medium Recommended", "High", "Extra", "Max 3.5×"]
        );
        // A level is named as the picker shows it, or by its whole title.
        for (name, title) in [
            ("Extra", "Extra"),
            ("Medium", "Medium Recommended"),
            ("Medium Recommended", "Medium Recommended"),
            ("Max", "Max 3.5×"),
        ] {
            let level = create_level(&levels, name).unwrap().unwrap();
            assert_eq!(level.name, title);
        }
        for refused in ["Ultracode", "Extra high", "Med", "Ultra"] {
            let Err(Error::Invalid(message)) = create_level(&levels, refused) else {
                panic!("{refused}")
            };
            assert_eq!(
                message,
                format!(
                    "{CREATE_REFUSED}: effort {refused:?} is not offered (the app offers \"Low\", \"Medium Recommended\", \"High\", \"Extra\", \"Max 3.5×\"); nothing was sent"
                )
            );
        }
        // The entry names the level without its badge, or its toggles.
        let model = picker(&closed, Kind::Model).unwrap();
        let panel_ = panel(
            &closed,
            &chat::Picker {
                node: model,
                label: closed.nodes[model].label().into(),
            },
        )
        .unwrap()
        .unwrap();
        assert_eq!(submenu_effort(&closed, panel_).as_deref(), Some("High"));
        assert!(level_confirmed(&levels, "Medium", "Medium"));
        assert!(level_confirmed(&levels, "Medium Recommended", "Medium"));
        assert!(level_confirmed(&levels, "Max", "Max"));
        assert!(!level_confirmed(&levels, "Max", "Extra"));
        assert!(!level_confirmed(&levels, "Medium Recommended", "Low"));
        // A badge never stands in for a level the app names exactly.
        let close = |names: &[&str]| -> Vec<ChatChoice> {
            names
                .iter()
                .enumerate()
                .map(|(node, name)| ChatChoice {
                    node,
                    name: (*name).into(),
                    detail: String::new(),
                    selected: false,
                })
                .collect()
        };
        let levels = close(&["Extra", "Extra High"]);
        assert_eq!(
            create_level(&levels, "Extra").unwrap().unwrap().name,
            "Extra"
        );
        assert!(!level_confirmed(&levels, "Extra High", "Extra"));
        let levels = close(&["Max 2×", "Max 3×"]);
        assert!(create_level(&levels, "Max").is_err());
        // A model without effort shows no entry.
        let t = tree("AXGroup title=\"Model: Haiku 5.5\"\n  AXMenuItem title=\"Haiku 5.5\"");
        assert_eq!(submenu_effort(&t, 0), None);
        let Error::Invalid(message) = create_no_effort("Max") else {
            panic!()
        };
        assert_eq!(
            message,
            "The new session's model and effort were not set: effort \"Max\" cannot be chosen: the app shows no effort for this model; nothing was sent"
        );
        let Error::Unavailable(message) = create_levels_closed("Max") else {
            panic!()
        };
        assert!(message.starts_with(CREATE_REFUSED) && message.ends_with("; nothing was sent"));
    }

    const POWER: [&str; 5] = ["Low", "Medium", "High", "Extra High", "Max"];

    /// The ChatGPT app 26.1103 as read on a Mac: each level is stated as its
    /// model and its slider name; its button reads "GPT-6 Astra High" at 3
    /// of 5.
    const ASTRA: [&str; 5] = [
        "GPT-6 Astra Light",
        "GPT-6 Astra Standard",
        "GPT-6 Astra Extended",
        "GPT-6 Astra Extra High",
        "GPT-6 Astra Ultra",
    ];

    /// Plays `walk` against a slider of `names` that starts at `start`
    /// (1-based) and moves one level per key; returns the keys and where it
    /// stopped.
    fn walk_names(
        names: &[&str],
        start: u32,
        target: &str,
    ) -> (Vec<PowerStep>, u32, Result<(), Error>) {
        let level = |p: u32| chat::PowerLevel {
            name: names[p as usize - 1].into(),
            position: p,
            total: names.len() as u32,
        };
        let mut position = start;
        let mut walk = PowerWalk::new(&level(start), target);
        let mut keys = Vec::new();
        loop {
            match walk.next(&level(position)) {
                Ok(Some(step)) => {
                    keys.push(step);
                    position = match step {
                        PowerStep::Lower => position - 1,
                        PowerStep::Higher => position + 1,
                    };
                }
                Ok(None) => return (keys, position, Ok(())),
                Err(error) => return (keys, position, Err(error)),
            }
        }
    }

    fn walk(start: u32, target: &str) -> (Vec<PowerStep>, u32, Result<(), Error>) {
        walk_names(&POWER, start, target)
    }

    #[test]
    fn power_walk_reaches_any_level_by_name() {
        use PowerStep::{Higher, Lower};
        assert_eq!(walk(3, "High").0, []);
        assert_eq!(walk(3, "Low").0, [Lower, Lower]);
        let (keys, at, done) = walk(3, "Extra High");
        assert_eq!((keys, at), (vec![Lower, Lower, Higher, Higher, Higher], 4));
        assert!(done.is_ok());
        let (keys, at, done) = walk(1, "Max");
        assert_eq!((keys.len(), at), (4, 5));
        assert!(done.is_ok());
    }

    #[test]
    fn power_walk_restores_the_start_before_refusing_an_unknown_name() {
        let (keys, at, done) = walk(3, "Middle");
        assert_eq!(at, 3, "walked back to the starting level: {keys:?}");
        let Err(Error::Invalid(message)) = done else {
            panic!()
        };
        assert_eq!(
            message,
            "The new session's model and effort were not set: effort \"Middle\" is not offered (the app offers \"Low\", \"Medium\", \"High\", \"Extra High\", \"Max\"); nothing was sent"
        );
    }

    #[test]
    fn power_walk_takes_the_app_name_or_the_name_without_the_shared_model() {
        use PowerStep::{Higher, Lower};
        // The app's own name stops where it is read.
        assert_eq!(walk_names(&ASTRA, 3, "GPT-6 Astra Light").0, [Lower, Lower]);
        assert_eq!(walk_names(&ASTRA, 3, "GPT-6 Astra Extended").0, []);
        // A name without the model is looked up after every level was read.
        let (keys, at, done) = walk_names(&ASTRA, 3, "Light");
        assert_eq!(
            keys,
            [
                Lower, Lower, Higher, Higher, Higher, Higher, Lower, Lower, Lower, Lower
            ]
        );
        assert_eq!(at, 1);
        assert!(done.is_ok());
        let (_, at, done) = walk_names(&ASTRA, 3, "Extended");
        assert_eq!(at, 3);
        assert!(done.is_ok());
        let (_, at, done) = walk_names(&ASTRA, 1, "Extra High");
        assert_eq!(at, 4);
        assert!(done.is_ok());
        // The button's words are not the slider's: "High" is not "Extra High".
        for name in ["High", "Max", "Astra Ultra", "GPT-6 Astra High", "Extra"] {
            let (keys, at, done) = walk_names(&ASTRA, 3, name);
            assert_eq!(at, 3, "walked back for {name:?}: {keys:?}");
            let Err(Error::Invalid(message)) = done else {
                panic!("{name:?} was accepted")
            };
            assert!(
                message.ends_with(
                    "(the app offers \"GPT-6 Astra Light\", \"GPT-6 Astra Standard\", \"GPT-6 Astra Extended\", \"GPT-6 Astra Extra High\", \"GPT-6 Astra Ultra\"); nothing was sent"
                ),
                "{message}"
            );
        }
    }

    #[test]
    fn power_names_drop_only_words_every_level_shares() {
        let seen = |names: &[&str]| -> BTreeMap<u32, String> {
            names
                .iter()
                .enumerate()
                .map(|(i, n)| (i as u32 + 1, n.to_string()))
                .collect()
        };
        assert_eq!(power_named(&seen(&ASTRA), "Ultra"), Some(5));
        assert_eq!(power_named(&seen(&ASTRA), "GPT-6 Astra Ultra"), Some(5));
        assert_eq!(power_named(&seen(&POWER), "Max"), Some(5));
        // Two models: only "GPT-6" is shared.
        let mixed = seen(&["GPT-6 Astra Light", "GPT-6 Sol Max"]);
        assert_eq!(power_named(&mixed, "Sol Max"), Some(2));
        assert_eq!(power_named(&mixed, "Max"), None);
        // A level is never named by nothing, and one level has no short name.
        assert_eq!(power_named(&seen(&["Max", "Max Extra"]), "Extra"), None);
        assert_eq!(power_named(&seen(&["GPT-6 Astra Ultra"]), "Max"), None);
        assert_eq!(power_named(&seen(&["A Max", "A Max"]), "Max"), None);
    }

    #[test]
    fn chatgpt_panel_is_found_under_the_renamed_button() {
        // Closed, the button reads "GPT-6 Astra High"; open, it and the
        // panel read "Select effort".
        let t = tree(
            "AXWebArea\n  AXPopUpButton title=\"Select effort\"\n  AXGroup\n    AXGroup title=\"Select effort\"\n      AXMenuItem title=\"Select model\"\n      AXMenuItem title=\"Reset to default\"\n      AXGroup/AXApplicationStatus\n        AXStaticText value=\"GPT-6 Astra Extended, 3 of 5.\"\n      AXMenuItem title=\"Power\"\n        AXGroup\n          AXGroup/AXEmptyGroup",
        );
        let opened = chat::Picker {
            node: 1,
            label: "GPT-6 Astra High".into(),
        };
        assert_eq!(panel(&t, &opened).unwrap(), None);
        let found = opened_panel(&t, &opened, Kind::Effort).unwrap().unwrap();
        assert_eq!(found, 3);
        let (level, _) = power(&t, found).unwrap();
        assert_eq!(
            (level.name.as_str(), level.position, level.total),
            ("GPT-6 Astra Extended", 3, 5)
        );
        assert_eq!(opened_panel(&t, &opened, Kind::Model).unwrap(), Some(3));
    }

    #[test]
    fn power_walk_stops_when_a_key_does_not_move_one_level() {
        let level = |name: &str, position| chat::PowerLevel {
            name: name.into(),
            position,
            total: 5,
        };
        let mut walk = PowerWalk::new(&level("High", 3), "Low");
        assert_eq!(
            walk.next(&level("High", 3)).unwrap(),
            Some(PowerStep::Lower)
        );
        let Err(Error::Unavailable(message)) = walk.next(&level("Low", 1)) else {
            panic!()
        };
        assert!(message.contains("moved from 3 to 1 of 5"), "{message}");
    }

    #[test]
    fn requested_names_are_short_exact_and_printable() {
        for good in ["", "Opus 5.5", "Extra high", "GPT-6.1 Sol"] {
            assert!(valid_name(good), "{good:?}");
        }
        for bad in [" Opus 5.5", "Opus 5.5 ", "Opus\n5.5", &"x".repeat(129)] {
            assert!(!valid_name(bad), "{bad:?}");
        }
    }
}
