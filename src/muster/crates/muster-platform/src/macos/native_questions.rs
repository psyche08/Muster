//! Native question form adapter. The transcript supplies the exact questions;
//! only controls in the verified session's visible AX form can receive answers.
use super::*;
use muster_agent::questions::{AnswerPlan, remaining_matches, validate};
use muster_proto::v1::{InteractionQuestion, QuestionAnswer};
use std::time::Instant;

pub(super) fn pending_question(
    snapshot: Option<&muster_agent::questions::Snapshot>,
    previous: Option<&PendingInteraction>,
) -> Option<PendingInteraction> {
    snapshot.and_then(|s| s.pending.clone()).or_else(|| {
        previous
            .filter(|q| {
                q.kind == "question"
                    && !snapshot.is_some_and(|s| {
                        s.resolved.contains(&q.id)
                            || s.retired.contains(&q.id)
                            || s.complete && s.find(&q.id).is_none()
                    })
            })
            .cloned()
    })
}

pub(super) fn update_question(
    previous: &mut Option<PendingInteraction>,
    current: Option<PendingInteraction>,
    events: &mut Vec<ControllerEvent>,
) {
    if *previous == current {
        return;
    }
    if let Some(old) = previous.take()
        && current.as_ref().is_none_or(|q| q.id != old.id)
    {
        events.push(ControllerEvent::Resolved {
            interaction: old.id,
        });
    }
    *previous = current.clone();
    if let Some(question) = current {
        events.push(ControllerEvent::Interaction(question));
    }
}

pub(super) fn update_snapshot(
    previous: &mut Option<PendingInteraction>,
    current: Option<PendingInteraction>,
    snapshot: Option<&muster_agent::questions::Snapshot>,
    events: &mut Vec<ControllerEvent>,
) {
    let mut changes = Vec::new();
    update_question(previous, current, &mut changes);
    events.extend(changes.into_iter().filter(|e| !matches!(e, ControllerEvent::Resolved { interaction } if snapshot.is_some_and(|s| s.find(interaction).is_some()))));
}

pub(super) fn update_additional(
    previous: &mut Vec<PendingInteraction>,
    snapshot: Option<&muster_agent::questions::Snapshot>,
    events: &mut Vec<ControllerEvent>,
) {
    let Some(snapshot) = snapshot else {
        return;
    };
    previous.retain(|q| {
        if snapshot.retired.contains(&q.id)
            || snapshot.resolved.contains(&q.id)
            || snapshot.complete && snapshot.find(&q.id).is_none()
        {
            events.push(ControllerEvent::Resolved {
                interaction: q.id.clone(),
            });
            false
        } else {
            true
        }
    });
    for q in &snapshot.additional {
        if let Some(old) = previous.iter_mut().find(|old| old.id == q.id) {
            if old != q || events.iter().any(|e| matches!(e, ControllerEvent::Resolved { interaction } if interaction == &q.id)) { *old = q.clone(); events.push(ControllerEvent::Interaction(q.clone())); }
        } else {
            previous.push(q.clone());
            events.push(ControllerEvent::Interaction(q.clone()));
        }
    }
    // A previously secondary question can become the primary one.
    previous.retain(|q| snapshot.pending.as_ref().is_none_or(|p| p.id != q.id));
}

/// The app's question cards. An app without them (CatDesk) has no question
/// source either, so its empty sets are never consulted.
fn cards(profile: &AppProfile) -> &'static muster_agent::profile::Questions {
    const NONE: muster_agent::profile::Questions = muster_agent::profile::Questions {
        open: &[],
        submit: &[],
        submit_via_focused_space: false,
        next: &[],
        back: &[],
        other: &[],
        paged: false,
        refresh_renderer: false,
        button_choices: false,
        immediate_suggestions: false,
    };
    profile.controls().questions.as_ref().unwrap_or(&NONE)
}

struct Node {
    element: Element,
    parent: Option<usize>,
    role: String,
    labels: Vec<String>,
    enabled: bool,
    composer: bool,
    collapsed: bool,
}
struct Tree {
    nodes: Vec<Node>,
}
struct Form {
    root: usize,
    choices: Vec<usize>,
    editor: Option<usize>,
    other: Option<usize>,
}
enum FilledQuestion {
    Immediate { needs_submit: bool },
    Form(Tree, Form),
}
// Preserve the distinction between a provider not supplying an optional focus
// attribute and an AX transport/proxy failure. The latter cannot authorize input.
fn submit_application_focus(
    observed: Result<bool, ax::AXError>,
) -> muster_agent::questions::SubmitApplicationFocus {
    use muster_agent::questions::SubmitApplicationFocus;
    match observed {
        Ok(true) => SubmitApplicationFocus::Matching,
        Ok(false) => SubmitApplicationFocus::Different,
        Err(-25205 | -25212) => SubmitApplicationFocus::NotSupplied,
        Err(_) => SubmitApplicationFocus::Failed,
    }
}

