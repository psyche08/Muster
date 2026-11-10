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
    /// The application element that application-directed keys go to.
    fn app(&self) -> &Element;
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
    fn app(&self) -> &Element {
        &self.controller.app
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

/// One application-directed key on an exactly focused control.
#[derive(Clone, Copy)]
enum Key {
    OpenSubmenu,
    Power(options::PowerStep),
}

/// Focuses `item`, proves that the app's focused element and window are
/// exactly it and the scope's window, and posts one key to the app with
/// `AXUIElementPostKeyboardEvent`: application-directed Accessibility input,
/// never global input or the pointer (the route `native_organization`
/// opens its submenus by).
async fn focused_key(scope: &impl Scope, item: &Element, key: Key) -> Result<(), Error> {
    scope.check()?;
    let web = scope.web();
    if !web.contains(item) {
        return Err(Error::Conflict(
            "The option left the requested session".into(),
        ));
    }
    let window = web
        .element("AXWindow")
        .ok_or_else(|| Error::Unavailable("The session exposes no AX window".into()))?;
    let app = scope.app();
    item.set_bool("AXFocused", true)
        .map_err(|e| Error::Unavailable(format!("Model/effort focus: {e}")))?;
    let step = Step::new("focus model/effort control", Duration::from_secs(1));
    loop {
        step.check()?;
        scope.check()?;
        if app.element("AXFocusedUIElement").as_ref() == Some(item)
            && app.element("AXFocusedWindow").as_ref() == Some(&window)
            && item.bool("AXFocused") == Some(true)
            && web.contains(item)
        {
            break;
        }
        tokio::time::sleep(ECHO_POLL).await;
    }
    // The key reaches the app, not the HID stream, so a held modifier could
    // still turn it into a shortcut.
    if modifiers_held() {
        return Err(Error::Conflict(
            "A modifier key is held; no model/effort key was sent".into(),
        ));
    }
    scope.check()?;
    match key {
        Key::OpenSubmenu => app.submenu_right(),
        Key::Power(options::PowerStep::Lower) => app.slider_lower(),
        Key::Power(options::PowerStep::Higher) => app.slider_higher(),
    }
    .map_err(|e| Error::Unavailable(format!("Model/effort key: {e}")))
}

/// Claude: the model panel's "Effort <level>" entry opens its levels on
/// hover, or on the Right arrow key while it is focused. Opens them that
/// way, presses `effort` and closes the panel. Success is only ever decided
/// by a later read.
async fn press_submenu_effort(scope: &impl Scope, effort: &str) -> Result<(), Error> {
    let (panel, (tree, elements)) = open(scope, Kind::Model).await?;
    let result = submenu_level(scope, effort, &tree, &elements, panel).await;
    if result.as_ref().is_ok_and(|pressed| *pressed) {
        tokio::time::sleep(Duration::from_millis(300)).await;
    }
    let close = close(scope, Kind::Model).await;
    result?;
    close
}

async fn submenu_level(
    scope: &impl Scope,
    effort: &str,
    tree: &chat::AxTree,
    elements: &[Element],
    panel: usize,
) -> Result<bool, Error> {
    let entries = chat::menu_entries(tree, panel);
    let Some(entry) = chat::effort_entry(&entries) else {
        return Err(options::create_no_effort(effort));
    };
    if options::submenu_effort(tree, panel).as_deref() == Some(effort) {
        return Ok(false);
    }
    let labels: Vec<String> = entries.iter().map(|e| e.label.clone()).collect();
    focused_key(scope, &elements[entry.node], Key::OpenSubmenu).await?;
    // Decide on a list two reads agree on: levels may still be mounting.
    let step = Step::new("effort levels", Duration::from_secs(2));
    let mut previous: Option<Vec<String>> = None;
    loop {
        let (tree, elements) = snapshot(scope).await?;
        let levels = options::effort_levels(&tree, &labels);
        let names: Vec<String> = levels.iter().map(|c| c.name.clone()).collect();
        if !levels.is_empty() && previous.as_ref() == Some(&names) {
            return match options::create_choice(&levels, Kind::Effort, effort)? {
                Some(level) => press(scope, &elements[level.node]).map(|()| true),
                None => Ok(false),
            };
        }
        previous = Some(names);
        if step.expired() {
            return Err(options::create_levels_closed(effort));
        }
        step.check()?;
        tokio::time::sleep(ECHO_POLL).await;
    }
}

/// The level the ChatGPT app's open Power panel states, and its "Power"
/// item, from a fresh read. The picker is found afresh too: a combined
/// picker's label names the level ("GPT-6 Astra High"), and so does its
/// panel's title.
async fn read_power(scope: &impl Scope) -> Result<(chat::PowerLevel, Element), Error> {
    let (tree, elements) = snapshot(scope).await?;
    let index = options::picker(&tree, Kind::Effort)?;
    let picker = chat::Picker {
        node: index,
        label: tree.nodes[index].label().to_owned(),
    };
    options::panel(&tree, &picker)?
        .and_then(|panel| {
            let (level, _) = options::power(&tree, panel)?;
            Some((level, elements[options::power_entry(&tree, panel)?].clone()))
        })
        .ok_or_else(|| options::create_unavailable("the Power panel closed"))
}

/// The ChatGPT app's Power slider: walks it with arrow keys on its exactly
/// focused "Power" item, reading the level the app states after each key,
/// until it states `effort` ([`options::PowerWalk`]).
async fn walk_power(scope: &impl Scope, effort: &str) -> Result<(), Error> {
    let (mut level, mut item) = read_power(scope).await?;
    let mut walk = options::PowerWalk::new(&level, effort);
    while let Some(step) = walk.next(&level)? {
        focused_key(scope, &item, Key::Power(step)).await?;
        let wait = Step::new("Power level", Duration::from_secs(2));
        loop {
            let (now, now_item) = read_power(scope).await?;
            if now.position != level.position {
                (level, item) = (now, now_item);
                break;
            }
            if wait.expired() {
                return Err(options::create_unavailable(format!(
                    "the Power slider stayed at {:?}",
                    level.name
                )));
            }
            wait.check()?;
            tokio::time::sleep(ECHO_POLL).await;
        }
    }
    Ok(())
}

/// A form's own effort picker (the ChatGPT app): its Power slider, or a
/// menu of levels. Presses or walks to `effort` and closes the panel.
async fn press_effort(scope: &impl Scope, effort: &str) -> Result<(), Error> {
    let (panel, (tree, elements)) = open(scope, Kind::Effort).await?;
    let result = if options::power(&tree, panel).is_some() {
        walk_power(scope, effort).await
    } else {
        let pressed = options::choices(&tree, panel, Kind::Effort).and_then(|choices| {
            match options::create_choice(&choices, Kind::Effort, effort)? {
                Some(choice) => press(scope, &elements[choice.node]).map(|()| true),
                None => Ok(false),
            }
        });
        if pressed.as_ref().is_ok_and(|pressed| *pressed) {
            tokio::time::sleep(Duration::from_millis(300)).await;
        }
        pressed.map(|_| ())
    };
    let close = close(scope, Kind::Effort).await;
    result?;
    close
}

/// Chooses a new session's model and effort on its form and reads both back
/// before anything is drafted (ADR 0177). Empty names keep the app's choice.
/// A name the app does not offer refuses with `options::CREATE_REFUSED`.
/// Returns what the pickers show, for one last read-only comparison just
/// before the first message.
pub(in crate::macos) async fn prepare_new(
    scope: &impl Scope,
    model: &str,
    effort: &str,
) -> Result<Option<String>, Error> {
    if model.is_empty() && effort.is_empty() {
        return Ok(None);
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
    // The model can change the offered efforts, and Codex's picker labels:
    // decide on fresh reads.
    let selectable_effort = effort_picker(scope).await?;
    if !effort.is_empty() {
        if selectable_effort {
            press_effort(scope, effort)
                .await
                .map_err(|error| match error {
                    // A model-only picker: asking again cannot find an effort.
                    Error::Unavailable(why) if why == NO_EFFORT_CONTROLS => {
                        options::create_no_effort(effort)
                    }
                    other => other,
                })?;
        } else {
            press_submenu_effort(scope, effort).await?;
        }
    }
    // Read both back; never infer success from a press or a key.
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
        let (confirmed, current) = if selectable_effort {
            let choices = read(scope, Kind::Effort).await?;
            let current = choices.iter().find(|c| c.selected).map(|c| c.name.clone());
            (options::confirmed(&choices, Kind::Effort, effort), current)
        } else {
            let current = submenu_effort(scope).await?;
            (current.as_deref() == Some(effort), current)
        };
        if !confirmed {
            return Err(options::create_refused(format!(
                "the app did not confirm effort {effort:?} (it shows {:?})",
                current.unwrap_or_default()
            )));
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
