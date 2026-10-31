//! The NativeApp controller (§3.6.2): an agent session shown in a desktop
//! app (Claude Code in the Claude app, Codex in the ChatGPT app, CatDesk), driven
//! through Accessibility. Each app is an [`AppProfile`]: how its process
//! is found and verified, how a web area maps to a session, and what its
//! composer and send button look like. The rules carried
//! over from `macos_claude_native_input.rs`: the target process must pass
//! its signature requirement and keep its start time; the composer is
//! replaced with the explicitly requested text; the user's own input counter must not move
//! between reading and writing; nothing is retried after a write that may
//! have taken effect. Locked-screen operation depends on the P0 check.

use std::sync::{Arc, Mutex};
use std::time::Duration;

use async_trait::async_trait;
use muster_agent::Error;
use muster_agent::controller::{Controller, ControllerEvent, ControllerEvents};
use muster_agent::controller::{ProcessIdentity, ProcessState};
use muster_agent::model::{ControllerKind, PendingInteraction, SessionState};
use muster_agent::native_step::Step;

use super::ax::{self, Element};
use super::codesign;
use super::input::user_input_generation;

#[path = "native_diagnostics.rs"]
mod diagnostics;
#[path = "native_options.rs"]
mod native_options;
#[path = "native_queue.rs"]
mod native_queue;
use diagnostics::{Trace, control_label, label};

const ECHO_POLL: Duration = Duration::from_millis(50);
const STATE_POLL: Duration = Duration::from_secs(1);

/// What a web area tells about the session it shows; the resolver turns
/// it into the session id the transcripts (and the phone) know.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct WebAreaKey {
    pub url: Option<String>,
    pub title: Option<String>,
}

impl WebAreaKey {
    pub fn of(node: &Element) -> WebAreaKey {
        WebAreaKey {
            url: node.string("AXURL"),
            title: node.string("AXTitle"),
        }
    }
}

/// Maps a web area to a session id: the daemon knows the transcripts, the
/// platform only the app.
pub type Resolver = Arc<dyn Fn(&WebAreaKey) -> Option<String> + Send + Sync>;

/// One desktop app the controller can drive: its compiled profile
/// (`muster_agent::profile`, from `profiles/<app>.toml`), which says who
/// the app is, how its process and windows are found, and how its controls
/// are recognized and driven.
pub type AppProfile = muster_agent::profile::App;
pub use muster_agent::profile::{APPS, CATDESK_APP, CLAUDE_APP, CODEX_APP, Gui};

/// Puts the requested text into the composer (replacing any draft); the
/// caller then waits for the exact echo before any press.
pub(super) fn draft(
    profile: &AppProfile,
    composer: &Element,
    text: &str,
) -> Result<(), ax::AXError> {
    match profile.controls().draft {
        muster_agent::profile::Draft::Value => draft_value(composer, text),
        muster_agent::profile::Draft::Selection => draft_selection(profile, composer, text),
    }
}

/// Electron editors that re-read their DOM (ProseMirror) take the text as
/// the composer's value.
fn draft_value(composer: &Element, text: &str) -> Result<(), ax::AXError> {
    composer.set_string("AXValue", text)
}

/// CatDesk's Lexical editor reverts DOM edits it did not make itself, and
/// Chromium writes a contenteditable's value straight into its DOM. Replacing
/// the selected text instead runs an editing command (`beforeinput`), which
/// the editor applies to its own document: focus, select the whole draft,
/// replace it. Nothing here submits.
fn draft_selection(
    profile: &AppProfile,
    composer: &Element,
    text: &str,
) -> Result<(), ax::AXError> {
    let generation = user_input_generation();
    let check = || {
        if user_input_generation() != generation || muster_agent::control_guard::check().is_err() {
            Err(-25200) // AX failure: authority/input changed between OS calls.
        } else {
            Ok(())
        }
    };
    check()?;
    composer.set_bool("AXFocused", true)?;
    check()?;
    let length = composer
        .string("AXValue")
        .map(|v| {
            if profile.is_empty(&v) {
                0
            } else {
                v.encode_utf16().count()
            }
        })
        .ok_or(-25212)?; // An unreadable draft is not an empty draft.
    check()?;
    composer.set_range("AXSelectedTextRange", 0, length)?;
    check()?;
    composer.set_string("AXSelectedText", text)
}

/// One attempt to empty an editor. Shared Rust chose the mechanism and
/// confirms the result; this performs the OS calls and rechecks authority and
/// local input between them. Nothing here can submit.
pub(super) fn clear_editor(
    profile: &AppProfile,
    editor: &Element,
    with: muster_agent::native_send::ClearWith,
) -> Result<(), ax::AXError> {
    use muster_agent::native_send::ClearWith;
    let generation = user_input_generation();
    let check = || {
        if user_input_generation() != generation || muster_agent::control_guard::check().is_err() {
            Err(-25200) // AX failure: authority/input changed between OS calls.
        } else {
            Ok(())
        }
    };
    check()?;
    match with {
        // A DOM write. Chromium applies it to a plain field; a rich editor
        // may re-read it or ignore it, which the confirmation decides.
        ClearWith::Value => editor.set_string("AXValue", ""),
        // An editing command (`beforeinput`) the editor applies to its own
        // document, like the selection draft write for Lexical.
        ClearWith::Selection => {
            editor.set_bool("AXFocused", true)?;
            check()?;
            let length = editor
                .string("AXValue")
                .map(|value| {
                    if profile.is_empty(&value) {
                        0
                    } else {
                        value.encode_utf16().count()
                    }
                })
                .ok_or(-25212)?; // An unreadable draft is not an empty draft.
            check()?;
            editor.set_range("AXSelectedTextRange", 0, length)?;
            check()?;
            editor.set_string("AXSelectedText", "")
        }
    }
}

/// A live element answers a profile's queries.
impl muster_agent::profile::AxQuery for Element {
    fn role(&self) -> String {
        Element::role(self)
    }
    fn title(&self) -> Option<String> {
        self.string("AXTitle").filter(|value| !value.is_empty())
    }
    fn description(&self) -> Option<String> {
        self.string("AXDescription")
            .filter(|value| !value.is_empty())
    }
    fn placeholder(&self) -> Option<String> {
        self.string("AXPlaceholderValue")
            .filter(|value| !value.is_empty())
    }
}

/// A control's names, read once so several selectors can test it.
fn names(node: &Element, role: String) -> muster_agent::desktop_chat::AxNode {
    muster_agent::desktop_chat::AxNode {
        role,
        title: text(node, "AXTitle"),
        description: text(node, "AXDescription"),
        ..Default::default()
    }
}

/// Whether this text area is the app's composer and holds a draft.
fn holds_draft(profile: &AppProfile, node: &Element, role: String) -> bool {
    let field = muster_agent::desktop_chat::AxNode {
        role,
        title: text(node, "AXTitle"),
        description: text(node, "AXDescription"),
        placeholder: text(node, "AXPlaceholderValue"),
        ..Default::default()
    };
    profile.controls().composer.matches(&field)
        && node
            .string("AXValue")
            .is_some_and(|value| profile.controls().is_draft(&value))
}

fn text(node: &Element, attr: &str) -> String {
    node.string(attr).unwrap_or_default()
}

/// A button's name: Chromium reports an `aria-label` as the description
/// and leaves the title empty.
fn button_label(node: &Element) -> String {
    node.string("AXTitle")
        .filter(|t| !t.is_empty())
        .or_else(|| node.string("AXDescription"))
        .unwrap_or_default()
}

#[derive(Debug, PartialEq, Eq)]
enum DraftAction {
    Write,
    Reuse,
}

fn draft_action(
    profile: &AppProfile,
    value: Option<&str>,
    text: &str,
) -> Result<DraftAction, Error> {
    match value {
        Some(value) if profile.is_empty(value) => Ok(DraftAction::Write),
        // Explicit sends replace differing native drafts with Muster's text.
        // Matching drafts need no rewrite. Unknown presses still cannot replay.
        Some(value) if value.trim_end() == text.trim_end() => Ok(DraftAction::Reuse),
        Some(_) => Ok(DraftAction::Write),
        None => Err(Error::Unavailable(
            "the prompt cannot be read; not sent".into(),
        )),
    }
}

/// Collect only OS ancestry facts. Shared Rust decides approval eligibility.
fn approval_in_scope(profile: &AppProfile, web_area: &Element, button: &Element) -> bool {
    let started = std::time::Instant::now();
    let mut node = button.clone();
    let mut seen = Vec::new();
    let mut ancestry = Vec::new();
    for _ in 0..64 {
        if node == *web_area {
            return profile.controls().approval_scope.admits(&ancestry, true);
        }
        if seen.contains(&node) || started.elapsed() > Duration::from_millis(250) {
            break;
        }
        ancestry.push(node.string("AXSubrole").unwrap_or_default());
        seen.push(node.clone());
        let Some(parent) = node.element("AXParent") else {
            break;
        };
        node = parent;
    }
    profile.controls().approval_scope.admits(&ancestry, false)
}

/// Every app the engine drives, in scan order. An app whose profile
/// records no controls (DeepSeek Harness) is viewed only: its windows are
/// never scanned.
pub fn profiles() -> Vec<&'static AppProfile> {
    APPS.into_iter().filter(|app| app.driven()).collect()
}

/// The app's GUI processes. With [`Gui::CommandSocket`], only the process
/// the socket names (and that passes the signature check later) shows
/// windows.
pub fn app_pids(profile: &AppProfile) -> Vec<u32> {
    // An app that is not driven shows no window Muster looks at.
    if profile.gui.is_none() {
        return Vec::new();
    }
    let pids = super::procs::pids_by_executable_suffix(profile.executable);
    if profile.gui == Some(Gui::Executable) {
        return pids;
    }
    let Some(gui) = muster_agent::catdesk_app::socket()
        .and_then(|socket| muster_agent::catdesk_app::health(&socket).ok())
    else {
        return Vec::new();
    };
    pids.into_iter().filter(|pid| *pid == gui).collect()
}

/// What the running CatDesk reports through its socket, bound to its
/// verified GUI process; `None` when it is not running or not genuine.
pub fn catdesk_runtime() -> Option<muster_agent::catdesk_app::Runtime> {
    use muster_agent::catdesk_app;
    let socket = catdesk_app::socket()?;
    let pid = catdesk_app::health(&socket).ok()?;
    if !codesign::process_satisfies(pid, &CATDESK_APP.app.requirement()) {
        return None;
    }
    let start = codesign::process_start(pid)?;
    let sessions = catdesk_app::sessions(&socket).ok()?;
    // Never pair a list with a process that restarted while it was read.
    (codesign::process_start(pid) == Some(start)).then_some(catdesk_app::Runtime {
        process: ProcessIdentity { pid, start },
        sessions,
    })
}

/// CatDesk's resolver: a popout names its session in its URL; the main
/// window shows the session the app reports as current.
pub fn catdesk_resolver() -> Resolver {
    use muster_agent::catdesk_ui::{Route, route};
    Arc::new(|key: &WebAreaKey| match route(key.url.as_deref()?)? {
        Route::Popout(id) => Some(id),
        Route::Main => {
            let socket = muster_agent::catdesk_app::socket()?;
            muster_agent::catdesk_app::current(&socket).ok().flatten()
        }
    })
}

