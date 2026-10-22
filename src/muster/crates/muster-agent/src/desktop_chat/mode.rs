//! Which mode the app shows (Chat or its agent mode), what stands in the
//! way (settings, dialogs), and how to switch.

use super::*;
use crate::profile::ModeSwitch;

/// Which mode the app shows and how to reach Chat.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct ModeView {
    /// `Some(true)`: Chat mode is on screen; `None`: cannot tell.
    pub chat: Option<bool>,
    /// A control that switches to Chat mode when pressed.
    pub switch: Option<usize>,
    /// A control that opens the mode menu (ChatGPT); the Chat entry then
    /// appears as `menu_item` in the next snapshot.
    pub opener: Option<usize>,
    /// The app shows its settings instead of either mode: Claude's
    /// /settings pages, ChatGPT's settings with "Back to app".
    pub settings: bool,
}

pub fn mode(app: ChatApp, tree: &AxTree) -> ModeView {
    let mut view = ModeView::default();
    let modes = &app.controls().modes;
    match modes.switch {
        ModeSwitch::Radio {
            group,
            other_path,
            settings,
            ..
        } => {
            // The page itself says which mode it is. In the Claude app, Code
            // pages are /epitaxy/, Chat pages /chat/, /new, /recents,
            // /project/, and neither /settings/ (both modes open their
            // settings there).
            if let Some(area) = conversation_area(app, tree) {
                let url = &tree.nodes[area].url;
                let in_settings = url_path(url).is_some_and(|p| {
                    p.strip_prefix(settings)
                        .is_some_and(|rest| rest.is_empty() || rest.starts_with('/'))
                });
                if in_settings {
                    view.settings = true;
                } else {
                    view.chat = Some(!url.contains(other_path));
                }
            }
            if let Some(radio) = mode_radio(tree, group, modes.chat)
                && view.chat.is_none()
                && !view.settings
            {
                view.chat = Some(tree.nodes[radio].selected);
            }
            view.switch = mode_control(app, tree, true);
        }
        ModeSwitch::Menu { prefix } => {
            if settings_visible(app, tree) {
                view.settings = true;
                view.switch = mode_control(app, tree, true);
                return view;
            }
            for i in tree.preorder(0) {
                let node = &tree.nodes[i];
                if node.role != "AXPopUpButton" {
                    continue;
                }
                let label = node.label();
                if let Some(current) = label.strip_prefix(prefix) {
                    view.chat = Some(modes.chat.contains(&current.trim()));
                    view.opener = Some(i);
                    break;
                }
            }
            view.switch = mode_control(app, tree, true);
        }
    }
    view
}

/// What covers the page and must be left before either mode can be used.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Obstruction {
    /// A dialog over the page (an announcement, ChatGPT's settings), with
    /// the Close button that dismisses it without choosing anything.
    Dialog(usize),
    /// The unique settings exit: Back to app, or the declared shell Home.
    Settings(usize),
}

impl Obstruction {
    pub fn control(self) -> usize {
        match self {
            Obstruction::Dialog(i) | Obstruction::Settings(i) => i,
        }
    }

    pub fn what(self) -> &'static str {
        match self {
            Obstruction::Dialog(_) => "the dialog's Close button",
            Obstruction::Settings(_) => "the settings exit",
        }
    }
}

/// A dialog or settings page between the app and its modes, and the one
/// control that leaves it. Only the profile's close control dismisses a
/// dialog; a dialog without one is left alone (its other buttons choose
/// something: "Try … now", "Delete", "Continue"). Claude's settings pages
/// are left through its mode switch instead (Go › Claude / Code), so its
/// profile names no settings exit.
pub fn obstruction(app: ChatApp, tree: &AxTree) -> Option<Obstruction> {
    let controls = app.controls();
    let dialog = tree.preorder(0).into_iter().find_map(|i| {
        let node = &tree.nodes[i];
        if node.subrole != "AXApplicationDialog" || in_menu_bar(tree, i) {
            return None;
        }
        tree.preorder(i).into_iter().find(|&b| {
            let button = &tree.nodes[b];
            button.enabled && controls.close_dialog.matches(button)
        })
    });
    if let Some(close) = dialog {
        return Some(Obstruction::Dialog(close));
    }
    if tree
        .preorder(0)
        .into_iter()
        .any(|i| tree.nodes[i].subrole == "AXApplicationDialog" && !in_menu_bar(tree, i))
    {
        return None;
    }
    settings_exit(app, tree).map(Obstruction::Settings)
}

