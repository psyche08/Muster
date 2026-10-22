//! Session-bound AX adapter for the coding composer's model and effort menus.
use super::*;
use muster_agent::{
    desktop_chat as chat,
    native_options::{self as options, Kind},
};
use muster_proto::v1;

type Snapshot = (chat::AxTree, Vec<Element>);

impl NativeAppController {
    fn options_guard(&self, generation: u64, web: &Element) -> Result<(), Error> {
        muster_agent::control_guard::check()?;
        if screen_locked() || user_input_generation() != generation {
            return Err(Error::Conflict(
                "Unlock the Mac and leave its input idle while changing model or effort".into(),
            ));
        }
        self.check_identity()?;
        if !self.app.contains(web)
            || (self.resolver)(&WebAreaKey::of(web)).as_deref() != Some(self.session.as_str())
        {
            return Err(Error::Conflict(
                "The session view changed; no further option was pressed".into(),
            ));
        }
        Ok(())
    }

    async fn options_snapshot(&self, generation: u64, web: &Element) -> Result<Snapshot, Error> {
        self.options_guard(generation, web)?;
        let root = web.clone();
        let snap = muster_agent::control_guard::spawn_blocking(move || {
            super::super::native_chat::snapshot(&root, false)
        })
        .await
        .map_err(|e| Error::Unavailable(format!("Reading model controls: {e}")))?;
        self.options_guard(generation, web)?;
        if !snap.0.complete {
            return Err(Error::Unavailable(
                "The session controls could not be read completely".into(),
            ));
        }
        Ok(snap)
    }

    fn options_press(
        &self,
        generation: u64,
        web: &Element,
        element: &Element,
    ) -> Result<(), Error> {
        self.options_guard(generation, web)?;
        if !web.contains(element) {
            return Err(Error::Conflict(
                "The option left the requested session".into(),
            ));
        }
        element
            .press()
            .map_err(|e| Error::Unavailable(format!("Model/effort control: {e}")))
    }

    async fn options_menu(
        &self,
        generation: u64,
        web: &Element,
        kind: Kind,
    ) -> Result<(usize, Snapshot), Error> {
        let (tree, elements) = self.options_snapshot(generation, web).await?;
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
        self.options_press(generation, web, &elements[index])?;
        let result = self.options_wait_menu(generation, web, &picker, kind).await;
        if result.is_err() {
            // Best-effort cleanup only after our own successful open. If local
            // input or session identity changed the guard prohibits cleanup.
            let _ = self.options_close(generation, web, kind).await;
        }
        result
    }

    async fn options_wait_menu(
        &self,
        generation: u64,
        web: &Element,
        picker: &chat::Picker,
        kind: Kind,
    ) -> Result<(usize, Snapshot), Error> {
        let mut step = Step::new("model/effort menu", Duration::from_secs(2));
        let mut entered_models = false;
        loop {
            step.check()?;
            let snapshot = self.options_snapshot(generation, web).await?;
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
                        self.options_press(generation, web, &snapshot.1[select.node])?;
                        entered_models = true;
                        step = Step::new("model submenu", Duration::from_secs(2));
                    }
                } else if !entries.is_empty() {
                    if kind == Kind::Effort && !options::effort_trigger(&picker.label) {
                        return Err(Error::Unavailable(
                            "This model picker exposes no effort controls".into(),
                        ));
                    }
                    return Ok((panel, snapshot));
                }
            }
            step.check()?;
            tokio::time::sleep(ECHO_POLL).await;
        }
    }

    async fn options_close(&self, generation: u64, web: &Element, kind: Kind) -> Result<(), Error> {
        let (tree, elements) = self.options_snapshot(generation, web).await?;
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
        self.options_press(generation, web, &elements[index])?;
        let step = Step::new("close model/effort menu", Duration::from_secs(1));
        loop {
            step.check()?;
            let (tree, _) = self.options_snapshot(generation, web).await?;
            step.check()?;
            if options::panel(&tree, &picker)?.is_none() {
                return Ok(());
            }
            step.check()?;
            tokio::time::sleep(ECHO_POLL).await;
        }
    }

    async fn options_read(
        &self,
        generation: u64,
        web: &Element,
        kind: Kind,
    ) -> Result<Vec<chat::ChatChoice>, Error> {
        let (menu, (tree, _)) = self.options_menu(generation, web, kind).await?;
        let result = options::choices(&tree, menu, kind);
        // Only dismiss menus this operation successfully opened. A refusal
        // because a user menu was already open must leave it alone.
        let close = self.options_close(generation, web, kind).await;
        let choices = result?;
        close?;
        if choices.is_empty() {
            return Err(Error::Unavailable(
                "The app exposed no selectable choices".into(),
            ));
        }
        Ok(choices)
    }

    async fn options_set(
        &self,
        generation: u64,
        web: &Element,
        kind: Kind,
        name: &str,
    ) -> Result<(), Error> {
        let (menu, (tree, elements)) = self.options_menu(generation, web, kind).await?;
        let result: Result<(), Error> = (|| {
            let choices = options::choices(&tree, menu, kind)?;
            let choice = options::choose(&choices, name)?;
            if !choice.selected {
                self.options_press(generation, web, &elements[choice.node])?;
            }
            Ok(())
        })();
        if result.is_ok() {
            tokio::time::sleep(Duration::from_millis(300)).await;
        }
        let close = self.options_close(generation, web, kind).await;
        result?;
        close?;
        let choices = self.options_read(generation, web, kind).await?;
        let selected = options::confirmed(&choices, kind, name);
        if !selected {
            return Err(Error::UnknownOutcome(
                "The app did not confirm the requested option; refresh before retrying".into(),
            ));
        }
        Ok(())
    }

    pub(super) async fn session_options(
        &self,
        set: Option<(&str, &str)>,
    ) -> Result<v1::AgentChatOptions, Error> {
        let _write = self
            .write
            .try_lock()
            .map_err(|_| Error::Conflict("Another native operation is in progress".into()))?;
        self.check_identity()?;
        let web = self.surface.lock().unwrap().web_area.clone();
        let generation = user_input_generation();
        self.options_guard(generation, &web)?;
        if let Some((model, effort)) = set {
            if !model.is_empty() {
                self.options_set(generation, &web, Kind::Model, model)
                    .await?;
            }
            if !effort.is_empty() {
                // Model changes can change the offered effort levels. Validate
                // against that model's fresh menu, never the previous one.
                if let Err(error) = self
                    .options_set(generation, &web, Kind::Effort, effort)
                    .await
                {
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
        let models = self.options_read(generation, &web, Kind::Model).await?;
        let (efforts, note) = match self.options_read(generation, &web, Kind::Effort).await {
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
        self.options_guard(generation, &web)?;
        Ok(v1::AgentChatOptions {
            session: self.session.clone(),
            models: convert(models),
            efforts: convert(efforts),
            effort,
            note,
        })
    }
}