fn normalized(s: &str) -> String {
    s.split_whitespace().collect::<Vec<_>>().join(" ")
}
impl Tree {
    fn read(root: &Element, profile: &AppProfile) -> Result<Self, Error> {
        let start = Instant::now();
        let mut nodes = Vec::new();
        let mut todo = vec![(root.clone(), None)];
        while let Some((element, parent)) = todo.pop() {
            if nodes.len() >= ax::MAX_NODES || start.elapsed() > ax::WALK_BUDGET {
                return Err(Error::Unavailable(
                    "The question accessibility tree is incomplete; try again".into(),
                ));
            }
            let id = nodes.len();
            let labels = ["AXTitle", "AXDescription", "AXValue"]
                .into_iter()
                .filter_map(|key| element.string(key))
                .filter(|s| !s.trim().is_empty())
                .map(|s| normalized(&s))
                .collect();
            let children = element.children();
            nodes.push(Node {
                role: element.role(),
                enabled: element.bool("AXEnabled").unwrap_or(false),
                composer: profile.controls().composer.matches(&element),
                collapsed: element.bool("AXExpanded") == Some(false),
                element,
                parent,
                labels,
            });
            todo.extend(children.into_iter().rev().map(|c| (c, Some(id))));
        }
        Ok(Self { nodes })
    }
    fn inside(&self, mut id: usize, root: usize) -> bool {
        loop {
            if id == root {
                return true;
            }
            match self.nodes[id].parent {
                Some(p) => id = p,
                None => return false,
            }
        }
    }
    fn named(&self, id: usize, label: &str) -> bool {
        let expected = normalized(label);
        // Labels may be on a text child of a radio/button (Claude's cards).
        self.nodes
            .iter()
            .enumerate()
            .any(|(i, n)| self.inside(i, id) && n.labels.iter().any(|s| s == &expected))
    }
    fn control(&self, id: usize, enabled_only: bool) -> bool {
        matches!(
            self.nodes[id].role.as_str(),
            "AXButton" | "AXRadioButton" | "AXCheckBox"
        ) && (!enabled_only || self.nodes[id].enabled)
    }
    fn unique(&self, root: usize, label: &str) -> Option<usize> {
        self.unique_control(root, label, true)
    }
    fn unique_control(&self, root: usize, label: &str, enabled_only: bool) -> Option<usize> {
        let found: Vec<_> = (0..self.nodes.len())
            .filter(|&i| {
                self.inside(i, root) && self.control(i, enabled_only) && self.named(i, label)
            })
            .collect();
        (found.len() == 1).then(|| found[0])
    }
    fn editor(&self, root: usize) -> Option<usize> {
        let found: Vec<_> = (0..self.nodes.len())
            .filter(|&i| {
                self.inside(i, root)
                    && matches!(self.nodes[i].role.as_str(), "AXTextArea" | "AXTextField")
                    && !self.nodes[i].composer
            })
            .collect();
        (found.len() == 1).then(|| found[0])
    }
    /// Claude virtualizes older messages. A live proposal may therefore have
    /// no form in the current tree until its earlier transcript rows are loaded.
    /// This is navigation only; never interpret a generic task button as Start.
    fn earlier_messages(&self, question: &InteractionQuestion) -> Option<usize> {
        if !question.id.starts_with("claude:proposal:") {
            return None;
        }
        let matches: Vec<_> = self
            .nodes
            .iter()
            .enumerate()
            .filter(|(_, n)| {
                n.role == "AXButton"
                    && n.enabled
                    && n.labels.iter().any(|label| {
                        matches!(label.as_str(), "Load earlier messages" | "加载更早的消息")
                    })
            })
            .map(|(i, _)| i)
            .collect();
        (matches.len() == 1).then(|| matches[0])
    }
    #[cfg(test)]
    fn form(&self, q: &InteractionQuestion, profile: &AppProfile) -> Option<Form> {
        self.form_labels(q, profile, &[normalized(&q.prompt)], None)
    }
    fn form_for(
        &self,
        pending: &PendingInteraction,
        q: &InteractionQuestion,
        profile: &AppProfile,
    ) -> Option<Form> {
        let mut labels = vec![normalized(&q.prompt)];
        if muster_agent::claude_cards::is_wrap(pending) {
            let combined = normalized(&format!("{} {}", pending.prompt, q.prompt));
            if self.nodes.iter().any(|n| n.labels.contains(&combined)) {
                labels = vec![combined];
            }
        }
        self.form_labels(
            q,
            profile,
            &labels,
            muster_agent::claude_cards::is_wrap(pending).then_some(pending.prompt.as_str()),
        )
    }
    fn form_labels(
        &self,
        q: &InteractionQuestion,
        profile: &AppProfile,
        labels: &[String],
        recap: Option<&str>,
    ) -> Option<Form> {
        let mut found: Option<Form> = None;
        for (id, node) in self.nodes.iter().enumerate() {
            if !node.labels.iter().any(|s| labels.contains(s)) {
                continue;
            }
            let mut parent = node.parent.or(Some(id));
            while let Some(root) = parent {
                // Never widen the question form to the main chat composer.
                if self
                    .nodes
                    .iter()
                    .enumerate()
                    .any(|(i, n)| self.inside(i, root) && n.composer)
                {
                    break;
                }
                if let Some(recap) = recap {
                    // A wrap-up is a small text-and-buttons box. Never cross
                    // unrelated transcript prose to borrow another card's buttons.
                    let mut allowed = labels.to_vec();
                    allowed.extend([normalized(recap), normalized(&q.prompt)]);
                    allowed.extend(["⌘", "↵", "Next step", "下一步"].map(str::to_owned));
                    for option in &q.options {
                        allowed.extend(
                            muster_agent::claude_cards::option_labels(option)
                                .into_iter()
                                .map(normalized),
                        );
                    }
                    if self.nodes.iter().enumerate().any(|(i, n)| {
                        self.inside(i, root)
                            && n.role == "AXStaticText"
                            && n.labels.iter().any(|label| !allowed.contains(label))
                    }) {
                        break;
                    }
                }
                let choices: Option<Vec<_>> = q
                    .options
                    .iter()
                    .map(|o| {
                        let mut matches: Vec<_> = muster_agent::claude_cards::option_labels(o)
                            .iter()
                            .filter_map(|label| self.unique(root, label))
                            .collect();
                        // Current Claude labels the dismissal with the exact
                        // proposal title; a generic prefix must never borrow
                        // the dismissal control of a neighbouring proposal.
                        if q.id
                            .starts_with(muster_agent::claude_cards::PROPOSAL_QUESTION)
                            && o.id == "action:dismiss"
                            && let Some(button) =
                                self.unique(root, &format!("Dismiss suggestion {}", q.prompt))
                        {
                            matches.push(button);
                        }
                        matches.sort_unstable();
                        matches.dedup();
                        (matches.len() == 1).then(|| matches[0])
                    })
                    .collect();
                let editor = self.editor(root);
                if let Some(choices) = choices {
                    if !choices.is_empty() || editor.is_some() {
                        let other = cards(profile)
                            .other
                            .iter()
                            .find_map(|label| self.unique(root, label));
                        let form = Form {
                            root,
                            choices,
                            editor,
                            other,
                        };
                        if let Some(previous) = &found {
                            if previous.choices != form.choices || previous.editor != form.editor {
                                return None;
                            }
                        } else {
                            found = Some(form);
                        }
                        break;
                    }
                }
                parent = self.nodes[root].parent;
            }
        }
        found
    }
    /// A collapsed reply panel exposes an opener beside its question text.
    /// Start at the button and stop at its first text-bearing container: never
    /// widen through a different card to find matching historical prompt text.
    fn question_opener(
        &self,
        pending: &PendingInteraction,
        question: &InteractionQuestion,
        profile: &AppProfile,
    ) -> Option<usize> {
        if self.form_for(pending, question, profile).is_some() {
            return None;
        }
        if question
            .id
            .starts_with(muster_agent::claude_cards::PROPOSAL_QUESTION)
            || question.id == muster_agent::claude_cards::OFFER_QUESTION
        {
            let found: Vec<_> = self
                .nodes
                .iter()
                .enumerate()
                .filter(|(i, n)| {
                    n.collapsed
                        && n.enabled
                        && matches!(n.role.as_str(), "AXButton" | "AXDisclosureTriangle")
                        && self.named(*i, &question.prompt)
                })
                .map(|(i, _)| i)
                .collect();
            return (found.len() == 1).then(|| found[0]);
        }
        if !pending.optional {
            return None;
        }
        let prompt = normalized(&question.prompt);
        let mut found = Vec::new();
        for (button, node) in self.nodes.iter().enumerate() {
            if node.role != "AXButton"
                || !node.enabled
                || !cards(profile)
                    .open
                    .iter()
                    .any(|label| self.named(button, label))
            {
                continue;
            }
            let mut parent = node.parent;
            while let Some(root) = parent {
                let descendants: Vec<_> = self
                    .nodes
                    .iter()
                    .enumerate()
                    .filter(|&(i, _)| self.inside(i, root))
                    .collect();
                if descendants.iter().any(|&(i, n)| {
                    n.composer
                        || matches!(n.role.as_str(), "AXTextArea" | "AXTextField")
                        || (self.control(i, false) && i != button)
                }) {
                    break;
                }
                let text: Vec<_> = descendants
                    .iter()
                    .filter(|&&(i, _)| !self.inside(i, button))
                    .flat_map(|(_, n)| &n.labels)
                    .filter(|s| !s.is_empty())
                    .collect();
                if !text.is_empty() {
                    if text.iter().any(|s| **s == prompt) {
                        found.push(button);
                    }
                    break;
                }
                parent = self.nodes[root].parent;
            }
        }
        (found.len() == 1).then(|| found[0])
    }

    fn option_needs_submit(
        &self,
        form: &Form,
        profile: &AppProfile,
        pending: &PendingInteraction,
        plan: &AnswerPlan,
    ) -> bool {
        // Optional suggestions send/advance immediately; Send is for free text.
        // Claude's final page needs its separate (initially disabled) Submit.
        !muster_agent::questions::immediate_option(cards(profile), pending, plan)
            && self
                .command_control(form, profile, cards(profile).submit, false)
                .is_some()
    }

    fn ready_form(
        &self,
        pending: &PendingInteraction,
        question: &InteractionQuestion,
        profile: &AppProfile,
        require_submit: bool,
    ) -> Option<Form> {
        let form = self.form_for(pending, question, profile)?;
        (!require_submit
            || self
                .command(&form, profile, cards(profile).submit)
                .is_some())
        .then_some(form)
    }

    fn visible_question<'a>(
        &self,
        pending: &'a PendingInteraction,
        profile: &AppProfile,
    ) -> Option<(&'a InteractionQuestion, Form)> {
        let mut matches = pending
            .questions
            .iter()
            .filter_map(|q| self.form_for(pending, q, profile).map(|form| (q, form)));
        let found = matches.next()?;
        matches.next().is_none().then_some(found)
    }

    fn individual_submit(
        &self,
        form: &Form,
        pending: &PendingInteraction,
        question: &InteractionQuestion,
        profile: &AppProfile,
    ) -> bool {
        let Some(submit) = self.command(form, profile, cards(profile).submit) else {
            return false;
        };
        let mut root = form.root;
        while !self.inside(submit, root) {
            let Some(parent) = self.nodes[root].parent else {
                return false;
            };
            root = parent;
        }
        !pending
            .questions
            .iter()
            .filter(|q| q.id != question.id)
            .any(|q| {
                self.nodes.iter().enumerate().any(|(i, node)| {
                    self.inside(i, root)
                        && node
                            .labels
                            .iter()
                            .any(|label| label == &normalized(&q.prompt))
                })
            })
    }
    fn command(&self, form: &Form, profile: &AppProfile, labels: &[&str]) -> Option<usize> {
        self.command_control(form, profile, labels, true)
    }
    fn command_control(
        &self,
        form: &Form,
        _profile: &AppProfile,
        labels: &[&str],
        enabled_only: bool,
    ) -> Option<usize> {
        let mut parent = Some(form.root);
        while let Some(root) = parent {
            if self
                .nodes
                .iter()
                .enumerate()
                .any(|(i, n)| self.inside(i, root) && n.composer)
            {
                return None;
            }
            let mut found: Vec<_> = labels
                .iter()
                .filter_map(|label| self.unique_control(root, label, enabled_only))
                .collect();
            found.sort_unstable();
            found.dedup();
            if found.len() == 1 {
                return Some(found[0]);
            }
            parent = self.nodes[root].parent;
        }
        None
    }
}

