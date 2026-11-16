//! Desktop-first destination policy and native project evidence.
use crate::Error;
use std::path::{Path, PathBuf};

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Destination {
    Desktop,
    Cli(PathBuf),
    Unavailable,
}

/// Foreground belongs to the executor. An input pause in another app never
/// grants permission to launch, raise a window or dispatch an activating URL.
/// While the provider itself is in front it belongs to its user: an uncovered
/// creation waits until another app owns the foreground (ADR 0220).
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum NativeConsole {
    Covered,
    Provider,
    Background(u32),
    Waiting,
}

impl NativeConsole {
    /// A visible surface does not need permission to navigate. For a hidden one
    /// the original focus owner must still be known, or this must be covered.
    pub fn check_navigation(self, current: Self) -> Result<(), Error> {
        if self == Self::Waiting || self != current {
            return Err(Error::Conflict(
                "Foreground owner changed or is unavailable during session navigation".into(),
            ));
        }
        Ok(())
    }

    /// Whether native creation may act on the provider now. Only a covered
    /// console or another application's foreground qualifies (ADR 0220).
    pub fn may_create(self) -> bool {
        matches!(self, Self::Covered | Self::Background(_))
    }

    /// Called after looking for an eligible form. A provider permit protects UI
    /// actions, not a wait for its owner to switch applications.
    pub fn check_wait(self, provider_held: bool, provider: &str) -> Result<(), Error> {
        if !provider_held {
            return Ok(());
        }
        let why = match self {
            Self::Covered => return Ok(()),
            Self::Provider => format!(
                "{provider} is the active app; the session starts after you switch to another app or lock the Mac"
            ),
            Self::Background(_) => {
                format!("{provider} has no background route to a new session yet")
            }
            Self::Waiting => format!("No app owns the foreground, so {provider} was not touched"),
        };
        Err(Error::Conflict(format!("{why}; nothing was submitted")))
    }

    pub fn observe(covered: bool, provider: Option<u32>, focus: crate::local_input::Owner) -> Self {
        use crate::local_input::Owner;
        if covered {
            return Self::Covered;
        }
        match (provider, focus) {
            (Some(pid), Owner::Process(front)) if pid == front => Self::Provider,
            (Some(_), Owner::Process(front)) => Self::Background(front),
            _ => Self::Waiting,
        }
    }
}

/// Only a window allocated by this daemon in this provider process can be
/// reclaimed for another creation. Window order and focus are not identities.
pub struct CreationWindow<T> {
    process: (u32, u64),
    window: T,
}

impl<T: PartialEq> CreationWindow<T> {
    pub fn new(pid: u32, start: u64, window: T) -> Self {
        Self {
            process: (pid, start),
            window,
        }
    }

    pub fn live(&self, pid: u32, start: u64, windows: &[T]) -> Option<&T> {
        (self.process == (pid, start) && windows.contains(&self.window)).then_some(&self.window)
    }
}

/// Choose an opaque new-session window. A reusable main shell must have been
/// uniquely verified by the OS adapter before navigation; the caller must
/// prove its new-session page, directory/mode and empty composer before writing.
pub fn session_window<T: PartialEq + Clone>(
    before_activation: &[T],
    before_route: &[T],
    current: &[T],
    allow_activation: bool,
    reusable_main: Option<&T>,
) -> Result<Option<T>, Error> {
    let fresh: Vec<_> = current
        .iter()
        .filter(|w| !before_route.contains(w))
        .collect();
    if fresh.len() > 1 {
        return Err(Error::Conflict("New window identity is ambiguous".into()));
    }
    if let Some(window) = fresh.first() {
        return Ok(Some((*window).clone()));
    }
    if allow_activation
        && before_activation.is_empty()
        && before_route.len() == 1
        && current.contains(&before_route[0])
    {
        return Ok(Some(before_route[0].clone()));
    }
    if let Some(main) = reusable_main
        && before_route.contains(main)
        && current.contains(main)
    {
        return Ok(Some(main.clone()));
    }
    Ok(None)
}

/// Running state and window count deliberately are not policy inputs.
pub fn destination(installed: Option<bool>, cli: Option<PathBuf>) -> Result<Destination, Error> {
    match installed {
        Some(true) => Ok(Destination::Desktop),
        Some(false) => Ok(cli
            .map(Destination::Cli)
            .unwrap_or(Destination::Unavailable)),
        None => Err(Error::Unavailable(
            "Desktop application inventory is unavailable".into(),
        )),
    }
}

