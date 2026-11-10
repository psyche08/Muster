//! Interpret native coding-session model/effort menus. No model catalogue is
//! cached or invented: a write must match one enabled choice in the live menu.
use crate::{
    Error,
    desktop_chat::{self as chat, AxTree, ChatChoice},
};

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
/// effort (ADR 0177). Such a creation sent nothing, and asking again
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

/// The effort Claude's model panel names on its "Effort <level>" entry. Its
/// levels open on hover only, so this is read, never chosen.
pub fn submenu_effort(tree: &AxTree, panel: usize) -> Option<String> {
    let entries = chat::menu_entries(tree, panel);
    let entry = chat::effort_entry(&entries)?;
    let level = entry.label.strip_prefix("Effort")?.trim();
    (!level.is_empty()).then(|| level.to_string())
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
        Err(Error::Invalid(_)) => Err(create_refused(match kind {
            Kind::Model => format!(
                "model {name:?} is not offered (the app offers {})",
                offered(choices)
            ),
            // The ChatGPT app's Power slider: only its current level and
            // a raise to the middle are reachable without a pointer.
            Kind::Effort => format!(
                "effort {name:?} cannot be chosen through Accessibility (it can choose {}); choose it once in the app",
                offered(choices)
            ),
        })),
        Err(other) => Err(other),
    }
}

/// Whether a fresh read of the form shows `name`: the menu marks it
/// selected, or the picker names it first (Claude marks no menu entry).
pub fn create_model_confirmed(choices: &[ChatChoice], shown: &[String], name: &str) -> bool {
    shown.first().is_some_and(|first| first == name) || confirmed(choices, Kind::Model, name)
}

/// The refusal for an effort Claude's form cannot choose: its levels open
/// on hover only, which Accessibility cannot do.
pub fn create_effort_differs(name: &str, current: Option<&str>) -> Error {
    match current {
        Some(current) => create_refused(format!(
            "effort {name:?} cannot be chosen through Accessibility and the app's effort is {current:?}; choose it once in the app"
        )),
        None => create_refused(format!(
            "effort {name:?} cannot be chosen: the app shows no effort for this model"
        )),
    }
}

fn offered(choices: &[ChatChoice]) -> String {
    if choices.is_empty() {
        return "none".into();
    }
    choices
        .iter()
        .map(|c| format!("{:?}", c.name))
        .collect::<Vec<_>>()
        .join(", ")
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
        AXMenuItem title=\"Effort High\"
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
        assert!(matches!(picker(&t, Kind::Effort), Err(Error::Unavailable(_))));
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

    #[test]
    fn claude_effort_is_checked_not_chosen() {
        let Error::Invalid(message) = create_effort_differs("Max", Some("High")) else {
            panic!()
        };
        assert_eq!(
            message,
            "The new session's model and effort were not set: effort \"Max\" cannot be chosen through Accessibility and the app's effort is \"High\"; choose it once in the app; nothing was sent"
        );
        let Error::Invalid(message) = create_effort_differs("Max", None) else {
            panic!()
        };
        assert!(message.starts_with(CREATE_REFUSED) && message.ends_with("nothing was sent"));
        // A model without effort shows no entry.
        let t = tree("AXGroup title=\"Model: Haiku 5.5\"\n  AXMenuItem title=\"Haiku 5.5\"");
        assert_eq!(submenu_effort(&t, 0), None);
    }

    #[test]
    fn effort_choices_keep_the_current_power_level_without_a_press() {
        let t = tree(
            "AXWebArea\n  AXPopUpButton desc=\"Select effort\"\n  AXGroup title=\"Select effort\"\n    AXStaticText/AXApplicationStatus value=\"High, 3 of 5.\"\n    AXMenuItem title=\"Select model\"\n    AXMenuItem title=\"Power\"\n      AXGroup/AXEmptyGroup",
        );
        let efforts = choices(&t, 2, Kind::Effort).unwrap();
        assert!(create_choice(&efforts, Kind::Effort, "High").unwrap().is_none());
        let Err(Error::Invalid(message)) = create_choice(&efforts, Kind::Effort, "Extra High")
        else {
            panic!()
        };
        assert_eq!(
            message,
            "The new session's model and effort were not set: effort \"Extra High\" cannot be chosen through Accessibility (it can choose \"High\"); choose it once in the app; nothing was sent"
        );
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