/// The web areas an app shows, raw (for diagnosis).
pub fn web_areas(pid: u32) -> Vec<WebAreaKey> {
    let mut out = Vec::new();
    if !ax::trusted() {
        return out;
    }
    let app = Element::application(pid);
    app.enable_manual_accessibility();
    app.walk(|node| {
        if node.role() == "AXWebArea" {
            out.push(WebAreaKey::of(node));
        }
        true
    });
    out
}

/// Read only passive labels belonging to this control. Never borrow text
/// from a nested button/menu or a sibling row. An incomplete read cannot
/// establish that the control has exactly one standalone title.
impl muster_agent::native_sidebar::LabelNode for Element {
    fn role(&self) -> String {
        Element::role(self)
    }
    fn text(&self) -> Option<String> {
        self.string("AXValue").or_else(|| self.string("AXTitle"))
    }
    fn children(&self) -> Vec<Self> {
        Element::children(self)
    }
}

/// The sessions an app shows, through the resolver.
pub fn discover(pid: u32, resolver: &Resolver) -> Vec<String> {
    web_areas(pid)
        .iter()
        .filter_map(|key| resolver(key))
        .collect()
}

/// Sidebar expansions ("Show 20 more in Recents") tried before giving up:
/// up to a hundred more sessions in the Claude app.
const SIDEBAR_EXPANSIONS: usize = 5;

/// What one look at the session list found: rows naming the session, the
/// rows seen, the navigation areas searched and the list's "more" control.
struct SidebarScan {
    candidates: Vec<Element>,
    rows: usize,
    navigation: usize,
    more: Option<Element>,
    complete: bool,
}

/// Read-only: finds the controls that name `session` in the app's session
/// list (see the profile's `SessionList`). Logs up to `log_rows` rows, and
/// relevant ones beyond them.
#[allow(clippy::too_many_arguments)]
fn scan_sidebar(
    profile: &AppProfile,
    app: &Element,
    session: &str,
    title: &str,
    title_is_unique: bool,
    resolver: &Resolver,
    trace: &Trace,
    log_rows: usize,
) -> SidebarScan {
    use muster_agent::profile::SessionList;
    let sessions = profile.controls().sessions;
    let mut navigation = Vec::new();
    let app_walk = app.walk_pruned_report(|node| {
        let id = node
            .string("AXIdentifier")
            .or_else(|| node.string("AXDOMIdentifier"))
            .unwrap_or_default();
        let subrole = text(node, "AXSubrole");
        let SessionList::Sidebar { landmarks, .. } = sessions else {
            // No landmark: rows are found from their title text inside the
            // main window's page.
            let main = node.role() == "AXWebArea"
                && node
                    .string("AXURL")
                    .as_deref()
                    .and_then(muster_agent::catdesk_ui::route)
                    == Some(muster_agent::catdesk_ui::Route::Main);
            if main {
                navigation.push(node.clone());
                return false;
            }
            return true;
        };
        if id == "app-shell-sidebar"
            || (landmarks
                && matches!(
                    subrole.as_str(),
                    "AXLandmarkNavigation" | "AXLandmarkComplementary"
                ))
        {
            navigation.push(node.clone());
            // Without landmarks the shell sidebar is the one list; with them
            // the lists nested inside it are searched too.
            if !landmarks {
                return false;
            }
        }
        true
    });
    let mut complete = app_walk.1 == "complete";
    let navigation_count = navigation.len();
    trace.record(format_args!(
        "navigation_areas={navigation_count} app_walk={app_walk:?} (nodes, stop, ms)"
    ));
    let mut candidates = Vec::new();
    let mut rows = 0usize;
    let mut logged = 0usize;
    let mut more_rule = None;
    let mut more = None;
    let standalone_title = match sessions {
        SessionList::Sidebar {
            standalone_title,
            more: rule,
            ..
        } => {
            more_rule = rule;
            standalone_title
        }
        SessionList::PageRows => {
            for area in &navigation {
                candidates = catdesk_rows(area, title, title_is_unique, trace);
            }
            rows = candidates.len();
            false
        }
    };
    for area in navigation
        .iter()
        .filter(|_| sessions != SessionList::PageRows)
    {
        let area_walk = area.walk_report(|node| {
            if !matches!(node.role().as_str(), "AXButton" | "AXLink" | "AXRow") {
                return true;
            }
            let key = WebAreaKey::of(node);
            rows += 1;
            let by_id = key.url.as_ref().is_some() && resolver(&key).as_deref() == Some(session);
            let node_title = text(node, "AXTitle");
            let description = text(node, "AXDescription");
            let standalone = (standalone_title
                && title_is_unique
                && node.role() == "AXButton"
                && node_title != title
                && description != title)
                .then(|| muster_agent::native_sidebar::read_label(node));
            let by_title = muster_agent::native_sidebar::title_matches(
                standalone_title,
                title,
                title_is_unique,
                &node_title,
                &description,
                standalone.as_ref().and_then(|r| r.0.as_deref()),
            );
            let press = node.has_action("AXPress");
            if more.is_none()
                && press
                && more_rule.is_some_and(|rule| {
                    rule.matches(if node_title.is_empty() {
                        &description
                    } else {
                        &node_title
                    })
                })
            {
                more = Some(node.clone());
            }
            let relevant = by_id || by_title || (!title.is_empty() && node_title.contains(title));
            if logged < log_rows || (logged < log_rows + 24 && relevant) {
                // Metadata belongs to a sidebar control, never the main pane.
                let labels = standalone.as_ref().map(|r| {
                    let texts = r.0.as_ref().map(|v| {
                        v.iter().take(4).map(|s| label(s)).collect::<Vec<_>>()
                    });
                    (texts, r.1, r.2, r.3)
                });
                trace.record(format_args!("row={rows} role={} title={:?} description={:?} url_present={} by_id={by_id} by_title={by_title} press={press} label_read={labels:?} (texts, nodes, skipped_controls, stop)",
                    node.role(), label(&node_title), label(&description), key.url.is_some()));
                logged += 1;
            }
            if (by_id || by_title) && press && !candidates.contains(node) {
                candidates.push(node.clone());
            }
            true
        });
        complete &= area_walk.1 == "complete";
        trace.record(format_args!("area_walk={area_walk:?} (nodes, stop, ms)"));
    }
    trace.record(format_args!(
        "rows={rows} logged_rows={logged} candidates={}",
        candidates.len()
    ));
    SidebarScan {
        candidates,
        rows,
        navigation: navigation_count,
        more,
        complete,
    }
}

/// Select only a uniquely identified navigation control, then resolve the
/// resulting web area again. Never write to whichever composer is focused.
pub async fn select_session(
    profile: &AppProfile,
    pid: u32,
    session: &str,
    title: &str,
    title_is_unique: bool,
    resolver: &Resolver,
) -> Result<(), Error> {
    let mut trace = Trace::new("select", profile.app.provider, session);
    trace.record(format_args!(
        "pid={pid} target_title={:?} unique_title={title_is_unique}",
        label(title)
    ));
    let start =
        codesign::process_start(pid).ok_or_else(|| Error::NotFound(format!("pid {pid}")))?;
    if !ax::trusted() || !codesign::process_satisfies(pid, &profile.app.requirement()) {
        return Err(Error::Unavailable(
            "desktop Accessibility identity check failed".into(),
        ));
    }
    let app = Element::application(pid);
    app.enable_manual_accessibility();
    let identity_check = || -> Result<(), Error> {
        muster_agent::control_guard::check()?;
        if codesign::process_start(pid) != Some(start)
            || super::lock::session_state()
                .is_none_or(|s| s.screen_locked || !s.login_done || !s.on_console)
        {
            return Err(Error::Unavailable(
                "desktop changed during session selection".into(),
            ));
        }
        Ok(())
    };
    trace.stage("selection_preparation");
    let (prepared, generation) = muster_agent::control_guard::prepare_readonly(
        "session selection",
        Duration::from_secs(3),
        user_input_generation,
        || async {
            identity_check()?;
            match find_surface_traced(profile, &app, session, resolver, Some(&trace)) {
                Ok(_) => return Ok(Some(None)),
                Err(Error::NotFound(_)) => {}
                // Ambiguity is not an unready renderer and cannot be retried
                // into a convenient choice of composer.
                Err(error) => return Err(error),
            }
            let scan = scan_sidebar(
                profile,
                &app,
                session,
                title,
                title_is_unique,
                resolver,
                &trace,
                24,
            );
            // Zero navigation areas is an unready or unsupported UI, not proof
            // that this session is absent. Bounded reads may observe it later.
            Ok((scan.navigation > 0).then_some(Some(scan)))
        },
    )
    .await?;
    let check = || -> Result<(), Error> {
        identity_check()?;
        let current = user_input_generation();
        if current != generation {
            log::info!("session selection input generation changed {generation} -> {current}");
            return Err(Error::Conflict(
                "local input changed during session selection; nothing more was pressed".into(),
            ));
        }
        Ok(())
    };
    check()?;
    let Some(mut scan) = prepared else {
        trace.record(format_args!("target_already_visible=true"));
        trace.finish();
        return Ok(());
    };
    // A session older than the rows shown is behind the list's "more"
    // control: press it (a navigation, under the same guard as a row) and
    // look again, a bounded number of times.
    let mut expansions = 0;
    while scan.candidates.is_empty() && expansions < SIDEBAR_EXPANSIONS {
        let Some(more) = scan.more.take() else {
            break;
        };
        check()?;
        trace.stage("sidebar_more");
        more.press()
            .map_err(|code| Error::Unavailable(format!("AX sidebar expansion failed ({code})")))?;
        expansions += 1;
        let before = scan.rows;
        let step = Step::new("expanded sidebar rows", Duration::from_secs(2));
        loop {
            tokio::time::sleep(Duration::from_millis(200)).await;
            check()?;
            scan = scan_sidebar(
                profile,
                &app,
                session,
                title,
                title_is_unique,
                resolver,
                &trace,
                0,
            );
            if scan.rows > before || !scan.candidates.is_empty() || step.expired() {
                break;
            }
        }
        trace.record(format_args!(
            "expansion={expansions} rows {before} -> {}",
            scan.rows
        ));
        // A press that listed nothing more is not repeated.
        if scan.rows <= before {
            break;
        }
    }
    let desktop_id =
        (profile.app.provider == "claude" && scan.candidates.is_empty() && scan.complete)
            .then(|| {
                let root = muster_agent::claude::desktop_registry()?;
                muster_agent::claude::desktop_navigation_id(&root, session)
            })
            .flatten();
    let route = muster_agent::native_sidebar::absent_session_route(
        profile.app.provider,
        desktop_id.as_deref().unwrap_or(""),
        scan.navigation,
        scan.complete,
        scan.candidates.len(),
    );
    let candidates = scan.candidates;
    if candidates.len() == 1 {
        check()?;
        trace.stage("select_press");
        candidates[0]
            .press()
            .map_err(|code| Error::Unavailable(format!("AX session selection failed ({code})")))?;
    } else if let Some(route) = route {
        let bundle = super::procs::process_bundle(pid)
            .ok_or_else(|| Error::Unavailable("desktop bundle identity unavailable".into()))?;
        check()?;
        if !codesign::process_satisfies(pid, &profile.app.requirement()) {
            return Err(Error::Unavailable(
                "desktop signature changed before navigation".into(),
            ));
        }
        check()?;
        trace.stage("select_exact_id");
        // No shell, default scheme handler, prompt, or title. Dispatch once to
        // the observed signed bundle. Command success is NOT selection proof.
        let dispatched = tokio::time::timeout(
            Duration::from_secs(2),
            tokio::process::Command::new("/usr/bin/open")
                .arg("-a")
                .arg(bundle)
                .arg(route)
                .stdin(std::process::Stdio::null())
                .stdout(std::process::Stdio::null())
                .stderr(std::process::Stdio::null())
                .kill_on_drop(true)
                .status(),
        )
        .await;
        check()?;
        if !matches!(dispatched, Ok(Ok(status)) if status.success()) {
            return Err(Error::Unavailable(
                "exact-session navigation dispatch failed or timed out; not retried".into(),
            ));
        }
        trace.record(format_args!(
            "exact_id_dispatched=true surface_verified=false"
        ));
    } else {
        return Err(Error::Unavailable(format!(
            "cannot uniquely select the session in the desktop sidebar ({} matches, {} navigation areas, unique title={title_is_unique}, {expansions} expansions, complete={})",
            candidates.len(),
            scan.navigation,
            scan.complete
        )));
    }
    let step = Step::new("selected session surface", Duration::from_secs(3));
    trace.stage("verify_selected_surface");
    let mut renderer_queries = 0u32;
    loop {
        check()?;
        match find_surface(profile, &app, session, resolver) {
            Ok(_) => {
                log::info!(
                    "AX selected {} session {}",
                    profile.app.provider,
                    &session[..8.min(session.len())]
                );
                trace.finish();
                return Ok(());
            }
            Err(Error::NotFound(_)) => {}
            // Ambiguity cannot become authority by polling until one view vanishes.
            Err(error) => return Err(error),
        }
        // Read-only cache refresh after the one navigation press. Hit-test
        // results are discarded; only a fresh exact-session resolution admits
        // later writes. At most three rounds, within the original deadline.
        if !step.expired()
            && renderer_queries < 3
            && step.elapsed() >= Duration::from_secs(u64::from(renderer_queries))
        {
            let mut areas = Vec::new();
            app.walk(|node| {
                if node.role() == "AXWebArea" {
                    areas.push(node.clone());
                }
                areas.len() < 8
            });
            for area in &areas {
                check()?;
                if step.expired() {
                    break;
                }
                let accepted = app.query_renderer(area);
                trace.record(format_args!(
                    "selection_renderer_query round={} accepted={accepted}",
                    renderer_queries + 1
                ));
            }
            renderer_queries += 1;
        }
        if step.expired() {
            // Content-free evidence for an ignored or slow selection: whether
            // the pressed row survived and reports itself selected. Never
            // press it again; a second press is not admitted here.
            trace.record(format_args!(
                "pressed_row_present={} pressed_row_selected={:?} last_surface_error={:?}",
                candidates.first().is_some_and(|row| app.contains(row)),
                candidates.first().and_then(|row| row.bool("AXSelected")),
                find_surface_traced(profile, &app, session, resolver, Some(&trace)).err()
            ));
            return Err(Error::Unavailable(
                "desktop did not show the selected session within 3s".into(),
            ));
        }
        tokio::time::sleep(Duration::from_millis(50)).await;
    }
}

