//! Desktop Chat mode: the ordinary conversations of the Claude app ("Claude"
//! mode) and the ChatGPT app (ChatGPT mode). They have no transcript on
//! disk, so the platform snapshots the app's Accessibility tree and this
//! module interprets it: which mode the app shows, what its sidebar lists,
//! which conversation is on screen and what it says. Nothing here touches
//! the OS; the platform presses the node indices it is given and re-checks
//! them against a fresh snapshot before every write.
//!
//! Every rule that depends on an app's UI lives in this module, so a changed
//! app build is calibrated here (see `muster debug ax-chat`); what names a
//! control is the app's profile (`crate::profile`).
//!
//! The interpreters are one submodule each; this file re-exports them so
//! callers see one `desktop_chat` namespace, and holds the tests.

mod app;
mod conversation;
mod history;
mod mode;
mod pickers;
mod transcript;
mod tree;

pub use app::*;
pub use conversation::*;
pub use history::*;
pub use mode::*;
pub use pickers::*;
pub use transcript::*;
pub use tree::*;

#[cfg(test)]
pub(crate) mod tests {
    use super::*;
    use crate::model::Role;

    /// Desktop locks are keyed by these names across daemon releases.
    #[test]
    fn chat_app_names_stay_the_lock_keys() {
        assert_eq!(ChatApp::Claude.name(), "Claude");
        assert_eq!(ChatApp::ChatGpt.name(), "ChatGPT");
    }

    /// Builds a tree from `muster debug ax-dump`-style lines: two spaces of
    /// indentation per level, `Role/Subrole`, then key="value" pairs
    /// (title, desc, value, url, id, sel, disabled).
    pub fn tree(dump: &str) -> AxTree {
        let mut tree = AxTree {
            nodes: Vec::new(),
            complete: true,
        };
        let mut stack: Vec<(usize, usize)> = Vec::new();
        for line in dump.lines().filter(|l| !l.trim().is_empty()) {
            let depth = (line.len() - line.trim_start().len()) / 2;
            let body = line.trim_start();
            let (head, mut rest) = body.split_once(' ').unwrap_or((body, ""));
            let (role, subrole) = head.split_once('/').unwrap_or((head, ""));
            let mut node = AxNode {
                role: role.into(),
                subrole: subrole.into(),
                enabled: true,
                ..Default::default()
            };
            while let Some(eq) = rest.find("=\"") {
                let key = rest[..eq].trim();
                let after = &rest[eq + 2..];
                let end = after.find('"').unwrap();
                let value = after[..end].to_string();
                match key {
                    "title" => node.title = value,
                    "desc" => node.description = value,
                    "value" => node.value = value,
                    "url" => node.url = value,
                    "id" => node.identifier = value,
                    "label" | "ph" => node.placeholder = value,
                    "sel" => node.selected = value == "1",
                    "enabled" => node.enabled = value == "1",
                    _ => {}
                }
                rest = &after[end + 1..];
            }
            while stack.last().is_some_and(|(d, _)| *d >= depth) {
                stack.pop();
            }
            let index = tree.nodes.len();
            if let Some((_, parent)) = stack.last() {
                node.parent = Some(*parent);
                tree.nodes[*parent].children.push(index);
            }
            tree.nodes.push(node);
            stack.push((depth, index));
        }
        tree
    }

