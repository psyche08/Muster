//! Bounded readiness after a single draft write. This never authorizes a
//! submission; the adapter must still check its live guard and exact surface.
use std::time::Duration;

/// Tab extraction must never replace an existing draft. A missing AX value is
/// unknown, not empty; Claude may expose a newline and Codex its placeholder.
/// A usable (or unreadable) submit control prevents treating literal placeholder
/// text that the user typed as an empty composer.
pub fn completion_baseline<'a>(
    value: Option<&'a str>,
    placeholders: &[&str],
    may_submit: bool,
) -> Result<&'a str, crate::Error> {
    match value {
        Some(value)
            if !may_submit && (value.trim().is_empty() || placeholders.contains(&value.trim())) =>
        {
            Ok(value)
        }
        Some(_) => Err(crate::Error::Conflict(
            "The native composer contains a draft; it was preserved".into(),
        )),
        None => Err(crate::Error::Unavailable(
            "Cannot read the native draft".into(),
        )),
    }
}

#[cfg(test)]
mod completion_tests {
    use super::*;
    #[test]
    fn completion_preserves_existing_and_unreadable_drafts() {
        assert_eq!(completion_baseline(Some("\n"), &[], false).unwrap(), "\n");
        assert_eq!(completion_baseline(Some(""), &[], false).unwrap(), "");
        assert!(completion_baseline(Some("existing draft"), &[], false).is_err());
        assert!(completion_baseline(Some(" \n existing draft \n"), &[], false).is_err());
        assert!(completion_baseline(None, &[], false).is_err());
        let placeholders = ["Work with Codex"];
        assert_eq!(
            completion_baseline(Some("Work with Codex\n"), &placeholders, false).unwrap(),
            "Work with Codex\n"
        );
        assert!(completion_baseline(Some("Work with Codex"), &placeholders, true).is_err());
        assert!(completion_baseline(Some("another suggestion"), &placeholders, false).is_err());
    }
}

/// What a conversation's visible controls say about its turn, when the
/// app's own runtime status is not available. A draft in the composer hides
/// Stop in both Claude and Codex (Claude shows Send, Codex Queue), and
/// Claude's Send looks the same idle or busy: a draft without Stop says
/// nothing, rather than idle.
pub fn observed_state(
    waiting_approval: bool,
    stop: bool,
    draft: bool,
) -> crate::model::SessionState {
    use crate::model::SessionState;
    if waiting_approval {
        SessionState::WaitingApproval
    } else if stop {
        SessionState::Running
    } else if draft {
        SessionState::Unknown
    } else {
        SessionState::Idle
    }
}

/// A bounded AX tree projection; parents must precede their children.
pub struct ControlNode {
    pub parent: Option<usize>,
    pub composer: bool,
    pub submit: bool,
    pub queued_delete: bool,
}

/// Keep submit controls inside the unique composer's branch. A queued-message
/// row has its own delete action and must not be mistaken for the composer.
pub fn composer_submits(nodes: &[ControlNode]) -> Vec<usize> {
    if nodes
        .iter()
        .enumerate()
        .any(|(i, n)| n.parent.is_some_and(|p| p >= i))
    {
        return Vec::new();
    }
    let composers: Vec<_> = nodes
        .iter()
        .enumerate()
        .filter(|(_, n)| n.composer)
        .map(|(i, _)| i)
        .collect();
    if composers.len() != 1 {
        return Vec::new();
    }
    let mut has_composer: Vec<_> = nodes.iter().map(|n| n.composer).collect();
    let mut has_delete: Vec<_> = nodes.iter().map(|n| n.queued_delete).collect();
    for i in (0..nodes.len()).rev() {
        if let Some(p) = nodes[i].parent {
            has_composer[p] |= has_composer[i];
            has_delete[p] |= has_delete[i];
        }
    }
    nodes
        .iter()
        .enumerate()
        .filter_map(|(i, n)| {
            if !n.submit {
                return None;
            }
            let mut parent = n.parent;
            while let Some(p) = parent {
                // The web area is not an acceptable composer scope.
                nodes[p].parent?;
                if has_composer[p] {
                    return Some(i);
                }
                if has_delete[p] {
                    return None;
                }
                parent = nodes[p].parent;
            }
            None
        })
        .collect()
}