pub fn cli(provider: &str) -> Option<PathBuf> {
    if !matches!(provider, "codex" | "claude") {
        return None;
    }
    if let Some(path) = crate::cli::runtime::program::resolve(provider) {
        return Some(path);
    }
    let mut dirs: Vec<PathBuf> = std::env::var_os("PATH")
        .map(|p| {
            std::env::split_paths(&p)
                .filter(|p| p.is_absolute())
                .collect()
        })
        .unwrap_or_default();
    if let Some(home) = std::env::var_os("HOME") {
        dirs.push(PathBuf::from(home).join(".local/bin"));
    }
    dirs.extend([
        PathBuf::from("/opt/homebrew/bin"),
        PathBuf::from("/usr/local/bin"),
    ]);
    dirs.into_iter()
        .map(|d| d.join(provider))
        .find(|p| executable(p))
}

fn executable(path: &Path) -> bool {
    let Ok(meta) = path.metadata() else {
        return false;
    };
    if !meta.is_file() {
        return false;
    }
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        meta.permissions().mode() & 0o111 != 0
    }
    #[cfg(not(unix))]
    {
        true
    }
}

/// Accept only the full path or its exact known-home display abbreviation.
pub fn directory_label_matches(label: &str, cwd: &Path, home: Option<&Path>) -> bool {
    if cwd.to_str() == Some(label) {
        return true;
    }
    home.and_then(|home| cwd.strip_prefix(home).ok())
        .and_then(Path::to_str)
        .is_some_and(|relative| !relative.is_empty() && label == format!("~/{relative}"))
}

/// A rich-text Codex composer can store literal input as Markdown, including
/// hard breaks. Compare the whole expected serialization, never a substring.
pub fn first_prompt_matches(provider: &str, recorded: &str, expected: &str) -> bool {
    let recorded = recorded.trim_end();
    let expected = expected.trim_end();
    if recorded == expected {
        return true;
    }
    if provider != "codex" {
        return false;
    }
    codex_composer_record(expected) == recorded
}

fn codex_composer_record(expected: &str) -> String {
    let mut rendered = String::with_capacity(expected.len());
    for (i, line) in expected.split('\n').enumerate() {
        if i != 0 {
            rendered.push_str("\\\n");
        }
        let heading = line.bytes().take_while(|c| *c == b'#').count();
        if (1..=6).contains(&heading) && line.as_bytes().get(heading) == Some(&b' ') {
            rendered.push('\\');
        }
        for ch in line.chars() {
            if matches!(ch, '\\' | '*' | '`' | '<') {
                rendered.push('\\');
            }
            rendered.push(ch);
        }
    }
    rendered
}

/// First turn reconstructed independently from the enrolled service issue.
pub fn multica_issue_prompt(issue: &str, title: &str, body: &str) -> String {
    format!(
        "Multica issue {issue}\n\n{}\n\n{}",
        title.trim(),
        body.trim()
    )
    .trim_end()
    .to_owned()
}

/// A Multica run's working directory is `<workspace>/<run[..8]>/workdir`.
/// Its native session is created in `<workspace>`, a folder the owner adds
/// to each app once, never in the per-run directory (ADR 0221).
pub fn multica_workspace_directory<'a>(workdir: &'a Path, run: &str) -> Option<&'a Path> {
    use std::path::Component;
    let task = run
        .get(..8)
        .filter(|t| t.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'-'))?;
    if !workdir.is_absolute()
        || !workdir
            .components()
            .all(|c| matches!(c, Component::RootDir | Component::Normal(_)))
        || workdir.file_name()? != "workdir"
    {
        return None;
    }
    let run_dir = workdir.parent()?;
    if run_dir.file_name()? != task {
        return None;
    }
    let workspace = run_dir.parent()?;
    workspace.file_name().is_some().then_some(workspace)
}

/// Sent before a Multica first turn that runs in the workspace folder, so
/// the agent works in the run's own directory (ADR 0221).
pub fn multica_workspace_brief(workdir: &str) -> String {
    format!(
        "This Multica task's working directory is {workdir}. Before anything else, \
         change to it and read its AGENTS.md if it has one. Run every command and \
         make every change there; the other folders here belong to other tasks."
    )
}