/// CatDesk sidebar rows naming `title`. Its rows are plain pressable groups
/// without a landmark: start from each static text reading the title
/// exactly, take its nearest pressable ancestor, and keep it only if that
/// row's own labels (nested controls excluded) are the title and at most a
/// known badge. Traces are content-free: roles, counts and outcomes.
fn catdesk_rows(area: &Element, title: &str, title_is_unique: bool, trace: &Trace) -> Vec<Element> {
    let mut found: Vec<Element> = Vec::new();
    if !title_is_unique || title.trim().is_empty() {
        trace.record(format_args!(
            "catdesk_rows skipped unique_title={title_is_unique}"
        ));
        return found;
    }
    let mut texts = 0usize;
    let walk = area.walk_report(|node| {
        if node.role() != "AXStaticText" || node.string("AXValue").as_deref() != Some(title) {
            return true;
        }
        texts += 1;
        let mut chain = Vec::new();
        let mut current = node.element("AXParent");
        for _ in 0..6 {
            let Some(parent) = current.take() else {
                break;
            };
            if &parent == area {
                break;
            }
            let role = parent.role();
            let press = parent.has_action("AXPress");
            chain.push(format!(
                "{role}{}{}",
                parent
                    .string("AXSubrole")
                    .map(|s| format!("/{s}"))
                    .unwrap_or_default(),
                if press { "+press" } else { "" }
            ));
            if press && matches!(role.as_str(), "AXGroup" | "AXButton" | "AXLink" | "AXRow" | "AXCell") {
                let (labels, nodes, skipped, stop) =
                    muster_agent::native_sidebar::read_label(&parent);
                let matched = labels
                    .as_deref()
                    .is_some_and(|l| muster_agent::catdesk_ui::row_matches(l, title));
                trace.record(format_args!(
                    "catdesk_row text={texts} chain={chain:?} labels={:?} nodes={nodes} skipped={skipped} stop={stop} matched={matched}",
                    labels.as_ref().map(Vec::len)
                ));
                if matched && !found.contains(&parent) {
                    found.push(parent);
                }
                return true;
            }
            current = parent.element("AXParent");
        }
        trace.record(format_args!(
            "catdesk_text text={texts} no_pressable_row chain={chain:?}"
        ));
        true
    });
    trace.record(format_args!(
        "catdesk_rows title_texts={texts} candidates={} walk={walk:?} (nodes, stop, ms)",
        found.len()
    ));
    found
}

/// The Claude desktop app's resolver: `/epitaxy/<desktop id>` in the URL,
/// mapped to the CLI session.
pub fn claude_resolver() -> Resolver {
    Arc::new(|key: &WebAreaKey| key.url.as_deref().and_then(cli_session_from_url))
}

/// The desktop app's own id of the conversation a web area shows:
/// `local_<uuid>` in current builds, a bare uuid in older ones.
pub fn session_from_url(url: &str) -> Option<String> {
    let rest = url.split("/epitaxy/").nth(1)?;
    let id: String = rest
        .chars()
        .take_while(|c| c.is_ascii_alphanumeric() || *c == '-' || *c == '_')
        .collect();
    let uuid = id.rsplit('_').next().unwrap_or(&id);
    (uuid.len() == 36 && uuid.chars().all(|c| c.is_ascii_hexdigit() || c == '-'))
        .then_some(id.to_ascii_lowercase())
}

/// The CLI session behind a web area's URL.
pub fn cli_session_from_url(url: &str) -> Option<String> {
    session_from_url(url).and_then(|id| cli_session_for(&id))
}

/// The Claude Code session id for the desktop app's conversation id. The
/// desktop app records it in
/// `~/Library/Application Support/Claude/claude-code-sessions/<org>/<user>/<id>.json`
/// as `cliSessionId`; a bare uuid is already the CLI id. Results are
/// read on each resolution: resuming can replace cliSessionId.
pub fn cli_session_for(desktop_id: &str) -> Option<String> {
    if desktop_id.len() == 36 && !desktop_id.contains('_') {
        return Some(desktop_id.to_string());
    }
    desktop_session_file(desktop_id)
        .and_then(|path| std::fs::read_to_string(path).ok())
        .and_then(|text| serde_json::from_str::<serde_json::Value>(&text).ok())
        .and_then(|value| {
            value
                .get("cliSessionId")
                .and_then(|v| v.as_str())
                .map(|s| s.to_ascii_lowercase())
        })
}

fn desktop_session_file(desktop_id: &str) -> Option<std::path::PathBuf> {
    let home = std::env::var_os("HOME")?;
    let root = std::path::PathBuf::from(home)
        .join("Library/Application Support/Claude/claude-code-sessions");
    let file = format!("{desktop_id}.json");
    for org in std::fs::read_dir(&root).ok()?.flatten() {
        let Ok(users) = std::fs::read_dir(org.path()) else {
            continue;
        };
        for user in users.flatten() {
            let candidate = user.path().join(&file);
            if candidate.is_file() {
                return Some(candidate);
            }
        }
    }
    None
}

#[derive(Clone)]
struct Surface {
    web_area: Element,
    composer: Option<Element>,
    sends: Vec<Element>,
    // Content-free metadata from this scan; diagnostics must not walk again.
    control_diagnostics: Vec<String>,
}

impl Surface {
    fn record_controls(&self, trace: &Trace) {
        for detail in &self.control_diagnostics {
            trace.record(format_args!("{detail}"));
        }
    }
}

fn find_surface(
    profile: &AppProfile,
    app: &Element,
    session: &str,
    resolver: &Resolver,
) -> Result<Surface, Error> {
    find_surface_traced(profile, app, session, resolver, None)
}

/// OS facts about the windows that hold several views of one conversation:
/// the app's main window and its front-to-back window order. Read only.
fn view_places(app: &Element, views: &[Element]) -> Vec<muster_agent::native_scan::ViewPlace> {
    let main = app.element("AXMainWindow");
    let windows = app
        .attribute("AXWindows")
        .map(|value| Element::elements_of(&value))
        .unwrap_or_default();
    views
        .iter()
        .map(|view| {
            let window = view.element("AXWindow");
            muster_agent::native_scan::ViewPlace {
                main_window: window.is_some() && window == main,
                window_rank: window
                    .as_ref()
                    .and_then(|window| windows.iter().position(|w| w == window)),
                minimized: window
                    .as_ref()
                    .and_then(|window| window.bool("AXMinimized"))
                    .unwrap_or(false),
            }
        })
        .collect()
}

/// Why one surface scan did not produce a surface.
enum SurfaceScan {
    /// The control scan stopped early; Shared Rust decides whether to repeat.
    Incomplete(muster_agent::native_scan::ScanStop),
    Failed(Error),
}

impl From<Error> for SurfaceScan {
    fn from(error: Error) -> Self {
        Self::Failed(error)
    }
}

