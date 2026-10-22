//! Desktop Chat mode on macOS (docs/AGENT_INTERFACE.md): the Claude app's
//! "Claude" mode and the ChatGPT app's ChatGPT mode, driven through
//! Accessibility only. This file reads the element tree into a snapshot and
//! presses what Shared Rust (`muster_agent::desktop_chat`) picked from it.
//! The write rules of the Code-mode controller carry over: the target
//! process keeps its signature and start time, the conversation on screen
//! must resolve to the requested session before and after drafting, the
//! user's own input counter must not move, and nothing is pressed again
//! after a press whose outcome is unknown. It needs an unlocked console:
//! Electron exposes no windows to AX while the screen is locked.

use std::collections::{HashMap, HashSet};
use std::sync::{Arc, Mutex, OnceLock, Weak};
use std::time::{Duration, Instant};

use async_trait::async_trait;
use muster_agent::controller::{
    Controller, ControllerEvent, ControllerEvents, ProcessIdentity, ProcessState,
};
use muster_agent::desktop_chat::{self as chat, AxNode, AxTree, ChatApp, ChatKey};
use muster_agent::model::{ControllerKind, PendingInteraction, Role, SessionState};
use muster_agent::native_step::Step;
use muster_agent::{AgentService, Error};
use muster_proto::v1;

use super::ax::{self, Element};
use super::codesign;
use super::input::user_input_generation;
use super::native_app::AppProfile;

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
const POLL_WATCHED: Duration = Duration::from_secs(1);
const POLL_IDLE: Duration = Duration::from_secs(3);
const STEP: Duration = Duration::from_millis(100);
const NEW_CHAT_WAIT: Duration = Duration::from_secs(10);
/// After a send the page is observed this long even without a watcher, so
/// the reply and a new conversation's automatic title are recorded.
const FOLLOW_AFTER_SEND: Duration = Duration::from_secs(60);

fn profile(app: ChatApp) -> &'static AppProfile {
    app.profile()
}

/// The app's element tree in document order, bounded by nodes and time.
/// The menu bar is included only when asked (mode and menu fallbacks).
pub fn snapshot(root: &Element, menu_bar: bool) -> (AxTree, Vec<Element>) {
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
        let values = element.attributes(&ATTRIBUTES);
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
            enabled: flag(9).unwrap_or(true),
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
        if descend && let Some(children) = values[11].as_ref() {
            for child in Element::elements_of(children).into_iter().rev() {
                stack.push((child, Some(index), depth + 1));
            }
        }
    }
    (tree, elements)
}

async fn take(root: &Element, menu_bar: bool) -> (AxTree, Vec<Element>) {
    let root = root.clone();
    muster_agent::control_guard::spawn_blocking(move || snapshot(&root, menu_bar))
        .await
        .unwrap_or_default()
}

/// The app's main process, signature-checked, with its launch time.
fn process(app: ChatApp) -> Option<(u32, u64)> {
    let profile = profile(app);
    super::procs::pids_by_executable_suffix(profile.executable)
        .into_iter()
        .filter(|&pid| codesign::process_satisfies(pid, &profile.app.requirement()))
        .find_map(|pid| codesign::process_start(pid).map(|start| (pid, start)))
}

