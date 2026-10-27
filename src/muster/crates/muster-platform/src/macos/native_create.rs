//! Native new-session OS adapter. The daemon owns authorization and delivery.
use super::{
    ax::{self, Element},
    codesign, input, native_app,
};
use muster_agent::{Error, control_guard, profile::App};
use std::{
    path::{Path, PathBuf},
    time::{Duration, Instant},
};

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
}
impl Guard {
    fn check(&self) -> Result<(), Error> {
        console()?;
        if codesign::process_start(self.pid) != Some(self.start)
            || !codesign::process_satisfies(self.pid, &self.profile.app.requirement())
            || input::user_input_generation() != self.generation
        {
            return Err(conflict(
                "Native process or physical input changed; nothing is retried",
            ));
        }
        Ok(())
    }
    fn press(&self, e: &Element) -> Result<(), Error> {
        self.check()?;
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
    if !command
        .status()
        .map_err(|_| unavailable("App launch failed"))?
        .success()
    {
        return Err(unavailable("App launch was refused"));
    }
    Ok(())
}

async fn settled_windows(
    guard: &Guard,
    root: &Element,
    require_renderer: bool,
) -> Result<Vec<Element>, Error> {
    let until = Instant::now() + Duration::from_secs(25);
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
                ready &= unique(window, |n| guard.profile.controls.composer.matches(n))?.is_some();
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
    let deadline = Instant::now() + Duration::from_secs(25);
    let mut ready_since = None;
    loop {
        guard.check()?;
        if !windows(root).contains(window) {
            return Err(conflict("Native startup window was replaced"));
        }
        let ready = unique(window, |n| guard.profile.controls.composer.matches(n))?.is_some();
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

fn selection_matches(provider: &str, page: &Element, cwd: &Path) -> Result<bool, Error> {
    let all = nodes(page)?;
    let good = if provider == "claude" {
        all.iter().any(|n| {
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
        })
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
        let state = std::env::var_os("CODEX_HOME")
            .map(PathBuf::from)
            .unwrap_or_else(|| {
                PathBuf::from(std::env::var_os("HOME").unwrap_or_default()).join(".codex")
            })
            .join(".codex-global-state.json");
        labels.len() == 1
            && std::fs::read(state)
                .is_ok_and(|b| muster_agent::native_runtime::codex_project(&b, &labels[0], cwd))
            && all
                .iter()
                .any(|n| label(n, "Switch mode, current mode: Codex"))
            && all.iter().any(|n| {
                n.role() == "AXStaticText"
                    && n.string("AXValue").as_deref() == Some("This computer")
            })
    };
    Ok(good)
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

pub async fn submit(provider: &str, cwd: &Path, prompt: &str) -> Result<Submitted, Error> {
    let profile = muster_agent::profile::app(provider)
        .filter(|_| matches!(provider, "codex" | "claude"))
        .ok_or_else(|| Error::Invalid("Unsupported native provider".into()))?;
    console()?;
    if !ax::trusted() {
        return Err(unavailable(
            "Native creation needs Accessibility permission",
        ));
    }
    // Quiet admission precedes navigation, never a retry after a side effect.
    let deadline = Instant::now() + Duration::from_secs(5);
    while input::user_input_idle() < Duration::from_millis(500) {
        console()?;
        if Instant::now() >= deadline {
            return Err(conflict("Physical input is active"));
        }
        tokio::time::sleep(Duration::from_millis(100)).await;
    }
    let generation = input::user_input_generation();
    let bundle = bundle_path(profile)?;
    let was_running = !native_app::app_pids(profile).is_empty();
    if !was_running {
        open(&bundle, None)?;
    }
    let start_wait = Instant::now();
    let pid = loop {
        console()?;
        let pids = native_app::app_pids(profile);
        if pids.len() == 1 {
            break pids[0];
        }
        if pids.len() > 1 || start_wait.elapsed() > Duration::from_secs(10) {
            return Err(unavailable("Expected one native app process"));
        }
        tokio::time::sleep(Duration::from_millis(100)).await;
    };
    let guard = Guard {
        profile,
        pid,
        start: codesign::process_start(pid).ok_or_else(|| unavailable("App exited"))?,
        generation,
    };
    guard.check()?;
    let root = Element::application(pid);
    root.enable_manual_accessibility();
    let before_activation = if was_running {
        settled_windows(&guard, &root, false).await?
    } else {
        Vec::new()
    };
    guard.check()?;
    open(&bundle, None)?;
    // Cold startup may replace its preliminary window before the renderer is
    // ready. Bind the settled object set, not that transient startup shell.
    let before = settled_windows(&guard, &root, true).await?;
    // Never navigate over any nonempty draft if the provider ignores raise.
    for n in nodes(&root)? {
        if profile.controls.composer.matches(&n)
            && n.string("AXValue").is_none_or(|v| !profile.is_empty(&v))
        {
            return Err(conflict("Existing native draft is preserved"));
        }
    }
    let mut route = url::Url::parse(if provider == "codex" {
        "codex://new"
    } else {
        "claude://code/new"
    })
    .unwrap();
    let cwd_text = cwd
        .to_str()
        .ok_or_else(|| Error::Invalid("Directory must be UTF-8".into()))?;
    if provider == "codex" {
        route
            .query_pairs_mut()
            .append_pair("mode", "codex")
            .append_pair("path", cwd_text);
    } else {
        route.query_pairs_mut().append_pair("folder", cwd_text);
    }
    // Codex navigation reuses the foreground window; first create a new one.
    // Claude's code/new entry opens a standalone window itself.
    let window = if provider == "codex" {
        let menu = root
            .element("AXMenuBar")
            .ok_or_else(|| unavailable("No native menu bar"))?;
        let new = wait(&guard, &menu, |n| {
            n.role() == "AXMenuItem" && label(n, "New Window")
        })
        .await?;
        guard.press(&new)?;
        let deadline = Instant::now() + Duration::from_secs(8);
        let fresh = loop {
            guard.check()?;
            let fresh: Vec<_> = windows(&root)
                .into_iter()
                .filter(|w| !before.contains(w))
                .collect();
            if fresh.len() == 1 {
                break fresh[0].clone();
            }
            if fresh.len() > 1 || Instant::now() >= deadline {
                return Err(conflict("New window identity is ambiguous"));
            }
            tokio::time::sleep(Duration::from_millis(100)).await;
        };
        guard.check()?;
        fresh
            .raise()
            .map_err(|_| unavailable("Cannot raise the new window"))?;
        renderer_ready(&guard, &root, &fresh).await?;
        guard.check()?;
        open(&bundle, Some(&route))?;
        fresh
    } else {
        guard.check()?;
        open(&bundle, Some(&route))?;
        let deadline = Instant::now() + Duration::from_secs(8);
        loop {
            guard.check()?;
            if let Some(fresh) = muster_agent::native_runtime::new_window(
                &before_activation,
                &before,
                &windows(&root),
                true,
            )? {
                break fresh;
            }
            if Instant::now() >= deadline {
                return Err(conflict("New window identity is ambiguous"));
            }
            tokio::time::sleep(Duration::from_millis(100)).await;
        }
    };
    // Both providers can expose the window before a late trust dialog. Keep
    // its object identity and wait for the complete form, pressing trust once.
    let deadline = Instant::now() + Duration::from_secs(25);
    let mut trusted = false;
    let (page, composer) = loop {
        guard.check()?;
        if !windows(&root).contains(&window) {
            return Err(conflict("New native window was replaced"));
        }
        if !trusted
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
        if let Some(page) = unique(&window, |n| {
            n.role() == "AXWebArea"
                && n.string("AXURL").is_some_and(|u| {
                    if provider == "codex" {
                        u == "app://-/index.html"
                    } else {
                        matches!(u.as_str(), "about:blank" | "https://claude.ai/epitaxy")
                    }
                })
        })? && selection_matches(provider, &page, cwd)?
            && let Some(composer) = unique(&page, |n| profile.controls.composer.matches(n))?
        {
            break (page, composer);
        }
        if Instant::now() >= deadline {
            return Err(unavailable(
                "Native form did not become ready; no submission",
            ));
        }
        tokio::time::sleep(Duration::from_millis(100)).await;
    };
    let selection = || -> Result<bool, Error> {
        guard.check()?;
        if !windows(&root).contains(&window) || !window.contains(&page) || !page.contains(&composer)
        {
            return Err(conflict("Native form was closed or replaced"));
        }
        selection_matches(provider, &page, cwd)
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
        profile.controls.send.matches(n) && n.bool("AXEnabled") == Some(true)
    })
    .await?;
    form()?;
    if composer.string("AXValue").as_deref() != Some(prompt) {
        return Err(conflict("Draft changed before submit"));
    }
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