    #[test]
    fn full_history_uses_ids_and_never_collects_transcript_links() {
        let dump = r#"AXApplication
  AXWebArea url="https://claude.ai/chats"
    AXGroup/AXLandmarkMain
      AXTable
        AXRow
          AXLink desc="Same title" url="https://claude.ai/chat/aaaaaaaa-bbbb-4ccc-8ddd-eeeeffff0001"
        AXRow
          AXLink desc="Same title" url="https://claude.ai/chat/aaaaaaaa-bbbb-4ccc-8ddd-eeeeffff0002"
        AXRow
          AXLink desc="Cowork task" url="https://claude.ai/cowork/task-123"
"#;
        let t = tree(dump);
        assert!(sidebar(ChatApp::Claude, &t).is_empty());
        let all = history_entries(ChatApp::Claude, &t, true);
        assert_eq!(all.len(), 2);
        assert_ne!(all[0].key, all[1].key);
        assert!(history_exhausted(ChatApp::Claude, &t));
        let transcript = tree(&dump.replacen(
            "https://claude.ai/chats",
            "https://claude.ai/chat/aaaaaaaa-bbbb-4ccc-8ddd-eeeeffff0003",
            1,
        ));
        assert!(history_entries(ChatApp::Claude, &transcript, true).is_empty());
        assert!(!history_exhausted(ChatApp::Claude, &transcript));
        let loading = tree(
            r#"AXApplication
  AXWebArea url="https://claude.ai/chats"
    AXGroup/AXLandmarkComplementary
      AXGroup
        AXButton title="Same title"
        AXPopUpButton desc="More options for Same title"
    AXGroup/AXLandmarkMain
      AXProgressIndicator
"#,
        );
        assert_eq!(sidebar(ChatApp::Claude, &loading).len(), 1);
        assert!(history_entries(ChatApp::Claude, &loading, true).is_empty());
        assert!(!history_exhausted(ChatApp::Claude, &loading));
    }
    #[test]
    fn drains_multiple_history_lists_but_ignores_message_buttons() {
        let t = tree(
            r#"AXApplication
  AXWebArea url="app://-/index.html"
    AXGroup id="app-shell-sidebar"
      AXGroup/AXLandmarkNavigation
        AXList
          AXButton title="Show more"
        AXList
          AXButton title="Show more"
    AXGroup/AXLandmarkMain
      AXButton title="Show more"
"#,
        );
        assert_eq!(history_more(ChatApp::ChatGpt, &t), Some(5));
        let mut loading = t.clone();
        loading.nodes[5].enabled = false;
        assert_eq!(history_more(ChatApp::ChatGpt, &loading), Some(7));
        assert!(history_loading(ChatApp::ChatGpt, &loading));
        loading.complete = false;
        assert!(!history_exhausted(ChatApp::ChatGpt, &loading));
    }

    fn line(role: Role, text: &str) -> ChatLine {
        ChatLine {
            role,
            text: text.into(),
        }
    }

    // The ChatGPT app's page, abridged: history rows as buttons in the
    // navigation landmark, turns separated by "You said:" / "ChatGPT said:".
    const CHATGPT: &str = r#"
AXApplication title="ChatGPT"
  AXWindow/AXStandardWindow title="ChatGPT"
    AXGroup
      AXWebArea title="Trip planning" url="app://-/index.html"
        AXGroup id="root"
          AXGroup/AXLandmarkComplementary
            AXGroup id="app-shell-sidebar"
              AXGroup/AXLandmarkNavigation title="Home" desc="Home"
                AXPopUpButton title="Switch mode, current mode: ChatGPT" desc="Switch mode, current mode: ChatGPT"
                AXButton title="New chat"
                  AXButton title="Quick chat" desc="Quick chat"
                AXList/AXContentList
                  AXGroup
                    AXButton title="Life"
                      AXPopUpButton title="Project actions for Life"
                      AXButton title="New local chat in Life"
                  AXGroup title="Kyoto" desc="Kyoto"
                    AXButton title="Kyoto"
                      AXPopUpButton title="Project actions for Kyoto"
                    AXList/AXContentList title="Scheduled tasks in Kyoto"
                      AXGroup/AXApplicationGroup
                        AXButton title="Trip planning" desc="Trip planning"
                          AXPopUpButton title="Chat actions"
                AXList/AXContentList
                  AXGroup/AXApplicationGroup
                    AXButton title="Trip planning" desc="Trip planning"
                      AXButton title="Pin chat" desc="Pin chat"
                      AXButton title="Archive chat" desc="Archive chat"
                  AXGroup/AXApplicationGroup
                    AXButton title="Rust lifetimes" desc="Rust lifetimes"
                      AXButton title="Pin chat" desc="Pin chat"
                  AXGroup
                    AXButton title="Twice" desc="Twice"
                      AXPopUpButton title="Chat actions"
                  AXGroup
                    AXButton title="Twice" desc="Twice"
                      AXPopUpButton title="Chat actions"
          AXGroup/AXLandmarkMain
            AXGroup
              AXSplitter title="Today 2:18 PM"
              AXHeading title="You said:"
                AXStaticText value="You said:"
              AXGroup
                AXStaticText value="Plan three days in Kyoto"
              AXStaticText value="2:18 PM"
              AXGroup
                AXButton title="Copy message"
              AXHeading title="ChatGPT said:"
                AXStaticText value="ChatGPT said:"
              AXGroup/AXApplicationStatus
              AXGroup
                AXStaticText value="Here is a plan with "
                AXGroup/AXStrongStyleGroup
                  AXStaticText value="three"
                AXStaticText value=" days."
              AXList/AXContentList
                AXGroup
                  AXListMarker value="• "
                  AXStaticText value="Day 1: temples"
                AXGroup
                  AXListMarker value="• "
                  AXStaticText value="Day 2: food"
              AXHeading title="Notes"
                AXStaticText value="Notes"
              AXGroup
                AXStaticText value="Book early."
              AXGroup
                AXButton title="Copy"
              AXGroup
                AXStaticText value="ChatGPT can make mistakes. Check important info."
              AXHeading title="Latest response"
                AXStaticText value="Latest response"
            AXGroup
              AXTextArea label="Write in Markdown…" value="notes"
            AXGroup
              AXTextArea desc="Ask anything" value=""
              AXButton desc="Send prompt" enabled="0"
"#;

