//! Local desktop/CLI identity mapping. Titles and remote aliases are not keys.
use crate::viewer::is_uuid;
use std::path::Path;

pub fn local_id(id: &str) -> bool {
    is_uuid(id.strip_prefix("local_").unwrap_or(id))
}

pub fn session_from_url(raw: &str) -> Option<String> {
    let url = url::Url::parse(raw).ok()?;
    if url.scheme() != "https" || url.host_str() != Some("claude.ai") {
        return None;
    }
    let id = url.path().strip_prefix("/epitaxy/")?;
    local_id(id).then(|| id.to_ascii_lowercase())
}

#[derive(Clone)]
struct Entry {
    desktop: Option<String>,
    cli: Option<String>,
    excluded: bool,
}
type Cached = (std::time::SystemTime, u64, Option<Entry>);
static CACHE: std::sync::LazyLock<
    std::sync::Mutex<std::collections::HashMap<std::path::PathBuf, Cached>>,
> = std::sync::LazyLock::new(Default::default);

fn parse(bytes: &[u8]) -> Option<Entry> {
    let v: serde_json::Value = serde_json::from_slice(bytes).ok()?;
    if !v.is_object() {
        return None;
    }
    Some(Entry {
        desktop: v
            .get("sessionId")
            .map(|v| v.as_str().unwrap_or("").to_owned()),
        cli: v["cliSessionId"]
            .as_str()
            .filter(|id| is_uuid(id))
            .map(str::to_owned),
        excluded: v["isArchived"] == true
            || ["sshConfig", "sshRemoteTranscriptPath", "sshRemoteProcessId"]
                .iter()
                .any(|key| v.get(key).is_some_and(|v| !v.is_null())),
    })
}

pub(crate) fn mappings(root: &Path) -> Option<Vec<(String, String)>> {
    let started = std::time::Instant::now();
    let mut seen = 0;
    let mut result = Vec::new();
    let mut cache = CACHE.lock().unwrap_or_else(|p| p.into_inner());
    let mut present = std::collections::HashSet::new();
    for org in std::fs::read_dir(root).ok()? {
        let org = org.ok()?;
        if !org.file_type().ok()?.is_dir() {
            continue;
        }
        for user in std::fs::read_dir(org.path()).ok()? {
            let user = user.ok()?;
            if !user.file_type().ok()?.is_dir() {
                continue;
            }
            for file in std::fs::read_dir(user.path()).ok()? {
                seen += 1;
                if seen > crate::claude::MAX_FILES
                    || started.elapsed() > std::time::Duration::from_millis(250)
                {
                    return None;
                }
                let file = file.ok()?;
                let path = file.path();
                if path.extension().is_none_or(|s| s != "json") {
                    continue;
                }
                let stem = path.file_stem()?.to_str()?;
                if !local_id(stem) {
                    continue;
                }
                let meta = file.metadata().ok()?;
                if !file.file_type().ok()?.is_file() || meta.len() > 4 << 20 {
                    return None;
                }
                let stamp = (meta.modified().ok()?, meta.len());
                let entry = match cache.get(&path) {
                    Some((mtime, len, entry)) if (*mtime, *len) == stamp => entry.clone(),
                    _ => {
                        let entry = parse(&std::fs::read(&path).ok()?);
                        cache.insert(path.clone(), (stamp.0, stamp.1, entry.clone()));
                        entry
                    }
                }?;
                present.insert(path.clone());
                if entry.excluded {
                    continue;
                }
                let Some(cli) = entry.cli else {
                    continue;
                };
                if entry.desktop.as_deref().is_some_and(|id| id != stem) {
                    return None;
                }
                result.push((stem.to_ascii_lowercase(), cli.to_ascii_lowercase()));
            }
        }
    }
    cache.retain(|path, _| !path.starts_with(root) || present.contains(path));
    Some(result)
}

