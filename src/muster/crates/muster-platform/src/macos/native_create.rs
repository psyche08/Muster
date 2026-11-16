//! Native new-session OS adapter. The daemon owns authorization and delivery.
use super::{
    ax::{self, Element},
    codesign, input, native_app,
    native_app::native_options,
};
use muster_agent::{
    Error, control_guard,
    native_runtime::{CreationWindow, NativeConsole},
    profile::App,
};
use std::{
    path::{Path, PathBuf},
    sync::Mutex,
    time::{Duration, Instant},
};

/// How long a window may take to show its renderer before navigation. A new
/// ChatGPT window renders only after its feature-flag bootstrap, which reached
/// its 60 s deadline on every window of one Mac (ADR 0154). Nothing has been
/// navigated or typed while this runs, and the guard still stops on input.
const WINDOW_STARTUP: Duration = Duration::from_secs(90);

// All native creation entry points already hold the daemon's provider lock.
static CODEX_WINDOW: Mutex<Option<CreationWindow<Element>>> = Mutex::new(None);

fn unavailable(s: impl Into<String>) -> Error {
    Error::Unavailable(s.into())
}
fn conflict(s: &str) -> Error {
    Error::Conflict(s.into())
}
fn nodes(root: &Element) -> Result<Vec<Element>, Error> {
    let mut nodes = Vec::new();
    let (_, report, _) = root.walk_report(|n| {
        nodes.push(n.clone());
        true
    });
    if report != "complete" {
        return Err(unavailable("Incomplete native form"));
    }
    Ok(nodes)
}
fn unique(root: &Element, test: impl Fn(&Element) -> bool) -> Result<Option<Element>, Error> {
    let matches: Vec<_> = nodes(root)?.into_iter().filter(test).collect();
    match matches.len() {
        0 => Ok(None),
        1 => Ok(matches.into_iter().next()),
        _ => Err(conflict("Ambiguous native control")),
    }
}
fn windows(root: &Element) -> Vec<Element> {
    root.attribute("AXWindows")
        .map(|v| Element::elements_of(&v))
        .unwrap_or_default()
}
fn label(n: &Element, value: &str) -> bool {
    ["AXTitle", "AXDescription"]
        .iter()
        .any(|a| n.string(a).as_deref() == Some(value))
}
fn console() -> Result<(), Error> {
    control_guard::check()?;
    if super::lock::session_state()
        .is_none_or(|s| s.screen_locked || !s.on_console || !s.login_done)
    {
        return Err(conflict(
            "Native creation requires an unlocked active console",
        ));
    }
    Ok(())
}
struct Guard {
    profile: &'static App,
    pid: u32,
    start: u64,
    generation: u64,
    mode: NativeConsole,
    shown: Option<Shown>,
}

/// A hidden app renders nothing, so its AX tree stops changing. Background
/// creation shows it without activating it, which leaves the foreground app
/// and its key window alone, and hides it again when the creation ends unless
/// its user has brought it to the front meanwhile (ADR 0220).
struct Shown {
    app: Element,
    pid: u32,
    hide: bool,
}
impl Shown {
    fn reveal(app: &Element, pid: u32) -> Result<Option<Self>, Error> {
        if app.bool("AXHidden") != Some(true) {
            return Ok(None);
        }
        app.set_bool("AXHidden", false)
            .map_err(|_| unavailable("The hidden app could not be shown without activating it"))?;
        Ok(Some(Self {
            app: app.clone(),
            pid,
            hide: true,
        }))
    }
}
impl Drop for Shown {
    fn drop(&mut self) {
        if self.hide
            && super::events::foreground_owner()
                != muster_agent::local_input::Owner::Process(self.pid)
            && self.app.bool("AXHidden") == Some(false)
            && self.app.set_bool("AXHidden", true).is_err()
        {
            log::info!("Native creation could not hide its provider again");
        }
    }
}
impl Guard {
    fn check(&self) -> Result<(), Error> {
        console()?;
        if codesign::process_start(self.pid) != Some(self.start)
            || !codesign::process_satisfies(self.pid, &self.profile.app.requirement())
        {
            return Err(conflict(
                "Native process identity changed; nothing is retried",
            ));
        }
        if self.mode != NativeConsole::Covered
            && self.mode
                != NativeConsole::observe(false, Some(self.pid), super::events::foreground_owner())
        {
            return Err(conflict(
                "Foreground application changed during native creation",
            ));
        }
        let generation = if matches!(self.mode, NativeConsole::Background(_)) {
            input::target_input_generation(self.pid)
        } else {
            input::user_input_generation()
        };
        if generation != self.generation {
            return Err(conflict("Physical input changed; nothing is retried"));
        }
        Ok(())
    }
    fn press(&self, e: &Element) -> Result<(), Error> {
        self.check()?;
        muster_agent::native_navigation::restore::dispatch(false);
        e.press()
            .map_err(|_| unavailable("Native control refused the action"))
    }
}
async fn wait(
    guard: &Guard,
    root: &Element,
    test: impl Fn(&Element) -> bool,
) -> Result<Element, Error> {
    let deadline = Instant::now() + Duration::from_secs(8);
    loop {
        guard.check()?;
        if let Some(n) = unique(root, &test)? {
            return Ok(n);
        }
        if Instant::now() >= deadline {
            return Err(unavailable("Native control did not appear; no submission"));
        }
        tokio::time::sleep(Duration::from_millis(100)).await;
    }
}
fn bundle_path(profile: &App) -> Result<PathBuf, Error> {
    let name = profile
        .executable
        .split('/')
        .next()
        .ok_or_else(|| unavailable("Invalid app profile"))?;
    let mut roots = vec![PathBuf::from("/Applications")];
    if let Some(home) = std::env::var_os("HOME") {
        roots.push(PathBuf::from(home).join("Applications"))
    }
    let path = roots
        .into_iter()
        .map(|r| r.join(name))
        .find(|p| p.is_dir())
        .ok_or_else(|| unavailable("Installed desktop app was not found in Applications"))?;
    let status = std::process::Command::new("/usr/bin/codesign")
        .args(["--verify", "--strict", "-R"])
        .arg(format!("={}", profile.app.requirement()))
        .arg(&path)
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .status()
        .map_err(|_| unavailable("Cannot verify app signature"))?;
    if !status.success() {
        return Err(unavailable("Desktop app signature is invalid"));
    }
    Ok(path)
}
fn open(path: &Path, route: Option<&url::Url>) -> Result<(), Error> {
    console()?;
    let mut command = std::process::Command::new("/usr/bin/open");
    command.arg("-a").arg(path);
    if let Some(route) = route {
        command.arg(route.as_str());
    }
    muster_agent::native_navigation::restore::dispatch(true);
    if !command
        .status()
        .map_err(|_| unavailable("App launch failed"))?
        .success()
    {
        return Err(unavailable("App launch was refused"));
    }
    super::navigation_restore::note_foreground();
    Ok(())
}

