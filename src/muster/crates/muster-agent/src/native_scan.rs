//! Bounded Accessibility scans. A walk that stops early cannot prove what it
//! did not see, so the adapter refuses on it; this decides what the early stop
//! means, whether walking again can help, and which subtrees a session scan
//! never needs to enter. Nothing here reads or drives the OS.
use std::time::Duration;

/// Why a bounded walk stopped before covering its whole subtree.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ScanStop {
    /// The node ceiling. The same tree reaches it again.
    NodeLimit,
    /// The time budget, which is already spent.
    TimeLimit,
    /// A node failed its read, with the AX status when there was one:
    /// destroyed while the walk ran, or the app did not answer in time.
    NodeRead(Option<i32>),
}

impl ScanStop {
    /// Content-free wording for logs and refusals.
    pub fn describe(self) -> String {
        match self {
            Self::NodeLimit => "node limit".into(),
            Self::TimeLimit => "time limit".into(),
            Self::NodeRead(Some(code)) => format!("node read failed ({code})"),
            Self::NodeRead(None) => "node read failed".into(),
        }
    }
}

/// Walks for one scan, the first included.
pub const SCAN_ATTEMPTS: usize = 3;
/// The pause before walking again, for the app to finish its rebuild.
pub const SCAN_RETRY_PAUSE: Duration = Duration::from_millis(100);

/// Whether to walk again after `attempts` walks ended with `stop`.
///
/// Electron rebuilds the composer and its controls around a write and
/// re-renders a streaming transcript. A node it destroys mid-walk fails its
/// read, and the scan was refused as incomplete within half a second of a
/// successful write; a fresh walk a moment later sees a consistent tree. A
/// partial walk is never patched by skipping the failed node: its subtree may
/// have held the composer or a second submit control. The node ceiling and the
/// time budget are not transient, and repeating them only spends the
/// operation's time. Walking is read-only, so a repeat costs time, never
/// safety.
pub fn repeat_scan(stop: ScanStop, attempts: usize) -> bool {
    attempts < SCAN_ATTEMPTS && matches!(stop, ScanStop::NodeRead(_))
}

/// Whether a session scan descends into a web area whose own key `resolves`
/// to a conversation.
///
/// A web area is resolved from its own URL and title, never from what it
/// contains, and a resolved one is that conversation's view: its subtree is
/// the transcript and composer, which the control scan walks separately.
/// Descending into every transcript made the app-wide scan grow with each
/// open conversation until it hit the node ceiling — after it had already
/// found the target — and refused the send as incomplete. A web area that
/// resolves to nothing (the app's shell, a home page) may hold conversation
/// views and is entered.
///
/// This never lets a send reach another conversation: the target is still
/// matched only by its own key, and the control scan still requires exactly
/// one composer inside it. A view nested inside another conversation's view,
/// which no supported app has, would go unseen and refuse as not found.
pub fn descend_web_area(resolves: bool) -> bool {
    !resolves
}

/// Where one view of the target conversation sits among the app's windows.
/// Facts only; `choose_view` decides.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ViewPlace {
    /// The view's window is the app's main window.
    pub main_window: bool,
    /// The view's window in the app's front-to-back window order; `None` when
    /// its window could not be read or placed.
    pub window_rank: Option<usize>,
    /// The view's window is minimized.
    pub minimized: bool,
}

/// Which of several views of one conversation an operation runs in, or
/// `None` to refuse as ambiguous.
///
/// The Codex app opens a window per thread activation, so one conversation can
/// be shown in several windows at once: on M4Pro three of nine windows showed
/// the same thread, and every send to it was refused as "more than one view
/// shows this session". They are one conversation, so any of them reaches it;
/// what must hold is that the whole operation runs in a single view the owner
/// can see. That is the view in the app's main window. An app that is not
/// active may report no main window, as under Locked Use; then the frontmost of
/// the views in the app's front-to-back window order is used. A minimized
/// window is never chosen: its renderer need not update what the echo check
/// reads. Two views in the main window, two frontmost views sharing a window,
/// or a view whose window cannot be placed still refuse.
pub fn choose_view(views: &[ViewPlace]) -> Option<usize> {
    if views.len() == 1 {
        return Some(0);
    }
    let usable: Vec<usize> = (0..views.len()).filter(|&i| !views[i].minimized).collect();
    let main: Vec<usize> = usable
        .iter()
        .copied()
        .filter(|&i| views[i].main_window)
        .collect();
    match main.as_slice() {
        [only] => return Some(*only),
        [] => {}
        _ => return None,
    }
    let mut ranked = Vec::with_capacity(usable.len());
    for &i in &usable {
        ranked.push((views[i].window_rank?, i));
    }
    ranked.sort_unstable();
    match ranked.as_slice() {
        [(_, only)] => Some(*only),
        [(front, chosen), (next, _), ..] if front < next => Some(*chosen),
        _ => None,
    }
}

