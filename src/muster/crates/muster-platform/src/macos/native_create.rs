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

mod codex_picker;

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
    single(nodes(root)?.into_iter().filter(test))
}

fn single<T>(mut matches: impl Iterator<Item = T>) -> Result<Option<T>, Error> {
    let first = matches.next();
    if matches.next().is_some() {
        return Err(conflict("Ambiguous native control"));
    }
    Ok(first)
}
/// Deeper than any recorded app tree; reached only by a cycle.
const MAX_ANCESTRY: usize = 256;

/// A composer of the session's own view: not one inside a region that holds
/// a conversation of its own, such as Claude's side chat (ADR 0221). An
/// ancestry too deep to read counts as no composer.
fn session_composer(profile: &App, n: &Element) -> bool {
    let controls = profile.controls();
    if !controls.composer.matches(n) {
        return false;
    }
    if controls.auxiliary_regions.is_empty() {
        return true;
    }
    let mut parent = n.element("AXParent");
    for _ in 0..MAX_ANCESTRY {
        let Some(node) = parent else {
            return true;
        };
        let subrole = node.string("AXSubrole").unwrap_or_default();
        if controls.is_auxiliary_region(&subrole, &node) {
            return false;
        }
        if node.role() == "AXWindow" {
            return true;
        }
        parent = node.element("AXParent");
    }
    false
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
    covered: bool,
    foreground: Option<input::QuietWindow>,
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
    covered_owner: Option<NativeConsole>,
}
impl Shown {
    fn reveal(app: &Element, guard: &Guard) -> Result<Option<Self>, Error> {
        if app.bool("AXHidden") != Some(true) {
            return Ok(None);
        }
        guard.check()?;
        app.set_bool("AXHidden", false)
            .map_err(|_| unavailable("The hidden app could not be shown without activating it"))?;
        Ok(Some(Self {
            app: app.clone(),
            pid: guard.pid,
            hide: true,
            covered_owner: guard.covered.then_some(guard.mode),
        }))
    }
}
impl Drop for Shown {
    fn drop(&mut self) {
        if self.hide
            && self.covered_owner.is_none_or(|mode| {
                console().is_ok()
                    && mode.creation_focus_holds(
                        true,
                        NativeConsole::observe(
                            false,
                            Some(self.pid),
                            super::events::foreground_owner(),
                        ),
                    )
            })
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
            && !self.mode.creation_focus_holds(
                self.covered,
                NativeConsole::observe(false, Some(self.pid), super::events::foreground_owner()),
            )
        {
            return Err(conflict(
                "Foreground application changed during native creation",
            ));
        }
        if let Some(watch) = &self.foreground {
            watch.check()?;
            if !self.covered {
                return Ok(());
            }
        }
        let generation = if self.mode.target_scoped_input(self.covered) {
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
                ready &= unique(window, |n| session_composer(guard.profile, n))?.is_some();
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

/// Claude's environments, each a name for a main folder (ADR 0221, 0230).
fn claude_environment_state() -> Option<Vec<u8>> {
    let home = PathBuf::from(std::env::var_os("HOME")?);
    std::fs::read(home.join("Library/Application Support/Claude/ccd-session-presets.json")).ok()
}

/// The environment the form names, when its main folder is `cwd`. Its extra
/// folders are attached to the session and do not change its folder.
fn claude_environment(cwd: &Path) -> Option<String> {
    claude_environment_state()
        .and_then(|state| muster_agent::native_runtime::claude_environment(&state, cwd))
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
    let state = codex_project_state()
        .ok_or_else(|| unavailable("Codex project registry is unavailable"))?;
    muster_agent::native_runtime::require_codex_project(&state, cwd).map(|_| ())
}

fn selection_matches(
    provider: &str,
    page: &Element,
    cwd: &Path,
    require_codex_home: bool,
) -> Result<Option<Selection>, Error> {
    let all = nodes(page)?;
    let proof = if provider == "claude" {
        let environments: Vec<_> = all
            .iter()
            .filter(|n| n.role() == "AXPopUpButton")
            .filter_map(|n| n.string("AXDescription"))
            .filter_map(|d| d.strip_prefix("Environment: ").map(str::to_owned))
            .collect();
        let folders = all.iter().any(|n| {
            n.role() == "AXPopUpButton"
                && n.string("AXDescription")
                    .is_some_and(|d| d.starts_with("Folder: "))
        });
        // A named environment carries its main folder (and any extra ones)
        // and shows no folder picker; "This computer" shows the folder it is
        // set to (ADR 0230).
        let named = matches!(environments.as_slice(), [name] if name != "This computer")
            && !folders
            && claude_environment(cwd).as_deref() == environments.first().map(String::as_str);
        let good = named
            || all.iter().any(|n| {
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

/// Opens the environment picker and presses the local environment `name`,
/// first showing the rest of the local list when the app folds it. Only
/// items under the picker's "Local" group count; the picker is closed again
/// without one.
async fn choose_claude_environment(
    guard: &Guard,
    window: &Element,
    environment: &Element,
    name: &str,
) -> Result<(), Error> {
    if !menu_items(window)?.is_empty() {
        return Err(conflict("A menu is already open in the provider window"));
    }
    guard.press(environment)?;
    let mut expanded = false;
    let mut deadline = Instant::now() + Duration::from_secs(2);
    let outcome = loop {
        guard.check()?;
        let local = unique(window, |n| {
            n.role() == "AXGroup" && n.string("AXTitle").as_deref() == Some("Local")
        });
        let items = match local {
            Ok(Some(group)) => nodes(&group)?
                .into_iter()
                .filter(|n| n.role().starts_with("AXMenuItem"))
                .collect(),
            Ok(None) => Vec::new(),
            Err(error) => break Err(error),
        };
        let matching: Vec<_> = items
            .iter()
            .filter(|i| item_labels(i).iter().any(|l| l == name))
            .collect();
        match matching.as_slice() {
            [item] => break Ok((*item).clone()),
            [_, _, ..] => break Err(conflict("Claude lists the environment more than once")),
            [] => {}
        }
        let more: Vec<_> = items
            .iter()
            .filter(|i| {
                i.string("AXTitle")
                    .and_then(|t| t.strip_prefix("More ").map(|n| n.parse::<u32>().is_ok()))
                    .unwrap_or(false)
            })
            .collect();
        if !expanded && let [more] = more.as_slice() {
            guard.press(more)?;
            expanded = true;
            deadline = Instant::now() + Duration::from_secs(2);
        } else if Instant::now() >= deadline {
            break Err(if items.is_empty() {
                unavailable("The environment picker did not open; nothing was submitted")
            } else {
                Error::Invalid(format!(
                    "Claude does not list the {name} environment; nothing was submitted"
                ))
            });
        }
        tokio::time::sleep(Duration::from_millis(100)).await;
    };
    match outcome {
        Ok(item) => guard.press(&item),
        Err(error) => {
            // Toggle only the picker this step opened; never a global Escape.
            if !menu_items(window)?.is_empty() {
                guard.press(environment)?;
                let deadline = Instant::now() + Duration::from_secs(2);
                while !menu_items(window)?.is_empty() && Instant::now() < deadline {
                    tokio::time::sleep(Duration::from_millis(100)).await;
                    guard.check()?;
                }
            }
            Err(error)
        }
    }
}

/// Claude's new-session form names a folder through its environment picker:
/// a named environment of the app's own runs in its main folder, with any
/// extra folders attached, and "This computer" adds a folder picker. Prefer
/// the environment whose main folder is `cwd` (ADR 0230); otherwise choose
/// "This computer" and the exact folder from the app's recent list. Muster
/// never adds a folder or an environment: one Claude does not list fails.
async fn choose_claude_directory(
    guard: &Guard,
    window: &Element,
    page: &Element,
    cwd: &Path,
) -> Result<(), Error> {
    let environment =
        picker(page, "Environment: ")?.ok_or_else(|| unavailable("No environment picker"))?;
    if let Some(name) = claude_environment(cwd) {
        if environment.string("AXDescription") == Some(format!("Environment: {name}")) {
            return Ok(());
        }
        return choose_claude_environment(guard, window, &environment, &name).await;
    }
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

fn require_focus(root: &Element, window: &Element) -> Result<(), Error> {
    if root.element("AXFocusedWindow").as_ref() != Some(window) {
        return Err(conflict("Native route target is not the focused window"));
    }
    Ok(())
}

#[derive(PartialEq)]
enum BackgroundRoute<A> {
    Ready,
    Direct(A),
    ProjectPicker(A, String),
}

impl<A> BackgroundRoute<A> {
    fn priority(&self) -> u8 {
        match self {
            Self::Ready => 0,
            Self::Direct(_) => 1,
            Self::ProjectPicker(..) => 2,
        }
    }
}

#[derive(PartialEq)]
struct BackgroundTarget<W = Element, A = Element> {
    window: W,
    route: BackgroundRoute<A>,
}

fn preferred_target<W: PartialEq, A>(
    targets: Vec<BackgroundTarget<W, A>>,
    focused: Option<&W>,
) -> Option<BackgroundTarget<W, A>> {
    let best = targets.iter().map(|t| t.route.priority()).min()?;
    let mut candidates = targets.into_iter().filter(|t| t.route.priority() == best);
    let first = candidates.next()?;
    if focused == Some(&first.window) {
        return Some(first);
    }
    let Some(second) = candidates.next() else {
        return Some(first);
    };
    std::iter::once(second)
        .chain(candidates)
        .find(|t| Some(&t.window) == focused)
}

fn is_dialog(n: &Element) -> bool {
    n.role() == "AXSheet"
        || matches!(
            n.string("AXSubrole").as_deref(),
            Some(
                "AXApplicationDialog" | "AXDialog" | "AXSystemDialog" | "AXApplicationAlertDialog"
            )
        )
}

fn has_dialog(tree: &[Element]) -> bool {
    tree.iter().any(is_dialog)
}

fn safe_to_navigate(profile: &App, window: &Element, tree: &[Element]) -> bool {
    !has_dialog(tree) && draft_free_window(profile, window, tree)
}

fn draft_free_window(profile: &App, window: &Element, tree: &[Element]) -> bool {
    if window.role() != "AXWindow" || window.bool("AXMinimized") == Some(true) {
        return false;
    }
    let mut editors = tree.iter().filter(|n| session_composer(profile, n));
    editors
        .next()
        .and_then(|e| e.string("AXValue"))
        .is_some_and(|v| profile.is_empty(&v))
        && editors.next().is_none()
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
/// window's own in-app action that opens one: a project button, the Codex
/// sidebar New chat with a project picker, or Claude's sidebar New. Navigation requires a
/// draft-free window without a sheet or dialog (ADRs 0220 and 0297).
fn ready_background_window(
    root: &Element,
    guard: &Guard,
    provider: &str,
    cwd: &Path,
    explain: bool,
) -> Result<Option<BackgroundTarget>, Error> {
    // A covered creation looks once; it logs why a window offers no route.
    let skip = |index: usize, reason: &str| {
        if explain {
            log::info!("Native creation in-app route: window={index} {reason}");
        }
    };
    let live = windows(root);
    let pin = CODEX_WINDOW
        .lock()
        .unwrap()
        .as_ref()
        .and_then(|p| p.live(guard.pid, guard.start, &live))
        .cloned();
    let project_name = (provider == "codex")
        .then(codex_project_state)
        .flatten()
        .and_then(|b| muster_agent::native_runtime::codex_sidebar_project(&b, cwd));
    let project_action = project_name
        .as_ref()
        .map(|name| format!("Start new chat in {name}"));
    let mut targets = Vec::new();
    let focused = root.element("AXFocusedWindow");
    if explain {
        log::info!(
            "Native creation in-app route: windows={} pinned={} project_button={}",
            live.len(),
            pin.is_some(),
            project_action.is_some()
        );
    }
    for (index, window) in live.into_iter().enumerate() {
        if guard
            .foreground
            .as_ref()
            .is_some_and(|w| w.window() != &window)
        {
            continue;
        }
        if provider == "codex" && pin.as_ref().is_some_and(|p| p != &window) {
            skip(index, "not the pinned window");
            continue;
        }
        let Ok(tree) = nodes(&window) else {
            skip(index, "tree unreadable");
            continue;
        };
        if !safe_to_navigate(guard.profile, &window, &tree) {
            skip(
                index,
                "window unavailable, dialog or nonempty/ambiguous composer",
            );
            continue;
        }
        let pages: Vec<_> = tree.iter().filter(|n| new_page(provider, n)).collect();
        if let [page] = pages.as_slice()
            && (provider != "claude" || empty_session_marker(page)?)
            && selection_matches(provider, page, cwd, true)?.is_some()
        {
            let target = BackgroundTarget {
                window,
                route: BackgroundRoute::Ready,
            };
            if focused.as_ref() == Some(&target.window) {
                return Ok(Some(target));
            }
            targets.push(target);
            continue;
        }
        if explain {
            let page = pages.first();
            skip(
                index,
                &format!(
                    "pages={} empty_marker={:?} project={:?} focused={}",
                    pages.len(),
                    page.map(|p| empty_session_marker(p).ok()),
                    page.map(|p| selection_matches(provider, p, cwd, true)
                        .map(|m| m.is_some())
                        .ok()),
                    focused.as_ref() == Some(&window)
                ),
            );
        }
        let action = match provider {
            "codex" => project_action
                .as_deref()
                .map(|name| ("AXDescription", name)),
            _ if claude_code_window(&tree) => Some(("AXTitle", "New")),
            _ => None,
        };
        if let Some((attribute, name)) = action {
            let found: Vec<_> = tree
                .iter()
                .filter(|n| n.role() == "AXButton" && n.string(attribute).as_deref() == Some(name))
                .take(2)
                .collect();
            if let [button] = found.as_slice() {
                targets.push(BackgroundTarget {
                    window,
                    route: BackgroundRoute::Direct((*button).clone()),
                });
            } else if found.is_empty()
                && provider == "codex"
                && let Some(project) = &project_name
                && let Some(button) = codex_picker::new_chat(&tree)?
            {
                targets.push(BackgroundTarget {
                    window,
                    route: BackgroundRoute::ProjectPicker(button, project.clone()),
                });
            } else {
                skip(index, &format!("route buttons={}", found.len()));
            }
        } else {
            skip(index, "no route control");
        }
    }
    Ok(preferred_target(targets, focused.as_ref()))
}

struct Admission {
    guard: Guard,
    root: Element,
    bundle: PathBuf,
    background_window: Option<BackgroundTarget>,
    launched: bool,
    started: Instant,
}

async fn await_activation(
    pid: u32,
    previous: u32,
    window: &Element,
    generation: u64,
) -> Result<(), Error> {
    let deadline = Instant::now() + Duration::from_secs(3);
    loop {
        console()?;
        if muster_agent::native_runtime::handoff_arrived(
            previous,
            pid,
            super::events::foreground_owner(),
            generation != input::user_input_generation(),
        )? {
            return require_focus(&Element::application(pid), window);
        }
        if Instant::now() >= deadline {
            return Err(conflict("Native application did not take focus"));
        }
        tokio::time::sleep(Duration::from_millis(50)).await;
    }
}

async fn prepare(provider: &str, cwd: &Path, covered: bool) -> Result<Admission, Error> {
    let profile = muster_agent::profile::app(provider)
        .filter(|_| matches!(provider, "codex" | "claude"))
        .ok_or_else(|| Error::Invalid("Unsupported native provider".into()))?;
    let started = control_guard::execution_started();
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
    missing_directory(provider, cwd)?;
    let deadline = started + WINDOW_STARTUP;
    let mut launch = None;
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
        if let Some((front, generation)) = launch {
            let focus = super::events::foreground_owner();
            if input::user_input_generation() != generation
                || !matches!(focus, muster_agent::local_input::Owner::Process(p) if Some(p) == pid || p == front)
            {
                return Err(conflict("Desktop changed during native launch"));
            }
        }
        let mode = NativeConsole::for_creation(covered, pid, super::events::foreground_owner());
        if mode == NativeConsole::Provider
            && let Some(mut taken) = shown.take()
        {
            // Its user brought the app to the front: it stays shown.
            taken.hide = false;
        }
        if let Some(pid) = pid {
            let foreground = mode == NativeConsole::Provider;
            let quiet = if mode.target_scoped_input(covered) {
                input::target_input_idle(&[pid])
            } else {
                input::user_input_idle()
            };
            let reason = match mode {
                NativeConsole::Provider => "provider_foreground",
                NativeConsole::Background(_) if quiet < Duration::from_millis(500) => {
                    "provider_input"
                }
                NativeConsole::Background(_) if covered => "covered_other_app_foreground",
                NativeConsole::Background(_) => "other_app_foreground",
                NativeConsole::Waiting => "no_focus_owner",
                NativeConsole::Covered if quiet < Duration::from_millis(500) => "covered_input",
                NativeConsole::Covered => "covered",
            };
            if last_reason != Some(reason) {
                stage(reason);
                last_reason = Some(reason);
            }
            if mode.may_create() && (foreground || quiet >= Duration::from_millis(500)) {
                let watch = if foreground {
                    Some(input::quiet_foreground(pid).await?)
                } else {
                    None
                };
                let mut guard = Guard {
                    profile,
                    pid,
                    start: codesign::process_start(pid).ok_or_else(|| unavailable("App exited"))?,
                    generation: if mode.target_scoped_input(covered) {
                        input::target_input_generation(pid)
                    } else {
                        input::user_input_generation()
                    },
                    mode,
                    covered,
                    foreground: watch,
                    shown: None,
                };
                guard.check()?;
                let root = Element::application(pid);
                root.enable_manual_accessibility();
                let mut settle = Duration::ZERO;
                if shown.is_none()
                    && let Some(revealed) = Shown::reveal(&root, &guard)?
                {
                    shown = Some(revealed);
                    stage("revealed_hidden_app");
                    settle = Duration::from_secs(3);
                    tokio::time::sleep(Duration::from_millis(700)).await;
                }
                // Observation only: a changed console or input is a requeue.
                let until = Instant::now() + settle;
                let found = loop {
                    guard.check()?;
                    if let Some(window) =
                        ready_background_window(&root, &guard, provider, cwd, false)?
                    {
                        break Some(window);
                    }
                    if Instant::now() >= until {
                        break None;
                    }
                    tokio::time::sleep(Duration::from_millis(150)).await;
                };
                if let Some(window) = found {
                    guard.check()?;
                    stage(if foreground {
                        "foreground_form_ready"
                    } else {
                        "background_form_ready"
                    });
                    guard.shown = shown.take();
                    break (guard, root, Some(window));
                }
                // A directory route can take focus. A complete, unambiguous
                // window and registered project are required before a handoff.
                let live = windows(&root);
                let [window] = live.as_slice() else {
                    return Err(conflict(
                        "No unique native window for foreground navigation",
                    ));
                };
                let tree = nodes(window)?;
                if !safe_to_navigate(profile, window, &tree) {
                    return Err(conflict(
                        "Native window has a draft, dialog or ambiguous composer",
                    ));
                }
                if provider == "codex" {
                    let name = codex_project_state()
                        .and_then(|state| {
                            muster_agent::native_runtime::codex_sidebar_project(&state, cwd)
                        })
                        .ok_or_else(|| {
                            conflict("No unique Codex project for foreground navigation")
                        })?;
                    let action = format!("Start new chat in {name}");
                    if tree.iter().any(|n| {
                        n.role() == "AXButton"
                            && n.string("AXDescription").as_deref() == Some(&action)
                    }) || tree
                        .iter()
                        .any(|n| n.role() == "AXButton" && label(n, "New chat"))
                    {
                        return Err(conflict(
                            "Codex navigation controls are ambiguous or unavailable",
                        ));
                    }
                }
                if provider == "claude"
                    && claude_code_window(&tree)
                    && tree.iter().any(|n| {
                        n.role() == "AXButton" && n.string("AXTitle").as_deref() == Some("New")
                    })
                {
                    return Err(conflict(
                        "Claude navigation controls are ambiguous or unavailable",
                    ));
                }
                if foreground {
                    guard.check()?;
                    break (guard, root, None);
                }
                if let NativeConsole::Background(front) = mode {
                    let outgoing = input::quiet_foreground(front).await?;
                    guard.check()?;
                    if !safe_to_navigate(profile, window, &nodes(window)?) {
                        return Err(conflict(
                            "Native window changed before foreground navigation",
                        ));
                    }
                    let generation = input::user_input_generation();
                    outgoing.check()?;
                    open(&bundle, None)?;
                    await_activation(pid, front, window, generation).await?;
                    guard.mode = NativeConsole::Provider;
                    guard.foreground = Some(input::quiet_foreground(pid).await?);
                    guard.generation = generation;
                    if input::user_input_generation() != generation {
                        return Err(conflict("User input during foreground handoff"));
                    }
                    guard.check()?;
                    require_focus(&root, window)?;
                    break (guard, root, None);
                }
            }
        } else if launch.is_none() {
            let muster_agent::local_input::Owner::Process(front) =
                super::events::foreground_owner()
            else {
                return Err(conflict("No foreground window for native launch"));
            };
            let outgoing = input::quiet_foreground(front).await?;
            console()?;
            let generation = input::user_input_generation();
            outgoing.check()?;
            open(&bundle, None)?;
            launch = Some((front, generation));
        }
        if pid.is_some() && !matches!(mode, NativeConsole::Background(_)) {
            mode.check_wait(provider)?;
        }
        if Instant::now() >= deadline {
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
        launched: launch.is_some(),
        started,
    })
}

/// Follows an in-app route of ADR 0220, rule 2: presses the window's control
/// for a new session, if the form is not already shown, and pins a Codex
/// window. Returns the window, whether Codex reuses it, and whether it is
/// Claude's main window.
fn in_app_route(
    guard: &Guard,
    root: &Element,
    provider: &str,
    target: BackgroundTarget,
    stage: &dyn Fn(&str),
) -> Result<(Element, bool, bool), Error> {
    let window = target.window;
    if let BackgroundRoute::Direct(action) | BackgroundRoute::ProjectPicker(action, _) =
        &target.route
    {
        guard.check()?;
        if !windows(root).contains(&window) || !window.contains(action) {
            return Err(conflict("Background creation target was replaced"));
        }
        let tree = nodes(&window)?;
        if !safe_to_navigate(guard.profile, &window, &tree) {
            return Err(conflict("Background window is no longer safe to navigate"));
        }
        let use_picker = matches!(target.route, BackgroundRoute::ProjectPicker(..));
        let home = if use_picker && let Some(page) = unique(&window, |n| new_page("codex", n))? {
            verified_empty_form("codex", &page) && picker(&page, "Change project: ")?.is_some()
        } else {
            false
        };
        if home {
            stage("codex_background_existing_home");
        } else {
            if use_picker {
                stage("codex_background_new_chat");
            }
            guard.press(action)?;
        }
    }
    if provider == "codex" {
        let mut pin = CODEX_WINDOW.lock().unwrap();
        let same = pin
            .as_ref()
            .and_then(|p| p.live(guard.pid, guard.start, std::slice::from_ref(&window)))
            .is_some();
        stage(if same {
            "background_pinned_window_reused"
        } else {
            "background_window_pinned"
        });
        *pin = Some(CreationWindow::new(guard.pid, guard.start, window.clone()));
    }
    Ok((window, provider == "codex", provider == "claude"))
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
) -> Result<Submitted, Error> {
    let Admission {
        mut guard,
        root,
        bundle,
        background_window,
        launched,
        started,
    } = prepare(provider, cwd, covered).await?;
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
    let mut picker_project = background_window
        .as_ref()
        .and_then(|target| match &target.route {
            BackgroundRoute::ProjectPicker(_, name) => Some(name.clone()),
            _ => None,
        });
    let mut in_app = background_window.is_some();
    let (window, reused_codex, reuses_main) = 'route: {
        if let Some(target) = background_window {
            break 'route in_app_route(&guard, &root, provider, target, &stage)?;
        }
        let was_running = !launched;
        // Foreground routes bind the quiet window checked during admission.
        let before_activation = if was_running {
            settled_windows(&guard, &root, false).await?
        } else {
            Vec::new()
        };
        guard.check()?;
        require_focus(
            &root,
            guard
                .foreground
                .as_ref()
                .ok_or_else(|| conflict("Foreground window unavailable"))?
                .window(),
        )?;
        stage("activated");
        let before = settled_windows(&guard, &root, true).await?;
        // Check every retained window independently: several complete forms must
        // not exhaust one form's node budget. Never navigate over a nonempty draft
        // if the provider ignores raise; missing/ambiguous editors fail closed.
        stage("windows_ready");
        let [current] = before.as_slice() else {
            return Err(conflict(
                "Foreground navigation requires a single identified window",
            ));
        };
        if !safe_to_navigate(profile, current, &nodes(current)?) {
            return Err(conflict(
                "Foreground window has a draft, dialog or ambiguous composer",
            ));
        }
        // Under cover, too, the in-app route is used when a window offers
        // it, and the directory route only when none does (ADR 0221). The
        // directory route reloads a form that is already shown, after the
        // form was verified and before the draft is written. The windows
        // are read here, once the app is active and they have settled.
        if let Some(target) = ready_background_window(&root, &guard, provider, cwd, true)? {
            if let BackgroundRoute::ProjectPicker(_, name) = &target.route {
                picker_project = Some(name.clone());
            }
            in_app = true;
            stage("foreground_in_app_route");
            break 'route in_app_route(&guard, &root, provider, target, &stage)?;
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
        let reused_codex = provider == "codex";
        let window = if reused_codex {
            require_focus(&root, current)?;
            guard.check()?;
            stage("dispatch_directory_route");
            open(&bundle, Some(&route))?;
            current.clone()
        } else {
            guard.check()?;
            if let Some(main) = &reusable_main {
                require_focus(&root, main)?;
                guard.check()?;
            }
            // The OS may focus a newly created Claude window. Keep the PID,
            // signature and global HID generation pinned across that one transition.
            guard.generation = input::user_input_generation();
            guard.check()?;
            guard.foreground = None;
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
                    require_focus(&root, &fresh)?;
                    let watch = input::quiet_foreground(pid).await?;
                    guard.check()?;
                    guard.foreground = Some(watch);
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
    // Keep the routed window identity while its form settles.
    stage("await_routed_form");
    let deadline = Instant::now() + Duration::from_secs(25);
    let mut chose_directory = false;
    let mut last_form_state = None;
    let (page, composer, project) = loop {
        guard.check()?;
        if !windows(&root).contains(&window) {
            return Err(conflict("New native window was replaced"));
        }
        if has_dialog(&nodes(&window)?) {
            return Err(conflict("Native window opened a dialog"));
        }
        let page = unique(&window, |n| new_page(provider, n))?;
        let mut form_state = (page.is_some(), false, false);
        if let Some(page) = page
            && (!reuses_main || empty_session_marker(&page)?)
        {
            let project = selection_matches(provider, &page, cwd, reused_codex)?;
            form_state.1 = project.is_some();
            if project.is_none()
                && let Some(name) = picker_project.as_deref()
                && !chose_directory
                && verified_empty_form("codex", &page)
                && let Some(selector) = picker(&page, "Change project: ")?
            {
                stage("codex_background_project_picker");
                codex_picker::choose(&guard, &window, &page, &selector, cwd, name).await?;
                chose_directory = true;
                continue;
            }
            if project.is_none()
                && in_app
                && provider == "claude"
                && !chose_directory
                && picker(&page, "Environment: ")?.is_some()
            {
                chose_directory = true;
                stage("choose_directory");
                choose_claude_directory(&guard, &window, &page, cwd).await?;
                continue;
            }
            if let Some(project) = project {
                let composer = unique(&page, |n| session_composer(profile, n))?;
                form_state.2 = composer.is_some();
                if let Some(composer) = composer {
                    super::navigation_restore::track_new_page(profile.provider(), &window, &page);
                    break (page, composer, project);
                }
            }
        }
        if last_form_state != Some(form_state) {
            log::info!(
                "Native creation form: page={} project={} composer={} elapsed_ms={}",
                form_state.0,
                form_state.1,
                form_state.2,
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
        if has_dialog(&nodes(&window)?) {
            return Err(conflict("Native window opened a dialog"));
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
    if guard.foreground.is_some() {
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
    if guard.foreground.is_some() {
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

#[cfg(test)]
mod route_tests {
    use super::*;
    use BackgroundRoute::*;

    #[test]
    fn forms_precede_project_buttons_then_generic_new_chat() {
        let pick = |routes: Vec<BackgroundRoute<()>>, focused: Option<usize>| {
            let windows = routes
                .into_iter()
                .enumerate()
                .map(|(window, route)| BackgroundTarget { window, route })
                .collect();
            preferred_target(windows, focused.as_ref()).map(|t| t.window)
        };
        let generic = || ProjectPicker((), "Project".into());
        assert_eq!(pick(vec![generic(), Direct(()), Ready], Some(0)), Some(2));
        assert_eq!(pick(vec![generic(), Direct(())], Some(0)), Some(1));
        assert_eq!(pick(vec![generic()], None), Some(0));
        assert_eq!(pick(vec![generic(), generic()], None), None);
        assert_eq!(pick(vec![generic(), generic()], Some(1)), Some(1));
        assert_eq!(pick(vec![generic(), Ready, Ready], Some(0)), None);
    }
}