async fn settled_windows(
    guard: &Guard,
    root: &Element,
    require_renderer: bool,
) -> Result<Vec<Element>, Error> {
    let until = Instant::now() + WINDOW_STARTUP;
    let mut previous = windows(root);
    let mut stable = Instant::now();
    let mut accessibility_enabled = false;
    let mut changes = 0;
    loop {
        guard.check()?;
        // The process can exist before it accepts AX writes (-25204). Retry
        // this idempotent OS adaptation until acknowledged, not navigation.
        if require_renderer && !accessibility_enabled {
            accessibility_enabled = root.set_bool("AXManualAccessibility", true).is_ok();
            let _ = root.set_bool("AXEnhancedUserInterface", true);
        }
        let current = windows(root);
        if current != previous {
            previous = current;
            stable = Instant::now();
            changes += 1;
        }
        let menu_ready = root.element("AXMenuBar").is_some();
        let mut ready = menu_ready;
        if require_renderer {
            ready &= !previous.is_empty();
            for window in &previous {
                ready &=
                    unique(window, |n| guard.profile.controls().composer.matches(n))?.is_some();
            }
        }
        if !ready {
            stable = Instant::now();
        }
        if ready && stable.elapsed() > Duration::from_millis(700) {
            return Ok(previous);
        }
        if Instant::now() >= until {
            return Err(unavailable(format!(
                "Native activation did not settle: windows={}, menu={menu_ready}, renderer={ready}, changes={changes}, accessibility={accessibility_enabled}",
                previous.len()
            )));
        }
        tokio::time::sleep(Duration::from_millis(100)).await;
    }
}

