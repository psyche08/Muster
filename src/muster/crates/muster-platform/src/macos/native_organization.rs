//! OS-only execution of Rust's validated native organization plan.
use super::*;
use muster_agent::{native_organization as selectors, organization::Plan};
use muster_proto::v1::agent_organize_request::Action;

type OrganizationSnapshot = (muster_agent::desktop_chat::AxTree, Vec<Element>);
impl NativeAppController {
    fn organization_guard(
        &self,
        generation: u64,
        web: &Element,
        window: &Element,
    ) -> Result<(), Error> {
        muster_agent::control_guard::check()?;
        self.check_identity()?;
        if screen_locked()
            || user_input_generation() != generation
            || !self.app.contains(window)
            || !window.contains(web)
            || (self.resolver)(&WebAreaKey::of(web)).as_deref() != Some(self.session.as_str())
        {
            return Err(Error::Conflict(
                "The native session or local input changed; no further controls were pressed"
                    .into(),
            ));
        }
        Ok(())
    }
    async fn organization_snapshot(
        &self,
        g: u64,
        web: &Element,
        window: &Element,
    ) -> Result<OrganizationSnapshot, Error> {
        self.organization_guard(g, web, window)?;
        let root = window.clone();
        let snap = muster_agent::control_guard::spawn_blocking(move || {
            super::super::native_chat::snapshot(&root, false)
        })
        .await
        .map_err(|e| Error::Unavailable(e.to_string()))?;
        self.organization_guard(g, web, window)?;
        if !snap.0.complete {
            return Err(Error::Unavailable(
                "Incomplete native organization controls".into(),
            ));
        }
        Ok(snap)
    }
    fn organization_press(
        &self,
        g: u64,
        web: &Element,
        window: &Element,
        element: &Element,
        mutation: bool,
    ) -> Result<(), Error> {
        self.organization_guard(g, web, window)?;
        if !window.contains(element) {
            return Err(Error::Conflict("Control left the requested window".into()));
        }
        if mutation {
            muster_agent::control_guard::delivery_attempt();
        }
        element
            .press()
            .map_err(|e| Error::Unavailable(format!("Native organization AXPress: {e}")))
    }
    async fn organization_menu_action(
        &self,
        g: u64,
        web: &Element,
        window: &Element,
        labels: &[&str],
        mutation: bool,
    ) -> Result<(), Error> {
        let step = Step::new("native organization menu", Duration::from_secs(2));
        loop {
            step.check()?;
            let (tree, elements) = self.organization_snapshot(g, web, window).await?;
            if let Ok(menu) = selectors::menu(&tree) {
                match selectors::action(&tree, menu, labels) {
                    Ok(item) => {
                        return self.organization_press(g, web, window, &elements[item], mutation);
                    }
                    Err(Error::Unavailable(_)) => {} // Submenu may not have mounted yet.
                    Err(error) => return Err(error),
                }
            }
            tokio::time::sleep(ECHO_POLL).await;
        }
    }
    pub(super) async fn session_organize(&self, plan: &Plan) -> Result<(), Error> {
        let _write = self
            .write
            .try_lock()
            .map_err(|_| Error::Conflict("Another native operation is in progress".into()))?;
        if plan.request.session != self.session {
            return Err(Error::Conflict("Organization session changed".into()));
        }
        let web = self.surface.lock().unwrap().web_area.clone();
        let window = web
            .element("AXWindow")
            .ok_or_else(|| Error::Unavailable("The session exposes no AX window".into()))?;
        let g = user_input_generation();
        let (tree, elements) = self.organization_snapshot(g, &web, &window).await?;
        if !muster_agent::desktop_chat::open_menus(&tree).is_empty()
            || selectors::safe_dialog(&tree).is_ok()
        {
            return Err(Error::Conflict(
                "Close the native menu or dialog before organizing sessions".into(),
            ));
        }
        if plan.action == Action::CreateSection {
            if let Ok(trigger) = selectors::action(&tree, 0, &["New section", "Add section"]) {
                self.organization_press(g, &web, &window, &elements[trigger], false)?;
            } else {
                let trigger = selectors::create_trigger(&tree)?;
                self.organization_press(g, &web, &window, &elements[trigger], false)?;
                self.organization_menu_action(
                    g,
                    &web,
                    &window,
                    &["New section", "Add section"],
                    false,
                )
                .await?;
            }
            let step = Step::new("new section dialog", Duration::from_secs(2));
            loop {
                step.check()?;
                let (tree, elements) = self.organization_snapshot(g, &web, &window).await?;
                if let Ok(dialog) = selectors::safe_dialog(&tree) {
                    let field =
                        selectors::unique(&tree, dialog, &["Section name"], &["AXTextField"])?;
                    self.organization_guard(g, &web, &window)?;
                    elements[field]
                        .set_string("AXValue", &plan.request.name)
                        .map_err(|e| Error::Unavailable(format!("Section name: {e}")))?;
                    if elements[field].string("AXValue").as_deref() != Some(&plan.request.name) {
                        return Err(Error::Conflict("Section name readback differs".into()));
                    }
                    // React enables Create only after the name input event.
                    // Resolve its current AX element after typing, not before.
                    loop {
                        step.check()?;
                        let (fresh, controls) =
                            self.organization_snapshot(g, &web, &window).await?;
                        let dialog = selectors::safe_dialog(&fresh)?;
                        let field =
                            selectors::unique(&fresh, dialog, &["Section name"], &["AXTextField"])?;
                        if fresh.nodes[field].value != plan.request.name {
                            return Err(Error::Conflict(
                                "Section name changed before creation".into(),
                            ));
                        }
                        if let Ok(create) = selectors::action(&fresh, dialog, &["Create section"]) {
                            return self.organization_press(
                                g,
                                &web,
                                &window,
                                &controls[create],
                                true,
                            );
                        }
                        tokio::time::sleep(ECHO_POLL).await;
                    }
                }
                tokio::time::sleep(ECHO_POLL).await;
            }
        }
        if plan.action == Action::DeleteSection {
            // Only an explicitly labelled group popup, never the session's Delete.
            let label = format!("More options for {}", plan.section_title);
            let sidebar = selectors::sidebar(&tree)?;
            if let Ok(trigger) = selectors::unique(&tree, sidebar, &[&label], &["AXPopUpButton"]) {
                self.organization_press(g, &web, &window, &elements[trigger], false)?;
            } else {
                let heading = selectors::section_heading(&tree, &plan.section_title)?;
                self.organization_guard(g, &web, &window)?;
                if !elements[heading].has_action("AXShowMenu") {
                    return Err(Error::Unavailable(
                        "The native section exposes no AX context menu".into(),
                    ));
                }
                elements[heading]
                    .perform("AXShowMenu")
                    .map_err(|e| Error::Unavailable(format!("Native section menu: {e}")))?;
            }
            let labels: &[&str] = if plan.section_kind == "project" {
                &["Remove project"]
            } else {
                &["Delete section", "Remove section"]
            };
            self.organization_menu_action(g, &web, &window, labels, true)
                .await?;
            tokio::time::sleep(Duration::from_millis(250)).await;
            let (confirmation, controls) = self.organization_snapshot(g, &web, &window).await?;
            if let Ok(button) = selectors::deletion_confirmation(&confirmation) {
                self.organization_press(g, &web, &window, &controls[button], true)?;
            }
            return Ok(());
        }
        // Navigation may use a unique label only to reveal the target. The
        // following UUID guard must succeed before any context action.
        let (tree, elements, row) = match selectors::row(&tree, &plan.native_id, &plan.title) {
            Ok(row) => (tree, elements, row),
            Err(_) => {
                let candidate = selectors::navigation_candidate(&tree, &plan.title)?;
                self.organization_press(g, &web, &window, &elements[candidate], false)?;
                tokio::time::sleep(Duration::from_millis(200)).await;
                let (fresh, controls) = self.organization_snapshot(g, &web, &window).await?;
                let candidate = selectors::navigation_candidate(&fresh, &plan.title)?;
                (fresh, controls, candidate)
            }
        };
        let labels: &[&str] = match plan.action {
            Action::Pin => &["Pin chat", "Pin"],
            Action::Unpin => &["Unpin chat", "Unpin"],
            Action::Archive => &["Archive chat", "Archive"],
            Action::Move if plan.section_kind == "project" => &["Move to project"],
            Action::Move => &["Move to section"],
            Action::Remove if plan.section_kind == "project" => &["Remove from project"],
            Action::Remove => &["Remove from section"],
            _ => unreachable!(),
        };
        if let Ok(control) = selectors::action(&tree, row, labels) {
            self.organization_press(
                g,
                &web,
                &window,
                &elements[control],
                plan.action != Action::Move,
            )?;
        } else {
            // Claude's row popup may be a sibling in its containing row group.
            let label = format!("More options for {}", plan.title);
            let parent = tree.nodes[row].parent.unwrap_or(row);
            let popup =
                selectors::unique(&tree, parent, &[&label, "More options"], &["AXPopUpButton"]);
            if let Ok(popup) = popup {
                self.organization_press(g, &web, &window, &elements[popup], false)?;
            } else if elements[row].has_action("AXShowMenu") {
                self.organization_guard(g, &web, &window)?;
                elements[row]
                    .perform("AXShowMenu")
                    .map_err(|e| Error::Unavailable(format!("Native row menu: {e}")))?;
            } else {
                return Err(Error::Unavailable(
                    "The selected UUID row exposes no AX context menu".into(),
                ));
            }
            self.organization_menu_action(g, &web, &window, labels, plan.action != Action::Move)
                .await?;
        }
        if plan.action == Action::Move {
            self.organization_menu_action(g, &web, &window, &[&plan.section_title], true)
                .await?;
        }
        Ok(())
    }
}
