//! AX adapter for the coding composer's model and effort menus: a session's
//! web area, or a new session's form before its first message (ADR 0177).
use super::*;
use muster_agent::{
    desktop_chat as chat,
    native_options::{self as options, Kind},
};
use muster_proto::v1;

type Snapshot = (chat::AxTree, Vec<Element>);

const NO_EFFORT_CONTROLS: &str = "This model picker exposes no effort controls";

/// The one web area whose model and effort controls are read and pressed.
/// `check` re-proves before and after every step that it is still the
/// requested target and that the person's own input has not moved.
pub(in crate::macos) trait Scope: Sync {
    fn check(&self) -> Result<(), Error>;
    fn web(&self) -> &Element;
}

/// A session's web area in a controller's window.
struct SessionScope<'a> {
    controller: &'a NativeAppController,
    generation: u64,
    web: Element,
}

impl Scope for SessionScope<'_> {
    fn check(&self) -> Result<(), Error> {
        let c = self.controller;
        muster_agent::control_guard::check()?;
        if screen_locked() || c.input_generation() != self.generation {
            return Err(Error::Conflict(
                "Unlock the Mac and leave its input idle while changing model or effort".into(),
            ));
        }
        c.check_identity()?;
        if !c.app.contains(&self.web)
            || (c.resolver)(&WebAreaKey::of(&self.web)).as_deref() != Some(c.session.as_str())
        {
            return Err(Error::Conflict(
                "The session view changed; no further option was pressed".into(),
            ));
        }
        Ok(())
    }
    fn web(&self) -> &Element {
        &self.web
    }
}

async fn snapshot(scope: &impl Scope) -> Result<Snapshot, Error> {
    scope.check()?;
    let root = scope.web().clone();
    let snap = muster_agent::control_guard::spawn_blocking(move || {
        super::super::native_chat::snapshot(&root, false)
    })
    .await
    .map_err(|e| Error::Unavailable(format!("Reading model controls: {e}")))?;
    scope.check()?;
    if !snap.0.complete {
        return Err(Error::Unavailable(
            "The session controls could not be read completely".into(),
        ));
    }
    Ok(snap)
}

fn press(scope: &impl Scope, element: &Element) -> Result<(), Error> {
    scope.check()?;
    if !scope.web().contains(element) {
        return Err(Error::Conflict(
            "The option left the requested session".into(),
        ));
    }
    element
        .press()
        .map_err(|e| Error::Unavailable(format!("Model/effort control: {e}")))
}

async fn open(scope: &impl Scope, kind: Kind) -> Result<(usize, Snapshot), Error> {
    let (tree, elements) = snapshot(scope).await?;
    if !chat::open_menus(&tree).is_empty() {
        return Err(Error::Conflict(
            "Close the open menu in the desktop app before changing model or effort".into(),
        ));
    }
    let index = options::picker(&tree, kind)?;
    let picker = chat::Picker {
        node: index,
        label: tree.nodes[index].label().to_owned(),
    };
    if chat::picker_panel(&tree, &picker).is_some() {
        return Err(Error::Conflict(
            "Close the open model/effort panel first".into(),
        ));
    }
    press(scope, &elements[index])?;
    let result = wait_menu(scope, &picker, kind).await;
    if result.is_err() {
        // Best-effort cleanup only after our own successful open. If local
        // input or session identity changed the guard prohibits cleanup.
        let _ = close(scope, kind).await;
    }
    result
}

async fn wait_menu(
    scope: &impl Scope,
    picker: &chat::Picker,
    kind: Kind,
) -> Result<(usize, Snapshot), Error> {
    let mut step = Step::new("model/effort menu", Duration::from_secs(2));
    let mut entered_models = false;
    loop {
        step.check()?;
        let snapshot = snapshot(scope).await?;
        step.check()?;
        let panel = options::panel(&snapshot.0, picker)?;
        if let Some(panel) = panel {
            if kind == Kind::Effort && options::power(&snapshot.0, panel).is_some() {
                return Ok((panel, snapshot));
            }
            let entries = chat::menu_entries(&snapshot.0, panel);
            if let Some(select) = entries
                .iter()
                .find(|e| e.label == "Select model" && e.enabled)
            {
                if kind == Kind::Model && !entered_models {
                    press(scope, &snapshot.1[select.node])?;
                    entered_models = true;
                    step = Step::new("model submenu", Duration::from_secs(2));
                }
            } else if !entries.is_empty() {
                if kind == Kind::Effort && !options::effort_trigger(&picker.label) {
                    return Err(Error::Unavailable(NO_EFFORT_CONTROLS.into()));
                }
                return Ok((panel, snapshot));
            }
        }
        step.check()?;
        tokio::time::sleep(ECHO_POLL).await;
    }
}

