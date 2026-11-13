//! OS observations for a single borrowed desktop projection. Titles are never
//! session identities. Keep the exact native window for its visibility flags.
use super::{ax::Element, codesign, events, input, lock, native_app, procs};
use muster_agent::{
    Error,
    controller::ProcessIdentity,
    local_input::Owner,
    native_navigation::{
        ActionPermit,
        restore::{self, Page},
    },
    profile::App,
};
use std::time::Duration;

fn unavailable(msg: &str) -> Error {
    Error::Unavailable(msg.into())
}
fn process(pid: u32) -> Option<ProcessIdentity> {
    Some(ProcessIdentity {
        pid,
        start: codesign::process_start(pid)?,
    })
}
fn front() -> Option<ProcessIdentity> {
    match events::foreground_owner() {
        Owner::Process(pid) => process(pid),
        _ => None,
    }
}
pub(super) fn note_foreground() {
    restore::observed_foreground(front());
}
fn window(app: &Element) -> Option<Element> {
    app.element("AXFocusedWindow")
        .or_else(|| app.element("AXMainWindow"))
}

fn from_url(provider: &str, raw: &str) -> Option<Page> {
    let url = url::Url::parse(raw).ok()?;
    let uuid = |id: &str| muster_agent::viewer::is_uuid(id).then(|| id.to_ascii_lowercase());
    match provider {
        "claude" => {
            if let Some(id) = native_app::cli_session_from_url(raw) {
                return Some(Page::Code(id));
            }
            if url.scheme() == "https" && url.host_str() == Some("claude.ai") {
                return uuid(url.path().strip_prefix("/chat/")?).map(Page::Chat);
            }
            None
        }
        "codex" => {
            if let Some(id) = muster_agent::native_sidebar::codex_thread_id(raw) {
                return Some(Page::Code(id));
            }
            if url.scheme() == "https" && url.host_str() == Some("chatgpt.com") {
                return uuid(url.path().strip_prefix("/c/")?).map(Page::Chat);
            }
            None
        }
        "catdesk" => native_app::catdesk_resolver()(&native_app::WebAreaKey {
            url: Some(raw.into()),
            title: None,
        })
        .map(Page::Code),
        _ => None,
    }
}
fn selection(provider: &str, window: &Element) -> Option<Page> {
    let mut pages = Vec::new();
    let (_, stop, _) = window.walk_pruned_report(|n| {
        if n.role() == "AXWebArea"
            && let Some(page) = n.string("AXURL").and_then(|u| from_url(provider, &u))
        {
            if !pages.contains(&page) {
                pages.push(page);
            }
            return false;
        }
        true
    });
    if stop == "complete" && pages.len() == 1 {
        pages.pop()
    } else {
        None
    }
}
// The provider may not allocate a session UUID before a failed creation.
// The signed process's new-window/page handles can prove an unchanged form.
#[derive(Clone)]
pub struct CreationForm {
    window: Element,
    page: Element,
    provider: &'static str,
}
tokio::task_local! { pub(super) static FORM: std::sync::Arc<std::sync::Mutex<Option<CreationForm>>>; }
pub async fn track_form<T>(
    slot: std::sync::Arc<std::sync::Mutex<Option<CreationForm>>>,
    operation: impl Future<Output = T>,
) -> T {
    FORM.scope(slot, operation).await
}
pub fn current_form() -> std::sync::Arc<std::sync::Mutex<Option<CreationForm>>> {
    FORM.try_with(Clone::clone).unwrap_or_default()
}
pub(super) fn track_new_page(provider: &'static str, window: &Element, page: &Element) {
    let _ = FORM.try_with(|slot| {
        *slot.lock().unwrap() = Some(CreationForm {
            window: window.clone(),
            page: page.clone(),
            provider,
        })
    });
}
pub struct Snapshot {
    profile: &'static App,
    single_window: bool,
    target: ProcessIdentity,
    front: ProcessIdentity,
    app: Element,
    window: Option<Element>,
    original: Option<Page>,
    hidden: Option<bool>,
    minimized: Option<bool>,
    hid: u64,
    pub creation: std::sync::Arc<std::sync::Mutex<Option<CreationForm>>>,
}
impl Snapshot {
    pub fn capture(profile: &'static App, existing_session: bool) -> Option<Self> {
        let hid = input::user_input_generation();
        let frontmost = front()?;
        let pids = native_app::app_pids(profile);
        let [pid] = pids.as_slice() else { return None };
        let target = process(*pid)?;
        if !codesign::process_satisfies(*pid, &profile.app.requirement()) {
            return None;
        }
        let app = Element::application(*pid);
        let single_window = existing_session && super::session_window::scoped(profile.provider());
        let window = if single_window {
            Some(super::session_window::root(&app).ok()?)
        } else {
            window(&app)
        };
        let original = window
            .as_ref()
            .and_then(|w| selection(profile.provider(), w));
        let hidden = app.bool("AXHidden");
        let minimized = window.as_ref().and_then(|w| w.bool("AXMinimized"));
        (input::user_input_generation() == hid && front() == Some(frontmost)).then_some(Self {
            profile,
            single_window,
            target,
            front: frontmost,
            app,
            window,
            original,
            hidden,
            minimized,
            hid,
            creation: Default::default(),
        })
    }
    fn current_window(&self) -> Option<Element> {
        if self.single_window {
            let current = super::session_window::selected(&self.app).ok()?;
            (Some(&current) == self.window.as_ref()).then_some(current)
        } else {
            window(&self.app)
        }
    }