fn console_ready() -> Result<(), Error> {
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

fn press(element: &Element, what: &str) -> Result<(), Error> {
    element
        .press()
        .map_err(|code| Error::Unavailable(format!("AX press on {what} failed ({code})")))
}

/// Navigation must not race the person at the Mac: the same app launch, an
/// unlocked console and an unchanged input counter from start to finish.
struct Guard {
    pid: u32,
    start: u64,
    generation: u64,
}

impl Guard {
    fn new(pid: u32, start: u64) -> Self {
        Self {
            pid,
            start,
            generation: user_input_generation(),
        }
    }

    fn check(&self) -> Result<(), Error> {
        muster_agent::control_guard::check()?;
        console_ready()?;
        if codesign::process_start(self.pid) != Some(self.start) {
            return Err(Error::Unavailable("the app was restarted".into()));
        }
        let current = user_input_generation();
        if current != self.generation {
            log::info!(
                "chat navigation input generation changed {} -> {current}",
                self.generation
            );
            return Err(Error::Conflict(
                "local input changed during chat navigation; nothing more was pressed".into(),
            ));
        }
        Ok(())
    }
}

/// Snapshots until `found` answers, checking the guard between tries.
async fn wait_for<T>(
    guard: &Guard,
    root: &Element,
    menu_bar: bool,
    timeout: Duration,
    what: &str,
    found: impl Fn(&AxTree) -> Option<T>,
) -> Result<(T, Vec<Element>), Error> {
    let step = Step::new(what, timeout);
    loop {
        step.check()?;
        guard.check()?;
        let (tree, elements) = take(root, menu_bar).await;
        step.check()?;
        guard.check()?;
        if let Some(value) = found(&tree) {
            return Ok((value, elements));
        }
        step.check()?;
        tokio::time::sleep(STEP).await;
    }
}

/// The session the page shows: a known conversation, the new-chat page,
/// or none (another mode, a list or settings page).
fn displayed(app: ChatApp, agent: &AgentService, tree: &AxTree) -> Option<String> {
    if chat::mode(app, tree).chat != Some(true) {
        return None;
    }
    let c = chat::conversation(app, tree)?;
    Some(agent.chat_session_for(app, &c.key))
}

async fn switch_to_chat(app: ChatApp, guard: &Guard, root: &Element) -> Result<(), Error> {
    switch_mode(app, guard, root, true).await
}

/// Switches the app to its Chat mode (`chat`) or back to its other mode
/// (Code / Codex): Claude's Mode radio (or Go › Claude / Code, which also
/// leaves its settings pages), or ChatGPT's mode menu, which is opened first
/// and then offers the entry. A dialog's Close button or ChatGPT's "Back to
/// app" is pressed first when one covers the modes. Confirmed within three
/// seconds.
async fn switch_mode(app: ChatApp, guard: &Guard, root: &Element, chat: bool) -> Result<(), Error> {
    let mut left = 0;
    let (tree, elements) = loop {
        let (tree, elements) = take(root, true).await;
        guard.check()?;
        if !tree.complete {
            return Err(Error::Unavailable(
                "incomplete desktop navigation snapshot".into(),
            ));
        }
        // A background mode switch does not prove a dialog/settings is gone.
        let Some(obstruction) = chat::obstruction(app, &tree) else {
            let view = chat::mode(app, &tree);
            if !chat::mode_switch_allowed(app, &tree) {
                return Err(Error::Unavailable(
                    "settings page or dialog has no unique safe exit".into(),
                ));
            }
            if chat::navigation_clear(app, &tree)
                && (view.chat == Some(chat) || (!chat && left > 0 && view.chat.is_none()))
            {
                // Some app shells have no mode switch after Home. Agent
                // selection still resolves the exact session before drafting.
                return Ok(());
            }
            break (tree, elements);
        };
        if left == 3 {
            return Err(Error::Unavailable(format!(
                "{} keeps showing a dialog or its settings",
                app.name()
            )));
        }
        left += 1;
        guard.check()?;
        press(&elements[obstruction.control()], obstruction.what())?;
        log::info!(
            "{}: pressed {} to reach its modes",
            app.name(),
            obstruction.what()
        );
        // Until it is gone (or another took its place); a second dialog
        // behind the first is handled by the next round.
        let kind = std::mem::discriminant(&obstruction);
        wait_for(
            guard,
            root,
            false,
            Duration::from_secs(3),
            "dialog dismissal",
            |tree| {
                (tree.complete
                    && chat::obstruction(app, tree)
                        .is_none_or(|o| std::mem::discriminant(&o) != kind))
                .then_some(())
            },
        )
        .await?;
    };
    let view = chat::mode(app, &tree);
    let what = if chat { "Chat" } else { "the other" };
    match (chat::mode_control(app, &tree, chat), view.opener) {
        (Some(control), _) => {
            guard.check()?;
            press(&elements[control], "the mode switch")?;
        }
        (None, Some(opener)) => {
            guard.check()?;
            press(&elements[opener], "the mode menu")?;
            let (item, elements) = wait_for(
                guard,
                root,
                false,
                Duration::from_millis(1500),
                "the entry of its mode menu",
                |t| chat::mode_control(app, t, chat),
            )
            .await?;
            guard.check()?;
            press(&elements[item], "the mode entry")?;
        }
        (None, None) => {
            return Err(Error::Unavailable(format!(
                "cannot find {}'s switch to {what} mode",
                app.name()
            )));
        }
    }
    wait_for(
        guard,
        root,
        false,
        Duration::from_secs(3),
        "the mode",
        |t| (chat::navigation_clear(app, t) && chat::mode(app, t).chat == Some(chat)).then_some(()),
    )
    .await
    .map(|_| ())
}

/// Local diagnosis: switch `app` to Chat mode or back (`muster debug
/// ax-chat-mode ChatGPT code`). Same guards as any navigation.
pub async fn set_mode(app: ChatApp, chat: bool) -> Result<(), Error> {
    console_ready()?;
    let (pid, start) = process(app)
        .ok_or_else(|| Error::Unavailable(format!("{} is not running on this Mac", app.name())))?;
    let root = Element::application(pid);
    root.enable_manual_accessibility();
    let guard = Guard::new(pid, start);
    guard.check()?;
    switch_mode(app, &guard, &root, chat).await
}

/// Before an agent operation: bring the app back to its Code (Codex) mode
/// from Chat mode, its settings or a dialog over it. Nothing is pressed when
/// the app already shows that mode, or when it shows nothing these rules
/// know (an app build without Chat mode); session selection then decides.
/// Returns whether the app had to be switched.
pub async fn return_to_code(app: ChatApp) -> Result<bool, Error> {
    let Some((pid, start)) = process(app) else {
        return Ok(false);
    };
    let root = Element::application(pid);
    root.enable_manual_accessibility();
    console_ready()?;
    let (tree, generation) = muster_agent::control_guard::prepare_readonly(
        "Code mode navigation",
        Duration::from_secs(3),
        user_input_generation,
        || async {
            console_ready()?;
            if codesign::process_start(pid) != Some(start) {
                return Err(Error::Unavailable("the app was restarted".into()));
            }
            let (tree, _) = take(&root, true).await;
            if !tree.complete {
                return Err(Error::Unavailable(
                    "incomplete desktop navigation snapshot".into(),
                ));
            }
            // Chromium may expose only the application/menu tree while its
            // accessibility renderer warms up. That is not a usable mode view.
            let ready = chat::navigation_ready(app, &tree);
            Ok(ready.then_some(tree))
        },
    )
    .await
    .map_err(|error| match error {
        Error::Unavailable(message) if message == "Code mode navigation: desktop content is not ready" => {
            Error::Unavailable(format!(
                "{} desktop content is not ready. Open the app, finish sign-in or loading, and retry; nothing was written or sent",
                app.name()
            ))
        }
        other => other,
    })?;
    let guard = Guard {
        pid,
        start,
        generation,
    };
    guard.check()?;
    let view = chat::mode(app, &tree);
    if view.chat != Some(true) && chat::navigation_clear(app, &tree) {
        return Ok(false);
    }
    switch_mode(app, &guard, &root, false).await?;
    log::info!(
        "{} returned to its {} mode for an agent operation",
        app.name(),
        app.controls()
            .modes
            .other
            .first()
            .copied()
            .unwrap_or_default()
    );
    Ok(true)
}

async fn show_new(app: ChatApp, guard: &Guard, root: &Element) -> Result<(), Error> {
    let (tree, elements) = take(root, true).await;
    if chat::conversation(app, &tree).is_some_and(|c| c.key == ChatKey::New) {
        return chat_composer(app, guard, root).await;
    }
    let control = chat::new_chat_control(app, &tree)
        .or_else(|| chat::menu_bar_item(&tree, "File", &["New Chat", "New chat"]))
        .ok_or_else(|| {
            Error::Unavailable(format!("cannot find {}'s New chat control", app.name()))
        })?;
    guard.check()?;
    press(&elements[control], "New chat")?;
    wait_for(
        guard,
        root,
        false,
        Duration::from_secs(3),
        "a new chat",
        |t| {
            chat::conversation(app, t)
                .filter(|c| c.key == ChatKey::New)
                .map(|_| ())
        },
    )
    .await?;
    chat_composer(app, guard, root).await
}

/// The new-chat page of the ChatGPT app offers Chat and Work; a desktop chat
/// starts as Chat. Pressed once and confirmed; pages without the choice pass.
async fn chat_composer(app: ChatApp, guard: &Guard, root: &Element) -> Result<(), Error> {
    let (tree, elements) = take(root, false).await;
    let Some((toggle, false)) = chat::composer_chat_toggle(app, &tree) else {
        return Ok(());
    };
    guard.check()?;
    press(&elements[toggle], "the Chat composer mode")?;
    wait_for(
        guard,
        root,
        false,
        Duration::from_secs(2),
        "its Chat composer mode",
        |t| {
            chat::composer_chat_toggle(app, t)
                .is_some_and(|(_, on)| on)
                .then_some(())
        },
    )
    .await
    .map(|_| ())
}

async fn show(
    app: ChatApp,
    agent: &AgentService,
    guard: &Guard,
    root: &Element,
    session: &str,
) -> Result<String, Error> {
    let (mut tree, mut elements) = take(root, false).await;
    if displayed(app, agent, &tree).as_deref() == Some(session) {
        return Ok(session.to_string());
    }
    if !chat::history_entries(app, &tree, true)
        .iter()
        .any(|e| agent.chat_row_matches(app, &e.key, session))
    {
        catalog(app, agent, true).await?;
        (tree, elements) = take(root, false).await;
    }
    let entries = chat::history_entries(app, &tree, true);
    let matches: Vec<_> = entries
        .iter()
        .filter(|e| agent.chat_row_matches(app, &e.key, session))
        .collect();
    let unique_title = matches.first().is_some_and(|m| match &m.key {
        ChatKey::Title(title) => entries.iter().filter(|e| &e.title == title).count() == 1,
        _ => true,
    });
    if matches.len() != 1 || !unique_title {
        return Err(Error::Unavailable(format!(
            "cannot uniquely find this conversation in the {} sidebar ({} matches); scroll it into the list or open it on the Mac",
            app.name(),
            matches.len()
        )));
    }
    let row_title = matches[0].title.clone();
    guard.check()?;
    press(&elements[matches[0].node], "the conversation")?;
    // A row known only by its title opens a page that may carry an id (the
    // Claude app): the page then counts when it shows that exact title, and
    // its own session is the one opened.
    let title_only = matches!(matches[0].key, ChatKey::Title(_));
    wait_for(
        guard,
        root,
        false,
        Duration::from_secs(4),
        "the conversation",
        |t| {
            if chat::mode(app, t).chat != Some(true) {
                return None;
            }
            let c = chat::conversation(app, t).filter(|c| c.key != ChatKey::New)?;
            let shown = agent.chat_session_for(app, &c.key);
            (shown == session || (title_only && c.title == row_title)).then_some(shown)
        },
    )
    .await
    .map(|(shown, _)| shown)
}

/// Switches the app to Chat mode and shows `session` (a listed chat) or a
/// new conversation (`None` or the `-new` id), then attaches its
/// controller. Returns the session shown.
pub async fn open(
    app: ChatApp,
    agent: &Arc<AgentService>,
    session: Option<&str>,
) -> Result<String, Error> {
    console_ready()?;
    let (pid, start) = process(app)
        .ok_or_else(|| Error::Unavailable(format!("{} is not running on this Mac", app.name())))?;
    let root = Element::application(pid);
    root.enable_manual_accessibility();
    let guard = Guard::new(pid, start);
    guard.check()?;
    switch_to_chat(app, &guard, &root).await?;
    let session = match session.filter(|s| !chat::is_new_session(s)) {
        None => {
            show_new(app, &guard, &root).await?;
            chat::new_session_id(app)
        }
        Some(id) => {
            if chat::app_of(id) != Some(app) {
                return Err(Error::Invalid(format!("{id} is not a {} chat", app.name())));
            }
            show(app, agent, &guard, &root, id).await?
        }
    };
    let driver = driver(app, pid, start, agent);
    // The observation is authoritative: a title-only conversation renamed
    // since it was listed resolves to the session it had.
    let session = match driver.observe(agent).await {
        Some(Some(observed)) => observed,
        _ => session,
    };
    driver.attach(&session);
    log::info!("{} chat shown: {}", app.name(), clip_id(&session));
    Ok(session)
}

/// AX adapter used by Claude history and by navigation to an off-screen row.
pub async fn catalog(
    app: ChatApp,
    agent: &AgentService,
    full_history: bool,
) -> Result<v1::AgentChatCatalog, Error> {
    let mut catalog = v1::AgentChatCatalog {
        provider: app.provider().as_str().into(),
        new_session: chat::new_session_id(app),
        ..Default::default()
    };
    let Some((pid, start)) = process(app) else {
        return Ok(catalog);
    };
    catalog.app_running = true;
    // Host selection happens before a Locked Use challenge. A read-only
    // probe may report the signed process while locked, but cannot inspect
    // its windows or enable AX. Shared Rust will request protected history
    // traversal only on the selected host; ChatGPT already has a local catalog.
    if !full_history
        && super::lock::session_state()
            .is_none_or(|s| s.screen_locked || !s.login_done || !s.on_console)
    {
        return Ok(catalog);
    }
    console_ready()?;
    let root = Element::application(pid);
    root.enable_manual_accessibility();
    let guard = Guard::new(pid, start);
    if full_history {
        switch_to_chat(app, &guard, &root).await?;
    }
    let (mut tree, mut elements) = take(&root, false).await;
    if full_history
        && !chat::history_page(app, &tree)
        && let Some(opener) = chat::history_opener(app, &tree)
    {
        guard.check()?;
        press(&elements[opener], "all chat history")?;
        // A history listed by title buttons opens as a page of its own.
        if matches!(
            app.controls().history,
            muster_agent::profile::HistoryRows::TitleButtons { .. }
        ) {
            (tree, elements) = wait_for(
                &guard,
                &root,
                false,
                Duration::from_secs(8),
                "all chat history",
                |t| chat::history_page(app, t).then(|| t.clone()),
            )
            .await?;
        } else {
            (tree, elements) = take(&root, false).await;
        }
    }
    catalog.chat_mode = chat::mode(app, &tree).chat == Some(true);
    if !catalog.chat_mode {
        return Ok(catalog);
    }
    catalog.current = displayed(app, agent, &tree).unwrap_or_default();
    let mut page = Step::new("chat history page", Duration::from_secs(10));
    let mut pages = 0;
    let mut seen = HashSet::new();
    let mut stable = 0;
    loop {
        guard.check()?;
        if !tree.complete {
            catalog.history_warning = "History accessibility snapshot is incomplete".into();
            break;
        }
        let entries = chat::history_entries(app, &tree, full_history);
        let previous = seen.len();
        for entry in &entries {
            let id = agent.chat_session_for(app, &entry.key);
            if seen.insert(id.clone()) {
                catalog.chats.push(v1::AgentChat {
                    current: id == catalog.current,
                    id,
                    title: entry.title.clone(),
                });
            }
        }
        if !full_history {
            break;
        }
        if seen.len() == previous {
            stable += 1;
        } else {
            stable = 0;
            pages += 1;
            page = Step::new("chat history page", Duration::from_secs(10));
        }
        if page.expired() || pages >= 120 {
            catalog.history_warning = "History list is still loading; refresh to continue".into();
            break;
        }
        if let Some(more) = chat::history_more(app, &tree) {
            guard.check()?;
            press(&elements[more], "more chat history")?;
        } else {
            let surfaces = chat::history_scroll_roots(&tree, &entries);
            let scroll = surfaces
                .iter()
                .copied()
                .find(|&i| elements[i].has_action("AXScrollDownByPage"));
            if stable >= 3 && chat::history_exhausted(app, &tree) {
                catalog.history_complete = true;
                break;
            }
            if let Some(scroll) = scroll {
                guard.check()?;
                elements[scroll]
                    .perform("AXScrollDownByPage")
                    .map_err(|code| {
                        Error::Unavailable(format!("History scroll failed ({code})"))
                    })?;
            } else if let Some(last) = entries
                .last()
                .filter(|e| elements[e.node].has_action("AXScrollToVisible"))
            {
                guard.check()?;
                let _ = elements[last.node].perform("AXScrollToVisible");
            } else if stable >= 3 && !chat::history_loading(app, &tree) {
                catalog.history_warning="The app does not expose a history paging control; showing all accessible conversations".into();
                break;
            }
        }
        tokio::time::sleep(Duration::from_millis(500)).await;
        (tree, elements) = take(&root, false).await;
    }
    Ok(catalog)
}

/// Local diagnosis: opens the model picker next to the prompt, lists its
/// entries with their texts and AX actions, and closes it again. Nothing is
/// chosen.
pub async fn inspect_options(app: ChatApp) -> Result<String, Error> {
    console_ready()?;
    let (pid, start) = process(app)
        .ok_or_else(|| Error::Unavailable(format!("{} is not running on this Mac", app.name())))?;
    let root = Element::application(pid);
    root.enable_manual_accessibility();
    let guard = Guard::new(pid, start);
    let o = open_picker(app, &guard, &root).await?;
    let mut out = format!("{}: picker {:?}\n", app.name(), o.picker.label);
    for status in chat::panel_status(&o.tree, o.panel) {
        out.push_str(&format!("  status {status:?}\n"));
    }
    for e in chat::menu_entries(&o.tree, o.panel) {
        let c = chat::choice(&o.tree, &e);
        out.push_str(&format!(
            "  {} {:?} / {:?} actions {:?}\n",
            if e.selected { "*" } else { " " },
            c.name,
            c.detail,
            o.elements[e.node].action_names()
        ));
    }
    close_picker(&guard, &root, &o.picker, o.menus_before).await?;
    Ok(out)
}

/// A read-only report of what the chat rules see in a running app, for
/// calibrating them against a new app build (`muster debug ax-chat Claude`).
pub fn diagnostic(name: &str) -> Result<String, String> {
    let app = if name.is_empty() {
        Some(ChatApp::Claude)
    } else {
        muster_core::apps::chat_app(name).and_then(|app| ChatApp::from_provider(app.provider))
    }
    .ok_or_else(|| {
        format!(
            "unknown app {:?}; use Claude or ChatGPT",
            name.to_ascii_lowercase()
        )
    })?;
    if !ax::trusted() {
        return Err("Accessibility not granted".into());
    }
    let (pid, _) = process(app).ok_or_else(|| format!("{} is not running", app.name()))?;
    let root = Element::application(pid);
    root.enable_manual_accessibility();
    let started = Instant::now();
    let (tree, _) = snapshot(&root, true);
    let mut out = format!(
        "{}: {} nodes in {} ms{}\n",
        app.name(),
        tree.nodes.len(),
        started.elapsed().as_millis(),
        if tree.complete { "" } else { " (truncated)" }
    );
    let view = chat::mode(app, &tree);
    out.push_str(&format!(
        "chat mode: {:?}; settings: {}; switch: {}; mode menu: {}; in the way: {}\n",
        view.chat,
        view.settings,
        view.switch.is_some(),
        view.opener.is_some(),
        chat::obstruction(app, &tree).map_or("nothing", |o| o.what())
    ));
    out.push_str(&format!(
        "new chat control: {}; composer Chat toggle: {:?}\n",
        chat::new_chat_control(app, &tree).is_some(),
        chat::composer_chat_toggle(app, &tree).map(|(_, on)| on)
    ));
    let sidebar = chat::sidebar(app, &tree);
    out.push_str(&format!("sidebar: {} conversations\n", sidebar.len()));
    for entry in sidebar.iter().take(10) {
        out.push_str(&format!(
            "  {} {:?}\n",
            chat::session_id(app, &entry.key),
            muster_agent::text::clip(&entry.title, 60)
        ));
    }
    match chat::conversation(app, &tree) {
        None => out.push_str("conversation: none\n"),
        Some(c) => {
            out.push_str(&format!(
                "conversation: {} title {:?}; prompt fields {}, send buttons {}, stop {}\n",
                chat::session_id(app, &c.key),
                c.title,
                c.composer.len(),
                c.send.len(),
                c.stop.is_some()
            ));
            for line in c.lines.iter().rev().take(6).rev() {
                out.push_str(&format!(
                    "  {}: {}\n",
                    line.role.as_str(),
                    muster_agent::text::clip(&line.text.replace('\n', " ⏎ "), 100)
                ));
            }
        }
    }
    Ok(out)
}

fn clip_id(id: &str) -> &str {
    &id[..id.len().min(24)]
}

// ------------------------------------------------------ model / reasoning

const PICKER_WAIT: Duration = Duration::from_millis(1500);
const CHATGPT_EFFORT_NOTE: &str = "ChatGPT sets Power with the arrow keys; through Accessibility Muster can only raise it to the middle level (a click at the track's centre) when it is lower";
const CLAUDE_EFFORT_NOTE: &str = "Claude's Effort and More models open on hover; Muster shows the effort but cannot change it through Accessibility";

/// The menu (or ChatGPT's panel) a picker opened, found again in `tree`.
fn picker_open(tree: &AxTree, picker: &chat::Picker, menus_before: usize) -> Option<usize> {
    chat::picker_panel(tree, picker).or_else(|| {
        let menus = chat::open_menus(tree);
        (menus.len() > menus_before).then(|| *menus.last().unwrap())
    })
}

struct Opened {
    picker: chat::Picker,
    menus_before: usize,
    panel: usize,
    tree: AxTree,
    elements: Vec<Element>,
}

async fn open_picker(app: ChatApp, guard: &Guard, root: &Element) -> Result<Opened, Error> {
    // A page just shown may still be rendering its prompt row.
    wait_for(guard, root, false, PICKER_WAIT, "its model picker", |t| {
        chat::composer_pickers(app, t).into_iter().next()
    })
    .await?;
    let (tree, elements) = take(root, false).await;
    let picker = chat::composer_pickers(app, &tree)
        .into_iter()
        .next()
        .ok_or_else(|| Error::Unavailable(format!("cannot find {}'s model picker", app.name())))?;
    let menus_before = chat::open_menus(&tree).len();
    if chat::picker_panel(&tree, &picker).is_none() {
        guard.check()?;
        press(&elements[picker.node], "the model picker")?;
    }
    wait_for(
        guard,
        root,
        false,
        PICKER_WAIT,
        "the model picker's menu",
        |t| picker_open(t, &picker, menus_before),
    )
    .await?;
    let (tree, elements) = take(root, false).await;
    let panel = picker_open(&tree, &picker, menus_before)
        .ok_or_else(|| Error::Unavailable("the model picker's menu closed".into()))?;
    Ok(Opened {
        picker,
        menus_before,
        panel,
        tree,
        elements,
    })
}

/// Closes what the picker opened without a new choice: the picker toggles
/// its menu; a menu entry's AXCancel dismisses it otherwise.
async fn close_picker(
    guard: &Guard,
    root: &Element,
    picker: &chat::Picker,
    menus_before: usize,
) -> Result<(), Error> {
    let closed = |t: &AxTree| {
        (chat::picker_panel(t, picker).is_none() && chat::open_menus(t).len() <= menus_before)
            .then_some(())
    };
    let (tree, elements) = take(root, false).await;
    if closed(&tree).is_some() {
        return Ok(());
    }
    if let Some(p) = chat::composer_pickers_like(&tree, picker) {
        guard.check()?;
        let _ = elements[p].press();
    }
    if wait_for(
        guard,
        root,
        false,
        Duration::from_millis(800),
        "the menu closed",
        closed,
    )
    .await
    .is_ok()
    {
        return Ok(());
    }
    let (tree, elements) = take(root, false).await;
    for menu in chat::open_menus(&tree).into_iter().rev() {
        if let Some(entry) = chat::menu_entries(&tree, menu).first() {
            guard.check()?;
            let _ = elements[entry.node].perform("AXCancel");
        }
    }
    wait_for(
        guard,
        root,
        false,
        Duration::from_millis(800),
        "the menu closed",
        closed,
    )
    .await
    .map(|_| ())
    .map_err(|_| Error::Unavailable("the model menu stayed open".into()))
}

fn option(tree: &AxTree, entry: &chat::MenuEntry) -> v1::AgentChatOption {
    let c = chat::choice(tree, entry);
    v1::AgentChatOption {
        name: c.name,
        detail: c.detail,
        selected: c.selected,
    }
}

async fn read_options(
    app: ChatApp,
    guard: &Guard,
    root: &Element,
) -> Result<v1::AgentChatOptions, Error> {
    let mut out = v1::AgentChatOptions::default();
    match app {
        ChatApp::Claude => {
            // Effort levels and More models are submenus that open on hover
            // only (AXPress, AXShowMenu and focus leave them closed).
            let o = open_picker(app, guard, root).await?;
            let entries = chat::menu_entries(&o.tree, o.panel);
            out.models = chat::model_entries(&entries)
                .into_iter()
                .map(|e| option(&o.tree, e))
                .collect();
            if let Some(effort) = chat::effort_entry(&entries) {
                out.effort = effort.label.trim_start_matches("Effort").trim().to_string();
            }
            out.note = CLAUDE_EFFORT_NOTE.into();
            close_picker(guard, root, &o.picker, o.menus_before).await?;
        }
        ChatApp::ChatGpt => {
            let o = open_picker(app, guard, root).await?;
            if let Some(status) = chat::panel_status(&o.tree, o.panel).first() {
                out.effort = chat::stated_effort(status);
                // Below the middle level the one reachable level is offered.
                if let Some(level) = chat::power_level(status) {
                    let middle = chat::middle_position(level.total);
                    if level.position < middle {
                        out.efforts = vec![
                            v1::AgentChatOption {
                                name: level.name.clone(),
                                detail: format!("{} of {}", level.position, level.total),
                                selected: true,
                            },
                            v1::AgentChatOption {
                                name: chat::RAISE_TO_MIDDLE.into(),
                                detail: format!("{middle} of {}", level.total),
                                selected: false,
                            },
                        ];
                    }
                }
            }
            out.note = CHATGPT_EFFORT_NOTE.into();
            let entries = chat::menu_entries(&o.tree, o.panel);
            if let Some(select) = entries.iter().find(|e| e.label == "Select model") {
                guard.check()?;
                press(&o.elements[select.node], "Select model")?;
                let picker = o.picker.clone();
                wait_for(guard, root, false, PICKER_WAIT, "the model list", |t| {
                    let panel = chat::picker_panel(t, &picker)?;
                    let list = chat::menu_entries(t, panel);
                    (!list.is_empty() && !list.iter().any(|e| e.label == "Select model"))
                        .then_some(())
                })
                .await?;
                let (tree, _) = take(root, false).await;
                if let Some(panel) = chat::picker_panel(&tree, &o.picker) {
                    out.models = chat::menu_entries(&tree, panel)
                        .iter()
                        .map(|e| option(&tree, e))
                        .collect();
                }
            }
            close_picker(guard, root, &o.picker, o.menus_before).await?;
        }
    }
    Ok(out)
}

async fn choose_model(
    app: ChatApp,
    guard: &Guard,
    root: &Element,
    model: &str,
) -> Result<(), Error> {
    let o = open_picker(app, guard, root).await?;
    let entries = chat::menu_entries(&o.tree, o.panel);
    let not_offered =
        || Error::Invalid(format!("{} does not offer the model {model:?}", app.name()));
    match app {
        ChatApp::Claude => {
            let Some(node) = chat::model_entries(&entries)
                .into_iter()
                .find(|e| {
                    let c = chat::choice(&o.tree, e);
                    c.name == model || e.label == model
                })
                .map(|e| e.node)
            else {
                close_picker(guard, root, &o.picker, o.menus_before).await?;
                return Err(Error::Invalid(format!(
                    "Claude's menu does not list the model {model:?} (More models open on hover and cannot be chosen through Accessibility)"
                )));
            };
            guard.check()?;
            press(&o.elements[node], "the model")?;
            wait_for(
                guard,
                root,
                false,
                Duration::from_secs(2),
                "the chosen model",
                |t| {
                    chat::composer_pickers(app, t)
                        .first()
                        .is_some_and(|p| p.label.contains(model))
                        .then_some(())
                },
            )
            .await
            .map(|_| ())
        }
        ChatApp::ChatGpt => {
            let select = entries
                .iter()
                .find(|e| e.label == "Select model")
                .ok_or_else(|| Error::Unavailable("ChatGPT's model list is missing".into()))?;
            guard.check()?;
            press(&o.elements[select.node], "Select model")?;
            let picker = o.picker.clone();
            wait_for(guard, root, false, PICKER_WAIT, "the model list", |t| {
                let panel = chat::picker_panel(t, &picker)?;
                chat::menu_entries(t, panel)
                    .iter()
                    .any(|e| chat::choice(t, e).name == model || e.label == model)
                    .then_some(())
            })
            .await
            .map_err(|_| not_offered())?;
            let (tree, elements) = take(root, false).await;
            let node = chat::picker_panel(&tree, &o.picker)
                .and_then(|panel| {
                    chat::menu_entries(&tree, panel)
                        .into_iter()
                        .find(|e| chat::choice(&tree, e).name == model || e.label == model)
                        .map(|e| e.node)
                })
                .ok_or_else(not_offered)?;
            guard.check()?;
            press(&elements[node], "the model")?;
            tokio::time::sleep(Duration::from_millis(300)).await;
            close_picker(guard, root, &o.picker, o.menus_before).await
        }
    }
}

/// ChatGPT: raises Power to its middle level by pressing the track's
/// centre, only while it is lower, and confirms the stated level. Answers
/// whether the track was pressed (false: already at the middle).
async fn raise_power_to_middle(guard: &Guard, root: &Element) -> Result<bool, Error> {
    let app = ChatApp::ChatGpt;
    let o = open_picker(app, guard, root).await?;
    let status = |tree: &AxTree| {
        chat::picker_panel(tree, &o.picker)
            .and_then(|p| chat::panel_status(tree, p).into_iter().next())
            .and_then(|s| chat::power_level(&s))
    };
    let Some(level) = status(&o.tree) else {
        close_picker(guard, root, &o.picker, o.menus_before).await?;
        return Err(Error::Unavailable(
            "ChatGPT does not state its Power level".into(),
        ));
    };
    let middle = chat::middle_position(level.total);
    if level.position >= middle {
        close_picker(guard, root, &o.picker, o.menus_before).await?;
        return if level.position == middle {
            Ok(false)
        } else {
            // A centre click would lower it.
            Err(Error::Invalid(format!(
                "Power is {} ({} of {}), above the middle level; not changed",
                level.name, level.position, level.total
            )))
        };
    }
    let track = chat::menu_entries(&o.tree, o.panel)
        .into_iter()
        .find(|e| e.label == "Power")
        .and_then(|power| chat::power_track(&o.tree, power.node));
    let Some(track) = track else {
        close_picker(guard, root, &o.picker, o.menus_before).await?;
        return Err(Error::Unavailable(
            "cannot find ChatGPT's Power track".into(),
        ));
    };
    guard.check()?;
    press(&o.elements[track], "the Power track")?;
    let reached = wait_for(
        guard,
        root,
        false,
        PICKER_WAIT,
        "the middle Power level",
        |t| status(t).filter(|l| l.position == middle).map(|_| ()),
    )
    .await;
    close_picker(guard, root, &o.picker, o.menus_before).await?;
    reached.map(|_| true).map_err(|_| {
        Error::Unavailable(format!(
            "ChatGPT did not confirm Power level {middle} of {}",
            level.total
        ))
    })
}

fn effort_unavailable(app: ChatApp) -> Error {
    Error::Unavailable(
        match app {
            ChatApp::Claude => CLAUDE_EFFORT_NOTE,
            ChatApp::ChatGpt => CHATGPT_EFFORT_NOTE,
        }
        .into(),
    )
}

/// Shows `session` and reads (or first sets) its model and effort choices.
pub async fn options(
    app: ChatApp,
    agent: &Arc<AgentService>,
    session: &str,
    set: Option<(&str, &str)>,
) -> Result<v1::AgentChatOptions, Error> {
    // Nothing moves on the desktop for a choice that cannot be made: the
    // only settable effort is ChatGPT's middle Power level.
    if let Some((_, effort)) = set
        && !effort.is_empty()
        && !(app == ChatApp::ChatGpt && effort == chat::RAISE_TO_MIDDLE)
    {
        return Err(effort_unavailable(app));
    }
    let shown = open(app, agent, Some(session)).await?;
    let driver = drivers()
        .lock()
        .unwrap()
        .get(&app)
        .cloned()
        .ok_or_else(|| Error::Unavailable(format!("{} is not running on this Mac", app.name())))?;
    let _write = driver
        .write
        .try_lock()
        .map_err(|_| Error::Conflict("Another native operation is in progress".into()))?;
    let guard = Guard::new(driver.pid, driver.start);
    guard.check()?;
    if let Some((model, effort)) = set {
        if !model.is_empty() {
            choose_model(app, &guard, &driver.root, model).await?;
            log::info!(
                "{} chat {}: model {model:?} chosen",
                app.name(),
                clip_id(&shown)
            );
        }
        if !effort.is_empty() {
            let pressed = raise_power_to_middle(&guard, &driver.root).await?;
            log::info!(
                "{} chat {}: Power {}",
                app.name(),
                clip_id(&shown),
                if pressed {
                    "raised to its middle level"
                } else {
                    "already at its middle level; nothing pressed"
                }
            );
        }
    }
    let mut out = read_options(app, &guard, &driver.root).await?;
    if let Some((model, _)) = set.filter(|(m, _)| !m.is_empty())
        && !out.models.iter().any(|m| m.selected && m.name == model)
    {
        return Err(Error::Unavailable(format!(
            "{} did not confirm the model {model:?}",
            app.name()
        )));
    }
    out.session = shown;
    Ok(out)
}

// ---------------------------------------------------------------- driver

fn drivers() -> &'static Mutex<HashMap<ChatApp, Arc<ChatDriver>>> {
    static DRIVERS: OnceLock<Mutex<HashMap<ChatApp, Arc<ChatDriver>>>> = OnceLock::new();
    DRIVERS.get_or_init(Default::default)
}