/// The first turn submitted in the workspace folder: the brief, a blank
/// line, then the run's own first turn unchanged.
pub fn multica_workspace_prompt(workdir: &str, text: &str) -> String {
    format!("{}\n\n{text}", multica_workspace_brief(workdir))
}

/// The run's own first turn, when `submitted` is its workspace prompt.
pub fn strip_multica_workspace_brief<'a>(workdir: &str, submitted: &'a str) -> Option<&'a str> {
    submitted
        .strip_prefix(&multica_workspace_brief(workdir))?
        .strip_prefix("\n\n")
}

/// The only alternative considered for a digest-bound receipt is a complete
/// reversible serialization. The caller still checks the original SHA-256.
pub fn codex_composer_original(record: &str) -> Option<String> {
    let mut original = String::with_capacity(record.len());
    let mut chars = record.chars();
    while let Some(ch) = chars.next() {
        if ch != '\\' {
            original.push(ch);
            continue;
        }
        let escaped = chars.next()?;
        if !matches!(escaped, '\\' | '\n' | '#' | '*' | '`' | '<') {
            return None;
        }
        original.push(escaped);
    }
    (original != record && codex_composer_record(&original) == record).then_some(original)
}

/// An identity proven against the provider's selected local project and exact root.
/// Retain it across form preparation; a different selection invalidates the proof.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CodexProject(String);

/// Only a single unambiguous selection can supply the project identity.
fn selected_codex_project(json: &serde_json::Value) -> Option<&serde_json::Value> {
    let mut scoped = json
        .as_object()?
        .iter()
        .filter(|(k, _)| {
            k.strip_prefix("electron-owner-value:selected-project:")
                .is_some_and(|owner| !owner.is_empty())
        })
        .map(|(_, v)| v);
    let first = scoped.next();
    match (json.get("selected-project"), first) {
        (_, Some(_)) if scoped.next().is_some() => None,
        (Some(legacy), Some(current)) if legacy != current => None,
        (_, Some(current)) => Some(current),
        (legacy, None) => legacy,
    }
}

/// Label for a project-specific sidebar action. The live selection is
/// verified independently after navigation. Never pick among duplicate names.
pub fn codex_sidebar_project(bytes: &[u8], cwd: &Path) -> Option<String> {
    let state: serde_json::Value = serde_json::from_slice(bytes).ok()?;
    let projects = state["local-projects"].as_object()?;
    let mut matches = projects.values().filter(|p| {
        p["rootPaths"]
            .as_array()
            .is_some_and(|roots| roots.len() == 1 && roots[0].as_str() == cwd.to_str())
    });
    let label = matches.next()?["name"].as_str()?.trim();
    if !cwd.is_absolute()
        || label.is_empty()
        || matches.next().is_some()
        || projects
            .values()
            .filter(|p| p["name"].as_str() == Some(label))
            .count()
            != 1
    {
        return None;
    }
    Some(label.to_owned())
}

/// Whether any of the app's local projects has `cwd` among its roots. A
/// directory none lists cannot be reached without adding a project, which
/// Muster never does (ADR 0220). Unreadable state proves nothing either way.
pub fn codex_lists_directory(bytes: &[u8], cwd: &Path) -> Option<bool> {
    let state: serde_json::Value = serde_json::from_slice(bytes).ok()?;
    let projects = state["local-projects"].as_object()?;
    Some(projects.values().any(|p| {
        p["rootPaths"]
            .as_array()
            .is_some_and(|roots| roots.iter().any(|r| r.as_str() == cwd.to_str()))
    }))
}

pub fn selected_codex_directory(bytes: &[u8], cwd: &Path) -> bool {
    let Ok(json) = serde_json::from_slice::<serde_json::Value>(bytes) else {
        return false;
    };
    project_in_directory(&json, None, cwd).is_some()
}

pub fn codex_project(bytes: &[u8], label: &str, cwd: &Path) -> Option<CodexProject> {
    project_in_directory(&serde_json::from_slice(bytes).ok()?, Some(label), cwd)
}

