//! Locate the provider's existing queue row; never copy its text into composer.
use super::*;
use muster_agent::queue::{self, Source};

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
fn select(
    controls: &muster_agent::profile::Queue,
    nodes: &[QueueNode],
    text: &str,
) -> Result<usize, Error> {
    let expected = queue::normalize(text);
    let mut matches = Vec::new();
    for (index, node) in nodes.iter().enumerate() {
        if !node.enabled || !controls.send_now.matches(node) {
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
            "Cannot uniquely locate the queued message's Send now control ({} matches); nothing was submitted",
            matches.len()
        )));
    }
    Ok(matches[0])
}
fn control(
    controls: &muster_agent::profile::Queue,
    web: &Element,
    text: &str,
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
    let index = select(controls, &nodes, text)?;
    Ok(elements[index].clone())
}
impl NativeAppController {
    pub(super) async fn send_queued_message(
        &self,
        source: &Source,
        interaction: &str,
    ) -> Result<(), Error> {
        let _write = self
            .write
            .try_lock()
            .map_err(|_| Error::Conflict("Another native operation is in progress".into()))?;
        let Some(controls) = self.profile.controls.queue.as_ref() else {
            return Err(Error::Unavailable(
                "Native message queue unavailable".into(),
            ));
        };
        // Re-read the evidence of an earlier unconfirmed write, then refuse
        // before any press: this request is then definitely not sent.
        self.unconfirmed.admit()?;
        self.check_identity()?;
        muster_agent::control_guard::check()?;
        let generation = user_input_generation();
        let queued = source()?;
        let message = queue::selected(&queued, interaction)?.clone();
        let web = self.surface.lock().unwrap().web_area.clone();
        if (self.resolver)(&WebAreaKey::of(&web)).as_deref() != Some(&self.session)
            || !self.app.contains(&web)
        {
            return Err(Error::Unavailable(
                "The selected session view changed".into(),
            ));
        }
        let button = control(controls, &web, &message.text)?;
        let receipt = (self.send_receipt_source)(&message.text)?;
        // Re-read provider state immediately before the sole mutating action.
        if queue::selected(&source()?, interaction)? != &message {
            return Err(Error::Conflict(
                "The queued message changed; nothing submitted".into(),
            ));
        }
        self.check_identity()?;
        muster_agent::control_guard::check()?;
        if user_input_generation() != generation
            || !web.contains(&button)
            || !button.bool("AXEnabled").unwrap_or(false)
        {
            return Err(Error::Conflict(
                "The queue control or local input changed; nothing submitted".into(),
            ));
        }
        muster_agent::control_guard::delivery_attempt();
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
        assert_eq!(select(codex, &nodes, "second queued message").unwrap(), 7);
        assert!(select(codex, &nodes, "second").is_err());
        nodes[7].enabled = false;
        assert!(select(codex, &nodes, "second queued message").is_err());
        nodes[7].enabled = true;
        nodes[6].label = "first queued message".into();
        assert!(select(codex, &nodes, "first queued message").is_err());
        nodes[4].label = "Delete file".into();
        nodes[8].label = "Delete file".into();
        assert!(select(codex, &nodes, "first queued message").is_err());
    }
}