/// The one driver of an app launch; a relaunch gets a new one.
fn driver(app: ChatApp, pid: u32, start: u64, agent: &Arc<AgentService>) -> Arc<ChatDriver> {
    let mut all = drivers().lock().unwrap();
    if let Some(existing) = all.get(&app)
        && existing.pid == pid
        && existing.start == start
    {
        return existing.clone();
    }
    let root = Element::application(pid);
    root.enable_manual_accessibility();
    let driver = Arc::new(ChatDriver {
        app,
        pid,
        start,
        root,
        agent: Arc::downgrade(agent),
        events: agent.controller_events(),
        state: Mutex::new(DriverState::default()),
        write: tokio::sync::Mutex::new(()),
    });
    all.insert(app, driver.clone());
    tokio::spawn(driver.clone().poll_loop());
    driver
}

#[derive(Default)]
struct DriverState {
    /// Sessions with an attached controller.
    sessions: HashSet<String>,
    /// The session the page resolved to at the last observation.
    shown: Option<String>,
    reported: HashMap<String, SessionState>,
    /// Sessions whose last send press may or may not have taken effect.
    unknown: HashSet<String>,
    /// Observe without a watcher until then (after a send).
    follow_until: Option<Instant>,
}

/// Follows one app launch: records the conversation on screen while anyone
/// watches, and performs the writes of its chat sessions.
pub struct ChatDriver {
    app: ChatApp,
    pid: u32,
    start: u64,
    root: Element,
    agent: Weak<AgentService>,
    events: ControllerEvents,
    state: Mutex<DriverState>,
    write: tokio::sync::Mutex<()>,
}