fn find_surface_traced(
    profile: &AppProfile,
    app: &Element,
    session: &str,
    resolver: &Resolver,
    trace: Option<&Trace>,
) -> Result<Surface, Error> {
    use muster_agent::native_scan::{SCAN_RETRY_PAUSE, repeat_scan};
    let mut attempts = 0;
    loop {
        attempts += 1;
        match scan_surface(profile, app, session, resolver, trace) {
            Ok(surface) => return Ok(surface),
            Err(SurfaceScan::Incomplete(stop)) if repeat_scan(stop, attempts) => {
                log::info!(
                    "native_app scan of {}: controls stopped early ({}); walking again after attempt {attempts}",
                    &session[..8.min(session.len())],
                    stop.describe()
                );
                std::thread::sleep(SCAN_RETRY_PAUSE);
            }
            Err(SurfaceScan::Incomplete(stop)) => {
                log::warn!(
                    "native_app scan of {}: controls stopped early ({}) after {attempts} walks; not sent",
                    &session[..8.min(session.len())],
                    stop.describe()
                );
                return Err(Error::Unavailable(format!(
                    "The native control scan was incomplete ({}); not sent",
                    stop.describe()
                )));
            }
            Err(SurfaceScan::Failed(error)) => return Err(error),
        }
    }
}

fn scan_surface(
    profile: &AppProfile,
    app: &Element,
    session: &str,
    resolver: &Resolver,
    trace: Option<&Trace>,
) -> Result<Surface, SurfaceScan> {
    use muster_agent::native_scan::descend_web_area;
    // Every view that shows the session; Shared Rust picks one when several do.
    let mut views: Vec<Element> = Vec::new();
    let mut areas = 0usize;
    // A conversation's view is resolved from its own key; its transcript is
    // never needed here and is left to the control scan.
    let walk = app.walk_pruned(|node| {
        if node.role() != "AXWebArea" {
            return ax::Visit::Descend;
        }
        areas += 1;
        let key = WebAreaKey::of(node);
        let resolved = resolver(&key);
        if let Some(trace) = trace.filter(|_| areas <= 8) {
            let route = key.url.as_deref().map(|url| {
                use muster_agent::catdesk_ui::{Route, route};
                if url.contains("/epitaxy/") { "claude_session".to_string() }
                else if url.contains("/epitaxy") { "claude_home".to_string() }
                else if let Some(r) = route(url) {
                    if r == Route::Main { "catdesk_main" } else { "catdesk_popout" }.to_string()
                }
                else if url.contains("/app.asar/dist/index.html") {
                    // The page's route name only: never its query.
                    let path = url.split_once('#').map_or("", |(_, f)| f.split('?').next().unwrap_or(""));
                    format!("catdesk_other:{}", label(path))
                }
                else if url.starts_with("file:") { "app_shell".to_string() }
                else { "other".to_string() }
            });
            trace.record(format_args!("web_area={areas} route={route:?} title_chars={} resolved_session={:?} target_match={}",
                key.title.as_ref().map_or(0, |s| s.chars().count()), resolved.as_ref().map(|s| label(s)), resolved.as_deref() == Some(session)));
        }
        if resolved.as_deref() == Some(session) {
            views.push(node.clone());
        }
        if descend_web_area(resolved.is_some()) {
            ax::Visit::Descend
        } else {
            ax::Visit::Skip
        }
    });
    if let Some(trace) = trace {
        trace.record(format_args!(
            "web_areas={areas} target_found={} target_views={} walk={walk:?} (nodes, stop, ms)",
            !views.is_empty(),
            views.len()
        ));
    }
    if walk.1 != "complete" {
        // Only the ceiling or the budget can stop this walk: its visitor
        // never ends it, and an unreadable node just has no children.
        log::warn!(
            "native_app scan of {}: session scan stopped at {} after {} nodes, {} ms ({areas} web areas)",
            &session[..8.min(session.len())],
            walk.1,
            walk.0,
            walk.2
        );
        return Err(Error::Unavailable(format!(
            "The native session scan was incomplete ({}); not sent",
            walk.1.replace('_', " ")
        ))
        .into());
    }
    let web_area = match views.len() {
        0 => {
            return Err(
                Error::NotFound(format!("session {session} is not open in the app")).into(),
            );
        }
        1 => views.remove(0),
        _ => {
            let places = view_places(app, &views);
            let chosen = muster_agent::native_scan::choose_view(&places);
            let facts = places
                .iter()
                .map(|p| {
                    format!(
                        "(main={} rank={:?} minimized={})",
                        p.main_window, p.window_rank, p.minimized
                    )
                })
                .collect::<Vec<_>>()
                .join(" ");
            if let Some(trace) = trace {
                trace.record(format_args!(
                    "duplicate_views={} chosen={chosen:?} places={facts}",
                    views.len()
                ));
            }
            let Some(index) = chosen else {
                log::warn!(
                    "native_app scan of {}: {} views show this session and none is the main or frontmost visible one: {facts}",
                    &session[..8.min(session.len())],
                    views.len()
                );
                return Err(
                    Error::Unavailable("more than one view shows this session".into()).into(),
                );
            };
            log::info!(
                "native_app scan of {}: {} views show this session; using view {} {}",
                &session[..8.min(session.len())],
                views.len(),
                index + 1,
                if places[index].main_window {
                    "in the main window"
                } else {
                    "in the frontmost visible window"
                }
            );
            views.swap_remove(index)
        }
    };
    let Controls {
        composer,
        send,
        diagnostics,
        stop,
    } = find_controls(profile, &web_area);
    if let Some(trace) = trace {
        for detail in &diagnostics {
            trace.record(format_args!("{detail}"));
        }
    }
    if let Some(stop) = stop {
        return Err(SurfaceScan::Incomplete(stop));
    }
    if composer.len() > 1 {
        return Err(
            Error::Unavailable(format!("expected one prompt, found {}", composer.len())).into(),
        );
    }
    Ok(Surface {
        web_area,
        composer: composer.into_iter().next(),
        sends: send,
        control_diagnostics: diagnostics,
    })
}

const CONTROL_ATTRIBUTES: [&str; 8] = [
    "AXRole",
    "AXTitle",
    "AXDescription",
    "AXPlaceholderValue",
    "AXChildren",
    "AXEnabled",
    "AXFocused",
    "AXSubrole",
];

fn control_metadata(
    values: &[Option<core_foundation::base::CFType>],
) -> Option<muster_agent::desktop_chat::AxNode> {
    let text = |i: usize| {
        values
            .get(i)
            .and_then(Option::as_ref)
            .and_then(ax::value_string)
            .unwrap_or_default()
    };
    let role = text(0);
    if role.is_empty() {
        return None;
    }
    Some(muster_agent::desktop_chat::AxNode {
        role,
        title: text(1),
        description: text(2),
        placeholder: text(3),
        ..Default::default()
    })
}

struct Controls {
    composer: Vec<Element>,
    send: Vec<Element>,
    diagnostics: Vec<String>,
    /// Why the walk stopped before the whole web area; `None` when complete.
    stop: Option<muster_agent::native_scan::ScanStop>,
}

fn find_controls(profile: &AppProfile, web_area: &Element) -> Controls {
    use muster_agent::native_scan::choose_composer;
    use muster_agent::native_send::{ControlNode, composer_submits_for};
    let (mut composer, mut send) = (Vec::new(), Vec::new());
    let mut nodes = Vec::new();
    let mut subroles = Vec::new();
    let mut elements = Vec::new();
    let mut todo = std::collections::VecDeque::from([(web_area.clone(), None)]);
    let mut buttons = 0;
    let mut diagnostics = Vec::new();
    let started = std::time::Instant::now();
    let mut stop = None;
    while let Some((node, parent)) = todo.pop_front() {
        use muster_agent::native_scan::ScanStop;
        if elements.len() >= ax::MAX_NODES {
            stop = Some(ScanStop::NodeLimit);
            break;
        }
        if started.elapsed() > ax::WALK_BUDGET {
            stop = Some(ScanStop::TimeLimit);
            break;
        }
        if elements.contains(&node) {
            continue;
        }
        let index = elements.len();
        // One IPC per node for both selector matching and traversal. Never
        // fetch transcript values: only control metadata is needed here.
        let values = match node.try_attributes(&CONTROL_ATTRIBUTES) {
            Ok(values) => values,
            Err(code) => {
                stop = Some(ScanStop::NodeRead(Some(code)));
                break;
            }
        };
        let Some(field) = control_metadata(&values) else {
            // A live element always has a role; one without is going away.
            stop = Some(ScanStop::NodeRead(None));
            break;
        };
        let is_composer = profile.controls().composer.matches(&field);
        let is_send = !is_composer && profile.controls().send.matches(&field);
        let is_button = field.role == "AXButton";
        let queued_delete = is_button
            && profile
                .controls()
                .queue
                .as_ref()
                .is_some_and(|queue| queue.row.matches(&field));
        subroles.push(
            values[7]
                .as_ref()
                .and_then(ax::value_string)
                .unwrap_or_default(),
        );
        nodes.push(ControlNode {
            parent,
            composer: is_composer,
            submit: is_send,
            queued_delete,
        });
        if is_composer {
            composer.push(node.clone());
            // Generic toolbar samples are often exhausted before the editor.
            // Preserve a separate bounded sample; never read the draft value.
            if log::log_enabled!(log::Level::Info) && composer.len() <= 4 {
                diagnostics.push(format!(
                    "composer index={index} parent={parent:?} title={:?} description={:?} placeholder={:?} enabled={:?} focused={:?}",
                    control_label(&field.title),
                    control_label(&field.description),
                    control_label(&field.placeholder),
                    values[5].as_ref().and_then(ax::value_bool),
                    values[6].as_ref().and_then(ax::value_bool)
                ));
            }
        }
        buttons += usize::from(is_button);
        if log::log_enabled!(log::Level::Info) {
            // Always report semantic submit/Stop controls, even after unrelated
            // toolbar buttons have exhausted the small generic sample budget.
            if is_send
                || (is_button && profile.controls().stop.matches(&field))
                || ((is_composer || is_button)
                    && diagnostics.len() < 16
                    && started.elapsed() < Duration::from_millis(150))
            {
                diagnostics.push(format!("control role={} title={:?} description={:?} composer_match={is_composer} send_match={is_send} enabled={:?}",
                    field.role, control_label(&field.title), control_label(&field.description), values[5].as_ref().and_then(ax::value_bool)));
            }
        }
        if let Some(children) = &values[4] {
            todo.extend(
                Element::elements_of(children)
                    .into_iter()
                    .map(|child| (child, Some(index))),
            );
        }
        elements.push(node);
    }
    let complete = stop.is_none();
    if complete {
        let selected = choose_composer(profile.controls().composer_scope, &nodes, &subroles);
        diagnostics.push(format!(
            "composer_scope={:?} candidates={} selected={selected:?}",
            profile.controls().composer_scope,
            composer.len()
        ));
        if let Some(index) = selected {
            composer = vec![elements[index].clone()];
        } else if composer.len() == 1 {
            // A sole editor explicitly inside an auxiliary pane is not the
            // conversation's composer. Multiple ambiguous candidates stay
            // present so find_surface reports the ambiguity rather than a miss.
            composer.clear();
        }
        // Queued rows carry their own submit controls: only those in the
        // composer's branch send.
        if profile.controls().queue.is_some() {
            send = selected
                .map(|index| composer_submits_for(&nodes, index))
                .unwrap_or_default()
                .into_iter()
                .map(|i| elements[i].clone())
                .collect();
        } else {
            send = nodes
                .iter()
                .enumerate()
                .filter(|(_, n)| n.submit)
                .map(|(i, _)| elements[i].clone())
                .collect();
        }
    }
    // Electron rebuilds the composer and its controls around a write. A
    // candidate it destroyed answers kAXErrorInvalidUIElement and can press
    // nothing, so it must not make the live control ambiguous. Its unreadable
    // enabled state otherwise refused the send as "found 2".
    let stale = send
        .iter()
        .filter(|button| button.status("AXEnabled") == ax::INVALID_ELEMENT)
        .count();
    send.retain(|button| button.status("AXEnabled") != ax::INVALID_ELEMENT);
    let enabled: Vec<_> = send.iter().map(|button| button.bool("AXEnabled")).collect();
    send = muster_agent::native_send::actionable_submits(&enabled)
        .into_iter()
        .map(|index| send[index].clone())
        .collect();
    diagnostics.push(format!("controls composers={} send_buttons={} stale_sends={stale} all_buttons={buttons} logged_controls={} walk=({}, complete={complete}, {} ms) stop={:?}", composer.len(), send.len(), diagnostics.len(), nodes.len(), started.elapsed().as_millis(), stop.map(|s| s.describe())));
    Controls {
        composer,
        send,
        diagnostics,
        stop,
    }
}

