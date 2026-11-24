//! The flat sidebar's New chat and the web picker on the resulting home form.
use super::*;
use muster_agent::native_runtime::codex_sidebar_project;

pub(super) fn new_chat(tree: &[Element]) -> Result<Option<Element>, Error> {
    if tree.iter().any(is_search) {
        return Ok(None);
    }
    // The Recents header also has a New chat button. The primary action
    // shares a group with the Codex mode switch inside sidebar navigation.
    let mut modes = tree
        .iter()
        .filter(|n| n.role() == "AXPopUpButton" && label(n, "Switch mode, current mode: Codex"));
    let Some(group) = modes.next().and_then(|n| n.element("AXParent")) else {
        return Ok(None);
    };
    if modes.next().is_some()
        || !group
            .element("AXParent")
            .is_some_and(|n| n.string("AXSubrole").as_deref() == Some("AXLandmarkNavigation"))
    {
        return Ok(None);
    }
    unique(&group, |n| n.role() == "AXButton" && label(n, "New chat"))
}

fn is_search(n: &Element) -> bool {
    n.role() == "AXComboBox"
        && (label(n, "Search projects")
            || n.string("AXPlaceholderValue").as_deref() == Some("Search projects"))
}

fn exact_choice<T>(
    name: &str,
    rows: impl Iterator<Item = (String, T)>,
    parent_text: impl Fn(&T) -> Option<String>,
) -> Result<Option<T>, Error> {
    single(rows.filter_map(|(label, item)| {
        (label == name && parent_text(&item).as_deref() != Some(name)).then_some(item)
    }))
}

const SETTLE: Duration = Duration::from_secs(3);

async fn until<T>(
    mut probe: impl FnMut() -> Result<Option<T>, Error>,
    timeout: &'static str,
) -> Result<T, Error> {
    let deadline = Instant::now() + SETTLE;
    loop {
        if let Some(value) = probe()? {
            return Ok(value);
        }
        if Instant::now() >= deadline {
            return Err(unavailable(timeout));
        }
        tokio::time::sleep(Duration::from_millis(100)).await;
    }
}

// This picker is an HTML popover, not an AXMenu. Keep all mutations on
// elements of the same home page, including error recovery.
struct Picker<'a> {
    guard: &'a Guard,
    window: &'a Element,
    page: &'a Element,
    selector: &'a Element,
    dialog: Option<Element>,
}

fn has_other_dialog<'a, T: PartialEq + 'a>(
    mut dialogs: impl Iterator<Item = &'a T>,
    owned: Option<&T>,
) -> bool {
    dialogs.any(|dialog| Some(dialog) != owned)
}

impl Picker<'_> {
    fn check(&self) -> Result<(), Error> {
        self.guard.check()?;
        let tree = nodes(self.window)?;
        if !windows(&Element::application(self.guard.pid)).contains(self.window)
            || !tree.contains(self.page)
            || !new_page("codex", self.page)
            || !codex_home_heading(&tree)
            || has_other_dialog(tree.iter().filter(|n| is_dialog(n)), self.dialog.as_ref())
            || !draft_free_window(self.guard.profile, self.window, &tree)
        {
            return Err(conflict("Codex project picker lost its empty home form"));
        }
        Ok(())
    }

    fn search(&self) -> Result<Option<Element>, Error> {
        unique(self.page, is_search)
    }

    fn toggle(&self) -> Result<(), Error> {
        self.check()?;
        if !self.page.contains(self.selector) {
            return Err(conflict("Codex project selector was replaced"));
        }
        self.guard.press(self.selector)
    }

    fn write(&self, field: &Element, value: &str) -> Result<(), Error> {
        self.check()?;
        if self.search()?.as_ref() != Some(field) {
            return Err(conflict("Codex project search was replaced"));
        }
        self.guard.check()?;
        field
            .set_string("AXValue", value)
            .map_err(|_| unavailable("Codex project search refused the value"))
    }

    fn choice(&self, field: &Element, name: &str) -> Result<Option<Element>, Error> {
        // The sidebar has unrelated AXLists. Search only the nearest group
        // containing both the combobox and its list, below the page shell.
        let mut parent = field.element("AXParent");
        for _ in 0..MAX_ANCESTRY {
            let Some(group) = parent else { break };
            if group == *self.page || group.role() == "AXWindow" {
                break;
            }
            let tree = nodes(&group)?;
            if tree
                .iter()
                .any(|n| n == self.selector || session_composer(self.guard.profile, n))
            {
                break;
            }
            let mut lists = tree.iter().filter(|n| n.role() == "AXList");
            if let Some(list) = lists.next() {
                if lists.next().is_some() {
                    return Err(conflict("Ambiguous Codex project lists"));
                }
                return exact_choice(
                    name,
                    nodes(list)?.into_iter().filter_map(|n| {
                        if n.role() != "AXStaticText" {
                            return None;
                        }
                        Some((n.string("AXValue")?, n))
                    }),
                    |n| {
                        n.element("AXParent")
                            .filter(|p| p.role() == "AXStaticText")?
                            .string("AXValue")
                    },
                );
            }
            parent = group.element("AXParent");
        }
        Ok(None)
    }

    fn closed(&self) -> Result<bool, Error> {
        self.guard.check()?;
        Ok(self.search()?.is_none() && self.selector.bool("AXExpanded") != Some(true))
    }
}

