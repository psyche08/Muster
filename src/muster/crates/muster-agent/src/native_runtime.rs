//! Desktop-first destination policy and native project evidence.
use crate::Error;
use std::path::{Path, PathBuf};

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Destination {
    Desktop,
    Cli(PathBuf),
    Unavailable,
}

/// Select only a newly created opaque window identity, including activation
/// from a proved empty baseline when that provider reuses its new main window.
pub fn new_window<T: PartialEq + Clone>(
    before_activation: &[T],
    before_route: &[T],
    current: &[T],
    allow_activation: bool,
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

/// Resolve only one selected project with one exact root.
pub fn codex_project(bytes: &[u8], label: &str, cwd: &Path) -> bool {
    let Ok(json) = serde_json::from_slice::<serde_json::Value>(bytes) else {
        return false;
    };
    let Some(projects) = json["local-projects"].as_object() else {
        return false;
    };
    let mut matches = projects
        .values()
        .filter(|p| p["name"].as_str() == Some(label));
    let Some(project) = matches.next() else {
        return false;
    };
    matches.next().is_none()
        && project["rootPaths"]
            .as_array()
            .is_some_and(|roots| roots.len() == 1 && roots[0].as_str() == cwd.to_str())
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn only_new_or_proved_activation_windows_are_eligible() {
        assert_eq!(new_window(&[1], &[1], &[1, 2], true).unwrap(), Some(2));
        assert_eq!(new_window(&[], &[2], &[2], true).unwrap(), Some(2));
        assert_eq!(new_window(&[], &[2], &[2], false).unwrap(), None);
        assert_eq!(new_window(&[2], &[2], &[2], true).unwrap(), None);
        assert_eq!(new_window(&[], &[1, 2], &[1, 2], true).unwrap(), None);
        assert_eq!(new_window(&[], &[2], &[], true).unwrap(), None);
        assert!(new_window(&[1], &[1], &[1, 2, 3], true).is_err());
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
    fn project_requires_unique_label_and_exact_single_root() {
        let cwd = Path::new("/tmp/test-project");
        for (data, ok) in [
            (
                r#"{"local-projects":{"one":{"name":"test","rootPaths":["/tmp/test-project"]}}}"#,
                true,
            ),
            (
                r#"{"local-projects":{"one":{"name":"test","rootPaths":["/tmp/other"]}}}"#,
                false,
            ),
            (
                r#"{"local-projects":{"one":{"name":"test","rootPaths":["/tmp/test-project","/tmp/other"]}}}"#,
                false,
            ),
            (
                r#"{"local-projects":{"one":{"name":"test","rootPaths":["/tmp/test-project"]},"two":{"name":"test","rootPaths":["/tmp/test-project"]}}}"#,
                false,
            ),
        ] {
            assert_eq!(codex_project(data.as_bytes(), "test", cwd), ok);
        }
    }
}