fn settings_visible(app: ChatApp, tree: &AxTree) -> bool {
    let controls = app.controls();
    tree.preorder(0).into_iter().any(|i| {
        let node = &tree.nodes[i];
        !in_menu_bar(tree, i)
            && (controls
                .leave_settings
                .as_ref()
                .is_some_and(|s| s.matches(node))
                || (node.subrole == "AXLandmarkNavigation"
                    && web_ancestor(tree, i).is_some()
                    && controls
                        .settings_navigation
                        .as_ref()
                        .is_some_and(|s| s.matches(node))))
    })
}

/// The control that leaves the app's settings page, when one is shown.
pub(super) fn settings_exit(app: ChatApp, tree: &AxTree) -> Option<usize> {
    if !tree.complete {
        return None;
    }
    let controls = app.controls();
    let leave = controls.leave_settings.as_ref()?;
    let exits: Vec<_> = tree
        .preorder(0)
        .into_iter()
        .filter(|&i| leave.matches(&tree.nodes[i]) && !in_menu_bar(tree, i))
        .collect();
    // A disabled or ambiguous explicit exit is not permission to choose Home.
    if !exits.is_empty() {
        return (exits.len() == 1 && tree.nodes[exits[0]].enabled).then_some(exits[0]);
    }
    let settings = settings_navigation(app, tree)?;
    let web = web_ancestor(tree, settings)?;
    let home = controls.settings_home.as_ref()?;
    let mut homes = tree.preorder(web).into_iter().filter(|&i| {
        let node = &tree.nodes[i];
        node.enabled
            && home.matches(node)
            && web_ancestor(tree, i) == Some(web)
            && tree
                .ancestors(i)
                .any(|a| tree.nodes[a].identifier == "app-shell-sidebar")
            && !tree.ancestors(i).any(|a| {
                a == settings
                    || tree.nodes[a].subrole == "AXApplicationDialog"
                    || tree.nodes[a].role == "AXMenuBar"
            })
    });
    let first = homes.next()?;
    homes.next().is_none().then_some(first)
}

fn web_ancestor(tree: &AxTree, i: usize) -> Option<usize> {
    tree.ancestors(i)
        .find(|&a| tree.nodes[a].role == "AXWebArea")
}

/// A title or transcript mentioning Settings is not a settings page.
fn settings_navigation(app: ChatApp, tree: &AxTree) -> Option<usize> {
    if !tree.complete {
        return None;
    }
    let selector = app.controls().settings_navigation.as_ref()?;
    let mut areas = tree.preorder(0).into_iter().filter(|&i| {
        let node = &tree.nodes[i];
        node.subrole == "AXLandmarkNavigation"
            && selector.matches(node)
            && web_ancestor(tree, i).is_some()
            && !in_menu_bar(tree, i)
    });
    let first = areas.next()?;
    areas.next().is_none().then_some(first)
}

/// A complete application/menu-only tree is not a ready renderer. It cannot
/// establish that the requested mode or session is absent. This only admits
/// observation; normal obstruction and exact-session checks still apply.
pub fn navigation_ready(app: ChatApp, tree: &AxTree) -> bool {
    tree.complete
        && (tree.nodes.iter().any(|n| n.role == "AXWebArea")
            || mode(app, tree).chat.is_some()
            || obstruction(app, tree).is_some())
}

/// A remembered mode behind settings or a modal is not a navigable page.
/// Unknown dialogs remain blockers even when they have no safe Close control.
pub fn navigation_clear(app: ChatApp, tree: &AxTree) -> bool {
    mode_switch_allowed(app, tree) && !mode(app, tree).settings
}

/// Claude leaves settings through its declared mode switch. Other profiles
/// require the settings exit first; no mode control bypasses a modal.
pub fn mode_switch_allowed(app: ChatApp, tree: &AxTree) -> bool {
    tree.complete
        && (!mode(app, tree).settings || app.controls().leave_settings.is_none())
        && !tree
            .preorder(0)
            .into_iter()
            .any(|i| tree.nodes[i].subrole == "AXApplicationDialog" && !in_menu_bar(tree, i))
}

pub(super) fn in_menu_bar(tree: &AxTree, i: usize) -> bool {
    tree.ancestors(i).any(|a| tree.nodes[a].role == "AXMenuBar")
}

/// What to press for Chat mode (`chat`) or the app's other mode (Code in
/// Claude, Codex in ChatGPT) when it is on screen now: Claude's Mode radio
/// (or Go › Claude / Code), or an entry of ChatGPT's open mode menu (open it
/// with `ModeView::opener` first).
pub fn mode_control(app: ChatApp, tree: &AxTree, chat: bool) -> Option<usize> {
    let modes = &app.controls().modes;
    let names = if chat { modes.chat } else { modes.other };
    match modes.switch {
        ModeSwitch::Radio { group, menu, .. } => {
            mode_radio(tree, group, names).or_else(|| menu_bar_item(tree, menu, names))
        }
        ModeSwitch::Menu { prefix } => names
            .iter()
            .find_map(|name| mode_menu_entry(tree, prefix, name)),
    }
}