// Electron can publish its native window/menu before its renderer subscribes
// to navigation. Do not dispatch a directory route into that startup interval.
async fn renderer_ready(guard: &Guard, root: &Element, window: &Element) -> Result<(), Error> {
    let deadline = Instant::now() + WINDOW_STARTUP;
    let mut ready_since = None;
    loop {
        guard.check()?;
        if !windows(root).contains(window) {
            return Err(conflict("Native startup window was replaced"));
        }
        let ready = unique(window, |n| guard.profile.controls().composer.matches(n))?.is_some();
        if ready {
            let since = ready_since.get_or_insert_with(Instant::now);
            if since.elapsed() >= Duration::from_millis(700) {
                return Ok(());
            }
        } else {
            ready_since = None;
        }
        if Instant::now() >= deadline {
            return Err(unavailable("Native renderer is not ready for navigation"));
        }
        tokio::time::sleep(Duration::from_millis(100)).await;
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
enum Selection {
    Claude,
    Codex(muster_agent::native_runtime::CodexProject),
}

fn codex_project_state() -> Option<Vec<u8>> {
    let home = std::env::var_os("CODEX_HOME")
        .map(PathBuf::from)
        .or_else(|| std::env::var_os("HOME").map(|h| PathBuf::from(h).join(".codex")))?;
    std::fs::read(home.join(".codex-global-state.json")).ok()
}

/// Advisory only. The provider lock and complete live form proof still
/// precede every write. Waits while the provider is in front; fails for a
/// directory that is no Codex project (ADR 0220). Claude's folder is checked
/// on its form, where its picker lists the folders.
pub fn multica_directory_admissible(provider: &str, cwd: &Path) -> Result<bool, Error> {
    let Some(app) =
        muster_agent::profile::app(provider).filter(|_| matches!(provider, "codex" | "claude"))
    else {
        return Ok(false);
    };
    let pids = native_app::app_pids(app);
    let [pid] = pids.as_slice() else {
        return Ok(false);
    };
    match NativeConsole::observe(false, Some(*pid), super::events::foreground_owner()) {
        NativeConsole::Background(_) if provider == "codex" => {
            missing_directory("codex", cwd)?;
            Ok(codex_project_state().is_some_and(|b| {
                muster_agent::native_runtime::selected_codex_directory(&b, cwd)
                    || muster_agent::native_runtime::codex_sidebar_project(&b, cwd).is_some()
            }))
        }
        NativeConsole::Background(_) => Ok(true),
        _ => Ok(false),
    }
}

/// Fails before any action when the app's own state shows that `cwd` is no
/// project of it. Readable without the console, so also while locked.
pub fn require_listed_directory(provider: &str, cwd: &Path) -> Result<(), Error> {
    missing_directory(provider, cwd)
}

/// Muster never adds a project or folder to the app. A directory the app
/// does not list as a project fails instead of waiting (ADR 0220).
fn missing_directory(provider: &str, cwd: &Path) -> Result<(), Error> {
    if provider != "codex" {
        return Ok(());
    }
    let listed = codex_project_state()
        .and_then(|state| muster_agent::native_runtime::codex_lists_directory(&state, cwd));
    if listed != Some(false) {
        return Ok(());
    }
    Err(Error::Invalid(format!(
        "{} is not a project in the ChatGPT app; add it there first. Nothing was submitted",
        cwd.display()
    )))
}

fn selection_matches(
    provider: &str,
    page: &Element,
    cwd: &Path,
    require_codex_home: bool,
) -> Result<Option<Selection>, Error> {
    let all = nodes(page)?;
    let proof = if provider == "claude" {
        let good = all.iter().any(|n| {
            n.role() == "AXPopUpButton"
                && n.string("AXDescription").as_deref() == Some("Environment: This computer")
        }) && all.iter().any(|n| {
            n.role() == "AXPopUpButton"
                && n.string("AXHelp").is_some_and(|display| {
                    let home = std::env::var_os("HOME").map(PathBuf::from);
                    muster_agent::native_runtime::directory_label_matches(
                        &display,
                        cwd,
                        home.as_deref(),
                    ) && n.string("AXDescription") == Some(format!("Folder: {display}"))
                })
        });
        good.then_some(Selection::Claude)
    } else {
        let labels: Vec<_> = all
            .iter()
            .filter_map(|n| {
                (n.role() == "AXPopUpButton")
                    .then(|| n.string("AXDescription"))
                    .flatten()
                    .and_then(|d| d.strip_prefix("Change project: ").map(str::to_owned))
            })
            .collect();
        if labels.len() == 1
            && (!require_codex_home || codex_home_heading(&all))
            && all
                .iter()
                .any(|n| label(n, "Switch mode, current mode: Codex"))
            && all.iter().any(|n| {
                n.role() == "AXStaticText"
                    && n.string("AXValue").as_deref() == Some("This computer")
            })
        {
            codex_project_state()
                .and_then(|b| muster_agent::native_runtime::codex_project(&b, &labels[0], cwd))
                .map(Selection::Codex)
        } else {
            None
        }
    };
    Ok(proof)
}

pub(super) fn verified_empty_form(provider: &str, page: &Element) -> bool {
    new_page(provider, page)
        && if provider == "codex" {
            nodes(page).is_ok_and(|all| codex_home_heading(&all))
        } else {
            empty_session_marker(page).unwrap_or(false)
        }
}
fn new_page(provider: &str, page: &Element) -> bool {
    page.role() == "AXWebArea"
        && page.string("AXURL").is_some_and(|u| {
            if provider == "codex" {
                u == "app://-/index.html"
            } else {
                matches!(u.as_str(), "about:blank" | "https://claude.ai/epitaxy")
            }
        })
}

fn codex_home_heading(all: &[Element]) -> bool {
    // The Codex home hero exposes the literal static prefix followed
    // by the project picker. An existing thread can have an empty editor.
    all.iter().any(|n| {
        n.role() == "AXStaticText"
            && n.string("AXValue").is_some_and(|v| {
                matches!(
                    v.trim(),
                    "What should we build in" | "What should we work on in"
                )
            })
    })
}

fn empty_session_marker(window: &Element) -> Result<bool, Error> {
    Ok(nodes(window)?.iter().any(|n| {
        n.role() == "AXStaticText" && n.string("AXValue").as_deref() == Some("New session")
    }))
}

fn picker(page: &Element, prefix: &str) -> Result<Option<Element>, Error> {
    unique(page, |n| {
        n.role() == "AXPopUpButton"
            && n.string("AXDescription")
                .is_some_and(|d| d.starts_with(prefix))
    })
}

fn menu_items(window: &Element) -> Result<Vec<Element>, Error> {
    Ok(nodes(window)?
        .into_iter()
        .filter(|n| n.role().starts_with("AXMenuItem"))
        .collect())
}

/// Every name an item shows or carries, its own and its descendants'.
fn item_labels(item: &Element) -> Vec<String> {
    let mut labels = Vec::new();
    let _ = item.walk_report(|n| {
        for a in ["AXTitle", "AXDescription", "AXHelp", "AXValue"] {
            if let Some(v) = n.string(a).map(|v| v.trim().to_owned())
                && !v.is_empty()
                && !labels.contains(&v)
            {
                labels.push(v);
            }
        }
        true
    });
    labels
}

/// Opens one of the form's own pickers and presses the single item `wanted`
/// accepts. Without one the picker is closed again and `missing` is the
/// answer: the app's lists are the only source, never a folder sheet.
async fn choose(
    guard: &Guard,
    window: &Element,
    picker: &Element,
    wanted: impl Fn(&[String]) -> bool,
    missing: Error,
) -> Result<(), Error> {
    if !menu_items(window)?.is_empty() {
        return Err(conflict("A menu is already open in the provider window"));
    }
    guard.press(picker)?;
    let deadline = Instant::now() + Duration::from_secs(2);
    let items = loop {
        guard.check()?;
        let items = menu_items(window)?;
        if !items.is_empty() {
            break items;
        }
        if Instant::now() >= deadline {
            return Err(unavailable(
                "The picker did not open; nothing was submitted",
            ));
        }
        tokio::time::sleep(Duration::from_millis(100)).await;
    };
    let matching: Vec<_> = items.iter().filter(|i| wanted(&item_labels(i))).collect();
    if let [item] = matching.as_slice() {
        return guard.press(item);
    }
    // Toggle only the picker this step opened; never a global Escape.
    guard.press(picker)?;
    let deadline = Instant::now() + Duration::from_secs(2);
    while !menu_items(window)?.is_empty() && Instant::now() < deadline {
        tokio::time::sleep(Duration::from_millis(100)).await;
        guard.check()?;
    }
    Err(if matching.is_empty() {
        missing
    } else {
        conflict("The picker lists the requested choice more than once")
    })
}

/// Claude's new-session form names its environment and folder in two
/// pickers. Choose "This computer" and the exact folder from the app's own
/// lists. Muster never adds a folder: one Claude does not list fails.
async fn choose_claude_directory(
    guard: &Guard,
    window: &Element,
    page: &Element,
    cwd: &Path,
) -> Result<(), Error> {
    let environment =
        picker(page, "Environment: ")?.ok_or_else(|| unavailable("No environment picker"))?;
    if environment.string("AXDescription").as_deref() != Some("Environment: This computer") {
        choose(
            guard,
            window,
            &environment,
            |labels| labels.iter().any(|l| l == "This computer"),
            Error::Invalid(
                "Claude offers no This computer environment; nothing was submitted".into(),
            ),
        )
        .await?;
        tokio::time::sleep(Duration::from_millis(300)).await;
    }
    let folder = picker(page, "Folder: ")?.ok_or_else(|| unavailable("No folder picker"))?;
    let home = std::env::var_os("HOME").map(PathBuf::from);
    choose(
        guard,
        window,
        &folder,
        |labels| {
            labels.iter().any(|l| {
                muster_agent::native_runtime::directory_label_matches(l, cwd, home.as_deref())
            })
        },
        Error::Invalid(format!(
            "{} is not a folder Claude lists; open it in Claude first. Nothing was submitted",
            cwd.display()
        )),
    )
    .await
}

/// Retain the exact window/page across submit and provider receipt discovery.
pub struct Submitted {
    guard: Guard,
    root: Element,
    window: Element,
    page: Element,
}
impl Submitted {
    pub fn visible(&self) -> Result<bool, Error> {
        self.guard.check()?;
        Ok(windows(&self.root).contains(&self.window) && self.window.contains(&self.page))
    }
}

/// The new session's form as the scope its model and effort pickers are
/// read and pressed in: every step re-proves the exact window, page,
/// directory and empty new-session state, the app's identity, an unlocked
/// console and the person's idle input.
struct Form<'a, F: Fn() -> Result<(), Error> + Sync> {
    check: &'a F,
    page: &'a Element,
    app: &'a Element,
}
impl<F: Fn() -> Result<(), Error> + Sync> native_options::Scope for Form<'_, F> {
    fn check(&self) -> Result<(), Error> {
        (self.check)()
    }
    fn web(&self) -> &Element {
        self.page
    }
    fn app(&self) -> &Element {
        self.app
    }
}