pub fn desktop_for_cli(root: &Path, cli: &str) -> Option<String> {
    if !is_uuid(cli) {
        return None;
    }
    let entries = mappings(root)?;
    let mut matches = entries
        .iter()
        .filter(|(_, id)| id.eq_ignore_ascii_case(cli));
    let (desktop, _) = matches.next()?;
    if matches.next().is_some() || entries.iter().filter(|(id, _)| id == desktop).count() != 1 {
        return None;
    }
    Some(desktop.clone())
}

pub fn cli_for_desktop(root: &Path, desktop: &str) -> Option<String> {
    if !local_id(desktop) {
        return None;
    }
    let entries = mappings(root)?;
    let mut matches = entries
        .iter()
        .filter(|(id, _)| id.eq_ignore_ascii_case(desktop));
    let (_, cli) = matches.next()?;
    if matches.next().is_some() || entries.iter().filter(|(_, id)| id == cli).count() != 1 {
        return None;
    }
    Some(cli.clone())
}

#[cfg(test)]
mod tests {
    use super::*;
    const CLI: &str = "11111111-1111-4111-8111-111111111111";
    const DESKTOP: &str = "local_22222222-2222-4222-8222-222222222222";
    #[test]
    fn only_local_routes_are_identity_evidence() {
        assert_eq!(
            session_from_url(&format!("https://claude.ai/epitaxy/{DESKTOP}")),
            Some(DESKTOP.into())
        );
        for url in [
            "https://claude.ai/epitaxy/session_example",
            "https://claude.ai/epitaxy/remote_22222222-2222-4222-8222-222222222222",
            "https://example.com/epitaxy/local_22222222-2222-4222-8222-222222222222",
            "https://claude.ai/epitaxy/local_22222222-2222-4222-8222-222222222222/other",
        ] {
            assert_eq!(session_from_url(url), None);
        }
    }
    #[test]
    fn mapping_is_bidirectional_fresh_local_and_unique() {
        let dir = tempfile::tempdir().unwrap();
        let user = dir.path().join("org/user");
        std::fs::create_dir_all(&user).unwrap();
        let file = user.join(format!("{DESKTOP}.json"));
        let mut v =
            serde_json::json!({"sessionId":DESKTOP,"cliSessionId":CLI,"title":"Same title"});
        let write = |v: &serde_json::Value| std::fs::write(&file, v.to_string()).unwrap();
        write(&v);
        assert_eq!(cli_for_desktop(dir.path(), DESKTOP).as_deref(), Some(CLI));
        assert_eq!(desktop_for_cli(dir.path(), CLI).as_deref(), Some(DESKTOP));
        v["title"] = "Renamed".into();
        write(&v);
        assert_eq!(cli_for_desktop(dir.path(), DESKTOP).as_deref(), Some(CLI));
        v["cliSessionId"] = "33333333-3333-4333-8333-333333333333".into();
        write(&v);
        assert_eq!(desktop_for_cli(dir.path(), CLI), None);
        assert_eq!(
            cli_for_desktop(dir.path(), DESKTOP).as_deref(),
            v["cliSessionId"].as_str()
        );
        v["sshConfig"] = serde_json::json!({"host":"example.invalid"});
        write(&v);
        assert_eq!(cli_for_desktop(dir.path(), DESKTOP), None);
        v.as_object_mut().unwrap().remove("sshConfig");
        v["isArchived"] = true.into();
        write(&v);
        assert_eq!(cli_for_desktop(dir.path(), DESKTOP), None);
        v["isArchived"] = false.into();
        write(&v);
        let other = dir.path().join("org/other");
        std::fs::create_dir_all(&other).unwrap();
        std::fs::copy(&file, other.join(format!("{DESKTOP}.json"))).unwrap();
        assert_eq!(cli_for_desktop(dir.path(), DESKTOP), None);
        assert_eq!(
            desktop_for_cli(dir.path(), v["cliSessionId"].as_str().unwrap()),
            None
        );
        assert_eq!(cli_for_desktop(dir.path(), "session_remote"), None);
    }
}