    // Claude.app in Chat mode: the claude.ai page inside the shell page;
    // a user turn is an article with a "You said:" heading, the reply
    // follows it.
    const CLAUDE: &str = r#"
AXApplication title="Claude"
  AXWindow/AXStandardWindow title="Claude"
    AXGroup
      AXWebArea url="file:///Applications/Claude.app/Contents/Resources/app.asar/index.html"
        AXWebArea title="Kyoto trip - Claude" url="https://claude.ai/chat/0f0e0d0c-1111-4222-8333-444455556666"
          AXGroup id="root"
            AXGroup/AXLandmarkComplementary desc="Sidebar"
              AXRadioGroup desc="Mode"
                AXRadioButton desc="Claude" sel="1"
                AXRadioButton desc="Code"
              AXLink title="New chat" url="https://claude.ai/new"
              AXLink url="https://claude.ai/chat/0f0e0d0c-1111-4222-8333-444455556666"
                AXStaticText value="Kyoto trip"
              AXLink url="https://claude.ai/chat/aaaaaaaa-bbbb-4ccc-8ddd-eeeeffff0000"
                AXStaticText value="Borrow checker"
            AXGroup/AXLandmarkMain
              AXGroup desc="Chat messages"
                AXGroup
                  AXStaticText value="Use the up and down arrow keys to move between messages."
                AXGroup/AXDocumentArticle desc="Message 1"
                  AXHeading title="You said: Plan three days in Kyoto"
                    AXStaticText value="You said: Plan three days in Kyoto"
                  AXGroup
                    AXStaticText value="Plan three days in Kyoto"
                  AXButton title="Show message actions"
                AXGroup
                  AXGroup
                    AXStaticText value="Day one: "
                    AXGroup/AXCodeStyleGroup
                      AXStaticText value="Fushimi Inari"
                    AXStaticText value="."
                AXGroup
                  AXStaticText value="Day two: Arashiyama."
                AXGroup
                  AXStaticText value="Claude is AI and can make mistakes. Please double-check responses."
              AXGroup/AXApplicationStatus
                AXStaticText value="Claude is responding"
              AXTextArea desc="Write your prompt to Claude" value=""
              AXButton desc="Stop response"
"#;

    #[test]
    fn chatgpt_page_yields_mode_sidebar_and_turns() {
        let tree = tree(CHATGPT);
        let mode = mode(ChatApp::ChatGpt, &tree);
        assert_eq!(mode.chat, Some(true));
        assert!(mode.opener.is_some());
        let chats = sidebar(ChatApp::ChatGpt, &tree);
        assert_eq!(
            chats.iter().map(|c| c.title.as_str()).collect::<Vec<_>>(),
            ["Trip planning", "Rust lifetimes"]
        );
        assert_eq!(chats[0].key, ChatKey::Title("Trip planning".into()));
        let new = new_chat_control(ChatApp::ChatGpt, &tree).unwrap();
        assert_eq!(tree.nodes[new].title, "New chat");
        assert!(composer_chat_toggle(ChatApp::ChatGpt, &tree).is_none());
        let c = conversation(ChatApp::ChatGpt, &tree).unwrap();
        assert_eq!(c.key, ChatKey::Title("Trip planning".into()));
        assert_eq!(c.composer.len(), 1);
        let composer = &tree.nodes[c.composer[0]];
        assert!(is_empty_draft(composer, "\nAsk anything"));
        assert!(!is_empty_draft(composer, "hello"));
        assert_eq!(c.send.len(), 1);
        assert!(c.stop.is_none());
        assert_eq!(
            c.lines,
            vec![
                line(Role::User, "Plan three days in Kyoto"),
                line(
                    Role::Assistant,
                    "Here is a plan with three days.\n\n- Day 1: temples\n- Day 2: food\n\n**Notes**\n\nBook early."
                ),
            ]
        );
    }