async fn allocate_codex_window(
    guard: &Guard,
    root: &Element,
    before: &[Element],
) -> Result<Element, Error> {
    let menu = root
        .element("AXMenuBar")
        .ok_or_else(|| unavailable("No native menu bar"))?;
    let new = wait(guard, &menu, |n| {
        n.role() == "AXMenuItem" && label(n, "New Window")
    })
    .await?;
    guard.press(&new)?;
    let deadline = Instant::now() + Duration::from_secs(8);
    loop {
        guard.check()?;
        let fresh: Vec<_> = windows(root)
            .into_iter()
            .filter(|w| !before.contains(w))
            .collect();
        if fresh.len() == 1 {
            return Ok(fresh[0].clone());
        }
        if fresh.len() > 1 || Instant::now() >= deadline {
            return Err(conflict("New window identity is ambiguous"));
        }
        tokio::time::sleep(Duration::from_millis(100)).await;
    }
}

fn require_focus(root: &Element, window: &Element) -> Result<(), Error> {
    if root.element("AXFocusedWindow").as_ref() != Some(window) {
        return Err(conflict("Native route target is not the focused window"));
    }
    Ok(())
}

#[derive(PartialEq)]
struct BackgroundTarget {
    window: Element,
    action: Option<Element>,
}

/// Claude's sidebar New starts a Code session only while the window shows
/// Code: a session or its empty new-session form, never a Chat page.
fn claude_code_window(tree: &[Element]) -> bool {
    tree.iter().any(|n| {
        n.role() == "AXWebArea"
            && n.string("AXURL").is_some_and(|u| {
                u == "https://claude.ai/epitaxy" || u.starts_with("https://claude.ai/epitaxy/")
            })
    }) || tree
        .iter()
        .any(|n| new_page("claude", n) && empty_session_marker(n).unwrap_or(false))
}

