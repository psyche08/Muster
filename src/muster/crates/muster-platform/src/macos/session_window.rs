//! Select from window metadata before reading any session content.
use super::{ax::Element, native_app::cli_session_from_url};
use muster_agent::{
    Error,
    native_navigation::restore::{Inspection, Navigation},
    native_scan::{inspection_window, preferred_window},
};

pub(super) fn scoped(provider: &str) -> bool {
    matches!(provider, "claude" | "codex")
}

pub(super) fn selected(app: &Element, inspection: Option<&Inspection>) -> Result<Element, Error> {
    let mut windows = app
        .attribute("AXWindows")
        .map(|v| Element::elements_of(&v))
        .unwrap_or_default();
    let focused = app.element("AXFocusedWindow");
    let main = app.element("AXMainWindow");
    let eligible = |w: &Element| {
        w.role() == "AXWindow"
            && !matches!(
                w.string("AXSubrole").as_deref(),
                Some("AXDialog" | "AXSystemDialog" | "AXFloatingWindow")
            )
    };
    let selected = if let Some(target) = inspection {
        let pid = app
            .pid()
            .ok_or_else(|| Error::Unavailable("Claude process unavailable".into()))?;
        let resolves = |url: &str| cli_session_from_url(url).as_deref() == Some(&*target.session);
        // A renderer handle previously attached to this exact session may
        // still identify its window while AXWindows omits it. Re-prove the
        // live URL and window relationship before using the handle.
        let retained = target
            .hint
            .as_ref()
            .and_then(|h| h.downcast_ref::<(Element, Element)>())
            .filter(|(w, area)| {
                w.pid() == Some(pid)
                    && area.string("AXURL").is_some_and(|url| resolves(&url))
                    && w.contains(area)
            })
            .map(|(w, _)| w);
        for w in focused
            .iter()
            .chain(main.iter())
            .chain(retained)
            .cloned()
            .chain(app.children())
        {
            if !windows.contains(&w) && w.role() == "AXWindow" {
                windows.push(w);
            }
        }
        let places: Vec<_> = windows
            .iter()
            .filter(|w| w.pid() == Some(pid) && eligible(w))
            .map(|w| {
                let exact = retained == Some(w)
                    || ["AXDocument", "AXURL"]
                        .iter()
                        .any(|a| w.string(a).is_some_and(|url| resolves(&url)));
                (w.clone(), exact, w.bool("AXMinimized") == Some(true))
            })
            .collect();
        inspection_window(&places, focused.as_ref(), main.as_ref()).cloned()
    } else {
        preferred_window(&windows, focused.as_ref(), main.as_ref(), |w| {
            eligible(w) && w.bool("AXMinimized") != Some(true)
        })
        .cloned()
    };
    selected.ok_or_else(|| Error::Unavailable("No usable desktop session window".into()))
}

pub(super) fn root(app: &Element) -> Result<Element, Error> {
    Navigation::bind_window(selected(app, Navigation::inspection().as_ref())?)
}

pub(super) fn for_provider(app: &Element, provider: &str) -> Result<Element, Error> {
    if scoped(provider) {
        root(app)
    } else {
        Ok(app.clone())
    }
}