    #[test]
    fn claude_page_yields_ids_from_urls_and_reply_after_user_article() {
        let tree = tree(CLAUDE);
        let mode = mode(ChatApp::Claude, &tree);
        assert_eq!(mode.chat, Some(true));
        assert!(mode.switch.is_some());
        let chats = sidebar(ChatApp::Claude, &tree);
        assert_eq!(chats.len(), 2);
        assert_eq!(
            chats[1].key,
            ChatKey::Id("aaaaaaaa-bbbb-4ccc-8ddd-eeeeffff0000".into())
        );
        assert_eq!(chats[1].title, "Borrow checker");
        let new = new_chat_control(ChatApp::Claude, &tree).unwrap();
        assert_eq!(tree.nodes[new].url, "https://claude.ai/new");
        let c = conversation(ChatApp::Claude, &tree).unwrap();
        assert_eq!(
            c.key,
            ChatKey::Id("0f0e0d0c-1111-4222-8333-444455556666".into())
        );
        assert_eq!(c.title, "Kyoto trip");
        assert!(c.stop.is_some());
        assert_eq!(
            c.lines,
            vec![
                line(Role::User, "Plan three days in Kyoto"),
                line(
                    Role::Assistant,
                    "Day one: Fushimi Inari.\n\nDay two: Arashiyama."
                ),
            ]
        );
    }

    // Claude.app 2.9939 in Chat mode, as observed: a "New" button, recents
    // as title buttons with their own options menus, no conversation links.
    const CLAUDE_APP_SIDEBAR: &str = r#"
AXApplication title="Claude"
  AXWindow/AXStandardWindow title="Claude"
    AXWebArea title="New session - Claude" url="https://claude.ai/new"
      AXGroup/AXLandmarkComplementary desc="Sidebar"
        AXRadioGroup desc="Mode"
          AXRadioButton desc="Claude" sel="1"
          AXRadioButton desc="Code, working"
        AXGroup id="frame-peek-popover"
          AXButton title="New"
          AXButton title="Projects"
          AXGroup
            AXButton title="Claude site access"
              AXStaticText value="Claude site access"
            AXGroup
              AXPopUpButton desc="More options for Claude site access"
          AXGroup
            AXButton title="dnsmasq"
            AXGroup
              AXPopUpButton desc="More options for dnsmasq"
      AXGroup/AXLandmarkMain
        AXTextArea desc="Write your prompt to Claude"
        AXPopUpButton desc="Model: Opus 5.5 Medium"
"#;

    #[test]
    fn claude_app_sidebar_rows_new_button_and_model_picker() {
        let tree = tree(CLAUDE_APP_SIDEBAR);
        let chats = sidebar(ChatApp::Claude, &tree);
        assert_eq!(
            chats.iter().map(|c| c.title.as_str()).collect::<Vec<_>>(),
            ["Claude site access", "dnsmasq"]
        );
        let new = new_chat_control(ChatApp::Claude, &tree).unwrap();
        assert_eq!(tree.nodes[new].title, "New");
        let c = conversation(ChatApp::Claude, &tree).unwrap();
        assert_eq!(c.key, ChatKey::New);
        assert_eq!(c.title, "");
        let pickers = composer_pickers(ChatApp::Claude, &tree);
        assert_eq!(pickers.len(), 1);
        assert_eq!(pickers[0].label, "Model: Opus 5.5 Medium");
        // "Code, working" still names the Code mode.
        assert!(mode_control(ChatApp::Claude, &tree, false).is_some());
    }