/// Where a provider's prompt editors may live inside its resolved web view.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ComposerScope {
    WebArea,
    MainConversation,
}

/// Return a global node index, never a guessed first/focused editor. Missing
/// ancestry cannot disambiguate two live editors. Parents precede children.
pub fn choose_composer(
    scope: ComposerScope,
    nodes: &[crate::native_send::ControlNode],
    subroles: &[String],
) -> Option<usize> {
    if nodes.len() != subroles.len()
        || nodes.iter().enumerate().any(|(i, n)| {
            if i == 0 {
                n.parent.is_some()
            } else {
                n.parent.is_none_or(|p| p >= i)
            }
        })
    {
        return None;
    }
    let candidates: Vec<_> = nodes
        .iter()
        .enumerate()
        .filter(|(_, n)| n.composer)
        .map(|(i, _)| i)
        .collect();
    if scope == ComposerScope::WebArea {
        return (candidates.len() == 1).then(|| candidates[0]);
    }
    // Parents precede children, so classify all ancestry in one bounded pass.
    let mut ancestry: Vec<(bool, bool)> = Vec::with_capacity(nodes.len());
    let mut main = Vec::new();
    let mut unknown = Vec::new();
    for (index, node) in nodes.iter().enumerate() {
        let (mut in_main, mut auxiliary) = node.parent.map(|p| ancestry[p]).unwrap_or_default();
        in_main |= subroles[index] == "AXLandmarkMain";
        auxiliary |= matches!(
            subroles[index].as_str(),
            "AXLandmarkComplementary" | "AXLandmarkNavigation" | "AXTabPanel"
        );
        ancestry.push((in_main, auxiliary));
        if node.composer && !auxiliary {
            if in_main {
                main.push(index);
            } else {
                unknown.push(index);
            }
        }
    }
    match (main.as_slice(), unknown.as_slice()) {
        ([only], []) => Some(*only),
        ([], [only]) if candidates.len() == 1 => Some(*only),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn control(
        parent: Option<usize>,
        composer: bool,
        submit: bool,
    ) -> crate::native_send::ControlNode {
        crate::native_send::ControlNode {
            parent,
            composer,
            submit,
            queued_delete: false,
        }
    }

    #[test]
    fn duplicate_composers_need_positive_main_and_auxiliary_evidence() {
        // The resolved web view contains a main composer and an auxiliary pane.
        let nodes = vec![
            control(None, false, false),
            control(Some(0), false, false),
            control(Some(1), false, false),
            control(Some(2), true, false),
            control(Some(2), false, true),
            control(Some(1), false, false),
            control(Some(5), true, false),
            control(Some(5), false, true),
        ];
        let mut roles = vec![String::new(); nodes.len()];
        roles[1] = "AXLandmarkMain".into();
        for role in [
            "AXLandmarkComplementary",
            "AXLandmarkNavigation",
            "AXTabPanel",
        ] {
            roles[5] = role.into();
            assert_eq!(
                choose_composer(crate::profile::CODEX.composer_scope, &nodes, &roles),
                Some(3)
            );
            assert_eq!(crate::native_send::composer_submits_for(&nodes, 3), vec![4]);
        }
        assert_eq!(
            choose_composer(ComposerScope::WebArea, &nodes, &roles),
            None
        );
        roles[5].clear(); // Two editors in the main conversation remain ambiguous.
        assert_eq!(
            choose_composer(crate::profile::CODEX.composer_scope, &nodes, &roles),
            None
        );
        roles[1].clear(); // A main editor plus another with unknown ancestry is insufficient.
        roles[2] = "AXLandmarkMain".into();
        assert_eq!(
            choose_composer(crate::profile::CODEX.composer_scope, &nodes, &roles),
            None
        );
        roles[5] = "AXLandmarkComplementary".into();
        assert_eq!(
            choose_composer(crate::profile::CODEX.composer_scope, &nodes, &roles),
            Some(3)
        );
        assert_eq!(
            choose_composer(crate::profile::CODEX.composer_scope, &nodes, &roles[..2]),
            None
        );
        let mut malformed = nodes;
        malformed[1].parent = Some(6);
        assert_eq!(
            choose_composer(crate::profile::CODEX.composer_scope, &malformed, &roles),
            None
        );
    }

    #[test]
    fn a_single_unlabelled_editor_remains_compatible_but_not_a_known_auxiliary() {
        let nodes = vec![control(None, false, false), control(Some(0), true, false)];
        let mut roles = vec![String::new(); nodes.len()];
        assert_eq!(
            choose_composer(ComposerScope::MainConversation, &nodes, &roles),
            Some(1)
        );
        roles[1] = "AXLandmarkComplementary".into();
        assert_eq!(
            choose_composer(ComposerScope::MainConversation, &nodes, &roles),
            None
        );
        assert_eq!(
            choose_composer(ComposerScope::WebArea, &nodes, &roles),
            Some(1)
        );
    }

    #[test]
    fn common_or_foreign_branches_cannot_supply_the_selected_editors_send() {
        let nodes = vec![
            control(None, false, false),
            control(Some(0), false, false),
            control(Some(1), false, false),
            control(Some(2), true, false),
            control(Some(1), false, false),
            control(Some(4), true, false),
            control(Some(4), false, true),
            control(Some(1), false, true),
        ];
        assert!(crate::native_send::composer_submits_for(&nodes, 3).is_empty());
        assert!(crate::native_send::composer_submits(&nodes).is_empty());
        assert!(crate::native_send::composer_submits_for(&nodes, 100).is_empty());
    }

    #[test]
    fn only_a_failed_node_read_is_walked_again_and_only_twice() {
        for code in [Some(-25202), Some(-25204), None] {
            let stop = ScanStop::NodeRead(code);
            assert!(repeat_scan(stop, 1));
            assert!(repeat_scan(stop, 2));
            assert!(!repeat_scan(stop, 3));
        }
        // The same tree reaches the ceiling again; the budget is spent.
        for stop in [ScanStop::NodeLimit, ScanStop::TimeLimit] {
            assert!(!repeat_scan(stop, 1));
        }
    }

    #[test]
    fn a_session_scan_enters_only_web_areas_that_resolve_to_nothing() {
        assert!(!descend_web_area(true));
        assert!(descend_web_area(false));
    }

    fn place(main_window: bool, window_rank: Option<usize>, minimized: bool) -> ViewPlace {
        ViewPlace {
            main_window,
            window_rank,
            minimized,
        }
    }

    #[test]
    fn the_view_in_the_main_window_is_chosen_among_duplicates() {
        // M4Pro: three windows showed the same thread.
        let views = [
            place(false, Some(2), false),
            place(true, Some(0), false),
            place(false, Some(7), false),
        ];
        assert_eq!(choose_view(&views), Some(1));
        // A single view is used as before, wherever it is.
        assert_eq!(choose_view(&[place(false, None, true)]), Some(0));
    }

    #[test]
    fn without_a_main_window_the_frontmost_visible_view_is_chosen() {
        // An inactive app (Locked Use) may report no main window.
        let views = [
            place(false, Some(5), false),
            place(false, Some(1), true),
            place(false, Some(3), false),
        ];
        assert_eq!(
            choose_view(&views),
            Some(2),
            "minimized windows are skipped"
        );
        // A minimized main window is not a usable main window either.
        let views = [place(true, Some(0), true), place(false, Some(4), false)];
        assert_eq!(choose_view(&views), Some(1));
    }

    #[test]
    fn undecidable_duplicates_still_refuse() {
        // Two views in the main window.
        assert_eq!(
            choose_view(&[place(true, Some(0), false), place(true, Some(0), false)]),
            None
        );
        // The two frontmost views share one window.
        assert_eq!(
            choose_view(&[
                place(false, Some(1), false),
                place(false, Some(1), false),
                place(false, Some(4), false)
            ]),
            None
        );
        // A window that cannot be placed.
        assert_eq!(
            choose_view(&[place(false, Some(0), false), place(false, None, false)]),
            None
        );
        // Only minimized windows.
        assert_eq!(
            choose_view(&[place(false, Some(0), true), place(false, Some(1), true)]),
            None
        );
    }

    #[test]
    fn stops_are_described_without_content() {
        assert_eq!(ScanStop::NodeLimit.describe(), "node limit");
        assert_eq!(ScanStop::TimeLimit.describe(), "time limit");
        assert_eq!(
            ScanStop::NodeRead(Some(-25202)).describe(),
            "node read failed (-25202)"
        );
        assert_eq!(ScanStop::NodeRead(None).describe(), "node read failed");
    }
}