/// A window whose empty form already shows the directory, or else the
/// window's own in-app action that opens one: Codex's project-specific new
/// chat, Claude's sidebar New. Either may navigate away from the session the
/// window shows, never from a draft, a sheet or an in-app dialog (ADR 0220).
fn ready_background_window(
    root: &Element,
    guard: &Guard,
    provider: &str,
    cwd: &Path,
) -> Result<Option<BackgroundTarget>, Error> {
    let live = windows(root);
    let pin = CODEX_WINDOW
        .lock()
        .unwrap()
        .as_ref()
        .and_then(|p| p.live(guard.pid, guard.start, &live))
        .cloned();
    let project_action = (provider == "codex")
        .then(codex_project_state)
        .flatten()
        .and_then(|b| muster_agent::native_runtime::codex_sidebar_project(&b, cwd))
        .map(|name| format!("Start new chat in {name}"));
    let mut routes = Vec::new();
    let mut candidates = Vec::new();
    let focused = root.element("AXFocusedWindow");
    for window in live {
        if provider == "codex" && pin.as_ref().is_some_and(|p| p != &window) {
            continue;
        }
        let Ok(tree) = nodes(&window) else { continue };
        if tree.iter().any(|n| {
            n.role() == "AXSheet" || n.string("AXSubrole").as_deref() == Some("AXApplicationDialog")
        }) {
            continue;
        }
        let editors: Vec<_> = tree
            .iter()
            .filter(|n| guard.profile.controls().composer.matches(*n))
            .collect();
        let [editor] = editors.as_slice() else {
            continue;
        };
        if editor
            .string("AXValue")
            .is_none_or(|v| !guard.profile.is_empty(&v))
        {
            continue;
        }
        let pages: Vec<_> = tree.iter().filter(|n| new_page(provider, n)).collect();
        if let [page] = pages.as_slice()
            && (provider != "claude" || empty_session_marker(page)?)
            && selection_matches(provider, page, cwd, true)?.is_some()
        {
            if focused.as_ref() == Some(&window) {
                return Ok(Some(BackgroundTarget {
                    window,
                    action: None,
                }));
            }
            candidates.push(window);
            continue;
        }
        let action = match provider {
            "codex" => project_action
                .as_deref()
                .map(|name| ("AXDescription", name)),
            _ if claude_code_window(&tree) => Some(("AXTitle", "New")),
            _ => None,
        };
        if let Some((attribute, name)) = action {
            let mut buttons = tree
                .iter()
                .filter(|n| n.role() == "AXButton" && n.string(attribute).as_deref() == Some(name));
            if let (Some(button), None) = (buttons.next(), buttons.next()) {
                routes.push((window, button.clone()));
            }
        }
    }
    if candidates.len() > 1 {
        return Ok(None);
    }
    if let Some(window) = candidates.pop() {
        return Ok(Some(BackgroundTarget {
            window,
            action: None,
        }));
    }
    let route = routes
        .iter()
        .position(|(window, _)| focused.as_ref() == Some(window))
        .or_else(|| (routes.len() == 1).then_some(0));
    Ok(route.map(|i| {
        let (window, action) = routes.swap_remove(i);
        BackgroundTarget {
            window,
            action: Some(action),
        }
    }))
}

pub struct Admission {
    guard: Guard,
    root: Element,
    bundle: PathBuf,
    background_window: Option<BackgroundTarget>,
    launched: bool,
    started: Instant,
}

impl Admission {
    /// The provider permit may have been held by another job while this
    /// read-only observation waited. Discard a changed plan before ledger.begin.
    pub fn still_ready(&self, cwd: &Path) -> bool {
        self.guard.check().is_ok()
            && self.background_window.is_some()
            && ready_background_window(
                &self.root,
                &self.guard,
                self.guard.profile.app.provider,
                cwd,
            )
            .is_ok_and(|now| now == self.background_window)
    }
}

/// Managed tasks call this with the live caller's cancellation guard, before
/// reserving the console, provider or submission ledger. No UI action is sent.
pub async fn managed_admission(provider: &str, cwd: &Path) -> Result<Admission, Error> {
    prepare(provider, cwd, false, false).await
}