    #[test]
    fn code_mode_is_not_chat_and_offers_the_switch() {
        let code = CLAUDE
            .replace(
                "https://claude.ai/chat/0f0e0d0c-1111-4222-8333-444455556666\"\n          AXGroup id=\"root\"",
                "https://claude.ai/epitaxy/local_0f0e0d0c-1111-4222-8333-444455556666\"\n          AXGroup id=\"root\"",
            )
            .replace("AXRadioButton desc=\"Claude\" sel=\"1\"", "AXRadioButton desc=\"Claude\"");
        let claude = tree(&code);
        let view = mode(ChatApp::Claude, &claude);
        assert_eq!(view.chat, Some(false));
        let switch = view.switch.unwrap();
        assert_eq!(claude.nodes[switch].description, "Claude");
        let code = mode_control(ChatApp::Claude, &claude, false).unwrap();
        assert_eq!(claude.nodes[code].description, "Code");
        assert!(conversation(ChatApp::Claude, &claude).is_none());

        let codex = tree(&CHATGPT.replace("current mode: ChatGPT", "current mode: Codex"));
        assert_eq!(mode(ChatApp::ChatGpt, &codex).chat, Some(false));
    }

    #[test]
    fn settings_pages_and_dialogs_stand_between_the_app_and_its_modes() {
        // Claude opens its settings as a claude.ai page in either mode:
        // neither mode is on screen, and Go › Code / Claude leaves it.
        let settings = CLAUDE.replace(
            "https://claude.ai/chat/0f0e0d0c-1111-4222-8333-444455556666\"\n          AXGroup id=\"root\"",
            "https://claude.ai/settings/claude-code\"\n          AXGroup id=\"root\"",
        );
        let claude = tree(&settings);
        let view = mode(ChatApp::Claude, &claude);
        assert!(view.settings);
        assert_eq!(view.chat, None);
        assert!(mode_control(ChatApp::Claude, &claude, false).is_some());
        assert_eq!(obstruction(ChatApp::Claude, &claude), None);
        assert!(mode_switch_allowed(ChatApp::Claude, &claude));
        assert!(!navigation_clear(ChatApp::Claude, &claude));

        // An announcement over ChatGPT hides the page; only its Close
        // button is pressed, never the choices it offers.
        let dialog = tree(
            r#"AXApplication title="ChatGPT"
  AXWindow/AXStandardWindow title="ChatGPT"
    AXGroup
      AXWebArea title="ChatGPT" url="app://-/index.html"
        AXGroup/AXApplicationDialog title="Introducing GPT-6.1 Sol"
          AXGroup
            AXImage title="GPT-6.1 Sol"
            AXButton title="Close" desc="Close"
          AXButton title="Continue with current model"
          AXButton title="Try GPT-6.1 Sol now""#,
        );
        let view = mode(ChatApp::ChatGpt, &dialog);
        assert_eq!(view.chat, None);
        let o = obstruction(ChatApp::ChatGpt, &dialog).unwrap();
        assert!(matches!(o, Obstruction::Dialog(_)));
        assert_eq!(dialog.nodes[o.control()].title, "Close");

        // A dialog without a Close button is not ours to answer.
        let choice = tree(
            r#"AXApplication title="ChatGPT"
  AXWindow/AXStandardWindow title="ChatGPT"
    AXWebArea url="app://-/index.html"
      AXGroup/AXApplicationDialog title="Delete chat?"
        AXButton title="Cancel"
        AXButton title="Delete""#,
        );
        assert_eq!(obstruction(ChatApp::ChatGpt, &choice), None);

        // ChatGPT's (Codex) settings page returns through "Back to app".
        let page = tree(
            r#"AXApplication title="ChatGPT"
  AXWindow/AXStandardWindow title="ChatGPT"
    AXWebArea url="app://-/index.html"
      AXGroup
        AXButton title="Back to app"
        AXHeading title="General"
  AXMenuBar
    AXMenuBarItem title="ChatGPT"
      AXMenu
        AXMenuItem title="Settings…""#,
        );
        let view = mode(ChatApp::ChatGpt, &page);
        assert!(view.settings);
        assert_eq!(view.chat, None);
        let o = obstruction(ChatApp::ChatGpt, &page).unwrap();
        assert!(matches!(o, Obstruction::Settings(_)));
        assert_eq!(page.nodes[o.control()].title, "Back to app");
    }

