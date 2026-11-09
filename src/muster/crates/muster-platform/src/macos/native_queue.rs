//! Locate the provider's existing queue row; never copy its text into composer.
use super::*;
use muster_agent::queue::{self, Action, Source};

struct QueueNode {
    parent: Option<usize>,
    role: String,
    label: String,
    enabled: bool,
}
fn inside(nodes: &[QueueNode], mut child: usize, parent: usize) -> bool {
    loop {
        if child == parent {
            return true;
        }
        match nodes[child].parent {
            Some(p) => child = p,
            None => return false,
        }
    }
}
/// A queue node's label is already its name (title, else description).
impl muster_agent::profile::AxQuery for QueueNode {
    fn role(&self) -> String {
        self.role.clone()
    }
    fn title(&self) -> Option<String> {
        (!self.label.is_empty()).then(|| self.label.clone())
    }
    fn description(&self) -> Option<String> {
        None
    }
    fn placeholder(&self) -> Option<String> {
        None
    }
}
/// The row control that performs `action` for the queued message showing
/// `text`: the one row holding exactly one delete control, at most one send
/// control, no editor, and that text.
fn select(
    controls: &muster_agent::profile::Queue,
    nodes: &[QueueNode],
    text: &str,
    action: Action,
) -> Result<usize, Error> {
    let target = match action {
        Action::SendNow => &controls.send_now,
        Action::Delete => &controls.delete,
    };
    let expected = queue::normalize(text);
    let mut matches = Vec::new();
    for (index, node) in nodes.iter().enumerate() {
        if !node.enabled || !target.matches(node) {
            continue;
        }
        let mut parent = node.parent;
        while let Some(row) = parent {
            if nodes[row].role == "AXWebArea" {
                break;
            }
            let row_nodes: Vec<_> = nodes
                .iter()
                .enumerate()
                .filter(|(i, _)| inside(nodes, *i, row))
                .collect();
            let deletes = row_nodes
                .iter()
                .filter(|(_, n)| controls.delete.matches(*n))
                .count();
            let sends = row_nodes
                .iter()
                .filter(|(_, n)| controls.send_now.matches(*n))
                .count();
            if deletes > 1 || sends > 1 || row_nodes.iter().any(|(_, n)| n.role == "AXTextArea") {
                break;
            }
            let texts: Vec<_> = row_nodes
                .iter()
                .filter(|(i, n)| {
                    if n.role != "AXStaticText" {
                        return false;
                    }
                    let mut parent = nodes[*i].parent;
                    while let Some(p) = parent {
                        if nodes[p].role == "AXButton" {
                            return false;
                        }
                        if p == row {
                            break;
                        }
                        parent = nodes[p].parent;
                    }
                    true
                })
                .map(|(_, n)| n.label.as_str())
                .collect();
            if deletes == 1
                && (texts.iter().any(|t| queue::normalize(t) == expected)
                    || queue::normalize(&texts.join(" ")) == expected)
            {
                matches.push(index);
                break;
            }
            if deletes == 1 && !texts.is_empty() {
                break;
            }
            parent = nodes[row].parent;
        }
    }
    if matches.len() != 1 {
        return Err(Error::Unavailable(format!(
            "Cannot uniquely locate the queued message's {} control ({} matches); nothing was pressed",
            match action {
                Action::SendNow => "Send now",
                Action::Delete => "delete",
            },
            matches.len()
        )));
    }
    Ok(matches[0])
}
fn control(
    controls: &muster_agent::profile::Queue,
    web: &Element,
    text: &str,
    action: Action,
) -> Result<Element, Error> {
    let started = std::time::Instant::now();
    let mut elements = Vec::new();
    let mut nodes = Vec::new();
    let mut todo = vec![(web.clone(), None)];
    while let Some((element, parent)) = todo.pop() {
        if nodes.len() >= ax::MAX_NODES || started.elapsed() > ax::WALK_BUDGET {
            return Err(Error::Unavailable(
                "Queue AX tree was incomplete; nothing submitted".into(),
            ));
        }
        let index = nodes.len();
        let role = element.role();
        let label = if role == "AXStaticText" {
            element
                .string("AXValue")
                .or_else(|| element.string("AXTitle"))
        } else {
            element
                .string("AXTitle")
                .filter(|v| !v.is_empty())
                .or_else(|| element.string("AXDescription"))
        }
        .unwrap_or_default();
        nodes.push(QueueNode {
            parent,
            role,
            label,
            enabled: element.bool("AXEnabled").unwrap_or(false),
        });
        todo.extend(
            element
                .children()
                .into_iter()
                .rev()
                .map(|child| (child, Some(index))),
        );
        elements.push(element);
    }
    let index = select(controls, &nodes, text, action)?;
    Ok(elements[index].clone())
}
impl NativeAppController {
    pub(super) async fn queued_message_action(
        &self,
        source: &Source,
        interaction: &str,
        action: Action,
    ) -> Result<(), Error> {
        let _write = self
            .write
            .try_lock()
            .map_err(|_| Error::Conflict("Another native operation is in progress".into()))?;
        let Some(controls) = self.profile.controls().queue.as_ref() else {
            return Err(Error::Unavailable(
                "Native message queue unavailable".into(),
            ));
        };
        // Re-read the evidence of an earlier unconfirmed write, then refuse
        // before any press: this request is then definitely not sent.
        self.unconfirmed.admit()?;
        self.check_identity()?;
        muster_agent::control_guard::check()?;
        let generation = self.input_generation();
        let queued = source()?;
        let message = queue::selected(&queued, interaction, action)?.clone();
        let returns_text = action == Action::Delete && controls.delete_returns_text;
        if returns_text {
            // The delete puts the text back into the composer, where it would
            // join a draft that Muster must not touch.
            let value = self.composer_value()?.ok_or_else(|| {
                Error::Unavailable("The prompt cannot be read; nothing deleted".into())
            })?;
            if !self.profile.is_empty(&value) {
                return Err(Error::Conflict(
                    "The desktop prompt holds a draft; clear it there first. Nothing deleted"
                        .into(),
                ));
            }
        }
        let web = self.surface.lock().unwrap().web_area.clone();
        if (self.resolver)(&WebAreaKey::of(&web)).as_deref() != Some(&self.session)
            || !self.app.contains(&web)
        {
            return Err(Error::Unavailable(
                "The selected session view changed".into(),
            ));
        }
        let button = control(controls, &web, &message.text, action)?;
        let mut receipt = (self.send_receipt_source)(&message.text)?;
        let removal = queue::Removal::new(source.clone(), &message);
        // Re-read provider state immediately before the sole mutating action.
        if queue::selected(&source()?, interaction, action)? != &message {
            return Err(Error::Conflict(
                "The queued message changed; nothing submitted".into(),
            ));
        }
        self.check_identity()?;
        muster_agent::control_guard::check()?;
        if self.input_generation() != generation
            || !web.contains(&button)
            || !button.bool("AXEnabled").unwrap_or(false)
        {
            return Err(Error::Conflict(
                "The queue control or local input changed; nothing submitted".into(),
            ));
        }
        muster_agent::control_guard::delivery_attempt();
        if action == Action::Delete {
            let pressed = button.press();
            log::info!(
                "AX queue delete: session={} interaction={} press={pressed:?}",
                self.session,
                interaction
            );
            let step = Step::new("queued message removal", Duration::from_secs(10));
            loop {
                // Taken by the turn instead: the delete lost the race.
                if receipt.confirmed() {
                    return Err(Error::Conflict(
                        "The queued message was sent before it could be deleted".into(),
                    ));
                }
                if removal.removed() {
                    break;
                }
                if step.expired() {
                    return Err(Error::UnknownOutcome(
                        "Waiting for the queued message to leave the queue".into(),
                    ));
                }
                tokio::time::sleep(Duration::from_millis(100)).await;
            }
            log::info!(
                "AX queue delete confirmed: session={} interaction={interaction}",
                self.session
            );
            if returns_text {
                self.take_back_returned_text(&message.text, generation)
                    .await;
            }
            return Ok(());
        }
        self.unconfirmed.arm(
            muster_agent::unconfirmed::Origin::now("queued send"),
            Some(receipt),
            None,
        )?;
        let pressed = button.press();
        log::info!(
            "AX queue send now: session={} interaction={} press={pressed:?}",
            self.session,
            interaction
        );
        let step = Step::new("queued message receipt", Duration::from_secs(10));
        loop {
            let confirmed = self.unconfirmed.reconcile().cleared();
            if confirmed {
                log::info!(
                    "AX queue receipt confirmed: session={} interaction={interaction}",
                    self.session
                );
                return Ok(());
            }
            if step.expired() {
                return Err(Error::UnknownOutcome(
                    "Waiting for the queued message receipt; do not send it again".into(),
                ));
            }
            tokio::time::sleep(Duration::from_millis(100)).await;
        }
    }
}
impl NativeAppController {
    /// The composer's value, from a fresh scan of this session's view.
    fn composer_value(&self) -> Result<Option<String>, Error> {
        let fresh = find_surface(self.profile, &self.app, &self.session, &self.resolver)?;
        let composer = fresh.composer.clone().ok_or_else(|| {
            Error::Unavailable("The native prompt is not visible; nothing deleted".into())
        })?;
        self.check_write_surface(&fresh.web_area, &composer)?;
        let value = composer.string("AXValue");
        *self.surface.lock().unwrap() = fresh;
        Ok(value)
    }

