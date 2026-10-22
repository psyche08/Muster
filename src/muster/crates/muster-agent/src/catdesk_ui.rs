//! What CatDesk's window shows through Accessibility, decided here so the
//! platform adapter only reads and presses: its page routes and how a
//! sidebar row names a session. Its controls are `profiles/catdesk.toml`.

/// Which page of the app a web area shows.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Route {
    /// The main window: the session it shows is the one the app reports as
    /// current, never inferred from the page.
    Main,
    /// A session popped out into its own window (its id is in the URL).
    Popout(String),
}

/// The app shell is `file://…/app.asar/dist/index.html`; the main window
/// loads it without a hash route, a popout with `#/popout?sessionId=<id>`.
/// Every other route (quick chat, pet overlay, diagnostics…) shows no session.
pub fn route(url: &str) -> Option<Route> {
    let rest = url.strip_prefix("file://")?;
    let (path, fragment) = rest.split_once('#').unwrap_or((rest, ""));
    if !path.ends_with("/app.asar/dist/index.html") {
        return None;
    }
    if fragment.is_empty() || fragment == "/" {
        return Some(Route::Main);
    }
    let query = fragment.strip_prefix("/popout?")?;
    let mut ids = query
        .split('&')
        .filter_map(|pair| pair.strip_prefix("sessionId="));
    let id = ids.next()?;
    (ids.next().is_none() && crate::viewer::is_uuid(id))
        .then(|| Route::Popout(id.to_ascii_lowercase()))
}

/// Badges a sidebar row may show after its title.
const ROW_BADGES: [&str; 2] = ["等待批准", "Awaiting Approval"];

/// Whether a pressable sidebar row's own text labels (read without nested
/// controls) name exactly `title`: the title, then at most the approval
/// badge. Substrings, prefixes and other extra text never match.
pub fn row_matches(labels: &[String], title: &str) -> bool {
    match labels {
        [only] => !title.trim().is_empty() && only == title,
        [first, badge] => {
            !title.trim().is_empty() && first == title && ROW_BADGES.contains(&badge.as_str())
        }
        _ => false,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const APP: &str =
        "file:///Applications/CatDesk.app/Contents/Resources/app.asar/dist/index.html";

    #[test]
    fn routes_only_the_main_window_and_popouts() {
        assert_eq!(route(APP), Some(Route::Main));
        assert_eq!(route(&format!("{APP}#/")), Some(Route::Main));
        assert_eq!(
            route(&format!(
                "{APP}#/popout?sessionId=E6841C79-D515-4C39-AA56-3B70C404B093"
            )),
            Some(Route::Popout("e6841c79-d515-4c39-aa56-3b70c404b093".into()))
        );
        for other in [
            format!("{APP}#/quickchat"),
            format!("{APP}#/pet-overlay"),
            format!("{APP}#/popout?sessionId=x"),
            format!(
                "{APP}#/popout?sessionId=e6841c79-d515-4c39-aa56-3b70c404b093&sessionId=f7779b23-8b79-4b62-b2bf-429fad56db1b"
            ),
            "https://km.example.com/#/chat?pageId=1".into(),
            "file:///tmp/index.html".into(),
        ] {
            assert_eq!(route(&other), None, "{other}");
        }
    }

    #[test]
    fn rows_match_their_exact_title_and_known_badge_only() {
        let labels = |v: &[&str]| v.iter().map(|s| s.to_string()).collect::<Vec<_>>();
        let title = "Sandbox 查询接口接入指南";
        assert!(row_matches(&labels(&[title]), title));
        assert!(row_matches(&labels(&[title, "等待批准"]), title));
        assert!(!row_matches(&labels(&[title, "其他"]), title));
        assert!(!row_matches(&labels(&["Sandbox 查询接口"]), title));
        assert!(!row_matches(&labels(&[title, title, "等待批准"]), title));
        assert!(!row_matches(&labels(&[]), title));
        assert!(!row_matches(&labels(&[""]), ""));
    }
}