trait Recovery {
    fn search_text(&self) -> Result<Option<String>, Error>;
    fn clear(&self) -> Result<(), Error>;
    fn close(&self) -> Result<(), Error>;
    fn closed(&self) -> Result<bool, Error>;
}

impl Recovery for Picker<'_> {
    fn search_text(&self) -> Result<Option<String>, Error> {
        self.check()?;
        self.search()?
            .map(|n| {
                n.string("AXValue")
                    .ok_or_else(|| unavailable("Codex project search is unreadable"))
            })
            .transpose()
    }
    fn clear(&self) -> Result<(), Error> {
        let field = self
            .search()?
            .ok_or_else(|| conflict("Codex project search disappeared"))?;
        self.write(&field, "")
    }
    fn close(&self) -> Result<(), Error> {
        self.toggle()
    }
    fn closed(&self) -> Result<bool, Error> {
        Picker::closed(self)
    }
}

async fn recover(menu: &impl Recovery, query: Option<&str>) -> Result<(), Error> {
    let current = menu.search_text()?;
    if let Some(value) = current.as_deref().filter(|s| !s.is_empty()) {
        if Some(value) != query {
            return Err(conflict(
                "Codex project search changed; preserving user input",
            ));
        }
        menu.clear()?;
        until(
            || {
                Ok(menu
                    .search_text()?
                    .is_none_or(|s| s.is_empty())
                    .then_some(()))
            },
            "Codex project search did not clear",
        )
        .await?;
    }
    if !menu.closed()? {
        menu.close()?;
        until(
            || Ok(menu.closed()?.then_some(())),
            "Codex project picker did not close",
        )
        .await?;
    }
    Ok(())
}