async fn close(scope: &impl Scope, kind: Kind) -> Result<(), Error> {
    let (tree, elements) = snapshot(scope).await?;
    let index = options::picker(&tree, kind)?;
    let picker = chat::Picker {
        node: index,
        label: tree.nodes[index].label().to_owned(),
    };
    if options::panel(&tree, &picker)?.is_none() {
        return Ok(());
    }
    // Toggle only the session's uniquely identified picker. Never send a
    // global Escape or click another window to dismiss the menu.
    press(scope, &elements[index])?;
    let step = Step::new("close model/effort menu", Duration::from_secs(1));
    loop {
        step.check()?;
        let (tree, _) = snapshot(scope).await?;
        step.check()?;
        if options::panel(&tree, &picker)?.is_none() {
            return Ok(());
        }
        step.check()?;
        tokio::time::sleep(ECHO_POLL).await;
    }
}

/// Opens the picker, reads its menu with `read`, and closes it again
/// without a new choice.
async fn read_with<T>(
    scope: &impl Scope,
    kind: Kind,
    read: impl FnOnce(&chat::AxTree, usize) -> Result<T, Error>,
) -> Result<T, Error> {
    let (menu, (tree, _)) = open(scope, kind).await?;
    let result = read(&tree, menu);
    // Only dismiss menus this operation successfully opened. A refusal
    // because a user menu was already open must leave it alone.
    let close = close(scope, kind).await;
    let value = result?;
    close?;
    Ok(value)
}

async fn read(scope: &impl Scope, kind: Kind) -> Result<Vec<chat::ChatChoice>, Error> {
    let choices = read_with(scope, kind, |tree, menu| options::choices(tree, menu, kind)).await?;
    if choices.is_empty() {
        return Err(Error::Unavailable(
            "The app exposed no selectable choices".into(),
        ));
    }
    Ok(choices)
}

/// Presses the choice `pick` names in the opened menu (if any), then closes
/// the menu. Success is only ever decided by a later fresh read.
async fn press_choice(
    scope: &impl Scope,
    kind: Kind,
    pick: impl FnOnce(&[chat::ChatChoice]) -> Result<Option<usize>, Error>,
) -> Result<(), Error> {
    let (menu, (tree, elements)) = open(scope, kind).await?;
    let result: Result<bool, Error> = (|| {
        let choices = options::choices(&tree, menu, kind)?;
        match pick(&choices)? {
            Some(node) => press(scope, &elements[node]).map(|()| true),
            None => Ok(false),
        }
    })();
    if result.as_ref().is_ok_and(|pressed| *pressed) {
        tokio::time::sleep(Duration::from_millis(300)).await;
    }
    let close = close(scope, kind).await;
    result?;
    close
}

async fn set(scope: &impl Scope, kind: Kind, name: &str) -> Result<(), Error> {
    press_choice(scope, kind, |choices| {
        let choice = options::choose(choices, name)?;
        Ok((!choice.selected).then_some(choice.node))
    })
    .await?;
    let choices = read(scope, kind).await?;
    if !options::confirmed(&choices, kind, name) {
        return Err(Error::UnknownOutcome(
            "The app did not confirm the requested option; refresh before retrying".into(),
        ));
    }
    Ok(())
}

/// What the form's model picker (and a separate effort picker, if any)
/// show, read without opening anything.
async fn shown(scope: &impl Scope) -> Result<(Vec<String>, String), Error> {
    let (tree, _) = snapshot(scope).await?;
    let model = options::picker(&tree, Kind::Model)?;
    let effort = options::picker(&tree, Kind::Effort).ok();
    let labels = [Some(model), effort.filter(|e| *e != model)]
        .into_iter()
        .flatten()
        .map(|i| tree.nodes[i].label().to_owned())
        .collect::<Vec<_>>()
        .join(" | ");
    Ok((options::shown(&tree, model), labels))
}

/// Whether the form offers an effort picker of its own (the ChatGPT app's
/// Codex composer). Claude's effort is only named in its model panel.
async fn effort_picker(scope: &impl Scope) -> Result<bool, Error> {
    let (tree, _) = snapshot(scope).await?;
    Ok(options::picker(&tree, Kind::Effort).is_ok())
}

/// The effort Claude's model panel names ("Effort High"), or `None`.
async fn submenu_effort(scope: &impl Scope) -> Result<Option<String>, Error> {
    read_with(scope, Kind::Model, |tree, menu| {
        Ok(options::submenu_effort(tree, menu))
    })
    .await
}