struct Shared {
    state: SessionState,
    interaction: Option<PendingInteraction>,
    additional: Vec<PendingInteraction>,
    options: Vec<(String, Element)>,
}

pub struct NativeAppController {
    profile: &'static AppProfile,
    resolver: Resolver,
    pid: u32,
    start: u64,
    session: String,
    app: Element,
    surface: Mutex<Surface>,
    shared: Mutex<Shared>,
    /// A submitted write without provider evidence blocks later writes.
    /// Its own lock: evidence reads never hold up state or pending readers.
    unconfirmed: muster_agent::unconfirmed::WriteGuard,
    /// Question presses and text writes made by the current operation.
    question_writes: std::sync::atomic::AtomicU64,
    write: tokio::sync::Mutex<()>,
    events: ControllerEvents,
    question_source: muster_agent::questions::Source,
    send_receipt_source: muster_agent::receipt::Source,
    queue_source: muster_agent::queue::Source,
    interaction_epoch: u128,
    next_interaction: std::sync::atomic::AtomicU64,
}

// SAFETY: AXUIElementRef is a CF object usable from any thread; access is serialised by the mutexes.
unsafe impl Send for NativeAppController {}
unsafe impl Sync for NativeAppController {}

impl NativeAppController {
    #[allow(clippy::too_many_arguments)]
    pub async fn attach(
        profile: &'static AppProfile,
        pid: u32,
        session: &str,
        resolver: Resolver,
        events: ControllerEvents,
        question_source: muster_agent::questions::Source,
        send_receipt_source: muster_agent::receipt::Source,
        queue_source: muster_agent::queue::Source,
        runtime: tokio::runtime::Handle,
    ) -> Result<Arc<Self>, Error> {
        if !ax::trusted() {
            return Err(Error::Unavailable(
                "musterd lacks Accessibility permission".into(),
            ));
        }
        if !codesign::process_satisfies(pid, &profile.app.requirement()) {
            return Err(Error::Invalid(format!(
                "the process is not the {} desktop app",
                profile.app.provider
            )));
        }
        let start =
            codesign::process_start(pid).ok_or_else(|| Error::NotFound(format!("pid {pid}")))?;
        let app = Element::application(pid);
        app.enable_manual_accessibility();
        let surface = find_surface(profile, &app, session, &resolver)?;
        let controller = Arc::new(Self {
            profile,
            resolver,
            pid,
            start,
            session: session.to_string(),
            app,
            surface: Mutex::new(surface),
            write: tokio::sync::Mutex::new(()),
            shared: Mutex::new(Shared {
                state: SessionState::Idle,
                interaction: None,
                additional: Vec::new(),
                options: Vec::new(),
            }),
            unconfirmed: Default::default(),
            question_writes: Default::default(),
            events,
            question_source,
            send_receipt_source,
            queue_source,
            interaction_epoch: rand::random(),
            next_interaction: std::sync::atomic::AtomicU64::new(1),
        });
        runtime.spawn(controller.clone().poll_loop());
        Ok(controller)
    }

    /// The target is still the process we attached to and still shows the session.
    fn check_identity(&self) -> Result<(), Error> {
        if codesign::process_start(self.pid) != Some(self.start) {
            return Err(Error::Unavailable("the app was restarted".into()));
        }
        if screen_locked() {
            // Electron apps expose no windows to AX while the screen is
            // locked, so the tree cannot be walked; the elements retained
            // from before the lock are used as they are (P0-1).
            return Ok(());
        }
        let mut surface = self.surface.lock().unwrap();
        let same_session = (self.resolver)(&WebAreaKey::of(&surface.web_area)).as_deref()
            == Some(self.session.as_str());
        if !same_session
            || surface
                .composer
                .as_ref()
                .is_none_or(|c| !surface.web_area.contains(c))
        {
            // Electron can replace its controls while locked. A write can
            // arrive immediately after unlock, before the background poll.
            // Resolve the same session uniquely in the same process; never
            // adopt whichever view happens to be focused or reset this
            // controller's unknown-write / interaction state by reattaching.
            let replacement = find_surface(self.profile, &self.app, &self.session, &self.resolver)?;
            if codesign::process_start(self.pid) != Some(self.start) || screen_locked() {
                return Err(Error::Unavailable(
                    "the app or lock state changed while refreshing the session view".into(),
                ));
            }
            *surface = replacement;
            log::info!(
                "native_app refreshed session view for {}",
                &self.session[..8.min(self.session.len())]
            );
        }
        // Send candidates are consumed only from a fresh surface scan, never
        // from this cache. Validating unused cached buttons would walk the
        // whole conversation again without making the next press safer.
        Ok(())
    }

    fn check_write_surface(&self, web_area: &Element, composer: &Element) -> Result<(), Error> {
        self.check_identity()?;
        if (self.resolver)(&WebAreaKey::of(web_area)).as_deref() != Some(self.session.as_str())
            || !self.app.contains(web_area)
            || !web_area.contains(composer)
        {
            return Err(Error::Unavailable(
                "the selected session view changed before input".into(),
            ));
        }
        Ok(())
    }

    /// Read-only, content-free view of what the controller would drive:
    /// the composer (counts, and which attributes the app lets AX set), the
    /// send and stop controls by their known labels. No value is written and
    /// nothing is pressed; the draft is reported by length only.
    fn surface_diagnostic(&self) -> Result<String, Error> {
        muster_agent::control_guard::check()?;
        self.check_identity()?;
        let fresh = find_surface(self.profile, &self.app, &self.session, &self.resolver)?;
        let composer = fresh.composer.as_ref().map(|c| {
            serde_json::json!({
                "role": c.role(),
                "subrole": c.string("AXSubrole"),
                "description": control_label(&text(c, "AXDescription")),
                "draft_chars": c.string("AXValue").map(|v| v.chars().count()),
                "settable": {
                    "AXValue": c.settable("AXValue"),
                    "AXSelectedText": c.settable("AXSelectedText"),
                    "AXSelectedTextRange": c.settable("AXSelectedTextRange"),
                    "AXFocused": c.settable("AXFocused"),
                },
                "focused": c.bool("AXFocused"),
                "actions": c.action_names(),
            })
        });
        let mut stops = 0usize;
        fresh.web_area.walk(|node| {
            if self.profile.controls().stop.matches(node) {
                stops += 1;
            }
            true
        });
        Ok(serde_json::json!({
            "session": self.session,
            "provider": self.profile.app.provider,
            "read_only": true,
            "web_area_url_present": fresh.web_area.string("AXURL").is_some(),
            "composers": usize::from(fresh.composer.is_some()),
            "composer": composer,
            "send_buttons": fresh.sends.iter().map(|b| serde_json::json!({
                "label": control_label(&button_label(b)),
                "enabled": b.bool("AXEnabled"),
            })).collect::<Vec<_>>(),
            "stop_buttons": stops,
        })
        .to_string())
    }

    async fn poll_loop(self: Arc<Self>) {
        let mut previous_process = None;
        loop {
            tokio::time::sleep(STATE_POLL).await;
            if Arc::strong_count(&self) == 1 {
                return;
            }
            let process = self.process_state();
            if previous_process != Some(process) {
                previous_process = Some(process);
                let _ = self
                    .events
                    .send((self.session.clone(), ControllerEvent::State(self.state())))
                    .await;
            }
            match process {
                ProcessState::Ended => {
                    self.unobserve(SessionState::Ended).await;
                    let _ = self
                        .events
                        .send((
                            self.session.clone(),
                            ControllerEvent::Detached {
                                process: self.process_identity(),
                            },
                        ))
                        .await;
                    return;
                }
                ProcessState::Unknown => {
                    self.unobserve(SessionState::Unknown).await;
                    continue;
                }
                ProcessState::Running => {}
            }
            // Provider evidence is a file read, not an AX one: an unconfirmed
            // write is reconciled while locked or while another conversation
            // is shown, not only when this one happens to be visible.
            if self.unconfirmed.is_unconfirmed() {
                self.unconfirmed.reconcile();
            }
            // Losing an AX surface (tab switch, lock or TCC change) is not a process exit.
            if screen_locked() || !ax::trusted() || self.check_identity().is_err() {
                self.unobserve(SessionState::Unknown).await;
                continue;
            }
            self.observe().await;
        }
    }

    async fn unobserve(&self, state: SessionState) {
        let snapshot = (self.question_source)();
        let mut events = Vec::new();
        {
            let mut shared = self.shared.lock().unwrap();
            let question =
                question_forms::pending_question(snapshot.as_ref(), shared.interaction.as_ref());
            question_forms::update_snapshot(
                &mut shared.interaction,
                question,
                snapshot.as_ref(),
                &mut events,
            );
            question_forms::update_additional(
                &mut shared.additional,
                snapshot.as_ref(),
                &mut events,
            );
            shared.options.clear();
            if shared.state != state {
                shared.state = state;
                events.push(ControllerEvent::State(state));
            }
        }
        for event in events {
            let _ = self.events.send((self.session.clone(), event)).await;
        }
    }

