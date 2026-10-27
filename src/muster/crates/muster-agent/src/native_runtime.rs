//! Desktop-first destination policy and native project evidence.
use crate::Error;
use std::path::{Path, PathBuf};

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Destination {
    Desktop,
    Cli(PathBuf),
    Unavailable,
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
    matches!(provider, "codex" | "claude")
        .then(|| crate::orca::runtime::program::resolve(provider))
        .flatten()
}

/// Resolve only the selected local project. Duplicate labels or multiple roots
/// cannot authorize a new native session, even if one root happens to match.
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