/// Resolve stale disabled duplicates only after semantic composer scoping.
/// Unknown enabled state remains a candidate; two actionable buttons still
/// refuse submission. Keep disabled-only candidates for the readiness wait.
pub fn actionable_submits(enabled: &[Option<bool>]) -> Vec<usize> {
    let has_enabled = enabled.contains(&Some(true));
    enabled
        .iter()
        .enumerate()
        .filter_map(|(index, state)| (!has_enabled || *state != Some(false)).then_some(index))
        .collect()
}

#[derive(Debug, PartialEq, Eq)]
pub enum Readiness {
    Wait,
    Focus,
    Ready,
    EchoTimeout,
    SubmitTimeout,
    MissingButton,
    DisabledButton,
    AmbiguousButtons,
}

/// Every echo observed on the diagnostic builds arrived within 472 ms; the
/// 2 s timeouts never recovered inside that window, yet a retry found the
/// exact draft in place. Wait longer, still well inside the 15 s Locked Use
/// lease. An unconfirmed draft is never submitted.
pub const ECHO_DEADLINE: Duration = Duration::from_secs(4);
/// After the echo, the composer's submit control must settle within this.
pub const SUBMIT_DEADLINE: Duration = Duration::from_secs(2);

/// Content-free description of a composer value that does not echo the
/// draft, for diagnostics only. It never relaxes the exact echo check.
pub fn echo_mismatch(value: Option<&str>, text: &str) -> &'static str {
    let Some(value) = value else {
        return "unreadable";
    };
    let (value, text) = (value.trim_end(), text.trim_end());
    if value == text {
        "exact"
    } else if value.trim().is_empty() {
        "empty"
    } else if text.starts_with(value) {
        "partial"
    } else if value.split_whitespace().eq(text.split_whitespace()) {
        // Unicode whitespace includes no-break spaces that editors substitute.
        "whitespace"
    } else {
        "different"
    }
}

#[derive(Default)]
pub struct DraftWait {
    ready_deadline: Option<Duration>,
    focused: bool,
    focus_completed: bool,
}

impl DraftWait {
    /// Focus is a separate OS action. Start send readiness after it returns,
    /// once only; repeated observations cannot prolong this step.
    pub fn focus_completed(&mut self, elapsed: Duration) {
        if self.focused && !self.focus_completed {
            self.focus_completed = true;
            self.ready_deadline = Some(elapsed + SUBMIT_DEADLINE);
        }
    }

    pub fn observe(
        &mut self,
        elapsed: Duration,
        echoed: bool,
        buttons: usize,
        enabled: bool,
    ) -> Readiness {
        let deadline = self.ready_deadline.unwrap_or(ECHO_DEADLINE);
        if !echoed {
            return if elapsed >= deadline {
                Readiness::EchoTimeout
            } else {
                Readiness::Wait
            };
        }
        if buttons > 1 {
            return Readiness::AmbiguousButtons;
        }
        if elapsed >= deadline {
            return if self.ready_deadline.is_none() {
                Readiness::EchoTimeout
            } else if buttons == 0 {
                Readiness::MissingButton
            } else if enabled {
                Readiness::SubmitTimeout
            } else {
                Readiness::DisabledButton
            };
        }
        if buttons == 1 && enabled {
            return Readiness::Ready;
        }
        self.ready_deadline.get_or_insert(elapsed + SUBMIT_DEADLINE);
        if !self.focused {
            self.focused = true;
            Readiness::Focus
        } else {
            Readiness::Wait
        }
    }
}

/// How to empty an editor before writing into it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ClearWith {
    /// Select the whole value and replace the selection with nothing: an
    /// editing command the editor applies to its own document.
    Selection,
    /// Write an empty value: a DOM write a rich editor may re-read.
    Value,
}

/// What an editor's value says about emptying it.
#[derive(Debug, PartialEq, Eq)]
pub enum Clearing {
    /// The editor is empty: write the text.
    Done,
    /// Perform this attempt now.
    Clear(ClearWith),
    /// An attempt is settling; read the value again.
    Wait,
    /// The editor still holds a draft. Nothing was written or submitted.
    Timeout,
    /// The value cannot be read. An unreadable draft is not an empty one.
    Unreadable,
}

