//! AX adapter for the coding composer's model and effort menus: a session's
//! web area, or a new session's form before its first message (ADR 0181).
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
        // A finished read decides even after the deadline (`Step::until`).
        let snapshot = snapshot(scope).await?;
        let panel = options::opened_panel(&snapshot.0, picker, kind)?;
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
    // 2 s, as for the menu to open (`wait_menu`): every read re-proves the
    // scope, which on a new-session form walks the page twice besides the
    // snapshot, about 250 ms a read. A 1 s budget allowed three or four
    // reads, the first right after the press, and timed out (1056-1137 ms)
    // on a Mac whose Claude was busy (ADR 0194).
    let picker = &picker;
    Step::new("close model/effort menu", Duration::from_secs(2))
        .until(ECHO_POLL, || async move {
            let (tree, _) = snapshot(scope).await?;
            Ok(options::panel(&tree, picker)?.is_none().then_some(()))
        })
        .await
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
    PowerDone,
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
        Key::PowerDone => app.slider_done(),
    }
    .map_err(|e| Error::Unavailable(format!("Model/effort key: {e}")))
}

/// Claude: the model panel's "Effort <level>" entry opens its levels on
/// hover, or on the Right arrow key while it is focused. Opens them that
/// way, presses `effort` and closes the panel. Success is only ever decided
/// by a later read; the levels it read (none when the entry already named
/// `effort`) are what that read is checked against.
async fn press_submenu_effort(
    scope: &impl Scope,
    effort: &str,
) -> Result<Vec<chat::ChatChoice>, Error> {
    let (panel, (tree, elements)) = open(scope, Kind::Model).await?;
    let result = submenu_level(scope, effort, &tree, &elements, panel).await;
    if result.as_ref().is_ok_and(|(pressed, _)| *pressed) {
        tokio::time::sleep(Duration::from_millis(300)).await;
    }
    let close = close(scope, Kind::Model).await;
    let (_, levels) = result?;
    close.map(|()| levels)
}

async fn submenu_level(
    scope: &impl Scope,
    effort: &str,
    tree: &chat::AxTree,
    elements: &[Element],
    panel: usize,
) -> Result<(bool, Vec<chat::ChatChoice>), Error> {
    let entries = chat::menu_entries(tree, panel);
    let Some(entry) = chat::effort_entry(&entries) else {
        return Err(options::create_no_effort(effort));
    };
    if options::submenu_effort(tree, panel).as_deref() == Some(effort) {
        return Ok((false, Vec::new()));
    }
    let title = entry.label.clone();
    focused_key(scope, &elements[entry.node], Key::OpenSubmenu).await?;
    // Decide on a list two reads agree on: levels may still be mounting.
    let step = Step::new("effort levels", Duration::from_secs(2));
    let mut previous: Option<Vec<String>> = None;
    loop {
        let (tree, elements) = snapshot(scope).await?;
        let levels = options::effort_levels(&tree, &title);
        let names: Vec<String> = levels.iter().map(|c| c.name.clone()).collect();
        if !levels.is_empty() && previous.as_ref() == Some(&names) {
            let pressed = match options::create_level(&levels, effort)? {
                Some(level) => press(scope, &elements[level.node]).map(|()| true)?,
                None => false,
            };
            return Ok((pressed, levels));
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
/// item, from a fresh read. The picker is found afresh too: its label
/// changes with the level, and while the panel is open the app names both
/// the picker and the panel "Select effort".
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
/// until it states `effort` ([`options::PowerWalk`]). Returns the app's own
/// name for that level.
async fn walk_power(scope: &impl Scope, effort: &str) -> Result<String, Error> {
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
    Ok(level.name)
}

/// The open Power panel's "Power" item, or `None` once the panel is closed.
async fn power_item(scope: &impl Scope) -> Result<Option<Element>, Error> {
    let (tree, elements) = snapshot(scope).await?;
    let index = options::picker(&tree, Kind::Effort)?;
    let picker = chat::Picker {
        node: index,
        label: tree.nodes[index].label().to_owned(),
    };
    Ok(options::panel(&tree, &picker)?
        .and_then(|panel| options::power_entry(&tree, panel))
        .map(|node| elements[node].clone()))
}

/// Closes the Power panel the way the app's own keyboard handling does:
/// Return on the exactly focused "Power" item. After Power keys, the
/// ChatGPT app 26.1103 leaves its panel open when its picker is pressed.
async fn finish_power(scope: &impl Scope) -> Result<(), Error> {
    let Some(item) = power_item(scope).await? else {
        return Ok(());
    };
    focused_key(scope, &item, Key::PowerDone).await?;
    let step = Step::new("close Power panel", Duration::from_secs(2));
    loop {
        step.check()?;
        if power_item(scope).await?.is_none() {
            return Ok(());
        }
        step.check()?;
        tokio::time::sleep(ECHO_POLL).await;
    }
}

/// Closes a new form's effort panel. A Power panel takes Return on its
/// focused "Power" item ([`finish_power`]): the ChatGPT app 26.1103 leaves
/// it open when its picker is pressed. After local input the guard refuses
/// both, and nothing is pressed.
async fn close_effort(scope: &impl Scope, power: bool) -> Result<(), Error> {
    if power {
        finish_power(scope).await?;
    }
    close(scope, Kind::Effort).await
}

/// A fresh read of a new form's effort choices, closed by [`close_effort`].
async fn read_effort(scope: &impl Scope) -> Result<Vec<chat::ChatChoice>, Error> {
    let (menu, (tree, _)) = open(scope, Kind::Effort).await?;
    let choices = options::choices(&tree, menu, Kind::Effort);
    let closed = close_effort(scope, options::power(&tree, menu).is_some()).await;
    let choices = choices?;
    closed?;
    if choices.is_empty() {
        return Err(Error::Unavailable(
            "The app exposed no selectable choices".into(),
        ));
    }
    Ok(choices)
}

/// A form's own effort picker (the ChatGPT app): its Power slider, or a
/// menu of levels. Presses or walks to `effort`, closes the panel, and
/// returns the name a fresh read must show for it.
async fn press_effort(scope: &impl Scope, effort: &str) -> Result<String, Error> {
    let (panel, (tree, elements)) = open(scope, Kind::Effort).await?;
    let walked = options::power(&tree, panel).is_some();
    let result = if walked {
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
        pressed.map(|_| effort.to_owned())
    };
    let closed = close_effort(scope, walked).await;
    let chosen = result?;
    closed?;
    Ok(chosen)
}

/// Chooses a new session's model and effort on its form and reads both back
/// before anything is drafted (ADR 0181). Empty names keep the app's choice.
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
    let mut levels = Vec::new();
    let mut chosen = effort.to_owned();
    if !effort.is_empty() {
        if selectable_effort {
            chosen = press_effort(scope, effort)
                .await
                .map_err(|error| match error {
                    // A model-only picker: asking again cannot find an effort.
                    Error::Unavailable(why) if why == NO_EFFORT_CONTROLS => {
                        options::create_no_effort(effort)
                    }
                    other => other,
                })?;
        } else {
            levels = press_submenu_effort(scope, effort).await?;
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
            let choices = read_effort(scope).await?;
            let current = choices.iter().find(|c| c.selected).map(|c| c.name.clone());
            (options::confirmed(&choices, Kind::Effort, &chosen), current)
        } else {
            let current = submenu_effort(scope).await?;
            let confirmed = current
                .as_deref()
                .is_some_and(|shown| options::level_confirmed(&levels, effort, shown));
            (confirmed, current)
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