async fn prepare(
    provider: &str,
    cwd: &Path,
    covered: bool,
    provider_held: bool,
) -> Result<Admission, Error> {
    let profile = muster_agent::profile::app(provider)
        .filter(|_| matches!(provider, "codex" | "claude"))
        .ok_or_else(|| Error::Invalid("Unsupported native provider".into()))?;
    let started = Instant::now();
    let stage = |name: &str| {
        log::info!(
            "Native creation stage: {name} elapsed_ms={}",
            started.elapsed().as_millis()
        )
    };
    stage("admitted");
    console()?;
    if !ax::trusted() {
        return Err(unavailable(
            "Native creation needs Accessibility permission",
        ));
    }
    let bundle = bundle_path(profile)?;
    stage("bundle_verified");
    let deadline = provider_held.then(|| Instant::now() + WINDOW_STARTUP);
    let mut launched = false;
    let mut waiting = false;
    let mut last_reason = None;
    // Kept across observations, so a wait does not show and hide the app.
    let mut shown: Option<Shown> = None;
    let (guard, root, background_window) = loop {
        console()?;
        let pids = native_app::app_pids(profile);
        if pids.len() > 1 {
            return Err(conflict("Ambiguous native app processes"));
        }
        let pid = pids.first().copied();
        let mode = NativeConsole::observe(covered, pid, super::events::foreground_owner());
        if mode == NativeConsole::Provider
            && let Some(mut taken) = shown.take()
        {
            // Its user brought the app to the front: it stays shown.
            taken.hide = false;
        }
        if let Some(pid) = pid {
            let background = matches!(mode, NativeConsole::Background(_));
            let quiet = if background {
                input::target_input_idle(&[pid])
            } else {
                input::user_input_idle()
            };
            let reason = match mode {
                NativeConsole::Provider => "provider_foreground",
                NativeConsole::Background(_) if quiet < Duration::from_millis(500) => {
                    "provider_input"
                }
                NativeConsole::Background(_) => "other_app_foreground",
                NativeConsole::Waiting => "no_focus_owner",
                NativeConsole::Covered if quiet < Duration::from_millis(500) => "covered_input",
                NativeConsole::Covered => "covered",
            };
            if last_reason != Some(reason) {
                stage(reason);
                last_reason = Some(reason);
            }
            if mode.may_create() && quiet >= Duration::from_millis(500) {
                let mut guard = Guard {
                    profile,
                    pid,
                    start: codesign::process_start(pid).ok_or_else(|| unavailable("App exited"))?,
                    generation: if background {
                        input::target_input_generation(pid)
                    } else {
                        input::user_input_generation()
                    },
                    mode,
                    shown: None,
                };
                if let Err(error) = guard.check() {
                    console()?;
                    if !provider_held && matches!(error, Error::Conflict(_)) {
                        tokio::time::sleep(Duration::from_millis(100)).await;
                        continue;
                    }
                    return Err(error);
                }
                let root = Element::application(pid);
                root.enable_manual_accessibility();
                if !background {
                    break (guard, root, None);
                }
                let mut settle = Duration::ZERO;
                if shown.is_none()
                    && let Some(revealed) = Shown::reveal(&root, pid)?
                {
                    shown = Some(revealed);
                    stage("revealed_hidden_app");
                    settle = Duration::from_secs(3);
                    tokio::time::sleep(Duration::from_millis(700)).await;
                }
                // Observation only: a changed console or input is a requeue.
                let until = Instant::now() + settle;
                let found = loop {
                    if guard.check().is_err() {
                        break None;
                    }
                    if let Some(window) = ready_background_window(&root, &guard, provider, cwd)? {
                        break Some(window);
                    }
                    if Instant::now() >= until {
                        break None;
                    }
                    tokio::time::sleep(Duration::from_millis(150)).await;
                };
                if let Some(window) = found {
                    guard.check()?;
                    stage("background_form_ready");
                    guard.shown = shown.take();
                    break (guard, root, Some(window));
                }
                missing_directory(provider, cwd)?;
            }
        } else if covered && !launched && input::user_input_idle() >= Duration::from_millis(500) {
            open(&bundle, None)?;
            launched = true;
        }
        mode.check_wait(provider_held, provider)?;
        if deadline.is_some_and(|d| Instant::now() >= d) {
            return Err(unavailable(
                "Native creation readiness timed out; nothing submitted",
            ));
        }
        if !waiting {
            stage("queued_for_native_form");
            waiting = true;
        }
        tokio::time::sleep(Duration::from_millis(350)).await;
    };
    Ok(Admission {
        guard,
        root,
        bundle,
        background_window,
        launched,
        started,
    })
}