    #[test]
    fn new_conversation_pages() {
        let fresh = CLAUDE.replace(
            "https://claude.ai/chat/0f0e0d0c-1111-4222-8333-444455556666\"\n          AXGroup id=\"root\"",
            "https://claude.ai/new\"\n          AXGroup id=\"root\"",
        );
        let c = conversation(ChatApp::Claude, &tree(&fresh)).unwrap();
        assert_eq!(c.key, ChatKey::New);

        let blank = r#"
AXApplication title="ChatGPT"
  AXWindow
    AXWebArea title="ChatGPT" url="app://-/index.html"
      AXGroup/AXLandmarkMain
        AXGroup/AXApplicationGroup title="Composer mode" desc="Composer mode"
          AXCheckBox/AXToggleButton title="Chat"
          AXCheckBox/AXToggleButton title="Work" sel="1"
        AXTextArea title="Work with ChatGPT" desc="Work with ChatGPT"
"#;
        let blank = tree(blank);
        let c = conversation(ChatApp::ChatGpt, &blank).unwrap();
        assert_eq!(c.key, ChatKey::New);
        assert_eq!(c.title, "");
        let (toggle, on) = composer_chat_toggle(ChatApp::ChatGpt, &blank).unwrap();
        assert_eq!(blank.nodes[toggle].title, "Chat");
        assert!(!on);
    }

    #[test]
    fn chatgpt_current_link_names_a_page_without_an_id_in_its_url() {
        let linked = CHATGPT
            .replace(
                "AXButton title=\"Trip planning\" desc=\"Trip planning\"",
                "AXLink title=\"Trip planning\" url=\"https://chatgpt.com/c/6801a2b3-44aa\" sel=\"1\"",
            )
            .replace(
                "AXButton title=\"Rust lifetimes\" desc=\"Rust lifetimes\"",
                "AXLink title=\"Rust lifetimes\" url=\"https://chatgpt.com/c/6801a2b3-55bb\"",
            );
        let tree = tree(&linked);
        let chats = sidebar(ChatApp::ChatGpt, &tree);
        assert_eq!(chats[0].key, ChatKey::Id("6801a2b3-44aa".into()));
        let c = conversation(ChatApp::ChatGpt, &tree).unwrap();
        assert_eq!(c.key, chats[0].key);
    }

    // The ChatGPT app with its mode menu open, as observed on 26.924, and a
    // menu bar whose recent items and window list also say "ChatGPT".
    const CHATGPT_MODE_MENU: &str = r#"
AXApplication title="ChatGPT"
  AXWindow/AXStandardWindow title="ChatGPT"
    AXWebArea title="Trip planning" url="app://-/index.html"
      AXGroup/AXLandmarkNavigation title="Home" desc="Home"
        AXPopUpButton title="Switch mode, current mode: Codex" desc="Switch mode, current mode: Codex"
      AXGroup
        AXMenu title="Switch mode, current mode: Codex"
          AXMenuItem title="ChatGPT Create, learn, and explore"
          AXMenuItem title="Codex Build, debug, and ship"
            AXStaticText value="Codex"
  AXMenuBar
    AXMenuBarItem title="Apple"
      AXMenu
        AXMenuItem title="Recent Items"
          AXMenu
            AXMenuItem title="ChatGPT"
    AXMenuBarItem title="File"
      AXMenu
        AXMenuItem title="New Chat"
    AXMenuBarItem title="Window"
      AXMenu
        AXMenuItem title="ChatGPT"
"#;

    #[test]
    fn chatgpt_mode_menu_entries_are_found_by_mode_name_only_in_the_mode_menu() {
        let open = tree(CHATGPT_MODE_MENU);
        let view = mode(ChatApp::ChatGpt, &open);
        assert_eq!(view.chat, Some(false));
        let chat = view.switch.unwrap();
        assert!(open.nodes[chat].title.starts_with("ChatGPT Create"));
        let codex = mode_control(ChatApp::ChatGpt, &open, false).unwrap();
        assert!(open.nodes[codex].title.starts_with("Codex Build"));
        let file = menu_bar_item(&open, "File", &["New Chat"]).unwrap();
        assert_eq!(open.nodes[file].title, "New Chat");
        // Closed menu: nothing to press but the opener.
        let closed = tree_without_menu(CHATGPT_MODE_MENU);
        assert!(mode(ChatApp::ChatGpt, &closed).switch.is_none());
        fn tree_without_menu(dump: &str) -> AxTree {
            let kept: Vec<&str> = dump
                .lines()
                .filter(|l| {
                    !l.contains("Switch mode, current mode: Codex\"") || l.contains("AXPopUpButton")
                })
                .filter(|l| !l.contains("Create, learn") && !l.contains("Build, debug"))
                .collect();
            tree(&kept.join("\n"))
        }
    }

