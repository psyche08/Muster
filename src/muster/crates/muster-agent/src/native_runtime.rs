//! Desktop-first destination policy and native project evidence.
use crate::Error;
use std::path::{Path, PathBuf};

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Destination {
    Desktop,
    Cli(PathBuf),
    Unavailable,
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
    if let Some(path) = crate::orca::runtime::program::resolve(provider) {
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

/// An identity proven against the provider's selected local project and exact root.
/// Retain it across form preparation; a different selection invalidates the proof.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CodexProject(String);

pub fn codex_project(bytes: &[u8], label: &str, cwd: &Path) -> Option<CodexProject> {
    let json: serde_json::Value = serde_json::from_slice(bytes).ok()?;
    let selected = &json["selected-project"];
    if selected["type"].as_str() != Some("local") || !cwd.is_absolute() {
        return None;
    }
    let id = selected["projectId"].as_str().filter(|id| !id.is_empty())?;
    let project = json["local-projects"].as_object()?.get(id)?;
    if project["name"].as_str() != Some(label)
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