/// A draft left in an editor is not replaced by writing a value over it:
/// Electron's rich editors re-read their own document and merge, so the field
/// ends up holding the old draft *and* Muster's text. The exact echo can then
/// never match, the request is reported as not sent, and the mixed draft stays
/// behind for the next attempt to grow. Every submission therefore empties
/// each editor it will use, confirms it empty, and only then writes. Both
/// mechanisms are attempted in turn because no profile can state which one a
/// given app build honors, and the confirmation decides.
pub const CLEAR_DEADLINE: Duration = Duration::from_secs(2);
/// How long one attempt may settle before the next mechanism is tried.
const CLEAR_SETTLE: Duration = Duration::from_millis(400);

#[derive(Default)]
pub struct ClearWait {
    attempts: usize,
    last: Option<Duration>,
}

impl ClearWait {
    /// `draft` is whether the editor's value is a draft; `None` is unreadable.
    pub fn observe(&mut self, elapsed: Duration, draft: Option<bool>) -> Clearing {
        match draft {
            None => Clearing::Unreadable,
            Some(false) => Clearing::Done,
            Some(true) if elapsed >= CLEAR_DEADLINE => Clearing::Timeout,
            Some(true) if self.last.is_some_and(|at| elapsed < at + CLEAR_SETTLE) => Clearing::Wait,
            Some(true) => {
                self.last = Some(elapsed);
                self.attempts += 1;
                Clearing::Clear(if self.attempts % 2 == 1 {
                    ClearWith::Selection
                } else {
                    ClearWith::Value
                })
            }
        }
    }

