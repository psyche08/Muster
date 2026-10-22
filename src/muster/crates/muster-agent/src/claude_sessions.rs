//! Claude desktop side sessions use desktop IDs, not transcript UUIDs.
//! Resolve only unambiguous provider links inside the same org/user registry.
use super::{Discovered, MAX_FILES};
use serde_json::Value;
use std::{
    collections::{HashMap, HashSet},
    path::{Path, PathBuf},
    time::SystemTime,
};

#[derive(Clone)]
struct Entry {
    desktop: String,
    cli: String,
    parent: Option<String>,
    relation: &'static str,
}
type Cached = (SystemTime, u64, Option<Entry>);
static CACHE: std::sync::LazyLock<std::sync::Mutex<HashMap<PathBuf, Cached>>> =
    std::sync::LazyLock::new(Default::default);

fn parse(bytes: &[u8]) -> Option<Entry> {
    let v: Value = serde_json::from_slice(bytes).ok()?;
    let cli = v["cliSessionId"]
        .as_str()
        .filter(|id| crate::viewer::is_uuid(id))?;
    let desktop = v["sessionId"].as_str()?;
    if !crate::viewer::is_uuid(desktop.strip_prefix("local_").unwrap_or(desktop)) {
        return None;
    }
    let spawn = &v["spawnedFrom"];
    let parent = spawn["sessionId"].as_str().map(str::to_owned);
    Some(Entry {
        desktop: desktop.into(),
        cli: cli.to_ascii_lowercase(),
        parent,
        relation: if spawn["userFork"] == true {
            "fork"
        } else {
            "subsession"
        },
    })
}

pub fn apply_desktop_relationships(root: &Path, found: &mut [Discovered]) {
    let mut cache = CACHE.lock().unwrap_or_else(|p| p.into_inner());
    let mut present = HashSet::new();
    let mut groups: HashMap<PathBuf, Vec<Entry>> = HashMap::new();
    let mut count = 0;
    for org in std::fs::read_dir(root).into_iter().flatten().flatten() {
        if !org.file_type().is_ok_and(|t| t.is_dir()) {
            continue;
        }
        for user in std::fs::read_dir(org.path())
            .into_iter()
            .flatten()
            .flatten()
        {
            if !user.file_type().is_ok_and(|t| t.is_dir()) {
                continue;
            }
            for file in std::fs::read_dir(user.path())
                .into_iter()
                .flatten()
                .flatten()
            {
                count += 1;
                // Partial registry evidence cannot establish unique relationships.
                if count > MAX_FILES {
                    return;
                }
                if !file.file_type().is_ok_and(|t| t.is_file()) {
                    continue;
                }
                let path = file.path();
                let Ok(meta) = file.metadata() else {
                    continue;
                };
                if path.extension().is_none_or(|x| x != "json") || meta.len() > 4 << 20 {
                    continue;
                }
                let stamp = (
                    meta.modified().unwrap_or(SystemTime::UNIX_EPOCH),
                    meta.len(),
                );
                let entry = match cache.get(&path) {
                    Some((mtime, len, entry)) if (*mtime, *len) == stamp => entry.clone(),
                    _ => {
                        let entry = std::fs::read(&path).ok().and_then(|b| parse(&b));
                        cache.insert(path.clone(), (stamp.0, stamp.1, entry.clone()));
                        entry
                    }
                };
                present.insert(path);
                if let Some(entry) = entry {
                    groups.entry(user.path()).or_default().push(entry);
                }
            }
        }
    }
    cache.retain(|path, _| !path.starts_with(root) || present.contains(path));
    let known: HashSet<_> = found.iter().map(|d| d.id.clone()).collect();
    let mut links: HashMap<String, HashSet<(String, &'static str)>> = HashMap::new();
    for entries in groups.values() {
        let mut ids: HashMap<&str, HashSet<&str>> = HashMap::new();
        for e in entries {
            ids.entry(&e.desktop).or_default().insert(&e.cli);
        }
        for e in entries {
            let Some(parent) = e.parent.as_ref().and_then(|p| ids.get(p.as_str())) else {
                continue;
            };
            if parent.len() != 1 {
                continue;
            }
            let parent = *parent.iter().next().unwrap();
            if parent != e.cli && known.contains(parent) {
                links
                    .entry(e.cli.clone())
                    .or_default()
                    .insert((parent.into(), e.relation));
            }
        }
    }
    for d in found.iter_mut().filter(|d| !d.hidden) {
        if let Some(links) = links.get(&d.id).filter(|l| l.len() == 1) {
            let (parent, relation) = links.iter().next().unwrap();
            d.parent_id = Some(parent.clone());
            d.relation = relation;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn parses_spawned_sessions_and_preserves_user_fork_distinction() {
        let mut v = serde_json::json!({"sessionId":"local_22222222-2222-4222-8222-222222222222", "cliSessionId":"33333333-3333-4333-8333-333333333333", "spawnedFrom":{"sessionId":"local_11111111-1111-4111-8111-111111111111","sideSession":true}});
        assert_eq!(
            parse(v.to_string().as_bytes()).unwrap().relation,
            "subsession"
        );
        v["spawnedFrom"]["userFork"] = true.into();
        assert_eq!(parse(v.to_string().as_bytes()).unwrap().relation, "fork");
        v["cliSessionId"] = "not-a-session".into();
        assert!(parse(v.to_string().as_bytes()).is_none());
    }
}