    /// Reads the visible state: an enabled Stop control means running,
    /// enabled approval options mean a decision is pending.
    async fn observe(&self) {
        let web_area = self.surface.lock().unwrap().web_area.clone();
        let controls = self.profile.controls();
        let mut stop = false;
        let mut draft = false;
        let mut options: Vec<(String, Element)> = Vec::new();
        let mut disabled_options = 0;
        web_area.walk(|node| {
            let role = node.role();
            if role == "AXTextArea" && !draft {
                draft = holds_draft(self.profile, node, role);
            } else if role == "AXButton" {
                let button = names(node, role);
                let enabled = || node.bool("AXEnabled").unwrap_or(false);
                if controls.stop.matches(&button) && enabled() {
                    stop = true;
                } else if controls.approve.matches(&button)
                    && approval_in_scope(self.profile, &web_area, node)
                {
                    if enabled() {
                        options.push((button.label().to_owned(), node.clone()));
                    } else {
                        disabled_options += 1;
                    }
                }
            }
            true
        });
        let mut snapshot = (self.question_source)();
        if !options.is_empty() {
            if let Some(snapshot) = &mut snapshot {
                if snapshot.pending.as_ref().is_some_and(|q| q.optional) {
                    let suggestion = snapshot.pending.take().unwrap();
                    snapshot.additional.insert(0, suggestion);
                }
            }
        }
        let mut to_send = Vec::new();
        {
            let mut shared = self.shared.lock().unwrap();
            let question = if options.is_empty() {
                question_forms::pending_question(snapshot.as_ref(), shared.interaction.as_ref())
            } else {
                snapshot.as_ref().and_then(|s| s.pending.clone())
            };
            let labels: Vec<String> = options.iter().map(|(t, _)| t.clone()).collect();
            let previous: Vec<String> = shared.options.iter().map(|(t, _)| t.clone()).collect();
            if question.is_some()
                || labels.is_empty()
                    && shared
                        .interaction
                        .as_ref()
                        .is_some_and(|i| i.kind == "question")
            {
                question_forms::update_snapshot(
                    &mut shared.interaction,
                    question,
                    snapshot.as_ref(),
                    &mut to_send,
                );
                shared.options.clear();
            } else if labels != previous {
                log::info!(
                    "AX approval options changed: session={} provider={} enabled={} disabled={} provider_receipt=not_checked",
                    self.session,
                    self.profile.app.provider,
                    labels.len(),
                    disabled_options
                );
                if let Some(old) = shared.interaction.take()
                    && !snapshot.as_ref().is_some_and(|s| s.find(&old.id).is_some())
                {
                    to_send.push(ControllerEvent::Resolved {
                        interaction: old.id,
                    });
                }
                if !labels.is_empty() {
                    let id = format!(
                        "ax-{:032x}-{}",
                        self.interaction_epoch,
                        self.next_interaction
                            .fetch_add(1, std::sync::atomic::Ordering::Relaxed)
                    );
                    let interaction = PendingInteraction {
                        id,
                        session: self.session.clone(),
                        kind: "approval".into(),
                        optional: false,
                        prompt: format!("{} asks for permission", self.profile.app.provider),
                        options: labels,
                        questions: Vec::new(),
                    };
                    shared.interaction = Some(interaction.clone());
                    to_send.push(ControllerEvent::Interaction(interaction));
                }
                shared.options = options;
            }
            question_forms::update_additional(
                &mut shared.additional,
                snapshot.as_ref(),
                &mut to_send,
            );
            let state = muster_agent::native_send::observed_state(
                shared.interaction.as_ref().is_some_and(|q| !q.optional),
                stop,
                draft,
            );
            if state != shared.state {
                shared.state = state;
                to_send.push(ControllerEvent::State(state));
            }
        }
        for event in to_send {
            let _ = self.events.send((self.session.clone(), event)).await;
        }
    }
}

impl NativeAppController {
    fn composer_question_guard(&self, expected: Option<&PendingInteraction>) -> Result<(), Error> {
        let snapshot = (self.question_source)();
        if snapshot.as_ref().is_some_and(|s| {
            s.iter()
                .any(|q| !q.optional && !muster_agent::claude_cards::uses_composer(q))
        }) {
            return Err(Error::Conflict(
                "Answer the pending question card before sending a new message".into(),
            ));
        }
        if let Some(expected) = expected {
            if !snapshot
                .and_then(|s| s.find(&expected.id).cloned())
                .is_some_and(|q| muster_agent::questions::remaining_matches(expected, &q))
            {
                return Err(Error::Conflict(
                    "The question was answered or changed; not sent".into(),
                ));
            }
        }
        Ok(())
    }
    /// Empty the composer and confirm it empty before any draft write.
    /// Electron's rich editors merge a value written over a draft, so the
    /// exact echo could never match and the request was reported as not sent
    /// while a mixed draft stayed behind. Shared Rust owns the policy; the
    /// surface is resolved again between attempts because a write can rebuild
    /// it. Nothing here presses anything.
    /// `answering` is the answer path, where a waiting question card is
    /// expected and must not refuse the clear; a send refuses while a card
    /// waits, as it always has. It also reports `None` for a conversation
    /// with no visible composer: there is nothing to clear, and the card's
    /// own controls are what the answer drives. A send needs the composer and
    /// says so.
    async fn empty_composer(
        &self,
        trace: &Trace,
        generation: u64,
        expected: Option<&PendingInteraction>,
        answering: bool,
    ) -> Result<Option<Element>, Error> {
        use muster_agent::native_send::{ClearWait, Clearing};
        let started = std::time::Instant::now();
        let mut wait = ClearWait::default();
        loop {
            muster_agent::control_guard::check()?;
            if user_input_generation() != generation {
                return Err(Error::Conflict(
                    "local input changed before clearing the composer; not sent".into(),
                ));
            }
            if !answering {
                self.composer_question_guard(expected)?;
            }
            let fresh = find_surface(self.profile, &self.app, &self.session, &self.resolver)?;
            let Some(composer) = fresh.composer.clone() else {
                *self.surface.lock().unwrap() = fresh;
                if answering {
                    return Ok(None);
                }
                return Err(Error::Unavailable(
                    "The native prompt is not visible; not sent".into(),
                ));
            };
            self.check_write_surface(&fresh.web_area, &composer)?;
            *self.surface.lock().unwrap() = fresh;
            let value = composer.string("AXValue");
            let draft = value.as_deref().map(|value| !self.profile.is_empty(value));
            match wait.observe(started.elapsed(), draft) {
                Clearing::Done => {
                    if wait.attempts() > 0 {
                        trace.record(format_args!(
                            "composer_cleared attempts={} elapsed_ms={}",
                            wait.attempts(),
                            started.elapsed().as_millis()
                        ));
                    }
                    return Ok(Some(composer));
                }
                Clearing::Clear(with) => {
                    trace.record(format_args!(
                        "composer_clear with={with:?} value_chars={:?} elapsed_ms={}",
                        value.as_ref().map(|value| value.chars().count()),
                        started.elapsed().as_millis()
                    ));
                    clear_editor(self.profile, &composer, with).map_err(|code| {
                        Error::Unavailable(format!("AX composer clear failed ({code}); not sent"))
                    })?;
                }
                Clearing::Wait => {}
                Clearing::Timeout => {
                    log::warn!(
                        "native_app send to {}: the composer still held a draft after {} ms ({} clear attempts); not sent",
                        &self.session[..8.min(self.session.len())],
                        started.elapsed().as_millis(),
                        wait.attempts()
                    );
                    return Err(Error::Unavailable(
                        "the native composer could not be emptied; nothing was written or sent"
                            .into(),
                    ));
                }
                Clearing::Unreadable => {
                    return Err(Error::Unavailable(
                        "the prompt cannot be read; not sent".into(),
                    ));
                }
            }
            tokio::time::sleep(ECHO_POLL).await;
        }
    }