    /// Whether this editor held a draft that had to be emptied. Diagnostics
    /// only; it never relaxes the confirmation.
    pub fn attempts(&self) -> usize {
        self.attempts
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn ms(n: u64) -> Duration {
        Duration::from_millis(n)
    }

    #[test]
    fn an_editor_is_emptied_through_both_mechanisms_before_any_write() {
        let mut wait = ClearWait::default();
        // An empty editor is written into directly.
        assert_eq!(wait.observe(ms(0), Some(false)), Clearing::Done);
        assert_eq!(wait.attempts(), 0);

        // A draft is emptied through the editor's own editing command first,
        // then by a value write, alternating while the value is re-read.
        let mut wait = ClearWait::default();
        assert_eq!(
            wait.observe(ms(0), Some(true)),
            Clearing::Clear(ClearWith::Selection)
        );
        assert_eq!(wait.observe(ms(100), Some(true)), Clearing::Wait);
        assert_eq!(wait.observe(ms(399), Some(true)), Clearing::Wait);
        assert_eq!(
            wait.observe(ms(400), Some(true)),
            Clearing::Clear(ClearWith::Value)
        );
        assert_eq!(
            wait.observe(ms(800), Some(true)),
            Clearing::Clear(ClearWith::Selection)
        );
        // The editor accepted it: the write may proceed.
        assert_eq!(wait.observe(ms(850), Some(false)), Clearing::Done);
        assert_eq!(wait.attempts(), 3);
    }

    #[test]
    fn an_editor_that_keeps_its_draft_or_cannot_be_read_blocks_the_write() {
        let mut wait = ClearWait::default();
        assert_eq!(
            wait.observe(ms(0), Some(true)),
            Clearing::Clear(ClearWith::Selection)
        );
        for at in [400, 800, 1_200, 1_600] {
            assert!(matches!(
                wait.observe(ms(at), Some(true)),
                Clearing::Clear(_)
            ));
            assert_eq!(wait.observe(ms(at + 100), Some(true)), Clearing::Wait);
        }
        assert_eq!(wait.observe(ms(2_000), Some(true)), Clearing::Timeout);
        // The deadline is final: a later read does not reopen the attempts.
        assert_eq!(wait.observe(ms(2_400), Some(true)), Clearing::Timeout);
        // An unreadable value is never treated as an emptied editor.
        assert_eq!(wait.observe(ms(10), None), Clearing::Unreadable);
    }

    #[test]
    fn observed_state_puts_approval_first_and_never_guesses_idle_over_a_draft() {
        use crate::model::SessionState::*;
        for stop in [false, true] {
            for draft in [false, true] {
                assert_eq!(observed_state(true, stop, draft), WaitingApproval);
            }
        }
        assert_eq!(observed_state(false, true, false), Running);
        assert_eq!(observed_state(false, true, true), Running);
        assert_eq!(observed_state(false, false, true), Unknown);
        assert_eq!(observed_state(false, false, false), Idle);
    }

    #[test]
    fn working_submit_excludes_stop_and_queued_row_steer() {
        use crate::profile::CODEX;
        let button = |label: &str| crate::desktop_chat::AxNode {
            role: "AXButton".into(),
            title: label.into(),
            ..Default::default()
        };
        let submit = |label: &str| CODEX.send.matches(&button(label));
        let node = |parent: Option<usize>, composer: bool, label: &str| ControlNode {
            parent,
            composer,
            submit: submit(label),
            queued_delete: CODEX.queue.as_ref().unwrap().row.matches(&button(label)),
        };
        let mut nodes = vec![
            node(None, false, ""),
            node(Some(0), false, ""),
            node(Some(1), false, ""),
            node(Some(2), false, "Steer"),
            node(Some(2), false, "Delete queued message"),
            node(Some(1), true, ""),
            node(Some(1), false, "Stop"),
            node(Some(0), false, "Send"), // unrelated action in another panel
        ];
        assert!(composer_submits(&nodes).is_empty());
        for label in ["Queue", "Steer", "Send"] {
            nodes[6].submit = submit(label);
            assert_eq!(composer_submits(&nodes), vec![6]);
        }
        // A queued row identified only by its actions menu is still not the composer.
        nodes[4] = node(Some(2), false, "Queued message actions");
        assert_eq!(composer_submits(&nodes), vec![6]);
        nodes.push(node(Some(1), true, ""));
        assert!(composer_submits(&nodes).is_empty());
        for label in [
            "Stop",
            "Stop generating",
            "Stop streaming",
            "Interrupt",
            "Resume",
        ] {
            assert!(!submit(label));
        }
    }

    #[test]
    fn disabled_duplicate_is_excluded_but_unknown_or_live_duplicates_are_ambiguous() {
        // Mac-C: Send/disabled plus Send/Send/enabled after exact draft echo.
        assert_eq!(actionable_submits(&[Some(false), Some(true)]), vec![1]);
        assert_eq!(actionable_submits(&[Some(true), Some(false)]), vec![0]);
        assert_eq!(actionable_submits(&[None, Some(true)]), vec![0, 1]);
        assert_eq!(actionable_submits(&[Some(true), Some(true)]), vec![0, 1]);
        assert_eq!(actionable_submits(&[Some(false)]), vec![0]);
        assert!(actionable_submits(&[]).is_empty());
        for candidates in [&[Some(true), Some(true)][..], &[None, Some(true)][..]] {
            assert_eq!(
                DraftWait::default().observe(
                    ms(100),
                    true,
                    actionable_submits(candidates).len(),
                    true
                ),
                Readiness::AmbiguousButtons
            );
        }
    }

    #[test]
    fn delayed_echo_then_delayed_button_can_become_ready_without_another_write() {
        let mut wait = DraftWait::default();
        assert_eq!(wait.observe(ms(600), false, 0, false), Readiness::Wait);
        assert_eq!(wait.observe(ms(750), true, 0, false), Readiness::Focus);
        assert_eq!(wait.observe(ms(1200), true, 1, false), Readiness::Wait);
        assert_eq!(wait.observe(ms(1400), true, 1, true), Readiness::Ready);
    }

    #[test]
    fn missing_or_disabled_buttons_get_one_focus_and_a_fixed_deadline() {
        for (buttons, expected) in [
            (0, Readiness::MissingButton),
            (1, Readiness::DisabledButton),
        ] {
            let mut wait = DraftWait::default();
            assert_eq!(
                wait.observe(ms(100), true, buttons, false),
                Readiness::Focus
            );
            assert_eq!(
                wait.observe(ms(1700), true, buttons, false),
                Readiness::Wait
            );
            assert_eq!(wait.observe(ms(2100), true, buttons, false), expected);
        }
    }

    #[test]
    fn a_late_echo_inside_the_deadline_still_becomes_ready() {
        let mut wait = DraftWait::default();
        assert_eq!(wait.observe(ms(2100), false, 1, true), Readiness::Wait);
        assert_eq!(wait.observe(ms(3900), false, 1, true), Readiness::Wait);
        assert_eq!(wait.observe(ms(3950), true, 1, true), Readiness::Ready);
    }

    #[test]
    fn slow_focus_does_not_consume_send_readiness_or_allow_repeated_extension() {
        let mut wait = DraftWait::default();
        assert_eq!(wait.observe(ms(3900), true, 0, false), Readiness::Focus);
        wait.focus_completed(ms(4150));
        assert_eq!(wait.observe(ms(6100), true, 1, false), Readiness::Wait);
        wait.focus_completed(ms(6100)); // cannot restart the same step
        assert_eq!(
            wait.observe(ms(6150), true, 1, true),
            Readiness::SubmitTimeout
        );
    }

    #[test]
    fn expired_echo_cannot_be_revived_by_late_controls() {
        let mut wait = DraftWait::default();
        assert_eq!(
            wait.observe(ms(4000), true, 1, true),
            Readiness::EchoTimeout
        );
    }

    #[test]
    fn echo_mismatch_describes_without_relaxing_the_check() {
        assert_eq!(echo_mismatch(None, "hello"), "unreadable");
        assert_eq!(echo_mismatch(Some("hello\n"), "hello"), "exact");
        assert_eq!(echo_mismatch(Some(""), "hello"), "empty");
        assert_eq!(echo_mismatch(Some("hel"), "hello"), "partial");
        assert_eq!(
            echo_mismatch(Some("a\u{a0} b\r\nc"), "a b\nc"),
            "whitespace"
        );
        assert_eq!(echo_mismatch(Some("older draft"), "hello"), "different");
    }

    #[test]
    fn never_accepts_a_missing_echo_or_ambiguous_submit() {
        let mut wait = DraftWait::default();
        assert_eq!(
            wait.observe(ECHO_DEADLINE, false, 1, true),
            Readiness::EchoTimeout
        );
        assert_eq!(
            DraftWait::default().observe(ms(10), true, 2, true),
            Readiness::AmbiguousButtons
        );
        let mut changed = DraftWait::default();
        assert_eq!(changed.observe(ms(100), true, 0, false), Readiness::Focus);
        assert_eq!(
            changed.observe(ms(2100), false, 1, true),
            Readiness::EchoTimeout
        );
    }
}

/// A positive ancestry hint requires every parent to still list its child.
/// Unknown, cyclic, replaced and over-budget paths are never positive evidence;
/// the OS adapter may fall back to a fresh bounded descendant scan.
pub fn reciprocal_ancestor<N: Clone + PartialEq>(
    ancestor: &N,
    descendant: &N,
    mut parent: impl FnMut(&N) -> Option<N>,
    mut lists_child: impl FnMut(&N, &N) -> bool,
    mut within_budget: impl FnMut() -> bool,
) -> bool {
    let mut node = descendant.clone();
    let mut seen = Vec::new();
    for _ in 0..64 {
        if !within_budget() || seen.contains(&node) {
            return false;
        }
        if &node == ancestor {
            return true;
        }
        seen.push(node.clone());
        let Some(next) = parent(&node) else {
            return false;
        };
        if !within_budget() || !lists_child(&next, &node) {
            return false;
        }
        node = next;
    }
    false
}

#[cfg(test)]
mod ancestry_tests {
    use super::reciprocal_ancestor;

    #[test]
    fn ancestry_checks_edges_without_visiting_unrelated_descendants() {
        let mut reads = 0;
        assert!(reciprocal_ancestor(
            &0u32,
            &3u32,
            |n| {
                reads += 1;
                n.checked_sub(1)
            },
            |parent, child| parent + 1 == *child,
            || true
        ));
        assert_eq!(reads, 3);
    }

    #[test]
    fn stale_edges_cycles_unknown_parents_and_budgets_are_not_proof() {
        assert!(!reciprocal_ancestor(
            &0,
            &3,
            |n| Some(n - 1),
            |_, _| false,
            || true
        ));
        assert!(!reciprocal_ancestor(
            &0,
            &3,
            |_| Some(3),
            |_, _| true,
            || true
        ));
        assert!(!reciprocal_ancestor(&0, &3, |_| None, |_, _| true, || true));
        assert!(!reciprocal_ancestor(
            &0,
            &3,
            |n| Some(n - 1),
            |_, _| true,
            || false
        ));
        assert!(!reciprocal_ancestor(
            &0,
            &100,
            |n| Some(n - 1),
            |_, _| true,
            || true
        ));
    }
}