/// Chooses a new session's model and effort on its form and reads both back
/// before anything is drafted (ADR 0177). Empty names keep the app's choice.
/// A name the app does not offer, or an effort it offers only on hover,
/// refuses with `options::CREATE_REFUSED`. Returns what the pickers show,
/// for one last read-only comparison just before the first message.
pub(in crate::macos) async fn prepare_new(
    scope: &impl Scope,
    model: &str,
    effort: &str,
) -> Result<Option<String>, Error> {
    if model.is_empty() && effort.is_empty() {
        return Ok(None);
    }
    let selectable_effort = effort_picker(scope).await?;
    // Claude: check the effort it cannot change before pressing anything.
    if !effort.is_empty() && !selectable_effort {
        let current = submenu_effort(scope).await?;
        if current.as_deref() != Some(effort) {
            return Err(options::create_effort_differs(effort, current.as_deref()));
        }
    }
    if !model.is_empty() {
        let (shown, _) = shown(scope).await?;
        press_choice(scope, Kind::Model, |choices| {
            if options::create_model_confirmed(choices, &shown, model) {
                // Validate the name is offered even when nothing moves.
                options::create_choice(choices, Kind::Model, model)?;
                return Ok(None);
            }
            Ok(options::create_choice(choices, Kind::Model, model)?.map(|c| c.node))
        })
        .await?;
    }
    // The model can change the offered efforts: decide on a fresh menu.
    if !effort.is_empty() && selectable_effort {
        press_choice(scope, Kind::Effort, |choices| {
            Ok(options::create_choice(choices, Kind::Effort, effort)?.map(|c| c.node))
        })
        .await
        .map_err(|error| match error {
            // A model-only picker: asking again cannot find an effort.
            Error::Unavailable(why) if why == NO_EFFORT_CONTROLS => {
                options::create_effort_differs(effort, None)
            }
            other => other,
        })?;
    }
    // Read both back; never infer success from a press.
    if !model.is_empty() {
        let (shown, _) = shown(scope).await?;
        let confirmed = options::create_model_confirmed(&[], &shown, model)
            || options::create_model_confirmed(&read(scope, Kind::Model).await?, &shown, model);
        if !confirmed {
            return Err(options::create_refused(format!(
                "the app did not confirm model {model:?} (it shows {:?})",
                shown.join(" ")
            )));
        }
    }
    if !effort.is_empty() {
        if selectable_effort {
            let choices = read(scope, Kind::Effort).await?;
            if !options::confirmed(&choices, Kind::Effort, effort) {
                return Err(options::create_refused(format!(
                    "the app did not confirm effort {effort:?}"
                )));
            }
        } else {
            let current = submenu_effort(scope).await?;
            if current.as_deref() != Some(effort) {
                return Err(options::create_effort_differs(effort, current.as_deref()));
            }
        }
    }
    let (_, labels) = shown(scope).await?;
    Ok(Some(labels))
}

/// Just before the first message: the pickers still show what was
/// confirmed. Read only; nothing is opened or pressed.
pub(in crate::macos) async fn still_shown(
    scope: &impl Scope,
    confirmed: &Option<String>,
) -> Result<(), Error> {
    let Some(confirmed) = confirmed else {
        return Ok(());
    };
    let (_, labels) = shown(scope).await?;
    if &labels != confirmed {
        return Err(options::create_refused(format!(
            "the model picker changed from {confirmed:?} to {labels:?} before the first message"
        )));
    }
    Ok(())
}

impl NativeAppController {
    pub(super) async fn session_options(
        &self,
        set: Option<(&str, &str)>,
    ) -> Result<v1::AgentChatOptions, Error> {
        let _write = self
            .write
            .try_lock()
            .map_err(|_| Error::Conflict("Another native operation is in progress".into()))?;
        self.check_identity()?;
        let scope = SessionScope {
            controller: self,
            generation: self.input_generation(),
            web: self.surface.lock().unwrap().web_area.clone(),
        };
        scope.check()?;
        if let Some((model, effort)) = set {
            if !model.is_empty() {
                self::set(&scope, Kind::Model, model).await?;
            }
            if !effort.is_empty() {
                // Model changes can change the offered effort levels. Validate
                // against that model's fresh menu, never the previous one.
                if let Err(error) = self::set(&scope, Kind::Effort, effort).await {
                    return Err(if model.is_empty() {
                        error
                    } else {
                        Error::Conflict(format!(
                            "Model changed to {model:?}; effort was not confirmed: {error}. Refresh the options before retrying."
                        ))
                    });
                }
            }
        }
        let models = read(&scope, Kind::Model).await?;
        let (efforts, note) = match read(&scope, Kind::Effort).await {
            Ok(e) => {
                let note = if e.iter().any(|c| c.detail.contains(" of ")) {
                    options::POWER_NOTE.into()
                } else {
                    String::new()
                };
                (e, note)
            }
            Err(Error::Unavailable(e)) => (Vec::new(), e),
            Err(e) => return Err(e),
        };
        let effort = efforts
            .iter()
            .find(|c| c.selected)
            .map(|c| c.name.clone())
            .unwrap_or_default();
        let convert = |choices: Vec<chat::ChatChoice>| {
            choices
                .into_iter()
                .map(|c| v1::AgentChatOption {
                    name: c.name,
                    detail: c.detail,
                    selected: c.selected,
                })
                .collect()
        };
        scope.check()?;
        Ok(v1::AgentChatOptions {
            session: self.session.clone(),
            models: convert(models),
            efforts: convert(efforts),
            effort,
            note,
        })
    }
}
