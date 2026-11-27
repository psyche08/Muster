//! Prefer the native CLI shipped with the installed desktop app, which tracks
//! its model support, before potentially stale standalone installations.
//! Also the PATH a CLI child starts with (ADR 0329).
use std::path::{Path, PathBuf};

pub(crate) fn resolve(agent: &str) -> Option<PathBuf> {
    let home = std::env::home_dir();
    let mut candidates = Vec::new();
    if cfg!(target_os = "macos") && agent == "codex" {
        let mut roots = vec![PathBuf::from("/Applications")];
        if let Some(home) = &home {
            roots.push(home.join("Applications"));
        }
        for root in roots {
            candidates.push(root.join(
                "ChatGPT.app/Contents/Resources/codex-cli/CodexCLI.app/Contents/MacOS/codex",
            ));
        }
    }
    let mut dirs: Vec<PathBuf> = std::env::var_os("PATH")
        .map(|p| {
            std::env::split_paths(&p)
                .filter(|p| p.is_absolute())
                .collect()
        })
        .unwrap_or_default();
    dirs.extend(user_dirs(home.as_deref(), None));
    let name = if cfg!(windows) {
        format!("{agent}.exe")
    } else {
        agent.into()
    };
    candidates.extend(dirs.into_iter().map(|dir| dir.join(&name)));
    first_native(candidates)
}

/// The directories a login shell has and a launchd daemon's PATH
/// (`/usr/bin:/bin:/usr/sbin:/sbin`) lacks, in the order a shell puts them:
/// the current nvm Node, the user's own, then Homebrew's.
fn user_dirs(home: Option<&Path>, nvm: Option<&str>) -> Vec<PathBuf> {
    let mut dirs = Vec::new();
    if let Some(home) = home {
        if let Some(version) = nvm {
            dirs.push(home.join(".nvm/versions/node").join(version).join("bin"));
        }
        dirs.push(home.join(".local/bin"));
    }
    dirs.extend([
        PathBuf::from("/opt/homebrew/bin"),
        PathBuf::from("/usr/local/bin"),
    ]);
    dirs
}

/// The PATH a CLI child starts with, or `None` to leave it the daemon's.
/// [`resolve`] finds the CLI in the user's directories, but the child
/// inherits the daemon's PATH, which under launchd holds only the system
/// directories: the session could not run Homebrew's tools, the user's own
/// or an nvm Node. Those of [`user_dirs`] that exist and are missing from
/// `inherited` go in front, in that order. `inherited` follows unchanged, so
/// a PATH someone set on purpose is only added to, never reordered.
#[cfg(unix)]
pub(crate) fn child_path(
    inherited: Option<&std::ffi::OsStr>,
    home: Option<&Path>,
    nvm: Option<&str>,
) -> Option<std::ffi::OsString> {
    prepend_missing(inherited, user_dirs(home, nvm))
}

#[cfg(unix)]
fn prepend_missing(
    inherited: Option<&std::ffi::OsStr>,
    candidates: Vec<PathBuf>,
) -> Option<std::ffi::OsString> {
    let have: Vec<PathBuf> = inherited
        .map(|p| std::env::split_paths(p).collect())
        .unwrap_or_default();
    let mut dirs: Vec<PathBuf> = candidates
        .into_iter()
        .filter(|dir| dir.is_dir() && !have.contains(dir))
        .collect();
    if dirs.is_empty() {
        return None;
    }
    dirs.extend(have);
    std::env::join_paths(dirs).ok()
}

/// How long `nvm current` may take. Sourcing nvm takes a fraction of a
/// second; a child is not held back longer than this for its Node.
#[cfg(unix)]
const NVM_TIMEOUT: std::time::Duration = std::time::Duration::from_secs(5);