pub(super) fn mode_radio(tree: &AxTree, group: &str, names: &[&str]) -> Option<usize> {
    let group = tree.preorder(0).into_iter().find(|&i| {
        let n = &tree.nodes[i];
        n.role == "AXRadioGroup" && n.label() == group
    })?;
    tree.nodes[group].children.iter().copied().find(|&radio| {
        let r = &tree.nodes[radio];
        // A radio can carry a status after its name ("Code, working").
        let name = r.label().split(',').next().unwrap_or("").trim();
        r.role == "AXRadioButton" && names.contains(&name)
    })
}

/// An entry of the open mode menu, by mode name. The menu is titled like its
/// opener (ChatGPT: "Switch mode, current mode: …") and each entry's title
/// is the mode name followed by a description ("ChatGPT Create, learn, and
/// explore"), so only the first word names the mode. Menus elsewhere (the
/// menu bar's recent items, the window list) never match.
pub fn mode_menu_entry(tree: &AxTree, prefix: &str, name: &str) -> Option<usize> {
    let menu = tree.preorder(0).into_iter().find(|&i| {
        let node = &tree.nodes[i];
        node.role == "AXMenu"
            && node.label().starts_with(prefix)
            && !tree.ancestors(i).any(|a| tree.nodes[a].role == "AXMenuBar")
    })?;
    let mut entries = tree.preorder(menu).into_iter().filter(|&i| {
        let node = &tree.nodes[i];
        matches!(
            node.role.as_str(),
            "AXMenuItem" | "AXMenuItemRadio" | "AXMenuItemCheckBox"
        ) && tree.text_of(i).split_whitespace().next() == Some(name)
    });
    let entry = entries.next()?;
    entries.next().is_none().then_some(entry)
}

/// A command of one of the app's own top-level menus (never the Apple menu
/// with its recent items).
pub fn menu_bar_item(tree: &AxTree, menu: &str, titles: &[&str]) -> Option<usize> {
    let bar = tree
        .preorder(0)
        .into_iter()
        .find(|&i| tree.nodes[i].role == "AXMenuBar")?;
    let top = tree.nodes[bar]
        .children
        .iter()
        .copied()
        .find(|&i| tree.nodes[i].title == menu)?;
    tree.preorder(top).into_iter().find(|&i| {
        tree.nodes[i].role == "AXMenuItem" && titles.contains(&tree.nodes[i].title.as_str())
    })
}

/// A control that opens the empty new-conversation page.
pub fn new_chat_control(app: ChatApp, tree: &AxTree) -> Option<usize> {
    let web = conversation_area(app, tree)?;
    tree.preorder(web).into_iter().find(|&i| {
        let node = &tree.nodes[i];
        let controls = app.controls();
        let named = node.enabled && controls.new_chat.matches(node);
        // claude.ai's New chat is a link to /new; the Claude app's sidebar
        // shows a plain "New" button.
        let linked = controls.url.new.is_some_and(|new| {
            node.role == "AXLink" && url_path(&node.url).is_some_and(|p| p == new)
        });
        let sidebar_new = controls
            .sidebar_new
            .as_ref()
            .is_some_and(|new| node.enabled && new.matches(node) && in_sidebar(tree, i));
        named || linked || sidebar_new
    })
}

pub(super) fn in_sidebar(tree: &AxTree, i: usize) -> bool {
    tree.ancestors(i).any(|a| {
        let n = &tree.nodes[a];
        matches!(
            n.subrole.as_str(),
            "AXLandmarkNavigation" | "AXLandmarkComplementary"
        ) || n.identifier == "app-shell-sidebar"
    })
}

#[cfg(test)]
mod settings_tests {
    use super::*;
    use crate::desktop_chat::tests::tree;

    #[test]
    fn application_menu_only_is_not_a_ready_navigation_snapshot() {
        let menu_only = tree(
            r#"AXApplication title="Claude"
  AXMenuBar
    AXMenuBarItem title="Go"
      AXMenu
        AXMenuItem title="Claude"
        AXMenuItem title="Code""#,
        );
        assert!(!navigation_ready(ChatApp::Claude, &menu_only));
        let mut page = tree(
            r#"AXApplication title="Claude"
  AXWindow
    AXWebArea url="https://claude.ai/epitaxy/local_test""#,
        );
        assert!(navigation_ready(ChatApp::Claude, &page));
        page.complete = false;
        assert!(!navigation_ready(ChatApp::Claude, &page));
    }

