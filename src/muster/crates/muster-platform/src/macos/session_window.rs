//! Select from window metadata before reading any session content.
use super::ax::Element;
use muster_agent::{Error, native_navigation::restore::Navigation, native_scan::preferred_window};

pub(super) fn scoped(provider: &str) -> bool {
    matches!(provider, "claude" | "codex")
}

pub(super) fn selected(app: &Element) -> Result<Element, Error> {
    let windows = app
        .attribute("AXWindows")
        .map(|v| Element::elements_of(&v))
        .unwrap_or_default();
    let focused = app.element("AXFocusedWindow");
    let main = app.element("AXMainWindow");
    preferred_window(&windows, focused.as_ref(), main.as_ref(), |w| {
        w.role() == "AXWindow"
            && w.bool("AXMinimized") != Some(true)
            && !matches!(
                w.string("AXSubrole").as_deref(),
                Some("AXDialog" | "AXSystemDialog" | "AXFloatingWindow")
            )
    })
    .cloned()
    .ok_or_else(|| Error::Unavailable("No usable desktop session window".into()))
}

// The opaque handle is carried by the operation into its AX workers. Fresh
// metadata can invalidate that handle but can never retarget the operation.
pub(super) fn root(app: &Element) -> Result<Element, Error> {
    let current = selected(app)?;
    Navigation::bind_window(current)
}

pub(super) fn for_provider(app: &Element, provider: &str) -> Result<Element, Error> {
    if scoped(provider) {
        root(app)
    } else {
        Ok(app.clone())
    }
}