    async fn send_prompt(
        &self,
        text: &str,
        expected: Option<&PendingInteraction>,
    ) -> Result<(), Error> {
        let mut trace = Trace::new("send", self.profile.app.provider, &self.session);
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
        self.check_identity()?;
        self.composer_question_guard(expected)?;
        if (self.question_source)().is_some_and(|s| {
            s.iter()
                .any(|q| !q.optional && !muster_agent::claude_cards::uses_composer(q))
        }) {
            return Err(Error::Conflict(
                "Answer the pending question card before sending a new message".into(),
            ));
        }
        // Re-read the provider evidence of an earlier unconfirmed write, then
        // refuse before any input: this request is then definitely not sent.
        self.unconfirmed.admit()?;
        if let Some(state) = super::lock::session_state() {
            log::info!(
                "native_app send to {}: screen_locked={} login_done={} on_console={}",
                &self.session[..8.min(self.session.len())],
                state.screen_locked,
                state.login_done,
                state.on_console
            );
        }
        // Preparation is read-only. A brief input change here may settle;
        // every retry resolves this exact session and reads its draft afresh.
        trace.stage("prepare_draft");
        let ((mut action, composer), generation) =
            muster_agent::control_guard::prepare_stable(user_input_generation, || {
                self.check_identity()?;
                self.composer_question_guard(expected)?;
                // A cached composer may still belong to the web area after
                // Electron replaces its active editor. Always resolve afresh.
                let fresh = find_surface(self.profile, &self.app, &self.session, &self.resolver)?;
                let composer = fresh
                    .composer
                    .clone()
                    .ok_or_else(|| Error::Unavailable("The native prompt is not visible".into()))?;
                let action =
                    draft_action(self.profile, composer.string("AXValue").as_deref(), text)?;
                self.check_write_surface(&fresh.web_area, &composer)?;
                *self.surface.lock().unwrap() = fresh;
                Ok((action, composer))
            })
            .await?;
        muster_agent::control_guard::check()?;
        if user_input_generation() != generation {
            return Err(Error::Conflict(
                "local input changed before drafting; not sent".into(),
            ));
        }
        if action == DraftAction::Write {
            // Activate the editor before replacing its whole value. Focus can
            // rebuild it; resolve the same session again before any text write.
            trace.stage("focus_draft");
            composer.set_bool("AXFocused", true).map_err(|code| {
                Error::Unavailable(format!("AX composer focus failed ({code}); not sent"))
            })?;
            let fresh = find_surface(self.profile, &self.app, &self.session, &self.resolver)?;
            let current = fresh.composer.clone().ok_or_else(|| {
                Error::Unavailable("The native prompt is not visible after focus; not sent".into())
            })?;
            self.check_write_surface(&fresh.web_area, &current)?;
            action = draft_action(self.profile, current.string("AXValue").as_deref(), text)?;
            *self.surface.lock().unwrap() = fresh;
            muster_agent::control_guard::check()?;
            if user_input_generation() != generation {
                return Err(Error::Conflict(
                    "local input changed during focus; not sent".into(),
                ));
            }
            if action == DraftAction::Write {
                // Start from an empty composer: a write over a draft is
                // merged, never replaced, so the echo below could not match.
                trace.stage("clear_draft");
                let current = self
                    .empty_composer(&trace, generation, expected, false)
                    .await?
                    .ok_or_else(|| {
                        Error::Unavailable("The native prompt is not visible; not sent".into())
                    })?;
                trace.stage("write_draft");
                self.composer_question_guard(expected)?;
                draft(self.profile, &current, text).map_err(|code| {
                    Error::Unavailable(format!("AX draft write failed ({code}); not sent"))
                })?;
            }
        }
        log::info!(
            "native_app send to {}: draft {:?}; checking echo",
            &self.session[..8.min(self.session.len())],
            action
        );
        trace.record(format_args!("draft_action={action:?}"));
        trace.stage("draft_echo");
        // Setting AXValue can rebuild Electron's composer and send button.
        // Re-resolve only this verified session, then confirm the exact draft.
        let started = std::time::Instant::now();
        let mut readiness = muster_agent::native_send::DraftWait::default();
        // Content-free evidence for an unconfirmed draft: how the value differs
        // and whether Electron replaced the composer element meanwhile.
        let mut previous_composer: Option<Element> = None;
        let mut composer_changes = 0usize;
        let mut last_miss: Option<&'static str> = None;
        let (web_area, composer, send) = loop {
            use muster_agent::native_send::Readiness;
            muster_agent::control_guard::check()?;
            if user_input_generation() != generation {
                return Err(Error::Conflict(
                    "local input changed; the draft was preserved, not sent".into(),
                ));
            }
            self.check_identity()?;
            self.composer_question_guard(expected)?;
            // Do not wait on the cached element: Electron may retain an old
            // composer in its tree while replacing the active editor.
            let mut fresh = find_surface(self.profile, &self.app, &self.session, &self.resolver)?;
            // A covered Chromium renderer may leave both AXValue and Send in
            // the browser cache after a successful write. Ask the renderer for
            // a read-only update, then resolve the exact session again. Never
            // rewrite the draft or use the hit-test result as an action target.
            let cached_echo = fresh
                .composer
                .as_ref()
                .and_then(|node| node.string("AXValue"))
                .is_some_and(|value| value.trim_end() == text.trim_end());
            if !cached_echo && started.elapsed() < muster_agent::native_send::ECHO_DEADLINE {
                let queried = self.app.query_renderer(&fresh.web_area);
                trace.record(format_args!("draft_renderer_query accepted={queried}"));
                fresh = find_surface(self.profile, &self.app, &self.session, &self.resolver)?;
            }
            let current = fresh.composer.clone();
            if previous_composer.is_some() && previous_composer != current {
                composer_changes += 1;
            }
            previous_composer = current.clone();
            let value = current.as_ref().and_then(|node| node.string("AXValue"));
            let echoed = value
                .as_deref()
                .is_some_and(|v| v.trim_end() == text.trim_end());
            if !echoed {
                let miss = if current.is_none() {
                    "no_composer"
                } else if value
                    .as_deref()
                    .is_some_and(|v| !v.trim().is_empty() && self.profile.is_empty(v))
                {
                    // The composer still reports its placeholder: the written
                    // text has not reached this AX read (not a different draft).
                    "placeholder"
                } else {
                    muster_agent::native_send::echo_mismatch(value.as_deref(), text)
                };
                if last_miss != Some(miss) {
                    last_miss = Some(miss);
                    trace.record(format_args!(
                        "echo_miss value={miss} value_chars={:?} text_chars={} composer_changes={composer_changes} elapsed_ms={}",
                        value.as_ref().map(|v| v.chars().count()),
                        text.chars().count(),
                        started.elapsed().as_millis()
                    ));
                }
            }
            // The surface scan already resolved all submit candidates in this
            // exact composer branch. Keep their count (including ambiguity)
            // and re-read enabled state without a second tree traversal.
            let buttons = &fresh.sends;
            let enabled = buttons.len() == 1 && buttons[0].bool("AXEnabled").unwrap_or(false);
            *self.surface.lock().unwrap() = fresh.clone();
            let state = readiness.observe(started.elapsed(), echoed, buttons.len(), enabled);
            if state != Readiness::Wait {
                fresh.record_controls(&trace);
                trace.record(format_args!("readiness={state:?} echoed={echoed} send_buttons={} enabled={enabled} elapsed_ms={}", buttons.len(), started.elapsed().as_millis()));
            }
            match state {
                Readiness::Ready => {
                    let current = current.ok_or_else(|| {
                        Error::Unavailable("The native prompt is not visible; not sent".into())
                    })?;
                    break (fresh.web_area.clone(), current, buttons[0].clone());
                }
                Readiness::Focus => {
                    let current = current.ok_or_else(|| {
                        Error::Unavailable("The native prompt is not visible; not sent".into())
                    })?;
                    trace.stage("wait_send_ready");
                    self.check_write_surface(&fresh.web_area, &current)?;
                    muster_agent::control_guard::check()?;
                    if user_input_generation() != generation {
                        return Err(Error::Conflict(
                            "local input changed before focus; not sent".into(),
                        ));
                    }
                    let focus = current.set_bool("AXFocused", true);
                    readiness.focus_completed(started.elapsed());
                    trace.record(format_args!("focus_once_result={focus:?}"));
                }
                Readiness::Wait => {}
                failure => {
                    let message = match failure {
                        Readiness::EchoTimeout => {
                            log::warn!(
                                "native_app send to {}: draft unconfirmed after {} ms (value={}, composer_changes={composer_changes}); not sent",
                                &self.session[..8.min(self.session.len())],
                                started.elapsed().as_millis(),
                                last_miss.unwrap_or("exact")
                            );
                            "the app did not confirm the draft; not sent".to_owned()
                        }
                        Readiness::SubmitTimeout => {
                            "send button readiness timed out; draft preserved, not sent".to_owned()
                        }
                        Readiness::DisabledButton => {
                            "the send button is disabled; draft preserved, not sent".to_owned()
                        }
                        _ => format!(
                            "expected one send button, found {}; draft preserved, not sent",
                            buttons.len()
                        ),
                    };
                    return Err(Error::Unavailable(message));
                }
            }
            tokio::time::sleep(ECHO_POLL).await;
        };
        let send = &send;
        muster_agent::control_guard::check()?;
        trace.stage("validate_before_send");
        self.check_write_surface(&web_area, &composer)
            .map_err(|error| {
                log::warn!(
                    "native_app send to {}: surface check rejected before press: {error}",
                    &self.session[..8.min(self.session.len())]
                );
                Error::Conflict("session view changed; draft preserved, not sent".into())
            })?;
        let current_buttons = find_controls(self.profile, &web_area).send;
        if current_buttons.as_slice() != std::slice::from_ref(send)
            || send.bool("AXEnabled") != Some(true)
            || !web_area.contains(send)
            || !composer
                .string("AXValue")
                .is_some_and(|v| v.trim_end() == text.trim_end())
        {
            return Err(Error::Conflict(
                "draft or send control changed; not sent".into(),
            ));
        }
        let current_generation = user_input_generation();
        if current_generation != generation {
            log::warn!(
                "native_app send to {}: local input changed before press ({generation} -> {current_generation}); not sent",
                &self.session[..8.min(self.session.len())]
            );
            return Err(Error::Conflict(
                "local input changed; draft preserved, not sent".into(),
            ));
        }
        muster_agent::control_guard::check()?;
        // Only the final press can submit a message. Pre-press failures retain
        // a draft, not an unknown submission. Never automatically press again.
        trace.stage("prepare_receipt");
        let receipt = (self.send_receipt_source)(text)?;
        let admission = if self.profile.controls().queue.is_some() {
            muster_agent::queue::Admission::new(
                self.queue_source.clone(),
                self.profile.app.provider,
                text,
            )
            .ok()
        } else {
            None
        };
        // From here an interrupted send can no longer be reported as not started.
        muster_agent::control_guard::delivery_attempt();
        muster_agent::control_guard::check()?;
        if user_input_generation() != generation {
            return Err(Error::Conflict(
                "Local input changed before submit; not sent".into(),
            ));
        }
        self.composer_question_guard(expected)?;
        self.unconfirmed.arm(
            muster_agent::unconfirmed::Origin::now("send"),
            Some(receipt),
            admission,
        )?;
        // AX can report an error after delivering the press. Only the newly
        // appended provider user message can confirm delivery in either case.
        trace.stage("send_press");
        let _press = send.press();
        trace.record(format_args!("press_result={_press:?}"));
        trace.stage("await_provider_receipt");
        let receipt_budget = muster_agent::control_guard::receipt_budget();
        let step = Step::new("provider send receipt", receipt_budget);
        loop {
            // The background poll may have read the evidence first: either
            // way the write is confirmed only once the guard is clear.
            if self.unconfirmed.reconcile().cleared() {
                break;
            }
            if step.expired() {
                log::warn!(
                    "native_app send to {}: receipt pending after {} ms; later writes refused until evidence arrives",
                    &self.session[..8.min(self.session.len())],
                    receipt_budget.as_millis()
                );
                return Err(Error::UnknownOutcome("Waiting for the provider's send receipt; delivery is unconfirmed, do not resend".into()));
            }
            tokio::time::sleep(Duration::from_millis(100)).await;
        }
        log::info!(
            "native_app send to {}: sent",
            &self.session[..8.min(self.session.len())]
        );
        {
            let mut shared = self.shared.lock().unwrap();
            shared.state = SessionState::Running;
        }
        let _ = self
            .events
            .send((
                self.session.clone(),
                ControllerEvent::State(SessionState::Running),
            ))
            .await;
        trace.finish();
        Ok(())
    }
}

#[async_trait]
impl Controller for NativeAppController {
    fn native_surface_ready(&self) -> Result<bool, Error> {
        muster_agent::control_guard::check()?;
        if !ax::trusted()
            || screen_locked()
            || codesign::process_start(self.pid) != Some(self.start)
        {
            return Ok(false);
        }
        let generation = user_input_generation();
        let fresh = match find_surface(self.profile, &self.app, &self.session, &self.resolver) {
            Ok(surface) if surface.composer.is_some() => surface,
            Ok(_) | Err(Error::NotFound(_)) => return Ok(false),
            Err(error) => return Err(error),
        };
        muster_agent::control_guard::check()?;
        if generation != user_input_generation()
            || screen_locked()
            || codesign::process_start(self.pid) != Some(self.start)
        {
            return Ok(false);
        }
        *self.surface.lock().unwrap() = fresh;
        Ok(true)
    }

    fn kind(&self) -> ControllerKind {
        ControllerKind::NativeApp
    }
    fn reports_delivery(&self) -> bool {
        true
    }
    fn session_id(&self) -> String {
        self.session.clone()
    }
    fn state(&self) -> SessionState {
        self.shared.lock().unwrap().state
    }
    fn process_identity(&self) -> ProcessIdentity {
        ProcessIdentity {
            pid: self.pid,
            start: self.start,
        }
    }
    fn process_state(&self) -> ProcessState {
        self.process_identity()
            .observe(codesign::observe_process_start(self.pid).map_err(|_| ()))
    }
    fn pending(&self) -> Vec<PendingInteraction> {
        let shared = self.shared.lock().unwrap();
        shared
            .interaction
            .iter()
            .chain(&shared.additional)
            .cloned()
            .collect()
    }

    fn confirm_delivered(
        &self,
        operation: &str,
        persist: &muster_agent::control_guard::Confirmation,
    ) -> Result<(), Error> {
        self.unconfirmed.user_confirm(operation, persist)
    }

    fn release_unconfirmed(&self, operation: &str) -> Result<(), Error> {
        self.unconfirmed.release(operation)
    }

    async fn model_options(
        &self,
        set: Option<(&str, &str)>,
    ) -> Result<muster_proto::v1::AgentChatOptions, Error> {
        self.session_options(set).await
    }

    async fn send(&self, text: &str) -> Result<(), Error> {
        self.send_prompt(text, None).await
    }

