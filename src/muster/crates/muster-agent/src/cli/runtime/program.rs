//! Prefer the native CLI shipped with the installed desktop app, which tracks
//! its model support, before potentially stale standalone installations.
use std::path::{Path, PathBuf};

pub(crate) fn resolve(agent: &str) -> Option<PathBuf> {
    let home = std::env::var_os("HOME").map(PathBuf::from);
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
    if let Some(home) = home {
        dirs.push(home.join(".local/bin"));
    }
    dirs.extend([
        PathBuf::from("/opt/homebrew/bin"),
        PathBuf::from("/usr/local/bin"),
    ]);
    let name = if cfg!(windows) {
        format!("{agent}.exe")
    } else {
        agent.into()
    };
    candidates.extend(dirs.into_iter().map(|dir| dir.join(&name)));
    first_native(candidates)
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
}
