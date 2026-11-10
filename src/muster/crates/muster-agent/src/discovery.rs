//! Filesystem invalidation for the read-only session catalog. File events carry
//! paths, not session authority. Shared stores invalidate their own snapshot.
use crate::{
    AgentConfig, catdesk, claude, codex, codex_history, deepseek, model::Provider,
    viewer::Discovered,
};
use std::{
    collections::BTreeSet,
    path::{Path, PathBuf},
};

/// FSEvents uses physical paths (for example /private/var), while callers may
/// configure symlink aliases. Resolve the existing ancestor even after deletion.
pub(crate) fn canonical_path(path: &Path) -> PathBuf {
    for ancestor in path.ancestors() {
        if let Ok(canonical) = std::fs::canonicalize(ancestor) {
            if let Ok(relative) = path.strip_prefix(ancestor) {
                return canonical.join(relative);
            }
        }
    }
    path.to_owned()
}

#[derive(Clone, Default)]
pub(crate) struct Changes {
    pub full: bool,
    pub paths: BTreeSet<PathBuf>,
}
impl Changes {
    pub fn full() -> Self {
        Self {
            full: true,
            ..Default::default()
        }
    }
    pub fn is_empty(&self) -> bool {
        !self.full && self.paths.is_empty()
    }
    pub fn merge(&mut self, other: Self) {
        self.full |= other.full;
        if !self.full {
            self.paths.extend(other.paths);
        }
        // A lost/oversized batch is explicit recovery, never silent loss.
        if self.paths.len() > 4096 {
            self.full = true;
        }
        if self.full {
            self.paths.clear();
        }
    }
    pub fn metadata_changed(&self, config: &AgentConfig) -> bool {
        ["codex", "claude", "deepseek"]
            .iter()
            .any(|p| self.provider_metadata_changed(config, p))
    }
    pub fn provider_metadata_changed(&self, config: &AgentConfig, provider: &str) -> bool {
        self.full
            || match provider {
                "codex" => config.codex_home.as_ref().is_some_and(|home| {
                    store_changed(self, home, "state_5.sqlite")
                        || self.touches(&home.join("session_index.jsonl"))
                        || self.touches(&home.join(".codex-global-state.json"))
                }),
                "claude" => config.claude_desktop.as_ref().is_some_and(|root| {
                    self.touches(root)
                        || root
                            .parent()
                            .is_some_and(|p| self.touches(&p.join("Local Storage/leveldb")))
                }),
                "deepseek" => config
                    .deepseek_home
                    .as_ref()
                    .is_some_and(|home| self.touches(&home.join("storages"))),
                _ => false,
            }
    }
    pub fn watch_root_changed(&self, config: &AgentConfig) -> bool {
        let mut roots = Vec::new();
        if let Some(home) = &config.codex_home {
            roots.extend([
                home.clone(),
                home.join("sessions"),
                home.join("archived_sessions"),
            ]);
        }
        if let Some(home) = &config.claude_home {
            roots.push(home.join("projects"));
        }
        if let Some(root) = &config.claude_desktop {
            roots.push(root.clone());
            if let Some(parent) = root.parent() {
                roots.push(parent.join("Local Storage/leveldb"));
            }
        }
        if let Some(home) = &config.catdesk_home {
            roots.push(home.clone());
        }
        if let Some(home) = &config.deepseek_home {
            roots.extend([home.join("sessions"), home.join("storages")]);
        }
        self.paths
            .iter()
            .any(|path| roots.iter().any(|root| root.starts_with(path)))
    }
    pub fn retain_relevant(&mut self, config: &AgentConfig) {
        if self.full {
            return;
        }
        self.paths.retain(|path| {
            if path
                .file_name()
                .is_some_and(|n| n.to_string_lossy().ends_with("-shm"))
            {
                return false;
            }
            let one = Self {
                full: false,
                paths: [path.clone()].into(),
            };
            one.metadata_changed(config)
                || config.codex_home.as_ref().is_some_and(|home| {
                    one.touches(&home.join("sessions"))
                        || one.touches(&home.join("archived_sessions"))
                        || store_changed(&one, home, "thread_history_1.sqlite")
                })
                || config
                    .claude_home
                    .as_ref()
                    .is_some_and(|home| one.touches(&home.join("projects")))
                || config
                    .catdesk_home
                    .as_ref()
                    .is_some_and(|home| store_changed(&one, home, catdesk::DB))
                || config
                    .deepseek_home
                    .as_ref()
                    .is_some_and(|home| one.touches(&home.join("sessions")))
        });
    }
    pub fn touches(&self, path: &Path) -> bool {
        self.full
            || self
                .paths
                .iter()
                .any(|p| p.starts_with(path) || path.starts_with(p))
    }
}