fn numeric_value(element: &Element) -> Option<i64> {
    use core_foundation::number::CFNumber;
    element
        .attribute("AXValue")?
        .downcast::<CFNumber>()?
        .to_i64()
}

fn selection_state(
    value: Option<bool>,
    number: Option<i64>,
    selected: Option<bool>,
) -> Option<bool> {
    // AXSelected describes selection of an accessibility element, not a
    // checkbox's checked state. Electron checkboxes can expose it as false
    // while AXValue is 1. Prefer the control value, and never treat a mixed
    // checkbox (2) as a confirmed answer.
    if let Some(value) = value {
        return Some(value);
    }
    match number {
        Some(0) => Some(false),
        Some(1) => Some(true),
        Some(_) => None,
        None => selected,
    }
}

fn selected(element: &Element) -> Option<bool> {
    selection_state(
        element.bool("AXValue"),
        numeric_value(element),
        element.bool("AXSelected"),
    )
}

impl NativeAppController {
    pub(super) fn inspect_question_form(&self) -> Result<String, Error> {
        let started = Instant::now();
        let initial = self.question_snapshot()?;
        let pending = (self.question_source)()
            .and_then(|s| s.pending)
            .ok_or_else(|| Error::NotFound("No pending native question".into()))?;
        let generation = user_input_generation();
        let mut history_pages = 0;
        if let Some(question) = pending.questions.first() {
            for _ in 0..3 {
                let tree = self.fresh_question_tree(&pending, generation)?;
                if tree.form_for(&pending, question, self.profile).is_some() {
                    break;
                }
                let Some(button) = tree.earlier_messages(question) else {
                    break;
                };
                self.check_question_control(&pending, generation, &tree.nodes[button].element)?;
                tree.nodes[button].element.press().map_err(|code| {
                    Error::Unavailable(format!("Load earlier messages failed ({code})"))
                })?;
                history_pages += 1;
                std::thread::sleep(Duration::from_millis(150));
            }
        }
        while started.elapsed() < Duration::from_secs(5) {
            muster_agent::control_guard::check()?;
            std::thread::sleep(Duration::from_millis(100));
        }
        let mut final_snapshot = self.question_snapshot()?;
        final_snapshot["initial_forms"] = initial["forms"].clone();
        final_snapshot["observed_for_ms"] = serde_json::json!(started.elapsed().as_millis());
        final_snapshot["history_pages_loaded"] = serde_json::json!(history_pages);
        Ok(final_snapshot.to_string())
    }

    fn question_snapshot(&self) -> Result<serde_json::Value, Error> {
        muster_agent::control_guard::check()?;
        self.check_identity()?;
        let pending = (self.question_source)()
            .and_then(|s| s.pending)
            .ok_or_else(|| Error::NotFound("No pending native question".into()))?;
        let web = self.surface.lock().unwrap().web_area.clone();
        if !self.app.contains(&web) {
            return Err(Error::Unavailable("Question is no longer visible".into()));
        }
        let renderer_query = cards(self.profile).refresh_renderer && self.app.query_renderer(&web);
        let tree = Tree::read(&web, self.profile)?;
        let forms: Vec<_> = pending.questions.iter().map(|q| {
            match tree.form_for(&pending, q, self.profile) {
                Some(form) => serde_json::json!({
                    "question": q.id, "matched": true, "multi_select": q.multi_select,
                    "choices": form.choices.iter().map(|&i| serde_json::json!({
                        "role": tree.nodes[i].role,
                        "selected": selected(&tree.nodes[i].element),
                        "ax_selected": tree.nodes[i].element.bool("AXSelected"),
                        "ax_value_bool": tree.nodes[i].element.bool("AXValue"),
                        "ax_value_number": numeric_value(&tree.nodes[i].element),
                        "enabled": tree.nodes[i].element.bool("AXEnabled")
                    })).collect::<Vec<_>>(),
                    "custom_editor": form.editor.is_some(), "other": form.other.is_some(),
                    "submit_visible": tree.command_control(&form, self.profile, cards(self.profile).submit, false).is_some(),
                    "submit_enabled": tree.command(&form, self.profile, cards(self.profile).submit).is_some()
                }),
                None => serde_json::json!({"question": q.id, "matched": false})
            }
        }).collect();
        let application_focus = self.app.element("AXFocusedUIElement");
        let nodes: Vec<_> = tree.nodes.iter().enumerate().map(|(id, n)| serde_json::json!({
            "id": id, "parent": n.parent, "role": n.role,
            "labels": n.labels.iter().map(|s| s.chars().take(400).collect::<String>()).collect::<Vec<_>>(),
            "enabled": n.enabled, "composer": n.composer,
            "selected": selected(&n.element),
            "focused": n.element.bool("AXFocused"),
            "application_focus_matches": application_focus.as_ref() == Some(&n.element),
        })).collect();
        Ok(
            serde_json::json!({"session": self.session, "interaction": pending.id, "forms": forms, "nodes": nodes, "read_only": true, "renderer_query": renderer_query}),
        )
    }

    fn question_guard(
        &self,
        expected: &PendingInteraction,
        generation: u64,
    ) -> Result<Element, Error> {
        muster_agent::control_guard::check()?;
        self.check_identity()?;
        if user_input_generation() != generation {
            return Err(Error::Conflict(
                "Local input changed; question submission stopped".into(),
            ));
        }
        if !(self.question_source)()
            .and_then(|s| s.find(&expected.id).cloned())
            .is_some_and(|current| remaining_matches(expected, &current))
        {
            return Err(Error::Conflict(
                "The question was answered or changed in the native app".into(),
            ));
        }
        let web = self.surface.lock().unwrap().web_area.clone();
        if (self.resolver)(&WebAreaKey::of(&web)).as_deref() != Some(self.session.as_str())
            || !self.app.contains(&web)
        {
            return Err(Error::Unavailable(
                "Question view is no longer the selected session".into(),
            ));
        }
        Ok(web)
    }
    fn check_question_control(
        &self,
        expected: &PendingInteraction,
        generation: u64,
        element: &Element,
    ) -> Result<(), Error> {
        let web = self.question_guard(expected, generation)?;
        if !web.contains(element) || !element.bool("AXEnabled").unwrap_or(false) {
            return Err(Error::Unavailable(
                "The question control changed; nothing was submitted".into(),
            ));
        }
        Ok(())
    }
    fn press_question(
        &self,
        expected: &PendingInteraction,
        generation: u64,
        element: &Element,
    ) -> Result<(), Error> {
        self.check_question_control(expected, generation, element)?;
        // Conservative: any question press may be the one that answers.
        muster_agent::control_guard::delivery_attempt();
        self.question_writes
            .fetch_add(1, std::sync::atomic::Ordering::SeqCst);
        element
            .press()
            .map_err(|code| Error::Unavailable(format!("Question AX press failed ({code})")))
    }
    fn fresh_question_tree(
        &self,
        pending: &PendingInteraction,
        generation: u64,
    ) -> Result<Tree, Error> {
        let started = Instant::now();
        self.question_guard(pending, generation)?;
        let guard_ms = started.elapsed().as_millis();
        // A valid composer/session identity does not imply that the retained
        // web-area proxy contains the current paginated form. Re-resolve from
        // a new application proxy, just as a new diagnostic inspection does.
        // This is read-only discovery of the same session, never a tab switch
        // or another press of the answer control.
        let app = Element::application(self.pid);
        let fresh = find_surface(self.profile, &app, &self.session, &self.resolver)?;
        let surface_ms = started.elapsed().as_millis();
        self.question_guard(pending, generation)?;
        let recheck_ms = started.elapsed().as_millis();
        let tree = Tree::read(&fresh.web_area, self.profile)?;
        log::info!(
            "AX question tree: session={} nodes={} guard_ms={guard_ms} surface_ms={} recheck_ms={} read_ms={} total_ms={}",
            self.session,
            tree.nodes.len(),
            surface_ms - guard_ms,
            recheck_ms - surface_ms,
            started.elapsed().as_millis() - recheck_ms,
            started.elapsed().as_millis()
        );
        *self.surface.lock().unwrap() = fresh;
        Ok(tree)
    }