    /// Empties the composer once it holds exactly the text a delete put back
    /// there. Anything else stays: the message is already off the queue, and
    /// other text in the composer is the person's.
    async fn take_back_returned_text(&self, text: &str, generation: u64) {
        let step = Step::new(
            "returned queued text",
            muster_agent::native_send::CLEAR_DEADLINE,
        );
        loop {
            match self.composer_value() {
                Ok(Some(value)) if queue::returned_to_composer(&value, text) => break,
                Ok(Some(value)) if !self.profile.is_empty(&value) => {
                    log::warn!(
                        "AX queue delete: session={} the prompt holds other text; left as is",
                        self.session
                    );
                    return;
                }
                Err(e) => {
                    log::warn!(
                        "AX queue delete: session={} prompt unreadable: {e}",
                        self.session
                    );
                    return;
                }
                _ if step.expired() => return,
                _ => {}
            }
            tokio::time::sleep(Duration::from_millis(100)).await;
        }
        let trace = Trace::new("queued delete", self.profile.app.provider, &self.session);
        if let Err(e) = self.empty_composer(&trace, generation, None, false).await {
            log::warn!(
                "AX queue delete: session={} the returned text stayed in the prompt: {e}",
                self.session
            );
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn node(parent: Option<usize>, role: &str, label: &str) -> QueueNode {
        QueueNode {
            parent,
            role: role.into(),
            label: label.into(),
            enabled: true,
        }
    }
    #[test]
    fn queued_control_is_scoped_to_one_exact_row() {
        let codex = muster_agent::profile::CODEX.queue.as_ref().unwrap();
        let mut nodes = vec![
            node(None, "AXWebArea", ""),
            node(Some(0), "AXGroup", ""),
            node(Some(1), "AXStaticText", "first queued message"),
            node(Some(1), "AXButton", "Steer"),
            node(Some(1), "AXButton", "Delete queued message"),
            node(Some(0), "AXGroup", ""),
            node(Some(5), "AXStaticText", "second queued message"),
            node(Some(5), "AXButton", "Steer"),
            node(Some(5), "AXButton", "Delete queued message"),
            node(Some(0), "AXButton", "Send"),
        ];
        let send = Action::SendNow;
        assert_eq!(
            select(codex, &nodes, "second queued message", send).unwrap(),
            7
        );
        assert_eq!(
            select(codex, &nodes, "second queued message", Action::Delete).unwrap(),
            8
        );
        assert!(select(codex, &nodes, "second", send).is_err());
        nodes[7].enabled = false;
        assert!(select(codex, &nodes, "second queued message", send).is_err());
        nodes[7].enabled = true;
        nodes[6].label = "first queued message".into();
        assert!(select(codex, &nodes, "first queued message", send).is_err());
        nodes[4].label = "Delete file".into();
        nodes[8].label = "Delete file".into();
        assert!(select(codex, &nodes, "first queued message", send).is_err());
    }
    /// Claude's queued prompt (recorded 2026-11-07, Claude desktop Code tab):
    /// an unread message whose "Message actions" toolbar carries Send now and
    /// Cancel and edit message; a delivered one carries neither.
    #[test]
    fn claude_queued_message_controls_are_its_own_toolbar() {
        let claude = muster_agent::profile::CLAUDE.queue.as_ref().unwrap();
        assert!(claude.delete_returns_text);
        let message = |nodes: &mut Vec<QueueNode>, n: usize, text: &str, queued: bool| {
            let article = nodes.len();
            nodes.push(node(Some(0), "AXGroup", &format!("Message {n}")));
            nodes.push(node(Some(article), "AXHeading", ""));
            nodes.push(node(
                Some(article + 1),
                "AXStaticText",
                &format!("You said: {text}"),
            ));
            nodes.push(node(Some(article), "AXGroup", ""));
            nodes.push(node(Some(article + 3), "AXStaticText", text));
            let toolbar = nodes.len();
            nodes.push(node(Some(article), "AXToolbar", "Message actions"));
            nodes.push(node(Some(toolbar), "AXButton", "Copy"));
            if queued {
                nodes.push(node(Some(toolbar), "AXButton", "Cancel and edit message"));
                nodes.push(node(Some(toolbar), "AXButton", "Send now"));
            } else {
                nodes.push(node(Some(toolbar), "AXButton", "Fork from here"));
            }
        };
        let mut nodes = vec![node(None, "AXWebArea", "")];
        message(&mut nodes, 1, "delivered prompt", false);
        message(&mut nodes, 2, "first queued", true);
        message(&mut nodes, 3, "second queued", true);
        nodes.push(node(Some(0), "AXTextArea", "Prompt"));
        nodes.push(node(Some(0), "AXButton", "Send"));
        let label = |i: usize| nodes[i].label.as_str();
        let send = select(claude, &nodes, "second queued", Action::SendNow).unwrap();
        assert_eq!(label(send), "Send now");
        assert!(inside(&nodes, send, 18));
        let delete = select(claude, &nodes, "first queued", Action::Delete).unwrap();
        assert_eq!(label(delete), "Cancel and edit message");
        assert!(inside(&nodes, delete, 9) && !inside(&nodes, delete, 18));
        assert!(select(claude, &nodes, "delivered prompt", Action::Delete).is_err());
    }
}