/// The version `nvm current` names in a fresh shell, such as `v22.11.0`,
/// when nvm is installed at `~/.nvm`. `None` without nvm, when it names no
/// installed version (`none`, `system`), or when it does not answer in time.
/// nvm is a shell function, so this runs a shell: a fixed script with no
/// caller input, in the home directory, with stdin and stderr closed.
#[cfg(unix)]
pub(crate) async fn nvm_current(home: &Path) -> Option<String> {
    use std::process::Stdio;
    let dir = home.join(".nvm");
    if !dir.join("nvm.sh").is_file() {
        return None;
    }
    let mut cmd = tokio::process::Command::new("/bin/bash");
    cmd.args(["-c", r#". "$NVM_DIR/nvm.sh" >/dev/null 2>&1; nvm current"#])
        .env("NVM_DIR", &dir)
        .current_dir(home)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .kill_on_drop(true);
    let output = tokio::time::timeout(NVM_TIMEOUT, cmd.output())
        .await
        .ok()?
        .ok()?;
    node_version(&String::from_utf8_lossy(&output.stdout))
}

/// `v` and dot-separated numbers, nothing else: what goes into the path is
/// a version nvm installs under `versions/node/`, never a path of its own.
#[cfg(unix)]
fn node_version(output: &str) -> Option<String> {
    let version = output.trim();
    let numbers = version.strip_prefix('v')?;
    let valid = version.len() <= 32
        && numbers
            .split('.')
            .all(|part| !part.is_empty() && part.bytes().all(|b| b.is_ascii_digit()));
    valid.then(|| version.to_owned())
}

fn first_native(candidates: impl IntoIterator<Item = PathBuf>) -> Option<PathBuf> {
    candidates.into_iter().find(|path| native_executable(path))
}

fn native_executable(path: &Path) -> bool {
    use std::io::Read;
    let Ok(metadata) = path.metadata() else {
        return false;
    };
    if !metadata.is_file() {
        return false;
    }
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        if metadata.permissions().mode() & 0o111 == 0 {
            return false;
        }
    }
    let mut magic = [0; 4];
    std::fs::File::open(path)
        .and_then(|mut f| f.read_exact(&mut magic))
        .is_ok()
        && (magic == *b"\x7fELF"
            || matches!(
                magic,
                [0xcf, 0xfa, 0xed, 0xfe] | [0xfe, 0xed, 0xfa, 0xcf] | [0xca, 0xfe, 0xba, 0xbe]
            )
            || magic[..2] == *b"MZ")
}

#[cfg(all(test, unix))]
mod tests {
    use super::*;
    use std::os::unix::fs::PermissionsExt;
    #[test]
    fn bundled_native_wins_and_invalid_candidates_fall_back_without_node() {
        let dir = tempfile::tempdir().unwrap();
        let app = dir.path().join("app");
        let standalone = dir.path().join("standalone");
        let wrapper = dir.path().join("wrapper");
        for p in [&app, &standalone] {
            std::fs::write(p, b"\xcf\xfa\xed\xfe").unwrap();
            std::fs::set_permissions(p, std::fs::Permissions::from_mode(0o755)).unwrap();
        }
        std::fs::write(&wrapper, b"#!/usr/bin/env node\n").unwrap();
        std::fs::set_permissions(&wrapper, std::fs::Permissions::from_mode(0o755)).unwrap();
        assert_eq!(
            first_native([app.clone(), standalone.clone()]),
            Some(app.clone())
        );
        std::fs::set_permissions(&app, std::fs::Permissions::from_mode(0o644)).unwrap();
        assert_eq!(
            first_native([app, wrapper.clone(), standalone.clone()]),
            Some(standalone)
        );
        assert_eq!(first_native([wrapper, dir.path().join("missing")]), None);
    }

    #[test]
    fn user_directories_follow_a_login_shells_order() {
        let home = Path::new("/home/example");
        assert_eq!(
            user_dirs(Some(home), Some("v22.11.0")),
            [
                "/home/example/.nvm/versions/node/v22.11.0/bin",
                "/home/example/.local/bin",
                "/opt/homebrew/bin",
                "/usr/local/bin",
            ]
            .map(PathBuf::from)
        );
        assert_eq!(
            user_dirs(Some(home), None),
            [
                "/home/example/.local/bin",
                "/opt/homebrew/bin",
                "/usr/local/bin"
            ]
            .map(PathBuf::from)
        );
    }

    #[test]
    fn missing_directories_go_in_front_and_the_inherited_path_keeps_its_order() {
        let dir = tempfile::tempdir().unwrap();
        let [node, local, brew] = ["node", "local", "brew"].map(|name| {
            let path = dir.path().join(name);
            std::fs::create_dir(&path).unwrap();
            path
        });
        let absent = dir.path().join("absent");
        let inherited = std::env::join_paths([brew.as_path(), Path::new("/usr/bin")]).unwrap();
        let path = prepend_missing(
            Some(&inherited),
            vec![node.clone(), local.clone(), absent, brew.clone()],
        )
        .unwrap();
        // The one already there stays where it was; the absent one is left out.
        assert_eq!(
            std::env::split_paths(&path).collect::<Vec<_>>(),
            vec![node, local, brew.clone(), PathBuf::from("/usr/bin")]
        );
        // Nothing to add leaves the daemon's PATH as it is.
        assert_eq!(prepend_missing(Some(&inherited), vec![brew]), None);
        // A daemon without a PATH still gets the user's directories.
        assert!(prepend_missing(None, vec![dir.path().to_owned()]).is_some());
    }

    #[test]
    fn only_a_plain_node_version_names_a_directory() {
        assert_eq!(node_version("v22.11.0\n").as_deref(), Some("v22.11.0"));
        assert_eq!(node_version("v20").as_deref(), Some("v20"));
        for output in [
            "",
            "none",
            "system",
            "N/A",
            "v",
            "v1..2",
            "22.11.0",
            "v22.11.0/../x",
            "v22 v20",
            "iojs-v3.3.1",
        ] {
            assert_eq!(node_version(output), None, "{output:?}");
        }
    }

    fn nvm_home(script: &str) -> tempfile::TempDir {
        let home = tempfile::tempdir().unwrap();
        std::fs::create_dir(home.path().join(".nvm")).unwrap();
        std::fs::write(home.path().join(".nvm/nvm.sh"), script).unwrap();
        home
    }

    #[tokio::test]
    async fn nvm_current_is_asked_of_nvm_itself() {
        // nvm.sh defines the function; Muster only sources it and asks. The
        // script also checks it was given its own directory and the home.
        let home = nvm_home(
            r#"nvm() { [ "$1" = current ] && [ -f "$NVM_DIR/nvm.sh" ] && [ -f .nvm/nvm.sh ] && echo v20.11.1; }
echo sourcing prints nothing that counts"#,
        );
        assert_eq!(nvm_current(home.path()).await.as_deref(), Some("v20.11.1"));
        let system = nvm_home("nvm() { echo system; }");
        assert_eq!(nvm_current(system.path()).await, None);
        let without = tempfile::tempdir().unwrap();
        assert_eq!(nvm_current(without.path()).await, None);
    }
}