    pub fn owns(&self, destination: &Page, expected_front: ProcessIdentity) -> Result<(), Error> {
        muster_agent::control_guard::check()?;
        if !lock::session_state().is_some_and(|v| v.on_console && v.login_done && !v.screen_locked)
            || process(self.target.pid) != Some(self.target)
        {
            return Err(unavailable("console or provider process changed"));
        }
        let shown = self.current_window().and_then(|w| {
            let selected = selection(self.profile.provider(), &w);
            if matches!(destination, Page::Creation(_))
                && selected.is_none()
                && let Some(f) = self.creation.lock().unwrap().as_ref()
                && w == f.window
                && f.window.contains(&f.page)
                && super::native_create::verified_empty_form(f.provider, &f.page)
            {
                return Some(destination.clone());
            }
            selected
        });
        restore::ownership(
            (self.hid, input::user_input_generation()),
            (expected_front, front()),
            (destination, shown.as_ref()),
        )
    }
    pub fn expected_front(
        &self,
        may_activate: bool,
        observed: Option<ProcessIdentity>,
    ) -> ProcessIdentity {
        if may_activate && observed == Some(self.target) {
            self.target
        } else {
            self.front
        }
    }

    fn exact_links(&self, page: &Page) -> Vec<Element> {
        let mut links = Vec::new();
        let Some(w) = self.current_window() else {
            return links;
        };
        let (_, stop, _) = w.walk_pruned_report(|node| {
            if matches!(node.role().as_str(), "AXLink" | "AXRow" | "AXButton")
                && let Some(url) = node.string("AXURL")
                && from_url(self.profile.provider(), &url).as_ref() == Some(page)
                && node.has_action("AXPress")
            {
                links.push(node.clone());
            }
            true
        });
        if stop != "complete" {
            links.clear();
        }
        links
    }
    pub async fn restore(
        &self,
        destination: &Page,
        expected: ProcessIdentity,
    ) -> Result<(), Error> {
        let permit = ActionPermit::for_restoration(self.hid);
        let guard = |page: &Page, owner: ProcessIdentity| {
            self.owns(page, owner)?;
            permit.check(input::user_input_generation())
        };
        guard(destination, expected)?;
        let original = self.original.as_ref();
        let owner = expected;
        if let Some(page) = original
            && page != destination
        {
            let route = match page {
                Page::Code(id) => {
                    muster_agent::native_sidebar::catalog_session_route(self.profile.provider(), id)
                }
                // Chat page controls can expose their own destination URL.
                // Resolve those in the running app instead of inventing a deep link.
                Page::Chat(_) | Page::Creation(_) => None,
            };
            // A provider without an exact dispatch route can still have the
            // original session in its original, separate window.
            let links = if self.single_window || route.is_none() || expected != self.target {
                self.exact_links(page)
            } else {
                Vec::new()
            };
            if !self.single_window
                && expected == self.target
                && let Some(w) = self.window.as_ref()
                && self.current_window().as_ref() != Some(w)
                && selection(self.profile.provider(), w).as_ref() == Some(page)
                && self.app.contains(w)
            {
                guard(destination, expected)?;
                w.perform("AXRaise")
                    .map_err(|_| unavailable("original window could not be raised"))?;
            } else if self.single_window || route.is_none() || expected != self.target {
                let link = match links.as_slice() {
                    [link] => Some(link.clone()),
                    [] if self.profile.provider() == "claude"
                        && let Page::Code(id) = page
                        && let Some(w) = self.window.as_ref()
                        && self.current_window().as_ref() == Some(w) =>
                    {
                        native_app::claude_return_row(w, id)
                    }
                    _ => None,
                }
                .ok_or_else(|| {
                    unavailable("no unique in-app return path to the original session")
                })?;
                guard(destination, expected)?;
                link.press()
                    .map_err(|_| unavailable("original session link refused"))?;
            } else {
                let route =
                    route.ok_or_else(|| unavailable("no exact restore route for original page"))?;
                let bundle = procs::process_bundle(self.target.pid)
                    .ok_or_else(|| unavailable("provider bundle unavailable"))?;
                if !codesign::process_satisfies(self.target.pid, &self.profile.app.requirement()) {
                    return Err(unavailable("provider signature changed"));
                }
                guard(destination, expected)?;
                let status = tokio::time::timeout(
                    permit.remaining(),
                    tokio::process::Command::new("/usr/bin/open")
                        .args(["-g", "-a"])
                        .arg(bundle)
                        .arg(route)
                        .kill_on_drop(true)
                        .status(),
                )
                .await
                .map_err(|_| unavailable("restore dispatch timed out"))?
                .map_err(|_| unavailable("restore dispatch failed"))?;
                if !status.success() {
                    return Err(unavailable("provider refused original session"));
                }
            }
            loop {
                permit.check(input::user_input_generation())?;
                if front() != Some(expected) {
                    return Err(unavailable("foreground changed during restore"));
                }
                if self
                    .current_window()
                    .and_then(|w| selection(self.profile.provider(), &w))
                    .as_ref()
                    == Some(page)
                {
                    break;
                }
                tokio::time::sleep(Duration::from_millis(60)).await;
            }
            guard(page, owner)?;
        }
        let page = original.unwrap_or(destination);
        if original.is_some() {
            if let Some(w) = self.window.as_ref()
                && self.minimized == Some(true)
                && w.bool("AXMinimized") == Some(false)
                && (self.single_window || self.app.contains(w))
            {
                guard(page, owner)?;
                w.set_bool("AXMinimized", true)
                    .map_err(|_| unavailable("could not restore minimized window"))?;
            }
            if self.hidden == Some(true) && self.app.bool("AXHidden") == Some(false) {
                guard(page, owner)?;
                self.app
                    .set_bool("AXHidden", true)
                    .map_err(|_| unavailable("could not restore hidden app"))?;
            }
        }
        if owner != self.front && front() != Some(self.front) {
            guard(page, owner)?;
            if process(self.front.pid) != Some(self.front)
                || !events::activate(self.front.pid as i32)
            {
                return Err(unavailable(
                    "could not restore original foreground application",
                ));
            }
        }
        Ok(())
    }
}