/// The page, located for a write to one session.
struct Located {
    composer: usize,
    send: Vec<usize>,
    stop: Option<usize>,
}

impl ChatDriver {
    fn identity(&self) -> ProcessIdentity {
        ProcessIdentity {
            pid: self.pid,
            start: self.start,
        }
    }

    fn following(&self) -> bool {
        self.state
            .lock()
            .unwrap()
            .follow_until
            .is_some_and(|until| Instant::now() < until)
    }

    fn attach(self: &Arc<Self>, session: &str) {
        let Some(agent) = self.agent.upgrade() else {
            return;
        };
        self.state
            .lock()
            .unwrap()
            .sessions
            .insert(session.to_string());
        agent.attach(Arc::new(ChatSessionController {
            driver: self.clone(),
            session: session.to_string(),
        }));
    }

    async fn poll_loop(self: Arc<Self>) {
        loop {
            let watched = self.agent.upgrade().is_some_and(|a| a.watched()) || self.following();
            tokio::time::sleep(if watched { POLL_WATCHED } else { POLL_IDLE }).await;
            let Some(agent) = self.agent.upgrade() else {
                return;
            };
            match self
                .identity()
                .observe(codesign::observe_process_start(self.pid).map_err(|_| ()))
            {
                ProcessState::Ended => {
                    let sessions: Vec<String> =
                        self.state.lock().unwrap().sessions.drain().collect();
                    for session in sessions {
                        let _ = self
                            .events
                            .send((
                                session,
                                ControllerEvent::Detached {
                                    process: self.identity(),
                                },
                            ))
                            .await;
                    }
                    let mut all = drivers().lock().unwrap();
                    if all.get(&self.app).is_some_and(|d| Arc::ptr_eq(d, &self)) {
                        all.remove(&self.app);
                    }
                    return;
                }
                ProcessState::Unknown => {
                    self.report(None, false).await;
                    continue;
                }
                ProcessState::Running => {}
            }
            if self.state.lock().unwrap().sessions.is_empty()
                || !(agent.watched() || self.following())
            {
                continue;
            }
            if console_ready().is_err() {
                self.report(None, false).await;
                continue;
            }
            self.observe(&agent).await;
        }
    }