#[derive(Default)]
pub(crate) struct Sources {
    rows: Vec<Discovered>,
    pub(crate) history: codex_history::RecencyCache,
}
impl Sources {
    pub fn update(
        &mut self,
        config: &AgentConfig,
        heads: &mut (codex::Heads, claude::Heads),
        changes: &Changes,
    ) -> Vec<Discovered> {
        if let Some(home) = &config.codex_home {
            let roots = [home.join("sessions"), home.join("archived_sessions")];
            let paths = scoped_paths(changes, &roots);
            replace_paths(
                &mut self.rows,
                Provider::Codex,
                &paths,
                codex::scan_paths(home, &mut heads.0, &paths),
            );
            if changes.touches(&home.join("session_index.jsonl")) {
                let titles = codex::load_index(&home.join("session_index.jsonl"));
                for row in self
                    .rows
                    .iter_mut()
                    .filter(|r| r.provider == Provider::Codex)
                {
                    if let Some(title) = titles.get(&row.id).filter(|s| !s.is_empty()) {
                        row.meta.title = title.clone();
                    }
                }
            }
            if store_changed(changes, home, "state_5.sqlite")
                || store_changed(changes, home, "thread_history_1.sqlite")
                || changes.touches(&home.join("session_index.jsonl"))
            {
                // A shared SQLite WAL does not identify the changed thread.
                // Refresh only its rows, never walk the transcript roots.
                if changes.full {
                    self.history = Default::default();
                }
                if let Ok(found) = self.history.scan(home) {
                    self.rows.retain(|r| !codex_history::is(r));
                    self.rows.extend(found);
                }
            }
        }
        if let Some(home) = &config.claude_home {
            let mut changes = changes.clone();
            for path in changes.paths.clone() {
                if path.file_name().is_some_and(|n| n == "sessions-index.json") {
                    if let Some(parent) = path.parent() {
                        changes.paths.insert(parent.to_owned());
                    }
                }
            }
            let paths = scoped_paths(&changes, &[home.join("projects")]);
            replace_paths(
                &mut self.rows,
                Provider::Claude,
                &paths,
                claude::scan_paths(home, &mut heads.1, &paths),
            );
        }
        if let Some(home) = &config.catdesk_home {
            if store_changed(changes, home, catdesk::DB) {
                if let Ok(found) = catdesk::scan_db(&home.join(catdesk::DB)) {
                    self.rows.retain(|r| r.provider != Provider::Catdesk);
                    self.rows.extend(found);
                } else if !home.join(catdesk::DB).exists() {
                    self.rows.retain(|r| r.provider != Provider::Catdesk);
                }
            }
        }
        if let Some(home) = &config.deepseek_home {
            let mut paths = scoped_paths(changes, &[home.join("sessions")]);
            // Native projection metadata may rename/archive any session.
            if changes.touches(&home.join("storages")) {
                paths = vec![home.join("sessions")];
            }
            // The source can rotate between plain and compressed generations.
            let root = home.join("sessions");
            for path in &mut paths {
                if let Ok(relative) = path.strip_prefix(&root) {
                    let parts: Vec<_> = relative.components().take(2).collect();
                    *path = parts.iter().fold(root.clone(), |p, c| p.join(c));
                }
            }
            replace_paths(
                &mut self.rows,
                Provider::Deepseek,
                &paths,
                deepseek::scan_paths(home, &paths),
            );
        }
        let paths = self.rows.iter().map(|r| r.path.clone()).collect();
        codex::retain_heads(&mut heads.0, &paths);
        claude::retain_heads(&mut heads.1, &paths);
        let mut rows = self.rows.clone();
        let claude = claude::newest(
            rows.iter()
                .filter(|r| r.provider == Provider::Claude)
                .cloned()
                .collect(),
        );
        rows.retain(|r| r.provider != Provider::Claude);
        rows.extend(claude);
        // Keep all source candidates so deleting the newest rollout exposes an
        // older segment. Apply the existing migrated-store precedence to output.
        let paginated: BTreeSet<_> = rows
            .iter()
            .filter(|r| codex_history::is(r) && r.entrypoint.as_deref() == Some("paginated"))
            .map(|r| r.id.clone())
            .collect();
        let files: BTreeSet<_> = rows
            .iter()
            .filter(|r| r.provider == Provider::Codex && !codex_history::is(r))
            .map(|r| r.id.clone())
            .collect();
        rows.retain(|r| {
            if codex_history::is(r) {
                paginated.contains(&r.id) || !files.contains(&r.id)
            } else {
                r.provider != Provider::Codex || !paginated.contains(&r.id)
            }
        });
        rows
    }
}