pub(super) async fn choose(
    guard: &Guard,
    window: &Element,
    page: &Element,
    selector: &Element,
    cwd: &Path,
    name: &str,
) -> Result<(), Error> {
    let mut menu = Picker {
        guard,
        window,
        page,
        selector,
        dialog: None,
    };
    let listed = || {
        missing_directory("codex", cwd)?;
        if codex_project_state()
            .and_then(|s| codex_sidebar_project(&s, cwd))
            .as_deref()
            != Some(name)
        {
            return Err(conflict(
                "Codex project is no longer unique for the directory",
            ));
        }
        Ok(())
    };
    listed()?;
    if !menu.closed()? {
        return Err(conflict("Codex project picker is already open"));
    }
    let mut query = None;
    let result = async {
        menu.toggle()?;
        let field = until(
            || {
                guard.check()?;
                menu.search()
            },
            "Codex project search did not open",
        )
        .await?;
        // Some Codex versions render the list in a web dialog. Bind only
        // the popup opened above, before writing the search or selecting.
        menu.dialog = field.element("AXParent").filter(|parent| {
            parent.role() == "AXGroup"
                && parent.string("AXSubrole").as_deref() == Some("AXApplicationDialog")
                && page.contains(parent)
        });
        if field.string("AXValue").as_deref() != Some("") {
            return Err(conflict("Codex project search is not empty"));
        }
        query = Some(name);
        menu.write(&field, name)?;
        let choice = until(
            || {
                guard.check()?;
                if field.string("AXValue").as_deref() != Some(name) {
                    return Ok(None);
                }
                menu.choice(&field, name)
            },
            "Codex project list did not show the requested project",
        )
        .await?;
        listed()?;
        menu.check()?;
        if menu.search()?.as_ref() != Some(&field)
            || field.string("AXValue").as_deref() != Some(name)
            || menu.choice(&field, name)?.as_ref() != Some(&choice)
        {
            return Err(conflict("Codex project choice changed before selection"));
        }
        guard.press(&choice)?;
        until(
            || Ok(menu.closed()?.then_some(())),
            "Codex project selection did not dismiss the picker",
        )
        .await
    }
    .await;
    if result.is_err() {
        recover(&menu, query).await?;
    }
    result
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::cell::{Cell, RefCell};

    #[test]
    fn only_the_pinned_search_dialog_is_allowed() {
        let picker = 1;
        let sheet_or_other_dialog = 2;
        assert!(has_other_dialog([&picker].into_iter(), None));
        assert!(!has_other_dialog([&picker].into_iter(), Some(&picker)));
        assert!(has_other_dialog(
            [&picker, &sheet_or_other_dialog].into_iter(),
            Some(&picker)
        ));
        assert!(has_other_dialog(
            [&sheet_or_other_dialog].into_iter(),
            Some(&picker)
        ));
    }

    #[test]
    fn only_a_single_exact_result_can_be_pressed() {
        let choose = |rows: &[&str]| {
            exact_choice(
                "Example",
                rows.iter()
                    .enumerate()
                    .map(|(id, label)| (label.to_string(), id)),
                |_| None,
            )
        };
        assert_eq!(choose(&["Example two", "Example"]).unwrap(), Some(1));
        assert_eq!(choose(&["example", "My Example"]).unwrap(), None);
        assert_eq!(choose(&[]).unwrap(), None);
        assert!(matches!(
            choose(&["Example", "Example"]),
            Err(Error::Conflict(_))
        ));
    }

    #[test]
    fn nested_text_is_one_entry_but_two_project_rows_are_ambiguous() {
        let choose = |count| {
            exact_choice(
                "Example",
                (0..count).map(|id| ("Example".to_owned(), id)),
                |id| (*id == 1).then(|| "Example".to_owned()),
            )
        };
        assert_eq!(choose(2).unwrap(), Some(0));
        assert!(matches!(choose(3), Err(Error::Conflict(_))));
    }

    struct Menu {
        query: RefCell<String>,
        open: Cell<bool>,
        actions: RefCell<Vec<&'static str>>,
    }

    impl Menu {
        fn open(text: &str) -> Self {
            Self {
                query: RefCell::new(text.into()),
                open: Cell::new(true),
                actions: RefCell::default(),
            }
        }
    }

    impl Recovery for Menu {
        fn search_text(&self) -> Result<Option<String>, Error> {
            Ok(self.open.get().then(|| self.query.borrow().clone()))
        }
        fn clear(&self) -> Result<(), Error> {
            self.actions.borrow_mut().push("clear");
            self.query.borrow_mut().clear();
            Ok(())
        }
        fn close(&self) -> Result<(), Error> {
            self.actions.borrow_mut().push("close");
            self.open.set(false);
            Ok(())
        }
        fn closed(&self) -> Result<bool, Error> {
            Ok(!self.open.get())
        }
    }

    #[tokio::test]
    async fn failed_selection_clears_its_query_before_closing_its_picker() {
        let menu = Menu::open("Example");
        recover(&menu, Some("Example")).await.unwrap();
        assert_eq!(*menu.actions.borrow(), ["clear", "close"]);
        assert!(menu.closed().unwrap());
        // Failure after dismissal cannot reopen the picker.
        recover(&menu, Some("Example")).await.unwrap();
        assert_eq!(*menu.actions.borrow(), ["clear", "close"]);
        let menu = Menu::open("");
        recover(&menu, None).await.unwrap();
        assert_eq!(*menu.actions.borrow(), ["close"]);
    }

    #[tokio::test]
    async fn recovery_preserves_a_changed_search() {
        let menu = Menu::open("User input");
        assert!(matches!(
            recover(&menu, Some("Example")).await,
            Err(Error::Conflict(_))
        ));
        assert!(menu.actions.borrow().is_empty());
    }
}