fn project_in_directory(
    json: &serde_json::Value,
    label: Option<&str>,
    cwd: &Path,
) -> Option<CodexProject> {
    let selected = selected_codex_project(json)?;
    if selected["type"].as_str() != Some("local") || !cwd.is_absolute() {
        return None;
    }
    let id = selected["projectId"].as_str().filter(|id| !id.is_empty())?;
    let project = json["local-projects"].as_object()?.get(id)?;
    if label.is_some_and(|name| project["name"].as_str() != Some(name))
        || project
            .get("id")
            .is_some_and(|value| value.as_str() != Some(id))
    {
        return None;
    }
    let roots = project["rootPaths"].as_array()?;
    (roots.len() == 1 && roots[0].as_str() == cwd.to_str()).then(|| CodexProject(id.to_owned()))
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn multica_sessions_live_in_the_workspace_folder() {
        let run = "6df77065-0000-4000-8000-000000000001";
        let workdir = Path::new("/srv/multica/ws-1/6df77065/workdir");
        assert_eq!(
            multica_workspace_directory(workdir, run),
            Some(Path::new("/srv/multica/ws-1"))
        );
        for other in [
            "/srv/multica/ws-1/6df77066/workdir",
            "/srv/multica/ws-1/6df77065/work",
            "/srv/multica/ws-1/../6df77065/workdir",
            "srv/multica/ws-1/6df77065/workdir",
            "/6df77065/workdir",
        ] {
            assert_eq!(multica_workspace_directory(Path::new(other), run), None);
        }
        assert_eq!(multica_workspace_directory(workdir, "short"), None);
        let dir = workdir.to_str().unwrap();
        let prompt = multica_workspace_prompt(dir, "## Task\n\nDo it.");
        assert!(prompt.starts_with("This Multica task's working directory is /srv/"));
        assert_eq!(
            strip_multica_workspace_brief(dir, &prompt),
            Some("## Task\n\nDo it.")
        );
        assert_eq!(strip_multica_workspace_brief("/other", &prompt), None);
        assert_eq!(strip_multica_workspace_brief(dir, "## Task"), None);
        // Codex records the whole submission in its composer form, which
        // reverses to the original before the brief is removed.
        let recorded = codex_composer_record(&prompt);
        let original = codex_composer_original(&recorded).unwrap();
        assert_eq!(
            strip_multica_workspace_brief(dir, &original),
            Some("## Task\n\nDo it.")
        );
    }

    #[test]
    fn desktop_receipt_requires_the_complete_expected_input() {
        let prompt = "## Task\n\nRead `x` and **return** <one>.";
        let record = "\\## Task\\\n\\\nRead \\`x\\` and \\*\\*return\\*\\* \\<one>.\n";
        assert_eq!(
            codex_composer_original(record.trim_end()).as_deref(),
            Some(prompt)
        );
        assert!(codex_composer_original("unrelated\\#text").is_none());
        assert!(first_prompt_matches("codex", record, prompt));
        assert!(!first_prompt_matches("claude", record, prompt));
        assert!(!first_prompt_matches("codex", record, "## Task"));
        assert!(!first_prompt_matches(
            "codex",
            &format!("extra{record}"),
            prompt
        ));
    }

    #[test]
    fn background_or_unknown_focus_never_admits_foreground_work() {
        use crate::local_input::Owner::*;
        use NativeConsole::*;
        assert_eq!(
            NativeConsole::observe(false, Some(2), Process(1)),
            Background(1)
        );
        assert_eq!(NativeConsole::observe(false, Some(2), Process(2)), Provider);
        for owner in [Nobody, Unknown] {
            assert_eq!(NativeConsole::observe(false, Some(2), owner), Waiting);
        }
        assert_eq!(NativeConsole::observe(false, None, Process(1)), Waiting);
        assert_eq!(NativeConsole::observe(true, None, Nobody), Covered);
    }

    #[test]
    fn session_navigation_binds_the_observed_console_at_dispatch() {
        use NativeConsole::*;
        for before in [Waiting, Provider, Background(7), Background(8), Covered] {
            for now in [Waiting, Provider, Background(7), Background(8), Covered] {
                assert_eq!(
                    before.check_navigation(now).is_ok(),
                    before == now && before != Waiting
                );
            }
        }
    }

    #[test]
    fn only_pre_admission_can_wait_for_the_provider_to_leave_the_foreground() {
        use NativeConsole::*;
        for mode in [Waiting, Background(5), Provider] {
            assert!(mode.check_wait(false, "test").is_ok());
            assert!(matches!(
                mode.check_wait(true, "test"),
                Err(Error::Conflict(_))
            ));
        }
        assert!(Covered.check_wait(true, "test").is_ok());
        let Err(Error::Conflict(why)) = Provider.check_wait(true, "Codex") else {
            panic!("the active provider must refuse");
        };
        assert!(
            why.starts_with("Codex is the active app") && why.ends_with("nothing was submitted")
        );
    }

    #[test]
    fn the_active_provider_is_never_driven_for_creation() {
        use NativeConsole::*;
        assert!(Background(3).may_create());
        assert!(Covered.may_create());
        assert!(!Provider.may_create());
        assert!(!Waiting.may_create());
    }

    #[test]
    fn only_the_allocated_window_in_the_same_process_is_reused() {
        let pin = CreationWindow::new(10, 20, 2);
        for windows in [[1, 2], [2, 1], [3, 2]] {
            assert_eq!(pin.live(10, 20, &windows), Some(&2));
        }
        assert_eq!(pin.live(10, 20, &[1]), None);
        assert_eq!(pin.live(10, 21, &[2]), None);
        assert_eq!(pin.live(11, 20, &[2]), None);
    }

    #[test]
    fn only_new_or_proved_activation_windows_are_eligible() {
        assert_eq!(
            session_window(&[1], &[1], &[1, 2], true, None).unwrap(),
            Some(2)
        );
        assert_eq!(
            session_window(&[], &[2], &[2], true, None).unwrap(),
            Some(2)
        );
        assert_eq!(session_window(&[], &[2], &[2], false, None).unwrap(), None);
        assert_eq!(session_window(&[2], &[2], &[2], true, None).unwrap(), None);
        assert_eq!(
            session_window(&[], &[1, 2], &[1, 2], true, None).unwrap(),
            None
        );
        assert_eq!(session_window(&[], &[2], &[], true, None).unwrap(), None);
        assert!(session_window(&[1], &[1], &[1, 2, 3], true, None).is_err());
        assert_eq!(
            session_window(&[1, 2], &[1, 2], &[1, 2], true, Some(&2)).unwrap(),
            Some(2)
        );
        assert_eq!(
            session_window(&[1], &[1], &[1], true, Some(&2)).unwrap(),
            None
        );
        assert_eq!(
            session_window(&[1, 2], &[1, 2], &[1], true, Some(&2)).unwrap(),
            None
        );
        assert!(session_window(&[1], &[1], &[1, 2, 3], true, Some(&1)).is_err());
        assert_eq!(
            session_window(&[1], &[1], &[1, 2], true, Some(&1)).unwrap(),
            Some(2)
        );
    }
    #[test]
    fn directory_labels_require_the_entire_authorized_path() {
        let home = Path::new("/tmp/native-fixture/home");
        let cwd = home.join("Documents/Multica/task");
        assert!(directory_label_matches(
            cwd.to_str().unwrap(),
            &cwd,
            Some(home)
        ));
        assert!(directory_label_matches(
            "~/Documents/Multica/task",
            &cwd,
            Some(home)
        ));
        for bad in [
            "task",
            "Multica/task",
            "~/task",
            "~/Documents/Multica/task-other",
            "/tmp/native-fixture/other/Documents/Multica/task",
        ] {
            assert!(!directory_label_matches(bad, &cwd, Some(home)));
        }
        assert!(!directory_label_matches(
            "~/Documents/Multica/task",
            &cwd,
            None
        ));
    }
    #[cfg(unix)]
    #[test]
    fn standalone_script_cli_is_available_only_when_executable() {
        use std::os::unix::fs::PermissionsExt;
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("codex");
        std::fs::write(&path, "#!/usr/bin/env node\n").unwrap();
        std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o644)).unwrap();
        assert!(!executable(&path));
        std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o755)).unwrap();
        assert!(executable(&path));
        assert!(!executable(dir.path()));
    }
    #[test]
    fn installed_always_selects_desktop_even_with_cli() {
        for cli in [None, Some(PathBuf::from("/bin/cli"))] {
            assert_eq!(destination(Some(true), cli).unwrap(), Destination::Desktop);
        }
        assert!(destination(None, Some(PathBuf::from("/bin/cli"))).is_err());
        assert_eq!(
            destination(Some(false), None).unwrap(),
            Destination::Unavailable
        );
        assert_eq!(
            destination(Some(false), Some("/bin/cli".into())).unwrap(),
            Destination::Cli("/bin/cli".into())
        );
    }
    #[test]
    fn owner_scoped_project_needs_one_consistent_record_and_exact_path() {
        let mut state = serde_json::json!({
            "electron-owner-value:selected-project:fixture": {"type":"local","projectId":"p"},
            "local-projects":{"p":{"id":"p","name":"Task","rootPaths":["/tmp/test-work"]}}
        });
        let matches = |s: &serde_json::Value| {
            let bytes = serde_json::to_vec(s).unwrap();
            selected_codex_directory(&bytes, Path::new("/tmp/test-work"))
                && codex_project(&bytes, "Task", Path::new("/tmp/test-work")).is_some()
        };
        let bytes = serde_json::to_vec(&state).unwrap();
        assert_eq!(
            codex_sidebar_project(&bytes, Path::new("/tmp/test-work")).as_deref(),
            Some("Task")
        );
        let mut duplicate = state.clone();
        duplicate["local-projects"]["other"] =
            serde_json::json!({"name":"Task","rootPaths":["/tmp/elsewhere"]});
        assert!(
            codex_sidebar_project(
                &serde_json::to_vec(&duplicate).unwrap(),
                Path::new("/tmp/test-work")
            )
            .is_none()
        );
        assert!(matches(&state));
        for (state, dir, listed) in [
            (&state, "/tmp/test-work", Some(true)),
            (&duplicate, "/tmp/test-work", Some(true)),
            (&state, "/tmp/other", Some(false)),
        ] {
            let bytes = serde_json::to_vec(state).unwrap();
            assert_eq!(codex_lists_directory(&bytes, Path::new(dir)), listed);
        }
        assert_eq!(
            codex_lists_directory(b"{}", Path::new("/tmp/test-work")),
            None
        );
        assert!(!selected_codex_directory(
            &serde_json::to_vec(&state).unwrap(),
            Path::new("/tmp/other")
        ));
        state["selected-project"] = serde_json::json!({"type":"local","projectId":"other"});
        assert!(!matches(&state));
        state.as_object_mut().unwrap().remove("selected-project");
        state["electron-owner-value:selected-project:another"] =
            state["electron-owner-value:selected-project:fixture"].clone();
        assert!(!matches(&state));
    }

    #[test]
    fn project_requires_selected_identity_and_exact_single_root() {
        let cwd = Path::new("/tmp/one/workdir");
        let base = serde_json::json!({
            "selected-project": {"type": "local", "projectId": "one"},
            "local-projects": {
                "one": {"id":"one", "name":"workdir", "rootPaths":["/tmp/one/workdir"]},
                "two": {"id":"two", "name":"workdir", "rootPaths":["/tmp/two/workdir"]}
            }
        });
        let proof =
            |j: &serde_json::Value| codex_project(&serde_json::to_vec(j).unwrap(), "workdir", cwd);
        let first = proof(&base).unwrap();
        for selected in [
            serde_json::Value::Null,
            serde_json::json!({"type":"remote", "projectId":"one"}),
            serde_json::json!({"type":"local", "projectId":"missing"}),
            serde_json::json!({"type":"local", "projectId":"two"}),
            serde_json::json!({"type":"local", "projectId":""}),
        ] {
            let mut changed = base.clone();
            changed["selected-project"] = selected;
            assert!(proof(&changed).is_none());
        }
        for roots in [
            serde_json::json!([]),
            serde_json::json!(["/tmp/two/workdir"]),
            serde_json::json!(["/tmp/one/workdir", "/tmp/two/workdir"]),
        ] {
            let mut changed = base.clone();
            changed["local-projects"]["one"]["rootPaths"] = roots;
            assert!(proof(&changed).is_none());
        }
        for (field, value) in [("name", "other"), ("id", "two")] {
            let mut changed = base.clone();
            changed["local-projects"]["one"][field] = value.into();
            assert!(proof(&changed).is_none());
        }
        let mut changed = base.clone();
        changed["local-projects"]["two"]["rootPaths"] = serde_json::json!(["/tmp/one/workdir"]);
        changed["selected-project"]["projectId"] = "two".into();
        assert_ne!(proof(&changed).unwrap(), first);
        assert!(codex_project(b"invalid", "workdir", cwd).is_none());
        assert!(codex_project(&serde_json::to_vec(&base).unwrap(), "other", cwd).is_none());
    }
}