    /// Records the conversation on screen and reports each attached
    /// session's activity: Running with a Stop button, Idle when shown,
    /// Unknown when the page shows something else.
    /// Answers what the page resolved to, or `None` when it was not observed
    /// (a write in progress observes the page itself).
    async fn observe(&self, agent: &AgentService) -> Option<Option<String>> {
        if self.write.try_lock().is_err() {
            return None;
        }
        let (tree, _) = take(&self.root, false).await;
        let conversation = (chat::mode(self.app, &tree).chat == Some(true))
            .then(|| chat::conversation(self.app, &tree))
            .flatten();
        let previous = self.state.lock().unwrap().shown.clone();
        let (shown, running) = match conversation {
            Some(c) if c.key == ChatKey::New => {
                (Some(chat::new_session_id(self.app)), c.stop.is_some())
            }
            Some(c) => {
                let running = c.stop.is_some();
                (
                    agent.chat_observed(self.app, previous.as_deref(), c.key, &c.title, c.lines),
                    running,
                )
            }
            None => (None, false),
        };
        self.report(shown.clone(), running).await;
        Some(shown)
    }

    async fn report(&self, shown: Option<String>, running: bool) {
        let changed: Vec<(String, SessionState)> = {
            let mut state = self.state.lock().unwrap();
            state.shown = shown.clone();
            let sessions: Vec<String> = state.sessions.iter().cloned().collect();
            sessions
                .into_iter()
                .filter_map(|session| {
                    let now = match &shown {
                        Some(s) if *s == session && running => SessionState::Running,
                        Some(s) if *s == session => SessionState::Idle,
                        _ => SessionState::Unknown,
                    };
                    (state.reported.insert(session.clone(), now) != Some(now))
                        .then_some((session, now))
                })
                .collect()
        };
        for (session, now) in changed {
            let _ = self
                .events
                .send((session, ControllerEvent::State(now)))
                .await;
        }
    }