    async fn locate_form(
        &self,
        pending: &PendingInteraction,
        generation: u64,
        question: &InteractionQuestion,
    ) -> Result<(Tree, Form), Error> {
        self.locate_controls(pending, generation, question, false)
            .await
    }

    async fn locate_controls(
        &self,
        pending: &PendingInteraction,
        generation: u64,
        question: &InteractionQuestion,
        require_submit: bool,
    ) -> Result<(Tree, Form), Error> {
        let mut step = Step::new(
            if require_submit {
                "question submit ready"
            } else {
                "locate question panel"
            },
            Duration::from_secs(3),
        );
        let mut opened = false;
        let mut history_pages = 0;
        loop {
            step.check()?;
            let tree = self.fresh_question_tree(pending, generation)?;
            step.check()?;
            let matched_at = Instant::now();
            let form = tree.ready_form(pending, question, self.profile, require_submit);
            log::info!(
                "AX question match: session={} question={} submit={} matched={} nodes={} match_ms={} step_ms={}",
                self.session,
                question.id,
                require_submit,
                form.is_some(),
                tree.nodes.len(),
                matched_at.elapsed().as_millis(),
                step.elapsed().as_millis()
            );
            step.check()?;
            if let Some(form) = form {
                log::info!(
                    "AX question controls ready: session={} question={} submit={} elapsed_ms={}",
                    self.session,
                    question.id,
                    require_submit,
                    step.elapsed().as_millis()
                );
                return Ok((tree, form));
            }
            if !require_submit
                && !opened
                && let Some(button) = tree.question_opener(pending, question, self.profile)
            {
                let element = &tree.nodes[button].element;
                self.check_question_control(pending, generation, element)?;
                // This profile-defined action only opens the reply panel. It
                // does not deliver an answer, and is never retried in this wait.
                opened = true;
                element.press().map_err(|code| {
                    Error::Unavailable(format!("Question panel AX press failed ({code})"))
                })?;
                // Waiting for the expanded controls is a new step. Locating
                // and pressing the opener must not consume this budget.
                step = Step::new("expanded question controls", Duration::from_secs(3));
                log::info!(
                    "AX question panel opened: session={} question={}",
                    self.session,
                    question.id
                );
            }
            if !require_submit
                && !opened
                && history_pages < 3
                && let Some(button) = tree.earlier_messages(question)
            {
                self.check_question_control(pending, generation, &tree.nodes[button].element)?;
                tree.nodes[button].element.press().map_err(|code| {
                    Error::Unavailable(format!(
                        "Load earlier messages failed ({code}); no answer submitted"
                    ))
                })?;
                history_pages += 1;
                step = Step::new("load earlier question controls", Duration::from_secs(3));
                log::info!(
                    "AX question history loaded: session={} question={} page={history_pages}",
                    self.session,
                    question.id
                );
                tokio::time::sleep(Duration::from_millis(150)).await;
                continue;
            }
            if step.expired() {
                let visible: Vec<_> = pending
                    .questions
                    .iter()
                    .filter(|q| tree.form_for(pending, q, self.profile).is_some())
                    .map(|q| q.id.as_str())
                    .collect();
                log::warn!(
                    "AX question controls timeout: session={} question={} submit={} visible={visible:?}",
                    self.session,
                    question.id,
                    require_submit
                );
                step.check()?;
            }
            // Ordinary attribute polling can keep returning Chromium's old
            // browser-side AX cache while covered. Ask the renderer a read-only
            // geometry question, then let its asynchronous update arrive. Never
            // repeat the preceding option press or relax the live guard.
            if cards(self.profile).refresh_renderer {
                let web = self.question_guard(pending, generation)?;
                let queried = self.app.query_renderer(&web);
                log::info!(
                    "AX question renderer query: session={} question={} accepted={queried}",
                    self.session,
                    question.id
                );
            }
            tokio::time::sleep(Duration::from_millis(100)).await;
        }
    }
    async fn first_question_page(
        &self,
        pending: &PendingInteraction,
        generation: u64,
        first: &InteractionQuestion,
    ) -> Result<(), Error> {
        // A failed attempt can leave Claude on a later page. Back is only
        // admitted inside a uniquely matched live question form, never the
        // application's identically named navigation control.
        for _ in 0..pending.questions.len() {
            let tree = self.fresh_question_tree(pending, generation)?;
            if tree.form_for(pending, first, self.profile).is_some() {
                return Ok(());
            }
            if !cards(self.profile).paged {
                break;
            }
            let Some((current, form)) = tree.visible_question(pending, self.profile) else {
                break;
            };
            let Some(index) = pending.questions.iter().position(|q| q.id == current.id) else {
                break;
            };
            let Some(previous) = index.checked_sub(1).map(|i| &pending.questions[i]) else {
                break;
            };
            let Some(back) = tree.command(&form, self.profile, cards(self.profile).back) else {
                break;
            };
            log::info!(
                "AX question back: session={} from={} to={}",
                self.session,
                current.id,
                previous.id
            );
            self.press_question(pending, generation, &tree.nodes[back].element)?;
            self.locate_form(pending, generation, previous).await?;
        }
        Err(Error::Unavailable(
            "The first question page is not visible; no answer option was pressed".into(),
        ))
    }

    async fn fill_question(
        &self,
        pending: &PendingInteraction,
        generation: u64,
        plan: &AnswerPlan,
    ) -> Result<FilledQuestion, Error> {
        // Locate each requested question. This is admission for the next
        // action, not a selected-value or provider-receipt confirmation.
        tokio::time::sleep(Duration::from_millis(200)).await;
        let (mut tree, mut form) = self
            .locate_form(pending, generation, &plan.question)
            .await?;
        let cards = cards(self.profile);
        let immediate = muster_agent::questions::immediate_option(cards, pending, plan)
            || (cards.button_choices
                && !plan.question.multi_select
                && plan.labels.len() == 1
                && plan.text.is_empty()
                && form
                    .choices
                    .iter()
                    .all(|&i| tree.nodes[i].role == "AXButton"));
        if immediate {
            let choice = plan
                .question
                .options
                .iter()
                .position(|o| plan.labels.contains(&o.label))
                .ok_or_else(|| Error::Invalid("The selected option is not offered".into()))?;
            log::info!(
                "AX question dispatch: session={} question={} option={} immediate=true",
                self.session,
                plan.question.id,
                choice
            );
            self.press_question(
                pending,
                generation,
                &tree.nodes[form.choices[choice]].element,
            )?;
            return Ok(FilledQuestion::Immediate {
                needs_submit: tree.option_needs_submit(&form, self.profile, pending, plan),
            });
        }
        for (index, option) in plan.question.options.iter().enumerate() {
            let want = plan.labels.contains(&option.label);
            // Read the current control only to avoid toggling an existing
            // checkbox in the wrong direction. Never wait for post-click echo.
            let current = selected(&tree.nodes[form.choices[index]].element);
            if want && current != Some(true)
                || !want && current == Some(true) && plan.question.multi_select
            {
                self.press_question(
                    pending,
                    generation,
                    &tree.nodes[form.choices[index]].element,
                )?;
                tree = self.fresh_question_tree(pending, generation)?;
                form = tree
                    .form_for(pending, &plan.question, self.profile)
                    .ok_or_else(|| {
                        Error::Unavailable(
                            "Question controls changed before the remaining actions".into(),
                        )
                    })?;
            }
        }
        if let Some(other) = form.other {
            let want = !plan.text.is_empty();
            let current = selected(&tree.nodes[other].element);
            if want && current != Some(true) || !want && current == Some(true) {
                self.press_question(pending, generation, &tree.nodes[other].element)?;
                tree = self.fresh_question_tree(pending, generation)?;
                form = tree
                    .form_for(pending, &plan.question, self.profile)
                    .ok_or_else(|| {
                        Error::Unavailable("Custom answer controls are no longer visible".into())
                    })?;
            }
        }
        if !plan.text.is_empty() {
            let editor = form.editor.ok_or_else(|| {
                Error::Unavailable("The question's custom answer field is not visible".into())
            })?;
            let element = &tree.nodes[editor].element;
            // Start from an empty field. A value written over a draft is
            // merged by Electron's editors, so the answer would be mixed with
            // whatever the field already held and submitted that way.
            self.empty_question_editor(pending, generation, element, &plan.text)
                .await?;
            self.check_question_control(pending, generation, element)?;
            muster_agent::control_guard::delivery_attempt();
            self.question_writes
                .fetch_add(1, std::sync::atomic::Ordering::SeqCst);
            element.set_string("AXValue", &plan.text).map_err(|code| {
                Error::Unavailable(format!("Question text input failed ({code})"))
            })?;
        }
        tree = self.fresh_question_tree(pending, generation)?;
        form = tree
            .form_for(pending, &plan.question, self.profile)
            .ok_or_else(|| {
                Error::Unavailable("Question controls changed before the page action".into())
            })?;
        Ok(FilledQuestion::Form(tree, form))
    }