    #[test]
    fn session_ids() {
        let claude = session_id(
            ChatApp::Claude,
            &ChatKey::Id("0F0E0D0C-1111-4222-8333-444455556666".into()),
        );
        assert_eq!(claude, "claude-chat-0f0e0d0c-1111-4222-8333-444455556666");
        assert_eq!(app_of(&claude), Some(ChatApp::Claude));
        let titled = session_id(ChatApp::ChatGpt, &ChatKey::Title("Trip".into()));
        assert!(titled.starts_with("codex-chat-t") && titled.len() == "codex-chat-t".len() + 16);
        assert_eq!(app_of(&titled), Some(ChatApp::ChatGpt));
        assert!(is_new_session("codex-chat-new"));
        assert_eq!(app_of("0f0e0d0c-1111-4222-8333-444455556666"), None);
        assert_eq!(app_of("claude-chat-"), None);
        assert_eq!(
            key_from_url(ChatApp::ChatGpt, "https://chatgpt.com/c/6801a2b3-44aa"),
            Some(ChatKey::Id("6801a2b3-44aa".into()))
        );
        assert_eq!(key_from_url(ChatApp::ChatGpt, "app://-/index.html"), None);
    }

    #[test]
    fn transcript_grows_in_place_and_resets_on_divergence() {
        let mut t = ChatTranscript::default();
        let first = t.apply(vec![line(Role::User, "hi")], 1);
        assert_eq!(first.changed, vec![0]);
        let grow = t.apply(
            vec![line(Role::User, "hi"), line(Role::Assistant, "Hel")],
            2,
        );
        assert_eq!(
            grow,
            ChatDelta {
                reset: false,
                changed: vec![1]
            }
        );
        let more = t.apply(
            vec![line(Role::User, "hi"), line(Role::Assistant, "Hello there")],
            3,
        );
        assert_eq!(more.changed, vec![1]);
        assert_eq!(t.message_id(1), "ax0-00001");
        // A truncated walk is not news.
        let partial = t.apply(
            vec![line(Role::User, "hi"), line(Role::Assistant, "Hello")],
            4,
        );
        assert_eq!(partial, ChatDelta::default());
        assert_eq!(t.lines[1].0.text, "Hello there");
        // A different conversation (or an edited turn) is a new generation.
        let other = t.apply(vec![line(Role::User, "bye")], 5);
        assert!(other.reset);
        assert_eq!(other.changed, vec![0]);
        assert_eq!(t.message_id(0), "ax1-00000");
    }

    /// Calibration against a live app: save `muster debug ax-dump Claude`
    /// (or ChatGPT) to a file and run
    /// `MUSTER_AX_DUMP=<file> MUSTER_AX_APP=claude|codex cargo test -p muster-agent
    /// dump_file -- --ignored --nocapture`.
    #[test]
    #[ignore]
    fn dump_file() {
        let path = std::env::var("MUSTER_AX_DUMP").expect("MUSTER_AX_DUMP");
        let app = ChatApp::from_provider(&std::env::var("MUSTER_AX_APP").unwrap_or_default())
            .unwrap_or(ChatApp::Claude);
        let dump = std::fs::read_to_string(path).unwrap();
        // The dump labels placeholders as label="…"; drop the ellipsis marker.
        let tree = tree(&dump.replace("… (truncated)", ""));
        println!("{} nodes", tree.nodes.len());
        println!("mode {:?}", mode(app, &tree));
        for chat in sidebar(app, &tree) {
            println!("sidebar {:?} {:?}", chat.key, chat.title);
        }
        println!("new chat control {:?}", new_chat_control(app, &tree));
        match conversation(app, &tree) {
            Some(c) => {
                println!("conversation {:?} {:?}", c.key, c.title);
                println!(
                    "composer {:?} send {:?} stop {:?}",
                    c.composer, c.send, c.stop
                );
                for line in c.lines {
                    println!(
                        "--- {:?}\n{}",
                        line.role,
                        crate::text::clip(&line.text, 300)
                    );
                }
            }
            None => {
                let main = tree
                    .preorder(0)
                    .into_iter()
                    .find(|&i| tree.nodes[i].subrole == "AXLandmarkMain");
                println!("no conversation; main region lines:");
                for line in main.map(|m| parse_lines(app, &tree, m)).unwrap_or_default() {
                    println!(
                        "--- {:?}\n{}",
                        line.role,
                        crate::text::clip(&line.text, 300)
                    );
                }
            }
        }
    }