fn store_changed(changes: &Changes, home: &Path, name: &str) -> bool {
    [name.to_owned(), format!("{name}-wal")]
        .iter()
        .any(|n| changes.touches(&home.join(n)))
}
fn scoped_paths(changes: &Changes, roots: &[PathBuf]) -> Vec<PathBuf> {
    let mut paths = BTreeSet::new();
    for root in roots {
        if changes.full {
            paths.insert(root.clone());
            continue;
        }
        for path in &changes.paths {
            if path.starts_with(root) {
                paths.insert(path.clone());
            } else if root.starts_with(path) {
                paths.insert(root.clone());
            }
        }
    }
    let mut minimal: Vec<PathBuf> = Vec::new();
    for path in paths {
        if !minimal.iter().any(|p| path.starts_with(p)) {
            minimal.push(path);
        }
    }
    minimal
}
fn replace_paths(
    rows: &mut Vec<Discovered>,
    provider: Provider,
    paths: &[PathBuf],
    found: Vec<Discovered>,
) {
    rows.retain(|r| r.provider != provider || !paths.iter().any(|p| r.path.starts_with(p)));
    rows.extend(found);
}

#[cfg(test)]
mod tests {
    use super::*;
    fn config(root: &Path) -> AgentConfig {
        AgentConfig {
            codex_home: Some(root.join("codex")),
            claude_home: Some(root.join("claude")),
            claude_desktop: None,
            catdesk_home: None,
            deepseek_home: None,
            ..Default::default()
        }
    }
    fn change(path: &Path) -> Changes {
        Changes {
            full: false,
            paths: [path.to_owned()].into(),
        }
    }
    fn claude_file(path: &Path, title: &str) {
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(
            path,
            format!(
                "{{\"type\":\"user\",\"message\":{{\"role\":\"user\",\"content\":\"{title}\"}}}}\n"
            ),
        )
        .unwrap();
    }
    #[test]
    fn sqlite_wal_and_native_metadata_are_relevant_but_runtime_noise_is_not() {
        let temp = tempfile::tempdir().unwrap();
        let mut cfg = config(temp.path());
        cfg.claude_desktop = Some(temp.path().join("desktop/claude-code-sessions"));
        let home = cfg.codex_home.as_ref().unwrap();
        for name in [
            "state_5.sqlite-wal",
            "thread_history_1.sqlite-wal",
            "session_index.jsonl",
            ".codex-global-state.json",
        ] {
            let mut c = change(&home.join(name));
            c.retain_relevant(&cfg);
            assert!(!c.is_empty(), "{name}");
        }
        for name in [
            "state_5.sqlite-shm",
            "logs/app.log",
            "shell_snapshots/current",
        ] {
            let mut c = change(&home.join(name));
            c.retain_relevant(&cfg);
            assert!(c.is_empty(), "{name}");
        }
        assert!(
            change(&temp.path().join("desktop/Local Storage/leveldb/000001.log"))
                .metadata_changed(&cfg)
        );
        assert!(!change(&home.join("sessions/day/rollout.jsonl")).metadata_changed(&cfg));
        assert!(change(&home.join("sessions")).watch_root_changed(&cfg));
    }

    #[cfg(unix)]
    #[test]
    fn event_aliases_remain_stable_when_the_notified_file_has_been_deleted() {
        let temp = tempfile::tempdir().unwrap();
        let real = temp.path().join("real");
        std::fs::create_dir(&real).unwrap();
        let alias = temp.path().join("alias");
        std::os::unix::fs::symlink(&real, &alias).unwrap();
        std::fs::write(real.join("transcript"), "text").unwrap();
        let before = canonical_path(&alias.join("transcript"));
        std::fs::remove_file(real.join("transcript")).unwrap();
        assert_eq!(canonical_path(&alias.join("transcript")), before);
        assert_eq!(before, canonical_path(&real).join("transcript"));
    }

    #[test]
    fn deleting_a_claude_duplicate_recovers_the_other_project_source() {
        let temp = tempfile::tempdir().unwrap();
        let cfg = config(temp.path());
        let root = cfg.claude_home.as_ref().unwrap().join("projects");
        let name = "11111111-1111-4111-8111-111111111111.jsonl";
        let a = root.join("alpha").join(name);
        let b = root.join("beta").join(name);
        claude_file(&a, "alpha");
        claude_file(&b, "beta");
        let mut sources = Sources::default();
        let mut heads = Default::default();
        let initial = sources.update(&cfg, &mut heads, &Changes::full());
        assert_eq!(initial.len(), 1);
        let chosen = initial[0].path.clone();
        std::fs::remove_file(&chosen).unwrap();
        let remaining = sources.update(&cfg, &mut heads, &change(&chosen));
        assert_eq!(remaining.len(), 1);
        assert_ne!(remaining[0].path, chosen);
    }