    /// Empty the question's answer field and confirm it empty before the
    /// answer is written. Shared Rust decides each attempt and owns the
    /// confirmation; this only reads and clears, and submits nothing. An
    /// answer that is already exactly in the field needs no clearing.
    async fn empty_question_editor(
        &self,
        pending: &PendingInteraction,
        generation: u64,
        element: &Element,
        text: &str,
    ) -> Result<(), Error> {
        use muster_agent::native_send::{ClearWait, Clearing};
        let started = Instant::now();
        let mut wait = ClearWait::default();
        loop {
            self.check_question_control(pending, generation, element)?;
            let draft = element
                .string("AXValue")
                .map(|value| !value.trim().is_empty() && value.trim() != text);
            match wait.observe(started.elapsed(), draft) {
                Clearing::Done => {
                    if wait.attempts() > 0 {
                        log::info!(
                            "AX question field cleared: session={} question={} attempts={} elapsed_ms={}",
                            self.session,
                            pending.id,
                            wait.attempts(),
                            started.elapsed().as_millis()
                        );
                    }
                    return Ok(());
                }
                Clearing::Clear(with) => {
                    super::clear_editor(self.profile, element, with).map_err(|code| {
                        Error::Unavailable(format!(
                            "Question answer field clear failed ({code}); nothing was submitted"
                        ))
                    })?;
                }
                Clearing::Wait => tokio::time::sleep(Duration::from_millis(50)).await,
                Clearing::Timeout => {
                    log::warn!(
                        "AX question field still held a draft: session={} question={} attempts={} elapsed_ms={}; nothing was submitted",
                        self.session,
                        pending.id,
                        wait.attempts(),
                        started.elapsed().as_millis()
                    );
                    return Err(Error::Unavailable(
                        "The question's answer field could not be emptied; nothing was submitted"
                            .into(),
                    ));
                }
                Clearing::Unreadable => {
                    return Err(Error::Unavailable(
                        "The question's answer field cannot be read; nothing was submitted".into(),
                    ));
                }
            }
        }
    }

    fn submit_question_page(
        &self,
        pending: &PendingInteraction,
        generation: u64,
        question: &InteractionQuestion,
        tree: &Tree,
        form: &Form,
    ) -> Result<(), Error> {
        let submit = tree
            .command(form, self.profile, cards(self.profile).submit)
            .ok_or_else(|| {
                Error::Unavailable("The question's submit button is not visible".into())
            })?;
        let element = &tree.nodes[submit].element;
        if !cards(self.profile).submit_via_focused_space {
            return self.press_question(pending, generation, element);
        }
        // Complete AX discovery before acquiring focus. A successful focus
        // write is not evidence that focus survives another tree traversal.
        let fresh = self.fresh_question_tree(pending, generation)?;
        let current = fresh
            .ready_form(pending, question, self.profile, true)
            .and_then(|form| fresh.command(&form, self.profile, cards(self.profile).submit))
            .map(|index| &fresh.nodes[index].element)
            .filter(|current| *current == element)
            .ok_or_else(|| Error::Conflict("Question Submit changed; no key sent".into()))?;
        self.check_question_control(pending, generation, current)?;
        current.set_bool("AXFocused", true).map_err(|code| {
            Error::Unavailable(format!(
                "Cannot focus the question Submit ({code}); no key sent"
            ))
        })?;
        use muster_agent::questions::SubmitApplicationFocus;
        let focus_started = Instant::now();
        let mut sample = 0;
        let (focus_source, same_window) = loop {
            muster_agent::control_guard::check()?;
            if user_input_generation() != generation {
                return Err(Error::Conflict(
                    "Local input changed; no submit key sent".into(),
                ));
            }
            let enabled = current.bool("AXEnabled");
            let focused = current.bool("AXFocused");
            let application_focus = self.app.element_result("AXFocusedUIElement");
            let focus_source = submit_application_focus(
                application_focus
                    .as_ref()
                    .map(|element| element == current)
                    .map_err(|code| *code),
            );
            let same_window = if focus_source == SubmitApplicationFocus::NotSupplied {
                current.element("AXWindow").is_some_and(|window| {
                    self.app.element("AXFocusedWindow").as_ref() == Some(&window)
                })
            } else {
                false
            };
            match muster_agent::questions::poll_focused_submit(
                enabled,
                focused,
                focus_source,
                same_window,
                sample,
                focus_started.elapsed(),
            ) {
                Ok(true) => break (focus_source, same_window),
                Ok(false) => {
                    sample += 1;
                    std::thread::sleep(Duration::from_millis(25));
                }
                Err(error) => {
                    log::warn!(
                        "AX question submit focus rejected: session={} question={} enabled={:?} focused={:?} application={:?} native_error={:?} same_window={} samples={} elapsed_ms={}; no activation key sent",
                        self.session,
                        question.id,
                        enabled,
                        focused,
                        focus_source,
                        application_focus.err(),
                        same_window,
                        sample + 1,
                        focus_started.elapsed().as_millis(),
                    );
                    return Err(error);
                }
            }
        };
        // No additional AX traversal after the focus sample: recheck the
        // cheap authorization/input guards immediately before the single key.
        muster_agent::control_guard::check()?;
        if user_input_generation() != generation {
            return Err(Error::Conflict(
                "Local input changed; no submit key sent".into(),
            ));
        }
        muster_agent::control_guard::delivery_attempt();
        self.question_writes
            .fetch_add(1, std::sync::atomic::Ordering::SeqCst);
        log::info!(
            "AX question submit dispatch: session={} question={} method=focused_space focus={:?} same_window={} samples={} elapsed_ms={}",
            self.session,
            question.id,
            focus_source,
            same_window,
            sample + 1,
            focus_started.elapsed().as_millis()
        );
        self.app.space().map_err(|code| {
            Error::Unavailable(format!(
                "Question Space dispatch failed ({code}); do not retry automatically"
            ))
        })
    }