    fn check_identity(&self) -> Result<(), Error> {
        if codesign::process_start(self.pid) != Some(self.start) {
            return Err(Error::Unavailable("the app was restarted".into()));
        }
        console_ready()
    }

    /// The page must be in Chat mode and resolve to `session` (the new-chat
    /// page for the `-new` session), with exactly one prompt field.
    fn locate(&self, agent: &AgentService, tree: &AxTree, session: &str) -> Result<Located, Error> {
        let name = self.app.name();
        if chat::mode(self.app, tree).chat != Some(true) {
            return Err(Error::Conflict(format!(
                "{name} is not showing its Chat mode; nothing was written"
            )));
        }
        let c = chat::conversation(self.app, tree).ok_or_else(|| {
            Error::Conflict(format!(
                "{name} is not showing a conversation; nothing was written"
            ))
        })?;
        let shown = if c.key == ChatKey::New {
            if chat::composer_chat_toggle(self.app, tree).is_some_and(|(_, on)| !on) {
                return Err(Error::Conflict(format!(
                    "{name}'s new chat is not in Chat mode; nothing was written"
                )));
            }
            chat::new_session_id(self.app)
        } else {
            let previous = self.state.lock().unwrap().shown.clone();
            agent
                .chat_observed(self.app, previous.as_deref(), c.key, &c.title, c.lines)
                .unwrap_or_default()
        };
        if shown != session {
            return Err(Error::Conflict(format!(
                "{name} shows a different conversation; nothing was written"
            )));
        }
        if c.composer.len() != 1 {
            return Err(Error::Unavailable(format!(
                "expected one prompt field in {name}, found {}",
                c.composer.len()
            )));
        }
        Ok(Located {
            composer: c.composer[0],
            send: c.send,
            stop: c.stop,
        })
    }

