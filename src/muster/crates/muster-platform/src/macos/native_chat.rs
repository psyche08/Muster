//! macOS Accessibility adapter for shared desktop chat control.
pub(crate) use super::ax::Element;
pub(crate) use super::codesign::{observe_process_start, process_start};
pub(crate) use super::input::target_input_generation;
pub(crate) use super::session_window::root as window;
use super::{ax, codesign};
pub use crate::native_chat::{
    catalog, diagnostic, inspect_options, open, options, return_to_code, set_mode,
};
use muster_agent::Error;
use muster_agent::desktop_chat::{AxNode, AxTree, ChatApp};
use std::time::{Duration, Instant};
pub(crate) fn draft(app: ChatApp, element: &Element, text: &str) -> Result<(), ax::AXError> {
    super::native_app::draft(app.profile(), element, text)
}
const ATTRIBUTES: [&str; 14] = [
    "AXRole",
    "AXSubrole",
    "AXTitle",
    "AXDescription",
    "AXValue",
    "AXURL",
    "AXIdentifier",
    "AXDOMIdentifier",
    "AXPlaceholderValue",
    "AXEnabled",
    "AXSelected",
    "AXChildren",
    "AXLabel",
    "AXARIACurrent",
];
const SNAPSHOT_NODES: usize = 12_000;
const SNAPSHOT_TIME: Duration = Duration::from_secs(3);
const MAX_DEPTH: usize = 256;
/// The app's element tree in document order, bounded by nodes and time.
/// The menu bar is included only when asked (mode and menu fallbacks).
pub fn snapshot(root: &Element, menu_bar: bool) -> (AxTree, Vec<Element>) {
    snapshot_impl(root, menu_bar, false)
}

/// Permission controls require an explicit enabled flag; absent is unavailable.
pub fn approval_snapshot(root: &Element) -> (AxTree, Vec<Element>) {
    snapshot_impl(root, false, true)
}

fn snapshot_impl(root: &Element, menu_bar: bool, strict: bool) -> (AxTree, Vec<Element>) {
    let started = Instant::now();
    let mut tree = AxTree {
        nodes: Vec::new(),
        complete: true,
    };
    let mut elements = Vec::new();
    let mut stack: Vec<(Element, Option<usize>, usize)> = vec![(root.clone(), None, 0)];
    while let Some((element, parent, depth)) = stack.pop() {
        if tree.nodes.len() >= SNAPSHOT_NODES || started.elapsed() > SNAPSHOT_TIME {
            tree.complete = false;
            break;
        }
        let values = if strict {
            match element.try_attributes(&ATTRIBUTES) {
                Ok(values) => values,
                Err(_) => {
                    tree.complete = false;
                    break;
                }
            }
        } else {
            element.attributes(&ATTRIBUTES)
        };
        let text = |i: usize| {
            values[i]
                .as_ref()
                .and_then(ax::value_string)
                .unwrap_or_default()
        };
        let flag = |i: usize| values[i].as_ref().and_then(ax::value_bool);
        let role = text(0);
        let checkable = matches!(
            role.as_str(),
            "AXRadioButton"
                | "AXCheckBox"
                | "AXMenuItem"
                | "AXMenuItemRadio"
                | "AXMenuItemCheckBox"
                | "AXTab"
        );
        let identifier = Some(text(6))
            .filter(|s| !s.is_empty())
            .unwrap_or_else(|| text(7));
        let placeholder = Some(text(8))
            .filter(|s| !s.is_empty())
            .unwrap_or_else(|| text(12));
        let index = tree.nodes.len();
        tree.nodes.push(AxNode {
            subrole: text(1),
            title: text(2),
            description: text(3),
            value: text(4),
            url: text(5),
            identifier,
            placeholder,
            enabled: flag(9).unwrap_or(!strict),
            selected: flag(10).unwrap_or(false)
                || (checkable && flag(4) == Some(true))
                || !matches!(text(13).as_str(), "" | "false" | "0"),
            children: Vec::new(),
            parent,
            role,
        });
        if let Some(parent) = parent {
            tree.nodes[parent].children.push(index);
        }
        elements.push(element);
        let descend = depth < MAX_DEPTH && (menu_bar || tree.nodes[index].role != "AXMenuBar");
        if strict && depth >= MAX_DEPTH {
            tree.complete = false;
        }
        if descend && let Some(children) = values[11].as_ref() {
            for child in Element::elements_of(children).into_iter().rev() {
                stack.push((child, Some(index), depth + 1));
            }
        }
    }
    (tree, elements)
}

/// The app's main process, signature-checked, with its launch time.
pub(crate) fn process(app: ChatApp) -> Option<(u32, u64)> {
    let profile = app.profile();
    super::procs::pids_by_executable_suffix(profile.executable)
        .into_iter()
        .filter(|&pid| codesign::process_satisfies(pid, &profile.app.requirement()))
        .find_map(|pid| codesign::process_start(pid).map(|start| (pid, start)))
}

pub(crate) fn chat_window(pid: u32) -> Result<Element, Error> {
    let app = Element::application(pid);
    app.enable_manual_accessibility();
    super::session_window::root(&app)
}

pub(crate) fn console_ready() -> Result<(), Error> {
    if !ax::trusted() {
        return Err(Error::Unavailable(
            "Accessibility is not granted to musterd on this Mac".into(),
        ));
    }
    if super::lock::session_state()
        .is_none_or(|s| s.screen_locked || !s.login_done || !s.on_console)
    {
        return Err(Error::Unavailable(
            "the Mac is locked or not at its console; desktop chats need it unlocked".into(),
        ));
    }
    Ok(())
}