    pub(super) async fn answer_form(
        &self,
        interaction: &str,
        answers: &[QuestionAnswer],
    ) -> Result<(), Error> {
        // An unconfirmed earlier send blocks answers too; its evidence is
        // re-read first and the refusal precedes any question action.
        self.unconfirmed.admit()?;
        // Settle brief input activity during read-only preparation. Once any
        // control is touched, retain this generation for the entire answer.
        let ((pending, plans), generation) =
            muster_agent::control_guard::prepare_stable(user_input_generation, || {
                self.check_identity()?;
                let pending = (self.question_source)()
                    .and_then(|s| s.find(interaction).cloned())
                    .ok_or_else(|| Error::NotFound("This question is no longer pending".into()))?;
                let plans = validate(&pending, answers)?;
                Ok((pending, plans))
            })
            .await?;
        // A draft left in the conversation's own composer breaks answering:
        // it adds a second, enabled Send to the surface and holds the app's
        // focus, so the card's Submit cannot take focus and no activation key
        // is sent. Empty every input this answer depends on first, and confirm
        // it empty, so the console starts from a known state.
        {
            let mut trace = Trace::new("answer", self.profile.app.provider, &self.session);
            trace.stage("clear_composer");
            // A conversation with no visible composer has nothing to clear.
            self.empty_composer(&trace, generation, None, true).await?;
            trace.finish();
        }
        if let Some(first) = plans.first().filter(|p| {
            cards(self.profile).paged && !muster_agent::claude_cards::immediate(&p.question)
        }) {
            self.first_question_page(&pending, generation, &first.question)
                .await?;
        }
        for (index, plan) in plans.iter().enumerate() {
            let snapshot = (self.question_source)();
            if snapshot
                .as_ref()
                .is_some_and(|s| s.resolved.contains(&pending.id))
            {
                return Ok(());
            }
            if snapshot
                .and_then(|s| s.find(&pending.id).cloned())
                .as_ref()
                .is_some_and(|q| {
                    remaining_matches(&pending, q) && !q.questions.contains(&plan.question)
                })
            {
                continue;
            }
            let page = self.fill_question(&pending, generation, plan).await?;
            let final_page = index + 1 == plans.len();
            match page {
                FilledQuestion::Immediate { needs_submit: true } if final_page => {
                    // Wait only for the next action's control, never a selected
                    // value or provider acknowledgement. A missing Submit is
                    // an explicit failure, not silently reported as dispatched.
                    let (tree, form) = self
                        .locate_controls(&pending, generation, &plan.question, true)
                        .await?;
                    self.submit_question_page(&pending, generation, &plan.question, &tree, &form)?;
                }
                FilledQuestion::Immediate { .. } => {}
                FilledQuestion::Form(_, _) if final_page => {
                    let (tree, form) = self
                        .locate_controls(&pending, generation, &plan.question, true)
                        .await?;
                    self.submit_question_page(&pending, generation, &plan.question, &tree, &form)?;
                }
                FilledQuestion::Form(tree, form) => {
                    if let Some(next) = tree.command(&form, self.profile, cards(self.profile).next)
                    {
                        self.press_question(&pending, generation, &tree.nodes[next].element)?;
                    } else if pending.optional
                        && tree.individual_submit(&form, &pending, &plan.question, self.profile)
                    {
                        self.submit_question_page(
                            &pending,
                            generation,
                            &plan.question,
                            &tree,
                            &form,
                        )?;
                    }
                }
            }
            log::info!(
                "AX question actions dispatched: session={} question={} completed={}/{}",
                self.session,
                plan.question.id,
                index + 1,
                plans.len()
            );
        }
        // The caller reports actions performed and asks the user to verify the
        // native conversation. A later provider event remains the only source
        // of InteractionResolved; no acknowledgement is fabricated here.
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    #[test]
    fn absent_focus_is_distinct_from_ax_failure_or_conflicting_focus() {
        use muster_agent::questions::SubmitApplicationFocus as Focus;
        assert_eq!(super::submit_application_focus(Ok(true)), Focus::Matching);
        assert_eq!(super::submit_application_focus(Ok(false)), Focus::Different);
        for code in [-25205, -25212] {
            assert_eq!(
                super::submit_application_focus(Err(code)),
                Focus::NotSupplied
            );
        }
        // Failure, invalid element, cannot complete, disabled API, unknown status.
        for code in [-25200, -25202, -25204, -25211, 0, -99999] {
            assert_eq!(super::submit_application_focus(Err(code)), Focus::Failed);
        }
    }

    use super::*;
    #[test]
    fn checkbox_value_wins_over_unrelated_element_selection() {
        assert_eq!(selection_state(None, Some(1), Some(false)), Some(true));
        assert_eq!(selection_state(None, Some(0), Some(true)), Some(false));
        assert_eq!(selection_state(Some(true), None, Some(false)), Some(true));
        assert_eq!(selection_state(Some(false), None, Some(true)), Some(false));
        assert_eq!(selection_state(None, Some(2), Some(true)), None);
        assert_eq!(selection_state(None, None, Some(true)), Some(true));
        assert_eq!(selection_state(None, None, None), None);
    }

    #[test]
    fn missing_observation_does_not_resolve_a_question() {
        let q = PendingInteraction {
            id: "question:call".into(),
            kind: "question".into(),
            ..Default::default()
        };
        assert_eq!(pending_question(None, Some(&q)), Some(q.clone()));
        let mut snapshot = muster_agent::questions::Snapshot::default();
        assert_eq!(pending_question(Some(&snapshot), Some(&q)), Some(q.clone()));
        snapshot.resolved.insert(q.id.clone());
        assert!(pending_question(Some(&snapshot), Some(&q)).is_none());
        snapshot.resolved.clear();
        snapshot.complete = true;
        assert!(pending_question(Some(&snapshot), Some(&q)).is_none());
    }
    fn node(parent: Option<usize>, role: &str, label: &str) -> Node {
        Node {
            element: Element::application(std::process::id()),
            parent,
            role: role.into(),
            labels: vec![label.into()],
            enabled: true,
            composer: false,
            collapsed: false,
        }
    }
    fn question(id: &str, prompt: &str) -> InteractionQuestion {
        InteractionQuestion {
            id: id.into(),
            prompt: prompt.into(),
            options: vec![muster_proto::v1::QuestionOption {
                id: "0".into(),
                label: "Yes".into(),
                ..Default::default()
            }],
            ..Default::default()
        }
    }
    fn collapsed_question(label: &str) -> (Tree, PendingInteraction) {
        // Reduced structure from the installed frontend's question wrapper:
        // question text and an Answer question button reopen its reply panel.
        let tree = Tree {
            nodes: vec![
                node(None, "AXWebArea", ""),
                node(Some(0), "AXGroup", ""),
                node(Some(1), "AXGroup", ""),
                node(Some(2), "AXStaticText", "First?"),
                node(Some(1), "AXGroup", ""),
                node(Some(4), "AXButton", ""),
                node(Some(5), "AXStaticText", label),
                node(Some(0), "AXTextArea", "Prompt"),
            ],
        };
        (
            tree,
            PendingInteraction {
                optional: true,
                questions: vec![question("0", "First?")],
                ..Default::default()
            },
        )
    }
    #[test]
    fn collapsed_optional_question_opens_by_exact_prompt_and_localized_button() {
        for label in ["Answer question", "回答问题", "回答問題"] {
            let (mut tree, mut pending) = collapsed_question(label);
            tree.nodes[7].composer = true;
            let q = pending.questions[0].clone();
            assert_eq!(tree.question_opener(&pending, &q, &CODEX_APP), Some(5));
            assert_eq!(tree.question_opener(&pending, &q, &CLAUDE_APP), None);
            pending.optional = false;
            assert_eq!(tree.question_opener(&pending, &q, &CODEX_APP), None);
            pending.optional = true;
            tree.nodes[5].enabled = false;
            assert_eq!(tree.question_opener(&pending, &q, &CODEX_APP), None);
        }
    }
    #[test]
    fn opener_does_not_cross_another_card_to_historical_prompt_text() {
        let (mut tree, pending) = collapsed_question("Answer question");
        let q = &pending.questions[0];
        tree.nodes[3].labels = vec!["Unrelated question?".into()];
        tree.nodes.push(node(Some(0), "AXStaticText", "First?"));
        assert_eq!(tree.question_opener(&pending, q, &CODEX_APP), None);
        tree.nodes[3].labels = vec!["First? extra words".into()];
        assert_eq!(tree.question_opener(&pending, q, &CODEX_APP), None);
    }
    #[test]
    fn opener_rejects_ambiguous_cards_composer_and_already_open_forms() {
        let (mut tree, pending) = collapsed_question("Answer question");
        let q = &pending.questions[0];
        tree.nodes.extend([
            node(Some(0), "AXGroup", ""),
            node(Some(8), "AXStaticText", "First?"),
            node(Some(8), "AXButton", "Answer question"),
        ]);
        assert_eq!(tree.question_opener(&pending, q, &CODEX_APP), None);
        tree.nodes.truncate(8);
        tree.nodes[7].parent = Some(1);
        tree.nodes[7].composer = true;
        assert_eq!(tree.question_opener(&pending, q, &CODEX_APP), None);
        tree.nodes[7].parent = Some(0);
        tree.nodes.push(node(Some(1), "AXRadioButton", "Yes"));
        assert!(tree.form(q, &CODEX_APP).is_some());
        assert_eq!(tree.question_opener(&pending, q, &CODEX_APP), None);
    }
    #[test]
    fn codex_suggestion_does_not_wait_for_free_text_send_after_answering() {
        let q = question("0", "First?");
        let pending = PendingInteraction {
            optional: true,
            questions: vec![q.clone()],
            ..Default::default()
        };
        let plan = AnswerPlan {
            question: q.clone(),
            labels: vec!["Yes".into()],
            text: String::new(),
        };
        let mut tree = Tree {
            nodes: vec![
                node(None, "AXWebArea", ""),
                node(Some(0), "AXGroup", ""),
                node(Some(1), "AXStaticText", "First?"),
                node(Some(1), "AXRadioButton", "Yes"),
                node(Some(1), "AXButton", "Send"),
            ],
        };
        for enabled in [false, true] {
            tree.nodes[4].enabled = enabled;
            let form = tree.form(&q, &CODEX_APP).unwrap();
            assert!(!tree.option_needs_submit(&form, &CODEX_APP, &pending, &plan));
            assert!(!tree.option_needs_submit(&form, &CLAUDE_APP, &pending, &plan));
            let required = PendingInteraction {
                optional: false,
                ..pending.clone()
            };
            assert!(tree.option_needs_submit(&form, &CLAUDE_APP, &required, &plan));
        }
    }

    #[test]
    fn claude_next_step_buttons_use_exact_labels_and_need_no_submit() {
        let mut q = question("0", "现在部署吗？");
        q.options[0].label = "部署".into();
        q.options.push(muster_proto::v1::QuestionOption {
            id: "1".into(),
            label: "先不部署".into(),
            ..Default::default()
        });
        let pending = PendingInteraction {
            optional: true,
            questions: vec![q.clone()],
            ..Default::default()
        };
        let plan = AnswerPlan {
            question: q.clone(),
            labels: vec!["部署".into()],
            text: String::new(),
        };
        // A next-step card has a recap, a question and immediate action
        // buttons; the primary button may also contain a keyboard shortcut.
        let mut tree = Tree {
            nodes: vec![
                node(None, "AXWebArea", ""),
                node(Some(0), "AXGroup", ""),
                node(Some(1), "AXStaticText", "Checks passed."),
                node(Some(1), "AXStaticText", "现在部署吗？"),
                node(Some(1), "AXButton", "部署 ⌘ ↵"),
                node(Some(4), "AXStaticText", "部署"),
                node(Some(4), "AXStaticText", "⌘"),
                node(Some(4), "AXStaticText", "↵"),
                node(Some(1), "AXButton", "先不部署"),
                node(Some(0), "AXTextArea", "Prompt"),
                node(Some(0), "AXButton", "Send"),
            ],
        };
        tree.nodes[9].composer = true;
        let form = tree.form(&q, &CLAUDE_APP).unwrap();
        assert_eq!(form.choices, vec![4, 8]);
        assert!(!tree.option_needs_submit(&form, &CLAUDE_APP, &pending, &plan));
        tree.nodes[8].enabled = false;
        assert!(tree.form(&q, &CLAUDE_APP).is_none());
        tree.nodes[8].enabled = true;
        tree.nodes[3].labels = vec!["Unrelated question".into()];
        assert!(tree.form(&q, &CLAUDE_APP).is_none());
    }

    #[test]
    fn independent_async_cards_scope_identical_options_and_submit_buttons() {
        let q1 = question("0", "First?");
        let q2 = question("1", "Second?");
        let tree = Tree {
            nodes: vec![
                node(None, "AXWebArea", ""),
                node(Some(0), "AXGroup", ""),
                node(Some(1), "AXStaticText", "First?"),
                node(Some(1), "AXRadioButton", "Yes"),
                node(Some(1), "AXButton", "Submit"),
                node(Some(0), "AXGroup", ""),
                node(Some(5), "AXStaticText", "Second?"),
                node(Some(5), "AXRadioButton", "Yes"),
                node(Some(5), "AXButton", "Submit"),
            ],
        };
        let pending = PendingInteraction {
            questions: vec![q1.clone(), q2.clone()],
            ..Default::default()
        };
        for (q, button) in [(&q1, 4), (&q2, 8)] {
            let form = tree.form(q, &CODEX_APP).unwrap();
            assert_eq!(
                tree.command(&form, &CODEX_APP, cards(&CODEX_APP).submit),
                Some(button)
            );
            assert!(tree.individual_submit(&form, &pending, q, &CODEX_APP));
        }
    }
    #[test]
    fn shared_submit_is_not_a_per_question_submit_and_aliases_are_deduplicated() {
        let q1 = question("0", "First?");
        let q2 = question("1", "Second?");
        let mut tree = Tree {
            nodes: vec![
                node(None, "AXWebArea", ""),
                node(Some(0), "AXGroup", ""),
                node(Some(1), "AXGroup", ""),
                node(Some(2), "AXStaticText", "First?"),
                node(Some(2), "AXRadioButton", "Yes"),
                node(Some(1), "AXGroup", ""),
                node(Some(5), "AXStaticText", "Second?"),
                node(Some(5), "AXRadioButton", "Yes"),
                node(Some(1), "AXButton", "Submit"),
            ],
        };
        tree.nodes[8].labels.push("Submit answers".into());
        let pending = PendingInteraction {
            questions: vec![q1.clone(), q2],
            ..Default::default()
        };
        let form = tree.form(&q1, &CODEX_APP).unwrap();
        assert_eq!(
            tree.command(&form, &CODEX_APP, cards(&CODEX_APP).submit),
            Some(8)
        );
        assert!(!tree.individual_submit(&form, &pending, &q1, &CODEX_APP));
        tree.nodes.push(node(Some(1), "AXTextArea", "Ask anything"));
        tree.nodes.last_mut().unwrap().composer = true;
        assert!(
            tree.command(&form, &CODEX_APP, cards(&CODEX_APP).submit)
                .is_none(),
            "must never reach the main composer"
        );
    }
    #[test]
    fn claude_question_lookup_excludes_history_and_sidebar_back() {
        let first = question("0", "First?");
        let second = question("1", "Second?");
        let third = question("2", "Third?");
        let pending = PendingInteraction {
            questions: vec![first.clone(), second.clone(), third.clone()],
            ..Default::default()
        };
        // Sanitized structure from the live Mac-B 2/3 form: history contains
        // every prompt, but only the current form owns option buttons and Back.
        let mut tree = Tree {
            nodes: vec![
                node(None, "AXWebArea", ""),
                node(Some(0), "AXButton", "Back"),
                node(Some(0), "AXGroup", ""),
                node(Some(2), "AXStaticText", "First?"),
                node(Some(2), "AXStaticText", "Second?"),
                node(Some(2), "AXStaticText", "Third?"),
                node(Some(0), "AXGroup", ""),
                node(Some(6), "AXStaticText", "2"),
                node(Some(6), "AXStaticText", "Second?"),
                node(Some(6), "AXButton", "Yes details 1"),
                node(Some(9), "AXStaticText", "Yes"),
                node(Some(6), "AXButton", "Back"),
                node(Some(6), "AXButton", "Next"),
                node(Some(0), "AXTextArea", "Prompt"),
            ],
        };
        tree.nodes[12].enabled = false;
        tree.nodes[13].composer = true;
        assert!(tree.form(&first, &CLAUDE_APP).is_none());
        let (visible, form) = tree.visible_question(&pending, &CLAUDE_APP).unwrap();
        assert_eq!(visible.id, "1");
        assert_eq!(form.choices, vec![9]);
        assert_eq!(tree.command(&form, &CLAUDE_APP, &["Back"]), Some(11));
        assert_eq!(tree.command(&form, &CLAUDE_APP, &["Next"]), None);
        assert!(tree.form(&second, &CLAUDE_APP).is_some());
        assert!(tree.form(&third, &CLAUDE_APP).is_none());
        assert!(tree.form(&first, &CLAUDE_APP).is_none());
        // A nonadjacent page or partial option list cannot acknowledge a press.
        tree.nodes[8].labels = vec!["Third?".into()];
        assert!(tree.form(&second, &CLAUDE_APP).is_none());
        tree.nodes[10].labels = vec!["Different answer".into()];
        assert!(tree.form(&third, &CLAUDE_APP).is_none());
    }

    #[test]
    fn claude_final_option_requires_its_separate_enabled_submit() {
        let q = question("2", "Third?");
        // Captured Mac-B structure: ordinary option buttons expose no checked
        // value, while a distinct Submit exists but starts disabled.
        let mut tree = Tree {
            nodes: vec![
                node(None, "AXWebArea", ""),
                node(Some(0), "AXGroup", ""),
                node(Some(1), "AXStaticText", "Third?"),
                node(Some(1), "AXButton", "Yes"),
                node(Some(1), "AXButton", "Submit"),
            ],
        };
        tree.nodes[4].enabled = false;
        let form = tree
            .ready_form(&PendingInteraction::default(), &q, &CLAUDE_APP, false)
            .unwrap();
        assert_eq!(
            tree.command_control(&form, &CLAUDE_APP, cards(&CLAUDE_APP).submit, false),
            Some(4)
        );
        assert!(
            tree.ready_form(&PendingInteraction::default(), &q, &CLAUDE_APP, true)
                .is_none()
        );
        // No selected-value echo is needed: only the required next control.
        tree.nodes[4].enabled = true;
        let form = tree
            .ready_form(&PendingInteraction::default(), &q, &CLAUDE_APP, true)
            .unwrap();
        assert_eq!(
            tree.command(&form, &CLAUDE_APP, cards(&CLAUDE_APP).submit),
            Some(4)
        );
        tree.nodes[2].labels = vec!["Different question".into()];
        assert!(
            tree.ready_form(&PendingInteraction::default(), &q, &CLAUDE_APP, true)
                .is_none()
        );
    }

    #[test]
    fn partial_question_update_does_not_emit_whole_call_resolution() {
        let mut previous = Some(PendingInteraction {
            id: "question:call".into(),
            questions: vec![question("0", "First?"), question("1", "Second?")],
            ..Default::default()
        });
        let mut partial = previous.clone().unwrap();
        partial.questions.remove(0);
        let mut events = Vec::new();
        update_question(&mut previous, Some(partial), &mut events);
        assert!(matches!(
            events.as_slice(),
            [ControllerEvent::Interaction(_)]
        ));
        events.clear();
        update_question(&mut previous, None, &mut events);
        assert!(matches!(
            events.as_slice(),
            [ControllerEvent::Resolved { .. }]
        ));
    }
    #[test]
    fn wrap_matches_combined_recap_ask_and_never_a_historical_or_sidebar_button() {
        let p=muster_agent::claude_cards::wrap_up(&serde_json::json!({"recap":"Checked.","ask":"Continue?","blocked":true,"actions":[{"action":"commit"}]}),"w","s").unwrap();
        let mut tree = Tree {
            nodes: vec![
                node(None, "AXWebArea", ""),
                node(Some(0), "AXGroup", ""),
                node(Some(1), "AXStaticText", "Continue?"),
                node(Some(1), "AXButton", "Commit"),
                node(Some(0), "AXGroup", ""),
                node(Some(4), "AXStaticText", "Checked. Continue?"),
                node(Some(4), "AXButton", "Commit changes"),
                node(Some(0), "AXTextArea", "Prompt"),
            ],
        };
        tree.nodes[7].composer = true;
        let form = tree.form_for(&p, &p.questions[0], &CLAUDE_APP).unwrap();
        assert_eq!(form.choices, vec![6]);
        tree.nodes[6].enabled = false;
        assert!(tree.form_for(&p, &p.questions[0], &CLAUDE_APP).is_none());
        tree.nodes[6].enabled = true;
        tree.nodes.push(node(Some(0), "AXGroup", ""));
        tree.nodes
            .push(node(Some(8), "AXStaticText", "Checked. Continue?"));
        tree.nodes.push(node(Some(8), "AXButton", "Commit"));
        assert!(tree.form_for(&p, &p.questions[0], &CLAUDE_APP).is_none());
    }
    #[test]
    fn optional_proposal_and_required_wrap_reordering_never_resolves_live_proposal() {
        let mut a = PendingInteraction {
            id: "a".into(),
            kind: "question".into(),
            optional: true,
            ..Default::default()
        };
        a.questions = vec![question("claude:proposal:task_a", "Task")];
        let b = PendingInteraction {
            id: "b".into(),
            kind: "question".into(),
            ..Default::default()
        };
        let snapshot = muster_agent::questions::Snapshot {
            pending: Some(b.clone()),
            additional: vec![a.clone()],
            ..Default::default()
        };
        let mut previous = Some(a.clone());
        let mut extras = Vec::new();
        let mut events = Vec::new();
        update_snapshot(&mut previous, Some(b), Some(&snapshot), &mut events);
        update_additional(&mut extras, Some(&snapshot), &mut events);
        assert!(
            !events
                .iter()
                .any(|e| matches!(e,ControllerEvent::Resolved {interaction} if interaction=="a"))
        );
        assert_eq!(extras, vec![a]);
    }
    #[test]
    fn proposal_buttons_are_bound_to_title_and_offer_is_immediate() {
        let p = muster_agent::claude_cards::proposal(
            &serde_json::json!({"title":"Review this","initiation":"offer"}),
            "o",
            "s",
            false,
        )
        .unwrap();
        let tree = Tree {
            nodes: vec![
                node(None, "AXWebArea", ""),
                node(Some(0), "AXGroup", ""),
                node(Some(1), "AXStaticText", "Review this"),
                node(Some(1), "AXButton", "New session"),
                node(Some(1), "AXButton", "Do it here"),
            ],
        };
        assert_eq!(
            tree.form_for(&p, &p.questions[0], &CLAUDE_APP)
                .unwrap()
                .choices,
            vec![3, 4]
        );
        assert!(muster_agent::questions::immediate_response(
            cards(&CLAUDE_APP),
            &p,
            &p.questions[0]
        ));
    }
    #[test]
    fn task_expansion_requires_explicit_disclosure_metadata_and_unique_title() {
        let p = muster_agent::claude_cards::proposal(
            &serde_json::json!({"title":"Task","initiation":"own_initiative"}),
            "p",
            "s",
            false,
        )
        .unwrap();
        let mut tree = Tree {
            nodes: vec![
                node(None, "AXWebArea", ""),
                node(Some(0), "AXButton", "Task"),
            ],
        };
        assert!(
            tree.question_opener(&p, &p.questions[0], &CLAUDE_APP)
                .is_none()
        );
        tree.nodes[1].collapsed = true;
        assert_eq!(
            tree.question_opener(&p, &p.questions[0], &CLAUDE_APP),
            Some(1)
        );
        let mut duplicate = node(Some(0), "AXButton", "Task");
        duplicate.collapsed = true;
        tree.nodes.push(duplicate);
        assert!(
            tree.question_opener(&p, &p.questions[0], &CLAUDE_APP)
                .is_none()
        );
    }

    #[test]
    fn historical_proposal_can_load_earlier_rows_without_guessing_a_task_action() {
        let mut p = muster_agent::claude_cards::proposal(
            &serde_json::json!({"title":"修正 1.18.2 文档与补齐 Filter 契约","initiation":"own_initiative"}),
            "p", "s", false,
        ).unwrap();
        p.questions[0].id = "claude:proposal:task_906e6165".into();
        let mut tree = Tree {
            nodes: vec![
                node(None, "AXWebArea", ""),
                node(Some(0), "AXButton", "Load earlier messages"),
                node(Some(0), "AXButton", "New session"),
            ],
        };
        assert!(tree.form_for(&p, &p.questions[0], &CLAUDE_APP).is_none());
        assert_eq!(tree.earlier_messages(&p.questions[0]), Some(1));
        let mut ordinary = p.questions[0].clone();
        ordinary.id = "claude:wrap_up".into();
        assert_eq!(tree.earlier_messages(&ordinary), None);
        tree.nodes
            .push(node(Some(0), "AXButton", "Load earlier messages"));
        assert_eq!(tree.earlier_messages(&p.questions[0]), None);
    }

    #[test]
    fn current_proposal_dismissal_requires_the_exact_task_title() {
        let p = muster_agent::claude_cards::proposal(
            &serde_json::json!({"title":"修正 1.18.2 文档与补齐 Filter 契约","initiation":"own_initiative"}),
            "p", "s", false,
        ).unwrap();
        let q = &p.questions[0];
        // Mac-A native AX shape after loading the two earlier message pages.
        let mut tree = Tree {
            nodes: vec![
                node(None, "AXWebArea", ""),
                node(Some(0), "AXGroup", "1 suggested task"),
                node(Some(1), "AXStaticText", "New session"),
                node(Some(1), "AXPopUpButton", &q.prompt),
                node(
                    Some(1),
                    "AXButton",
                    &format!("Dismiss suggestion {}", q.prompt),
                ),
                node(Some(1), "AXPopUpButton", "Options"),
                node(Some(1), "AXButton", "Do it here"),
                node(Some(1), "AXButton", "Start"),
            ],
        };
        assert_eq!(
            tree.form_for(&p, q, &CLAUDE_APP).unwrap().choices,
            vec![7, 4]
        );
        tree.nodes[4].labels = vec!["Dismiss suggestion A different task".into()];
        assert!(tree.form_for(&p, q, &CLAUDE_APP).is_none());
        tree.nodes[4].labels = vec![format!("Dismiss suggestion {}", q.prompt)];
        tree.nodes
            .push(node(Some(1), "AXButton", &tree.nodes[4].labels[0]));
        assert!(tree.form_for(&p, q, &CLAUDE_APP).is_none());
    }
}