    // Synthetic structure based on Mac-C's failure and the installed app's
    // Settings navigation / shell Home controls; not a captured AX fixture.
    const SETTINGS: &str = r#"AXApplication title="ChatGPT"
  AXWindow/AXStandardWindow
    AXWebArea title="Settings" url="app://-/index.html"
      AXGroup id="app-shell-sidebar"
        AXButton title="Home"
        AXGroup/AXLandmarkNavigation desc="Settings"
          AXButton title="General"
          AXButton title="Notifications"
      AXPopUpButton title="Switch mode, current mode: Codex""#;

    #[test]
    fn shell_home_recovers_settings_despite_remembered_codex_mode() {
        let t = tree(SETTINGS);
        assert!(mode(ChatApp::ChatGpt, &t).settings);
        assert_eq!(mode(ChatApp::ChatGpt, &t).chat, None);
        assert!(!navigation_clear(ChatApp::ChatGpt, &t));
        let exit = obstruction(ChatApp::ChatGpt, &t).unwrap();
        assert!(matches!(exit, Obstruction::Settings(_)));
        assert_eq!(t.nodes[exit.control()].title, "Home");
        let returned = tree(&SETTINGS.replace("desc=\"Settings\"", "desc=\"Tasks\""));
        assert!(navigation_clear(ChatApp::ChatGpt, &returned));
        assert_eq!(obstruction(ChatApp::ChatGpt, &returned), None);
    }

    #[test]
    fn settings_exit_prefers_back_and_refuses_disabled_or_duplicate_controls() {
        let mut t = tree(SETTINGS);
        let home = settings_exit(ChatApp::ChatGpt, &t).unwrap();
        t.nodes[home].enabled = false;
        assert_eq!(settings_exit(ChatApp::ChatGpt, &t), None);
        let explicit = SETTINGS.replace(
            "AXButton title=\"Home\"",
            "AXButton title=\"Home\"\n        AXLink title=\"Back to app\"",
        );
        let mut t = tree(&explicit);
        let back = settings_exit(ChatApp::ChatGpt, &t).unwrap();
        assert_eq!(t.nodes[back].title, "Back to app");
        t.nodes[back].enabled = false;
        assert_eq!(settings_exit(ChatApp::ChatGpt, &t), None);
        for dump in [
            SETTINGS.replace(
                "AXButton title=\"Home\"",
                "AXButton title=\"Home\"\n        AXButton title=\"Home\"",
            ),
            explicit.replace(
                "AXLink title=\"Back to app\"",
                "AXLink title=\"Back to app\"\n        AXLink title=\"Back to app\"",
            ),
        ] {
            let t = tree(&dump);
            assert_eq!(settings_exit(ChatApp::ChatGpt, &t), None);
            assert!(!navigation_clear(ChatApp::ChatGpt, &t));
        }
    }

    #[test]
    fn settings_home_requires_landmark_same_web_area_and_complete_read() {
        for dump in [
            SETTINGS.replace("AXGroup/AXLandmarkNavigation", "AXGroup"),
            SETTINGS.replace("id=\"app-shell-sidebar\"", "id=\"unrelated\""),
            SETTINGS.replace("desc=\"Settings\"", "desc=\"Settings for project\""),
            SETTINGS.replace("        AXButton title=\"Home\"\n", "")
                + "\n      AXButton title=\"Home\"",
            SETTINGS.replace("        AXButton title=\"Home\"\n", "")
                + "\n    AXWebArea\n      AXGroup id=\"app-shell-sidebar\"\n        AXButton title=\"Home\"",
        ] {
            assert_eq!(
                settings_exit(ChatApp::ChatGpt, &tree(&dump)),
                None,
                "{dump}"
            );
        }
        let mut t = tree(SETTINGS);
        t.complete = false;
        assert_eq!(settings_exit(ChatApp::ChatGpt, &t), None);
        assert!(!navigation_clear(ChatApp::ChatGpt, &t));
        assert_eq!(settings_exit(ChatApp::Claude, &tree(SETTINGS)), None);
    }

    #[test]
    fn modal_blocks_background_mode_and_only_its_close_is_used() {
        let dump = format!(
            "{SETTINGS}\n      AXGroup/AXApplicationDialog title=\"Confirm\"\n        AXButton title=\"Delete\""
        );
        let t = tree(&dump);
        assert!(!navigation_clear(ChatApp::ChatGpt, &t));
        assert_eq!(obstruction(ChatApp::ChatGpt, &t), None);
        let close = tree(&(dump + "\n        AXButton title=\"Close\""));
        let exit = obstruction(ChatApp::ChatGpt, &close).unwrap();
        assert!(matches!(exit, Obstruction::Dialog(_)));
        assert_eq!(close.nodes[exit.control()].title, "Close");
    }
}