/// Submits the first message of a new session. A requested `model` or
/// `effort` (names as the app shows them; empty keeps the app's choice) is
/// chosen and read back on the new session's form first; one the app does
/// not offer or confirm refuses before anything is drafted (ADR 0181).
pub async fn submit(
    provider: &str,
    cwd: &Path,
    prompt: &str,
    model: &str,
    effort: &str,
    covered: bool,
    admission: Option<Admission>,
) -> Result<Submitted, Error> {
    let Admission {
        guard,
        root,
        bundle,
        background_window,
        launched,
        started,
    } = match admission {
        Some(prepared) => prepared,
        None => prepare(provider, cwd, covered, true).await?,
    };
    let profile = guard.profile;
    let stage = |name: &str| {
        log::info!(
            "Native creation stage: {name} elapsed_ms={}",
            started.elapsed().as_millis()
        )
    };
    let cwd_text = cwd
        .to_str()
        .ok_or_else(|| Error::Invalid("Directory must be UTF-8".into()))?;
    let pid = guard.pid;
    let background = background_window.is_some();
    let (window, reused_codex, reuses_main) = if let Some(target) = background_window {
        let window = target.window;
        if let Some(action) = target.action {
            guard.check()?;
            if !windows(&root).contains(&window) || !window.contains(&action) {
                return Err(conflict("Background creation target was replaced"));
            }
            guard.press(&action)?;
        }
        if provider == "codex" {
            let mut pin = CODEX_WINDOW.lock().unwrap();
            let same = pin
                .as_ref()
                .and_then(|p| p.live(pid, guard.start, std::slice::from_ref(&window)))
                .is_some();
            stage(if same {
                "background_pinned_window_reused"
            } else {
                "background_window_pinned"
            });
            *pin = Some(CreationWindow::new(pid, guard.start, window.clone()));
        }
        (window, provider == "codex", provider == "claude")
    } else {
        let was_running = !launched;
        // Only a covered console reaches the foreground route (ADR 0220).
        let before_activation = if was_running {
            settled_windows(&guard, &root, false).await?
        } else {
            Vec::new()
        };
        guard.check()?;
        if was_running {
            open(&bundle, None)?;
        }
        stage("activated");
        let before = settled_windows(&guard, &root, true).await?;
        // Check every retained window independently: several complete forms must
        // not exhaust one form's node budget. Never navigate over a nonempty draft
        // if the provider ignores raise; missing/ambiguous editors fail closed.
        stage("windows_ready");
        for window in &before {
            let composer = unique(window, |n| profile.controls().composer.matches(n))?
                .ok_or_else(|| unavailable("Existing native composer is unavailable"))?;
            if composer
                .string("AXValue")
                .is_none_or(|v| !profile.is_empty(&v))
            {
                return Err(conflict("Existing native draft is preserved"));
            }
        }
        // Claude's documented code/new route targets its main renderer. Identify
        // that shell before navigation; only its verified empty new-session form
        // may later receive a draft. Existing popouts are not inferred by title.
        let mut reusable_main = None;
        if provider == "claude" {
            let shell = url::Url::from_file_path(
                bundle.join("Contents/Resources/app.asar/.vite/renderer/main_window/index.html"),
            )
            .map_err(|_| unavailable("Invalid native shell path"))?;
            for candidate in &before {
                if unique(candidate, |n| {
                    n.role() == "AXWebArea" && n.string("AXURL").as_deref() == Some(shell.as_str())
                })?
                .is_some()
                    && reusable_main.replace(candidate.clone()).is_some()
                {
                    return Err(conflict("Native main window is ambiguous"));
                }
            }
        }
        let mut route = url::Url::parse(if provider == "codex" {
            "codex://new"
        } else {
            "claude://code/new"
        })
        .unwrap();
        if provider == "codex" {
            route
                .query_pairs_mut()
                .append_pair("mode", "codex")
                .append_pair("path", cwd_text);
        } else {
            route.query_pairs_mut().append_pair("folder", cwd_text);
        }
        // Codex routes to its focused window. Retain the one Muster allocated.
        // Claude's code/new entry may create a window or reuse its verified main shell.
        let mut reused_codex = false;
        let window = if provider == "codex" {
            let pinned = CODEX_WINDOW
                .lock()
                .unwrap()
                .as_ref()
                .and_then(|pin| pin.live(pid, guard.start, &before))
                .cloned();
            // A cold launch can restore a conversation. Claim its only window
            // directly only when the new-session home is positively identified.
            let window = if let Some(window) = pinned {
                stage("reuse_codex_window");
                reused_codex = true;
                window
            } else {
                // An existing focused, empty Codex home is a safe initial
                // work window. Its project can differ before the directory
                // route; the exact requested project is checked after it.
                let startup = root
                    .element("AXFocusedWindow")
                    .filter(|w| before.contains(w));
                let window = if let Some(startup) = startup
                    && let Some(page) = unique(&startup, |n| new_page("codex", n))?
                    && {
                        let all = nodes(&page)?;
                        codex_home_heading(&all)
                            && all
                                .iter()
                                .any(|n| label(n, "Switch mode, current mode: Codex"))
                            && all.iter().any(|n| {
                                n.role() == "AXPopUpButton"
                                    && n.string("AXDescription")
                                        .is_some_and(|a| a.starts_with("Change project: "))
                            })
                    } {
                    reused_codex = true;
                    stage("reuse_startup_window");
                    startup
                } else {
                    stage("new_window_request");
                    allocate_codex_window(&guard, &root, &before).await?
                };
                *CODEX_WINDOW.lock().unwrap() =
                    Some(CreationWindow::new(pid, guard.start, window.clone()));
                window
            };
            guard.check()?;
            if root.element("AXFocusedWindow").as_ref() != Some(&window) {
                window
                    .raise()
                    .map_err(|_| unavailable("Cannot raise the target Codex window"))?;
            }
            stage("await_new_renderer");
            if covered {
                let limit = Instant::now() + Duration::from_secs(8);
                loop {
                    guard.check()?;
                    if !windows(&root).contains(&window) {
                        return Err(conflict("New native window was replaced"));
                    }
                    if window.role() == "AXWindow"
                        && root.element("AXFocusedWindow").as_ref() == Some(&window)
                        && unique(&window, |n| new_page("codex", n))?.is_some()
                    {
                        break;
                    }
                    if Instant::now() >= limit {
                        return Err(unavailable("New focused Codex document did not appear"));
                    }
                    tokio::time::sleep(Duration::from_millis(75)).await;
                }
            } else {
                renderer_ready(&guard, &root, &window).await?;
            }
            guard.check()?;
            require_focus(&root, &window)?;
            guard.check()?;
            stage("dispatch_directory_route");
            open(&bundle, Some(&route))?;
            window
        } else {
            guard.check()?;
            if let Some(main) = &reusable_main {
                if root.element("AXFocusedWindow").as_ref() != Some(main) {
                    main.raise()
                        .map_err(|_| unavailable("Cannot raise the Claude main window"))?;
                }
                require_focus(&root, main)?;
                guard.check()?;
            }
            open(&bundle, Some(&route))?;
            let deadline = Instant::now() + Duration::from_secs(8);
            loop {
                guard.check()?;
                if let Some(fresh) = muster_agent::native_runtime::session_window(
                    &before_activation,
                    &before,
                    &windows(&root),
                    true,
                    reusable_main.as_ref(),
                )? {
                    break fresh;
                }
                if Instant::now() >= deadline {
                    return Err(conflict("New window identity is ambiguous"));
                }
                tokio::time::sleep(Duration::from_millis(100)).await;
            }
        };
        let reuses_main =
            reusable_main.as_ref() == Some(&window) && before_activation.contains(&window);
        (window, reused_codex, reuses_main)
    };
    // Both providers can expose the window before a late trust dialog. Keep
    // its object identity and wait for the complete form, pressing trust once.
    stage("await_routed_form");
    let deadline = Instant::now() + Duration::from_secs(25);
    let mut trusted = false;
    let mut chose_directory = false;
    let mut last_form_state = None;
    let (page, composer, project) = loop {
        guard.check()?;
        if !windows(&root).contains(&window) {
            return Err(conflict("New native window was replaced"));
        }
        if !background
            && !trusted
            && let Some(trust) = unique(&window, |n| {
                n.role() == "AXButton" && (label(n, "Trust folder") || label(n, "Trust workspace"))
            })?
        {
            if !nodes(&window)?.iter().any(|n| {
                n.role() == "AXStaticText" && n.string("AXValue").as_deref() == Some(cwd_text)
            }) {
                return Err(conflict(
                    "Trust dialog does not name the requested directory",
                ));
            }
            guard.press(&trust)?;
            trusted = true;
        }
        let page = unique(&window, |n| new_page(provider, n))?;
        let mut form_state = (page.is_some(), false, false, trusted);
        if let Some(page) = page
            && (!reuses_main || empty_session_marker(&page)?)
        {
            let project = selection_matches(provider, &page, cwd, reused_codex)?;
            form_state.1 = project.is_some();
            if project.is_none()
                && background
                && provider == "claude"
                && !chose_directory
                && picker(&page, "Folder: ")?.is_some()
            {
                chose_directory = true;
                stage("choose_directory");
                choose_claude_directory(&guard, &window, &page, cwd).await?;
                continue;
            }
            if let Some(project) = project {
                let composer = unique(&page, |n| profile.controls().composer.matches(n))?;
                form_state.2 = composer.is_some();
                if let Some(composer) = composer {
                    super::navigation_restore::track_new_page(profile.provider(), &window, &page);
                    break (page, composer, project);
                }
            }
        }
        if last_form_state != Some(form_state) {
            log::info!(
                "Native creation form: page={} project={} composer={} trusted={} elapsed_ms={}",
                form_state.0,
                form_state.1,
                form_state.2,
                form_state.3,
                started.elapsed().as_millis()
            );
            last_form_state = Some(form_state);
        }
        if Instant::now() >= deadline {
            return Err(unavailable(
                "Native form did not become ready; no submission",
            ));
        }
        tokio::time::sleep(Duration::from_millis(100)).await;
    };
    stage("routed_form_verified");
    let selection = || -> Result<bool, Error> {
        guard.check()?;
        if !windows(&root).contains(&window) || !window.contains(&page) || !page.contains(&composer)
        {
            return Err(conflict("Native form was closed or replaced"));
        }
        Ok(new_page(provider, &page)
            && (!reuses_main || empty_session_marker(&page)?)
            && selection_matches(provider, &page, cwd, reused_codex)?.as_ref() == Some(&project))
    };
    let form = || -> Result<(), Error> {
        if selection()? {
            Ok(())
        } else {
            Err(conflict(
                "Exact native directory and local mode are not verified",
            ))
        }
    };
    if composer
        .string("AXValue")
        .is_none_or(|v| !profile.is_empty(&v))
    {
        return Err(conflict("Native composer is not empty"));
    }
    form()?;
    let options = Form {
        check: &form,
        page: &page,
        app: &root,
    };
    stage("prepare_options");
    let shown = native_options::prepare_new(&options, model, effort).await?;
    // Opening and closing the pickers must not have typed anything.
    if composer
        .string("AXValue")
        .is_none_or(|v| !profile.is_empty(&v))
    {
        return Err(conflict("Native composer is not empty"));
    }
    form()?;
    if !background {
        require_focus(&root, &window)?;
    }
    composer
        .set_string("AXValue", prompt)
        .map_err(|_| unavailable("Draft write refused"))?;
    let deadline = Instant::now() + Duration::from_secs(4);
    loop {
        form()?;
        if composer.string("AXValue").as_deref() == Some(prompt) {
            break;
        }
        if Instant::now() >= deadline {
            return Err(conflict("Draft did not echo; not submitted"));
        }
        tokio::time::sleep(Duration::from_millis(100)).await;
    }
    let send = wait(&guard, &page, |n| {
        profile.controls().send.matches(n) && n.bool("AXEnabled") == Some(true)
    })
    .await?;
    native_options::still_shown(&options, &shown).await?;
    form()?;
    if composer.string("AXValue").as_deref() != Some(prompt) {
        return Err(conflict("Draft changed before submit"));
    }
    if !background {
        require_focus(&root, &window)?;
    }
    stage("submit");
    control_guard::delivery_attempt();
    guard.press(&send).map_err(|e| {
        Error::UnknownOutcome(format!("Native submit attempted: {e}; do not replay"))
    })?;
    Ok(Submitted {
        guard,
        root,
        window,
        page,
    })
}