    #[test]
    fn updates_only_changed_sources_and_handles_move_delete_and_new_directory() {
        let temp = tempfile::tempdir().unwrap();
        let cfg = config(temp.path());
        let root = cfg.claude_home.as_ref().unwrap().join("projects");
        let a = root.join("alpha/11111111-1111-4111-8111-111111111111.jsonl");
        let b = root.join("beta/22222222-2222-4222-8222-222222222222.jsonl");
        claude_file(&a, "alpha");
        claude_file(&b, "beta");
        let mut sources = Sources::default();
        let mut heads = Default::default();
        assert_eq!(sources.update(&cfg, &mut heads, &Changes::full()).len(), 2);
        // A path event must not read a different source, even if that file changed.
        claude_file(&a, "alpha changed");
        claude_file(&b, "beta unseen");
        let rows = sources.update(&cfg, &mut heads, &change(&a));
        assert_eq!(
            rows.iter().find(|r| r.path == a).unwrap().meta.title,
            "alpha changed"
        );
        assert_eq!(
            rows.iter().find(|r| r.path == b).unwrap().meta.title,
            "beta"
        );
        let moved = root.join("gamma").join(a.file_name().unwrap());
        std::fs::create_dir_all(moved.parent().unwrap()).unwrap();
        std::fs::rename(&a, &moved).unwrap();
        let mut changes = change(&a);
        changes.merge(change(moved.parent().unwrap()));
        let rows = sources.update(&cfg, &mut heads, &changes);
        assert_eq!(rows.len(), 2);
        assert!(rows.iter().any(|r| r.path == moved));
        std::fs::remove_dir_all(moved.parent().unwrap()).unwrap();
        let rows = sources.update(&cfg, &mut heads, &change(moved.parent().unwrap()));
        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0].path, b);
        // Explicit lost-event recovery repairs the unreported edit.
        let rows = sources.update(&cfg, &mut heads, &Changes::full());
        assert_eq!(rows[0].meta.title, "beta unseen");
    }
    #[test]
    fn transcript_write_does_not_walk_other_codex_sources_and_delete_keeps_duplicate() {
        let temp = tempfile::tempdir().unwrap();
        let cfg = config(temp.path());
        let home = cfg.codex_home.as_ref().unwrap();
        let id = "11111111-1111-4111-8111-111111111111";
        let a = home.join(format!("sessions/day-a/rollout-{id}.jsonl"));
        let b = home.join(format!("sessions/day-b/rollout-{id}.jsonl"));
        let write = |path: &Path, title: &str| {
            std::fs::create_dir_all(path.parent().unwrap()).unwrap();
            std::fs::write(path, format!("{{\"type\":\"session_meta\",\"payload\":{{\"id\":\"{id}\"}}}}\n{{\"type\":\"turn_context\",\"payload\":{{}}}}\n{{\"type\":\"response_item\",\"payload\":{{\"type\":\"message\",\"role\":\"user\",\"content\":[{{\"type\":\"input_text\",\"text\":\"{title}\"}}]}}}}\n")).unwrap();
        };
        write(&a, "older");
        write(&b, "newer");
        let mut sources = Sources::default();
        let mut heads = Default::default();
        assert_eq!(sources.update(&cfg, &mut heads, &Changes::full()).len(), 2);
        write(&a, "changed");
        write(&b, "unreported");
        let rows = sources.update(&cfg, &mut heads, &change(&a));
        assert_eq!(
            rows.iter().find(|r| r.path == b).unwrap().meta.title,
            "newer"
        );
        std::fs::remove_file(&a).unwrap();
        let rows = sources.update(&cfg, &mut heads, &change(&a));
        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0].path, b);
        std::fs::write(
            home.join("session_index.jsonl"),
            format!("{{\"id\":\"{id}\",\"thread_name\":\"renamed\"}}\n"),
        )
        .unwrap();
        let rows = sources.update(&cfg, &mut heads, &change(&home.join("session_index.jsonl")));
        assert_eq!(rows[0].meta.title, "renamed");
    }
    #[test]
    fn bursts_coalesce_without_losing_paths_and_overflow_requests_recovery() {
        let mut changes = Changes::default();
        for i in 0..100 {
            changes.merge(change(Path::new(&format!("/synthetic/{i}"))));
        }
        assert_eq!(changes.paths.len(), 100);
        assert!(!changes.full);
        changes.merge(change(Path::new("/synthetic/1")));
        assert_eq!(changes.paths.len(), 100);
        for i in 100..4100 {
            changes.merge(change(Path::new(&format!("/synthetic/{i}"))));
        }
        assert!(changes.full);
        assert!(changes.paths.is_empty());
    }
}