    async fn send(&self, session: &str, text: &str) -> Result<String, Error> {
        let _write = self
            .write
            .try_lock()
            .map_err(|_| Error::Conflict("Another native operation is in progress".into()))?;
        if text.is_empty()
            || text.len() > 64 * 1024
            || text
                .chars()
                .any(|c| c.is_control() && c != '\n' && c != '\t')
        {
            return Err(Error::Invalid(
                "text must be 1 byte to 64 KiB without control characters".into(),
            ));
        }
        if self.state.lock().unwrap().unknown.contains(session) {
            // Refused before any input: this request is definitely not sent.
            return Err(Error::Conflict(
                "Not sent: an earlier send to this conversation is still unconfirmed. Check the conversation in the desktop app; Muster will not resend it automatically.".into(),
            ));
        }
        let agent = self
            .agent
            .upgrade()
            .ok_or_else(|| Error::Unavailable("the agent service stopped".into()))?;
        self.check_identity()?;
        // Read-only preparation; a brief local input settles like Code mode.
        let ((composer, write), generation) =
            muster_agent::control_guard::prepare_stable(user_input_generation, || {
                self.check_identity()?;
                let (tree, elements) = snapshot(&self.root, false);
                let located = self.locate(&agent, &tree, session)?;
                let field = &tree.nodes[located.composer];
                let write = if chat::is_empty_draft(field, &field.value) {
                    true
                } else {
                    // Explicit sends replace a differing draft; a matching
                    // one is reused as it is.
                    field.value.trim_end() != text.trim_end()
                };
                Ok((elements[located.composer].clone(), write))
            })
            .await?;
        muster_agent::control_guard::check()?;
        if user_input_generation() != generation {
            return Err(Error::Conflict(
                "local input changed before drafting; not sent".into(),
            ));
        }
        if write {
            super::native_app::draft(profile(self.app), &composer, text).map_err(|code| {
                Error::Unavailable(format!("AX set value failed ({code}); not sent"))
            })?;
        }
        // The prompt may be rebuilt by the write: confirm the exact draft on a
        // fresh read of this same conversation before pressing anything.
        let started = Instant::now();
        let mut readiness = muster_agent::native_send::DraftWait::default();
        let (send, elements, before) = loop {
            muster_agent::control_guard::check()?;
            if user_input_generation() != generation {
                return Err(Error::Conflict(
                    "local input changed; the draft was preserved, not sent".into(),
                ));
            }
            self.check_identity()?;
            let (tree, elements) = take(&self.root, false).await;
            let located = self.locate(&agent, &tree, session).map_err(|error| {
                Error::Conflict(format!("{error}; the draft was preserved, not sent"))
            })?;
            let echoed = tree.nodes[located.composer].value.trim_end() == text.trim_end();
            // Chat and Code use the same independent echo -> focus -> send
            // readiness steps. Each poll still verifies the exact draft.
            use muster_agent::native_send::Readiness;
            let enabled = located.send.len() == 1 && tree.nodes[located.send[0]].enabled;
            match readiness.observe(started.elapsed(), echoed, located.send.len(), enabled) {
                Readiness::Ready => {
                    let before = chat::conversation(self.app, &tree)
                        .ok_or_else(|| Error::Conflict("Conversation changed; not sent".into()))?;
                    break (located.send[0], elements, before);
                }
                Readiness::Focus => {
                    muster_agent::control_guard::check()?;
                    self.check_identity()?;
                    if user_input_generation() != generation {
                        return Err(Error::Conflict(
                            "Local input changed before focus; not sent".into(),
                        ));
                    }
                    let _ = elements[located.composer].set_bool("AXFocused", true);
                    readiness.focus_completed(started.elapsed());
                }
                Readiness::Wait => {}
                failure => {
                    let step = match failure {
                        Readiness::EchoTimeout => "draft echo",
                        Readiness::MissingButton => "send button appearance",
                        Readiness::DisabledButton | Readiness::SubmitTimeout => {
                            "send button readiness"
                        }
                        _ => "unique send button",
                    };
                    return Err(Error::Unavailable(format!(
                        "Accessibility step '{step}' failed; draft preserved, not sent"
                    )));
                }
            }
            tokio::time::sleep(Duration::from_millis(30)).await;
        };
        muster_agent::control_guard::check()?;
        self.check_identity()?;
        if user_input_generation() != generation {
            return Err(Error::Conflict(
                "local input changed; draft preserved, not sent".into(),
            ));
        }
        // Only this press can submit. Never press again automatically.
        muster_agent::control_guard::delivery_attempt();
        self.state
            .lock()
            .unwrap()
            .unknown
            .insert(session.to_string());
        let _press = elements[send].press();
        let landed = if chat::is_new_session(session) {
            self.follow_new(&agent, text).await
        } else {
            let step = Step::new("chat send receipt", Duration::from_secs(10));
            loop {
                if step.check().is_err() { break None; }
                let (tree, _) = take(&self.root, false).await;
                if step.check().is_err() { break None; }
                if let Some(after) = chat::conversation(self.app, &tree)
                    && chat::confirms_send(&before, &after, text)
                { break Some(session.to_string()); }
                tokio::time::sleep(Duration::from_millis(100)).await;
            }
        }.ok_or_else(|| Error::UnknownOutcome("Timed out waiting for the sent message to appear in this conversation; do not resend".into()))?;
        {
            let mut state = self.state.lock().unwrap();
            state.unknown.remove(session);
            state.follow_until = Some(Instant::now() + FOLLOW_AFTER_SEND);
        }
        log::info!(
            "{} chat send to {}: sent",
            self.app.name(),
            clip_id(session)
        );
        let _ = self
            .events
            .send((
                session.to_string(),
                ControllerEvent::State(SessionState::Running),
            ))
            .await;
        Ok(landed)
    }