    #[test]
    fn menu_entries_split_into_name_and_detail() {
        let tree = tree(
            r#"
AXApplication
  AXMenu title="Model: Opus 5.5 Medium"
    AXMenuItem title="Fable 5.1 For your toughest challenges"
      AXStaticText value="Fable 5.1"
      AXStaticText value="For your toughest challenges"
    AXMenuItem title="Opus 5.5 For complex work and everyday tasks" sel="1"
      AXStaticText value="Opus 5.5"
      AXStaticText value="For complex work and everyday tasks"
    AXMenuItem title="Effort Medium"
      AXStaticText value="Effort"
      AXStaticText value="Medium"
    AXMenuItem title="More models"
"#,
        );
        let menu = open_menus(&tree)[0];
        let entries = menu_entries(&tree, menu);
        let models: Vec<ChatChoice> = model_entries(&entries)
            .into_iter()
            .map(|e| choice(&tree, e))
            .collect();
        assert_eq!(models.len(), 2);
        assert_eq!(models[1].name, "Opus 5.5");
        assert_eq!(models[1].detail, "For complex work and everyday tasks");
        assert!(models[1].selected);
        assert_eq!(effort_entry(&entries).unwrap().label, "Effort Medium");
        assert_eq!(stated_effort("Medium, 2 of 5."), "Medium");
        assert_eq!(
            power_level("Medium, 2 of 5."),
            Some(PowerLevel {
                name: "Medium".into(),
                position: 2,
                total: 5
            })
        );
        assert_eq!(
            power_level("GPT-6 Astra Extended, 3 of 5.").unwrap().name,
            "GPT-6 Astra Extended"
        );
        assert_eq!(power_level("Use Left and Right arrow keys"), None);
        assert_eq!(power_level("High, 6 of 5."), None);
        assert_eq!(middle_position(5), 3);
        assert_eq!(middle_position(3), 2);
        assert_eq!(middle_position(1), 1);
        // Observed titles without separate texts.
        for (label, name, detail) in [
            (
                "Sonnet 5.5 Most efficient for simpler tasks",
                "Sonnet 5.5",
                "Most efficient for simpler tasks",
            ),
            ("GPT-5.6 Sol", "GPT-5.6 Sol", ""),
            (
                "GPT-5.5 Leaving on October 14",
                "GPT-5.5",
                "Leaving on October 14",
            ),
            ("Latest", "Latest", ""),
        ] {
            assert_eq!(
                split_versioned(label),
                (name.to_string(), detail.to_string())
            );
        }
    }

    #[test]
    fn clocks_are_not_text() {
        assert!(is_clock("2:18 PM"));
        assert!(is_clock("14:05"));
        assert!(!is_clock("Day 2: food"));
        assert!(!is_clock("12:345"));
    }
}

#[cfg(test)]
mod send_receipt_tests {
    use super::*;
    use crate::model::Role;
    #[test]
    fn delivery_requires_new_user_text_in_the_same_conversation() {
        let before = Conversation {
            key: ChatKey::New,
            title: String::new(),
            web_area: 0,
            lines: vec![ChatLine {
                role: Role::User,
                text: "hello".into(),
            }],
            composer: vec![],
            send: vec![],
            stop: None,
        };
        let mut after = before.clone();
        assert!(!confirms_send(&before, &after, "hello"));
        after.lines.push(ChatLine {
            role: Role::Assistant,
            text: "hello".into(),
        });
        assert!(!confirms_send(&before, &after, "hello"));
        after.lines.push(ChatLine {
            role: Role::User,
            text: "hello".into(),
        });
        assert!(confirms_send(&before, &after, "hello"));
        after.lines.remove(0);
        assert!(!confirms_send(&before, &after, "hello"));
    }
}