    async fn complete(&self) -> Result<String, Error> {
        if !matches!(self.profile.app.provider, "claude" | "codex") {
            return Err(Error::Unavailable(
                "Native Tab suggestions are only available for Claude and Codex".into(),
            ));
        }
        let _write = self
            .write
            .try_lock()
            .map_err(|_| Error::Conflict("Another native operation is in progress".into()))?;
        self.unconfirmed.admit()?;
        let generation = user_input_generation();
        let locate = || -> Result<Element, Error> {
            muster_agent::control_guard::check()?;
            self.composer_question_guard(None)?;
            if user_input_generation() != generation {
                return Err(Error::Conflict(
                    "Local input changed during completion; draft preserved".into(),
                ));
            }
            let fresh = find_surface(self.profile, &self.app, &self.session, &self.resolver)?;
            let composer = fresh
                .composer
                .clone()
                .ok_or_else(|| Error::Unavailable("The native prompt is not visible".into()))?;
            self.check_write_surface(&fresh.web_area, &composer)?;
            *self.surface.lock().unwrap() = fresh;
            Ok(composer)
        };
        let composer = locate()?;
        let original = composer.string("AXValue");
        let controls = find_controls(self.profile, &self.surface.lock().unwrap().web_area);
        if let Some(stop) = controls.stop {
            return Err(Error::Unavailable(format!(
                "The native control scan was incomplete ({}); no Tab sent",
                stop.describe()
            )));
        }
        let buttons = controls.send;
        let may_submit = buttons
            .iter()
            .any(|button| button.bool("AXEnabled") != Some(false));
        let original = muster_agent::native_send::completion_baseline(
            original.as_deref(),
            self.profile.controls().placeholders,
            may_submit,
        )?
        .to_owned();
        composer
            .set_bool("AXFocused", true)
            .map_err(|e| Error::Unavailable(format!("Cannot focus the native prompt ({e})")))?;
        let focused = locate()?;
        if focused.bool("AXFocused") != Some(true)
            || focused.string("AXValue").as_deref() != Some(original.as_str())
        {
            return Err(Error::Conflict(
                "The native draft or focus changed; no Tab sent".into(),
            ));
        }
        self.app
            .tab()
            .map_err(|e| Error::Unavailable(format!("Native Tab failed ({e})")))?;
        let started = std::time::Instant::now();
        loop {
            tokio::time::sleep(Duration::from_millis(100)).await;
            let current = locate()?;
            let value = current.string("AXValue").ok_or_else(|| {
                Error::Unavailable("Cannot read the completed native draft".into())
            })?;
            if value != original && !self.profile.is_empty(&value) {
                // Compare immediately before restoration; never overwrite a
                // person's edits or another editor after a view transition.
                muster_agent::control_guard::check()?;
                if user_input_generation() != generation
                    || current.string("AXValue").as_deref() != Some(value.as_str())
                {
                    return Err(Error::Conflict(
                        "Native draft changed; completion left untouched".into(),
                    ));
                }
                draft(self.profile, &current, "").map_err(|e| {
                    Error::Unavailable(format!(
                        "Cannot restore empty draft ({e}); check the desktop app before retrying"
                    ))
                })?;
                self.app
                    .query_renderer(&self.surface.lock().unwrap().web_area);
                let cleanup = std::time::Instant::now();
                loop {
                    tokio::time::sleep(Duration::from_millis(50)).await;
                    if locate()?
                        .string("AXValue")
                        .is_some_and(|v| self.profile.is_empty(&v))
                    {
                        return Ok(value.trim().to_owned());
                    }
                    if cleanup.elapsed() >= Duration::from_secs(1) {
                        return Err(Error::Unavailable("Empty draft restoration was not confirmed; check the desktop app before retrying".into()));
                    }
                }
            }
            if started.elapsed() >= Duration::from_secs(1) {
                return Ok(String::new());
            }
            self.app
                .query_renderer(&self.surface.lock().unwrap().web_area);
        }
    }

    async fn respond(&self, interaction: &str, option: &str, _answer: &str) -> Result<(), Error> {
        let _write = self
            .write
            .try_lock()
            .map_err(|_| Error::Conflict("Another native operation is in progress".into()))?;
        let button = {
            let shared = self.shared.lock().unwrap();
            match &shared.interaction {
                Some(i) if i.id == interaction => {}
                _ => return Err(Error::NotFound(format!("interaction {interaction}"))),
            }
            shared
                .options
                .iter()
                .find(|(title, _)| title == option)
                .map(|(_, el)| el.clone())
                .ok_or_else(|| Error::Invalid(format!("option {option:?} is not offered")))?
        };
        self.check_identity()?;
        let web_area = self.surface.lock().unwrap().web_area.clone();
        if button.bool("AXEnabled") != Some(true)
            || names(&button, button.role()).label() != option
            || !self.profile.controls().approve.matches(&button)
            || !approval_in_scope(self.profile, &web_area, &button)
        {
            return Err(Error::Unavailable(
                "Approval control changed or left the conversation; no press sent".into(),
            ));
        }
        muster_agent::control_guard::check()?;
        muster_agent::control_guard::delivery_attempt();
        // AX can report an error after delivering a press: never "not sent".
        button.press().map_err(|code| {
            Error::UnknownOutcome(format!(
                "The approval press reported an error ({code}) and may still have been delivered; check the app before retrying"
            ))
        })?;
        self.observe().await;
        Ok(())
    }

    fn question_diagnostic(&self) -> Result<String, Error> {
        if self.profile.controls().questions.is_none() {
            return self.surface_diagnostic();
        }
        // With no card waiting there is no form to read, but the composer and
        // send controls this session would drive are what the operator is
        // after. Report them instead of only "no pending native question".
        match self.inspect_question_form() {
            Err(Error::NotFound(_)) => self.surface_diagnostic(),
            other => other,
        }
    }

    async fn answer_questions(
        &self,
        interaction: &str,
        answers: &[muster_proto::v1::QuestionAnswer],
    ) -> Result<(), Error> {
        if let Some(pending) = (self.question_source)().and_then(|s| s.find(interaction).cloned()) {
            let plans = muster_agent::questions::validate(&pending, answers)?;
            if muster_agent::claude_cards::uses_composer(&pending)
                && plans.len() == 1
                && !plans[0].text.is_empty()
            {
                return self.send_prompt(&plans[0].text, Some(&pending)).await;
            }
        }
        let _write = self
            .write
            .try_lock()
            .map_err(|_| Error::Conflict("Another native operation is in progress".into()))?;
        use std::sync::atomic::Ordering;
        let writes = self.question_writes.load(Ordering::SeqCst);
        // Each question/control wait has its own deadline. A whole-form
        // timer would charge later questions for earlier pages. The live
        // Locked Use guard and OS IPC/walk bounds still apply throughout.
        let result = self.answer_form(interaction, answers).await;
        if result.is_err()
            && (self.question_source)().is_some_and(|s| s.resolved.contains(interaction))
        {
            self.observe().await;
            return Ok(());
        }
        // A failure after any question press or text write may follow a
        // delivered answer. Only a failure before the first one is "not sent".
        match result {
            Err(error)
                if !matches!(error, Error::UnknownOutcome(_))
                    && self.question_writes.load(Ordering::SeqCst) != writes =>
            {
                Err(Error::UnknownOutcome(format!(
                    "{error}; question actions were already performed, so the answer may have been submitted. Check the native conversation before retrying"
                )))
            }
            result => result,
        }
    }

    async fn send_queued(&self, interaction: &str) -> Result<(), Error> {
        self.send_queued_message(&self.queue_source, interaction)
            .await
    }

    async fn interrupt(&self) -> Result<(), Error> {
        let _write = self
            .write
            .try_lock()
            .map_err(|_| Error::Conflict("Another native operation is in progress".into()))?;
        self.check_identity()?;
        let web_area = self.surface.lock().unwrap().web_area.clone();
        let mut stop = None;
        web_area.walk(|node| {
            if self.profile.controls().stop.matches(node) {
                stop = Some(node.clone());
                return false;
            }
            true
        });
        let Some(stop) = stop else {
            let mut draft = false;
            web_area.walk(|node| {
                let role = node.role();
                draft = role == "AXTextArea" && holds_draft(self.profile, node, role);
                !draft
            });
            return Err(Error::Conflict(if draft {
                "The composer on the Mac holds an unsent draft, which hides Stop; clear it there to interrupt".into()
            } else {
                "no turn is running".into()
            }));
        };
        muster_agent::control_guard::delivery_attempt();
        muster_agent::control_guard::check()?;
        stop.press()
            .map_err(|code| Error::Unavailable(format!("AX press failed ({code})")))
    }

    async fn stop(&self) {}
}

fn screen_locked() -> bool {
    super::lock::session_state().is_some_and(|s| s.screen_locked || !s.login_done)
}

#[path = "native_questions.rs"]
mod question_forms;

#[cfg(test)]
mod send_tests {
    use super::*;

    #[test]
    fn batched_control_names_preserve_empty_title_fallback_and_composer_selectors() {
        use core_foundation::{base::TCFType, string::CFString};
        let metadata = |role: &str, title: &str, description: &str, placeholder: &str| {
            control_metadata(
                &[role, title, description, placeholder]
                    .map(|v| Some(CFString::new(v).as_CFType())),
            )
            .unwrap()
        };
        for profile in [&CODEX_APP, &CLAUDE_APP] {
            let send = metadata("AXButton", "", "Send", "");
            assert!(profile.controls().send.matches(&send));
            let stop = metadata("AXButton", "", "Stop", "");
            assert!(!profile.controls().send.matches(&stop));
            assert!(profile.controls().stop.matches(&stop));
        }
        let catdesk = metadata("AXTextArea", "", "聊天输入框", "");
        assert!(CATDESK_APP.controls().composer.matches(&catdesk));
        // A missing optional label must not erase a present description.
        let values = [
            Some(CFString::new("AXButton").as_CFType()),
            None,
            Some(CFString::new("Send").as_CFType()),
            None,
        ];
        assert!(
            CODEX_APP
                .controls()
                .send
                .matches(&control_metadata(&values).unwrap())
        );
        assert!(control_metadata(&[]).is_none());
        assert!(control_metadata(&[None]).is_none());
    }

    #[test]
    fn explicit_send_reuses_matching_drafts_and_replaces_different_drafts() {
        assert_eq!(
            draft_action(&CODEX_APP, Some("先本地安装测试"), "先本地安装测试").unwrap(),
            DraftAction::Reuse
        );
        assert_eq!(
            draft_action(&CODEX_APP, Some("\nDo anything"), "hello").unwrap(),
            DraftAction::Write
        );
        assert_eq!(
            draft_action(&CLAUDE_APP, Some(""), "hello").unwrap(),
            DraftAction::Write
        );
        for profile in [&CODEX_APP, &CLAUDE_APP] {
            assert_eq!(
                draft_action(profile, Some("old draft"), "hello").unwrap(),
                DraftAction::Write
            );
        }
        assert!(draft_action(&CODEX_APP, None, "hello").is_err());
    }
}