    /// After the first send from the new-chat page: the conversation it
    /// became, once the app gives it an id or a title and it opens with
    /// exactly this text.
    async fn follow_new(
        self: &ChatDriver,
        agent: &Arc<AgentService>,
        text: &str,
    ) -> Option<String> {
        let step = Step::new("new conversation receipt", NEW_CHAT_WAIT);
        while step.check().is_ok() {
            tokio::time::sleep(Duration::from_millis(300)).await;
            if step.check().is_err() {
                return None;
            }
            let (tree, _) = take(&self.root, false).await;
            if step.check().is_err() {
                return None;
            }
            let Some(c) = chat::conversation(self.app, &tree) else {
                continue;
            };
            // The page renders line breaks its own way; compare the words.
            let words = |s: &str| s.split_whitespace().collect::<Vec<_>>().join(" ");
            let opens_with_text = c
                .lines
                .first()
                .is_some_and(|l| l.role == Role::User && words(&l.text) == words(text));
            if c.key == ChatKey::New || !opens_with_text {
                continue;
            }
            let id = agent.chat_observed(self.app, None, c.key, &c.title, c.lines)?;
            self.state.lock().unwrap().shown = Some(id.clone());
            let this = drivers().lock().unwrap().get(&self.app).cloned();
            if let Some(this) = this.filter(|d| d.pid == self.pid && d.start == self.start) {
                this.attach(&id);
            }
            return Some(id);
        }
        None
    }

    async fn interrupt(&self, session: &str) -> Result<(), Error> {
        let _write = self
            .write
            .try_lock()
            .map_err(|_| Error::Conflict("Another native operation is in progress".into()))?;
        let agent = self
            .agent
            .upgrade()
            .ok_or_else(|| Error::Unavailable("the agent service stopped".into()))?;
        self.check_identity()?;
        let (tree, elements) = take(&self.root, false).await;
        let located = self.locate(&agent, &tree, session)?;
        let stop = located
            .stop
            .ok_or_else(|| Error::Conflict("no reply is being generated".into()))?;
        muster_agent::control_guard::delivery_attempt();
        muster_agent::control_guard::check()?;
        press(&elements[stop], "Stop")
    }
}

/// The controller of one chat session: a view onto its app's driver.
struct ChatSessionController {
    driver: Arc<ChatDriver>,
    session: String,
}

#[async_trait]
impl Controller for ChatSessionController {
    fn kind(&self) -> ControllerKind {
        ControllerKind::NativeApp
    }
    // Navigation presses cannot deliver; the send and Stop presses mark.
    fn reports_delivery(&self) -> bool {
        true
    }
    fn session_id(&self) -> String {
        self.session.clone()
    }
    fn state(&self) -> SessionState {
        self.driver
            .state
            .lock()
            .unwrap()
            .reported
            .get(&self.session)
            .copied()
            .unwrap_or(SessionState::Unknown)
    }
    fn process_identity(&self) -> ProcessIdentity {
        self.driver.identity()
    }
    fn process_state(&self) -> ProcessState {
        self.driver
            .identity()
            .observe(codesign::observe_process_start(self.driver.pid).map_err(|_| ()))
    }
    fn pending(&self) -> Vec<PendingInteraction> {
        Vec::new()
    }
    async fn send(&self, text: &str) -> Result<(), Error> {
        self.driver.send(&self.session, text).await.map(|_| ())
    }
    async fn send_to(&self, text: &str) -> Result<Option<String>, Error> {
        self.driver.send(&self.session, text).await.map(Some)
    }
    async fn respond(&self, _interaction: &str, _option: &str, _answer: &str) -> Result<(), Error> {
        Err(Error::Unavailable(
            "desktop chats have no approval prompts".into(),
        ))
    }
    async fn interrupt(&self) -> Result<(), Error> {
        self.driver.interrupt(&self.session).await
    }
    async fn stop(&self) {}
}
