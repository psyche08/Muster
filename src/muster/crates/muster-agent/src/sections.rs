//! Read-only native sidebar metadata. This is display data, never authority.
use crate::{AgentConfig, claude_identity, viewer::is_uuid};
use muster_proto::v1::{AgentSection, AgentSession};
use rusqlite::{Connection, OpenFlags};
use rusty_leveldb::{DB, MemEnv, Options, env::Env};
use serde_json::Value;
use std::{
    collections::{HashMap, HashSet},
    path::{Path, PathBuf},
    rc::Rc,
    time::{Duration, SystemTime},
};

#[derive(Clone, Default, PartialEq)]
pub(crate) struct Catalog {
    pub sections: Vec<AgentSection>,
    pub members: HashMap<(String, String), (String, i64)>,
    pub notices: Vec<(String, String)>,
    pub pinned: HashSet<(String, String)>,
    pub titles: HashMap<(String, String), String>,
}
impl Catalog {
    /// A failed read is not a new empty native state. Retain only that provider's
    /// last complete display snapshot; mutation preparation reads afresh.
    pub fn retain_unavailable(&mut self, previous: &Self) {
        for (provider, _) in &self.notices {
            self.sections.extend(
                previous
                    .sections
                    .iter()
                    .filter(|s| &s.provider == provider)
                    .cloned(),
            );
            self.members.extend(
                previous
                    .members
                    .iter()
                    .filter(|((p, _), _)| p == provider)
                    .map(|(k, v)| (k.clone(), v.clone())),
            );
            self.pinned.extend(
                previous
                    .pinned
                    .iter()
                    .filter(|(p, _)| p == provider)
                    .cloned(),
            );
            self.titles.extend(
                previous
                    .titles
                    .iter()
                    .filter(|((p, _), _)| p == provider)
                    .map(|(k, v)| (k.clone(), v.clone())),
            );
        }
    }

    pub fn apply(&self, session: &mut AgentSession) {
        session.pinned = self
            .pinned
            .contains(&(session.provider.clone(), session.id.clone()));
        if let Some((id, position)) = self
            .members
            .get(&(session.provider.clone(), session.id.clone()))
        {
            session.section_id = id.clone();
            session.section_position = *position;
        } else {
            session.section_id.clear();
            session.section_position = 0;
        }
    }
}
fn read_registry(path: &Path) -> Option<Value> {
    if std::fs::metadata(path).ok()?.len() > 4 << 20 {
        return None;
    }
    serde_json::from_slice(&std::fs::read(path).ok()?).ok()
}
fn read_title_file(path: &Path) -> Option<String> {
    read_registry(path)?["title"]
        .as_str()
        .filter(|s| !s.trim().is_empty())
        .map(str::to_owned)
}

/// One UUID's current native label, never a title-based session lookup.
pub(crate) fn native_title(config: &AgentConfig, provider: &str, id: &str) -> Option<String> {
    if !is_uuid(id) {
        return None;
    }
    match provider {
        "codex" => {
            let db = Connection::open_with_flags(
                config.codex_home.as_ref()?.join("state_5.sqlite"),
                OpenFlags::SQLITE_OPEN_READ_ONLY,
            )
            .ok()?;
            db.busy_timeout(Duration::from_millis(100)).ok()?;
            db.query_row(
                "SELECT title FROM threads WHERE id=?1 AND archived=0",
                [id],
                |r| r.get::<_, String>(0),
            )
            .ok()
            .filter(|s| !s.trim().is_empty())
        }
        "claude" => {
            let root = config.claude_desktop.as_ref()?;
            let desktop = claude_identity::desktop_for_cli(root, id)?;
            let mut matches = Vec::new();
            for org in std::fs::read_dir(root).ok()?.flatten() {
                if !org.file_type().ok()?.is_dir() {
                    continue;
                }
                for account in std::fs::read_dir(org.path()).ok()?.flatten() {
                    let path = account.path().join(format!("{desktop}.json"));
                    if path.is_file() {
                        matches.push(path);
                    }
                }
            }
            match matches.as_slice() {
                [one] => read_title_file(one),
                _ => None,
            }
        }
        _ => None,
    }
}
pub(crate) fn read(config: &AgentConfig) -> Catalog {
    let mut out = Catalog::default();
    if let Some(home) = &config.codex_home {
        match codex(home) {
            Ok(c) => merge(&mut out, c),
            Err(_) => out.notices.push((
                "codex".into(),
                "Codex native sections temporarily unavailable".into(),
            )),
        }
    }
    if let Some(root) = &config.claude_desktop {
        match claude(root) {
            Some(c) => merge(&mut out, c),
            None => out.notices.push((
                "claude".into(),
                "Claude native sections temporarily unavailable".into(),
            )),
        }
    }
    out
}
fn merge(out: &mut Catalog, c: Catalog) {
    out.sections.extend(c.sections);
    out.members.extend(c.members);
    out.pinned.extend(c.pinned);
    out.titles.extend(c.titles);
}
fn section(
    provider: &str,
    id: String,
    title: String,
    position: i64,
    collapsed: bool,
) -> AgentSection {
    AgentSection {
        kind: "section".into(),
        provider: provider.into(),
        id,
        title,
        position,
        collapsed,
        ..Default::default()
    }
}
fn codex(home: &Path) -> rusqlite::Result<Catalog> {
    let mut db = Connection::open_with_flags(
        home.join("state_5.sqlite"),
        OpenFlags::SQLITE_OPEN_READ_ONLY,
    )?;
    db.busy_timeout(Duration::from_millis(100))?;
    let tx = db.transaction()?;
    let mut out = Catalog::default();
    let mut q = tx.prepare("SELECT id, name FROM thread_sections ORDER BY id LIMIT 1000")?;
    for row in q.query_map([], |r| Ok((r.get::<_, String>(0)?, r.get::<_, String>(1)?)))? {
        let (id, name) = row?;
        if !id.is_empty() {
            let mut g = section(
                "codex",
                format!("section:{id}"),
                name,
                out.sections.len() as i64,
                false,
            );
            if id == crate::catalog::CODEX_PINNED_SECTION {
                g.kind = "pin".into();
            }
            out.sections.push(g);
        }
    }
    let mut q =
        tx.prepare("SELECT id, name, position FROM projects ORDER BY position, id LIMIT 1000")?;
    for row in q.query_map([], |r| {
        Ok((
            r.get::<_, String>(0)?,
            r.get::<_, String>(1)?,
            r.get::<_, i64>(2)?,
        ))
    })? {
        let (id, name, pos) = row?;
        let mut g = section(
            "codex",
            format!("project:{id}"),
            name,
            1000_i64.saturating_add(pos),
            false,
        );
        g.kind = "project".into();
        out.sections.push(g);
    }
    let known: HashSet<_> = out.sections.iter().map(|s| s.id.clone()).collect();
    let mut q = tx.prepare("SELECT id, thread_section_id, project_id, section_position FROM threads WHERE archived = 0 LIMIT 100001")?;
    for row in q.query_map([], |r| {
        Ok((
            r.get::<_, String>(0)?,
            r.get::<_, Option<String>>(1)?,
            r.get::<_, Option<String>>(2)?,
            r.get::<_, Option<i64>>(3)?,
        ))
    })? {
        let (id, sec, project, pos) = row?;
        let group = sec
            .map(|s| format!("section:{s}"))
            .or_else(|| project.map(|s| format!("project:{s}")));
        if is_uuid(&id)
            && let Some(group) = group.filter(|s| known.contains(s))
        {
            if group == format!("section:{}", crate::catalog::CODEX_PINNED_SECTION) {
                out.pinned.insert(("codex".into(), id.clone()));
            }
            out.members
                .insert(("codex".into(), id), (group, pos.unwrap_or(i64::MAX)));
        }
    }
    // Older provider schemas may not have this column. Missing pin metadata
    // must not discard otherwise valid native sections.
    if !out.sections.iter().any(|g| g.kind == "pin")
        && let Ok(mut q) =
            tx.prepare("SELECT id FROM threads WHERE archived=0 AND is_pinned=1 LIMIT 100001")
        && let Ok(rows) = q.query_map([], |r| r.get::<_, String>(0))
    {
        for id in rows.flatten().filter(|id| is_uuid(id)) {
            out.pinned.insert(("codex".into(), id));
        }
    }
    if let Ok(mut q) = tx.prepare("SELECT id,title FROM threads WHERE archived=0 LIMIT 100001")
        && let Ok(rows) = q.query_map([], |r| Ok((r.get::<_, String>(0)?, r.get::<_, String>(1)?)))
    {
        for (id, title) in rows
            .flatten()
            .filter(|(id, title)| is_uuid(id) && !title.trim().is_empty())
        {
            out.titles.insert(("codex".into(), id), title);
        }
    }
    codex_projects(home, &tx, &mut out);
    Ok(out)
}

fn codex_projects(home: &Path, db: &Connection, out: &mut Catalog) {
    let state_path = home.join(".codex-global-state.json");
    let state: Value = std::fs::metadata(&state_path)
        .ok()
        .filter(|m| m.len() <= 16 << 20)
        .and_then(|_| std::fs::read(&state_path).ok())
        .and_then(|b| serde_json::from_slice(&b).ok())
        .unwrap_or_default();
    let host = format!("local:{}", home.display());
    let migrated = &state["app-server-project-id-by-legacy-project-id-by-host"][&host];
    let known: HashSet<_> = out.sections.iter().map(|s| s.id.clone()).collect();
    let projectless: HashSet<_> = state["projectless-thread-ids"]
        .as_array()
        .into_iter()
        .flatten()
        .filter_map(Value::as_str)
        .collect();
    let mut roots: HashMap<String, HashSet<String>> = HashMap::new();
    if let Ok(mut q) = db.prepare("SELECT project_id,path FROM project_roots LIMIT 10000")
        && let Ok(rows) = q.query_map([], |r| Ok((r.get::<_, String>(0)?, r.get::<_, String>(1)?)))
    {
        for (id, path) in rows.flatten() {
            roots
                .entry(path)
                .or_default()
                .insert(format!("project:{id}"));
        }
    }
    if let Ok(mut q) = db.prepare("SELECT id,cwd FROM threads WHERE archived=0 LIMIT 100001")
        && let Ok(rows) = q.query_map([], |r| Ok((r.get::<_, String>(0)?, r.get::<_, String>(1)?)))
    {
        for (id, cwd) in rows.flatten() {
            let key = ("codex".into(), id.clone());
            if !is_uuid(&id) || out.members.contains_key(&key) || projectless.contains(id.as_str())
            {
                continue;
            }
            let membership_host = state["thread-project-membership-host-ids"][&id].as_str();
            if membership_host.is_some_and(|h| h != "local" && h != host) {
                continue;
            }
            let assignment = &state["thread-project-assignments"][&id];
            if assignment["hostId"]
                .as_str()
                .is_some_and(|h| h != "local" && h != host)
            {
                continue;
            }
            let assigned = if assignment["projectKind"] == "local" {
                assignment["projectId"]
                    .as_str()
                    .map(|p| format!("project:{}", migrated[p].as_str().unwrap_or(p)))
            } else if assignment.is_object() {
                continue;
            } else {
                let path = state["thread-workspace-root-hints"][&id]
                    .as_str()
                    .unwrap_or(&cwd);
                roots
                    .get(path)
                    .filter(|s| s.len() == 1)
                    .and_then(|s| s.iter().next().cloned())
            };
            if let Some(group) = assigned.filter(|p| known.contains(p)) {
                out.members.insert(key, (group, i64::MAX));
            }
        }
    }
}

// Chromium is never opened by LevelDB: copy a bounded stable file set into
// MemEnv. Recovery/compaction, locks and writes can touch only this RAM snapshot.
type Stamp = Vec<(PathBuf, u64, SystemTime)>;
fn stamp(root: &Path) -> Option<Stamp> {
    let mut out = Vec::new();
    let mut total = 0;
    for e in std::fs::read_dir(root).ok()? {
        let e = e.ok()?;
        let name = e.file_name();
        let name = name.to_str()?;
        if !(name == "CURRENT"
            || name.starts_with("MANIFEST-")
            || name.ends_with(".ldb")
            || name.ends_with(".log"))
        {
            continue;
        }
        if !e.file_type().ok()?.is_file() {
            return None;
        }
        let m = e.metadata().ok()?;
        total += m.len();
        if total > 64 << 20 || out.len() >= 512 {
            return None;
        }
        out.push((e.path(), m.len(), m.modified().ok()?));
    }
    out.sort();
    Some(out)
}
fn local_storage(root: &Path) -> Option<Value> {
    let before = stamp(root)?;
    let env = MemEnv::new();
    let memory = Path::new("sidebar");
    env.mkdir(memory).ok()?;
    for (path, len, _) in &before {
        use std::io::Read;
        let mut bytes = Vec::new();
        std::fs::File::open(path)
            .ok()?
            .take(len + 1)
            .read_to_end(&mut bytes)
            .ok()?;
        if bytes.len() as u64 != *len {
            return None;
        }
        env.open_writable_file(&memory.join(path.file_name()?))
            .ok()?
            .write_all(&bytes)
            .ok()?;
    }
    if stamp(root)? != before {
        return None;
    }
    let options = Options {
        env: Rc::new(Box::new(env)),
        create_if_missing: false,
        paranoid_checks: true,
        ..Options::default()
    };
    let bytes = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        let mut db = DB::open(memory, options).ok()?;
        db.get(b"_https://claude.ai\0\x01dframe-store")
    }))
    .ok()??;
    if bytes.len() > 4 << 20 {
        return None;
    }
    let text = match bytes.first()? {
        1 => bytes[1..].iter().map(|b| char::from(*b)).collect(),
        0 if (bytes.len() - 1) % 2 == 0 => String::from_utf16(
            &bytes[1..]
                .chunks_exact(2)
                .map(|b| u16::from_le_bytes([b[0], b[1]]))
                .collect::<Vec<_>>(),
        )
        .ok()?,
        _ => return None,
    };
    serde_json::from_str(&text).ok()
}
fn claude(root: &Path) -> Option<Catalog> {
    let state = local_storage(&root.parent()?.join("Local Storage/leveldb"))?;
    claude_state(root, &state["state"])
}
fn claude_state(root: &Path, state: &Value) -> Option<Catalog> {
    let scope = state["lastSidebarScopeKey"].as_str()?;
    let (org, user) = scope.split_once('/')?;
    if !is_uuid(user) || !is_uuid(org) {
        return None;
    }
    let entries = state["codeSidebarByScope"][scope]["sections"].as_array()?;
    if entries.len() > 1000 {
        return None;
    }
    let mut entries: Vec<_> = entries.iter().collect();
    entries.sort_by_key(|s| match s["kind"].as_str() {
        Some("pinned") => 0,
        Some("manual") => 1,
        _ => 2,
    });
    let account = root.join(org).join(user);
    let mut out = Catalog::default();
    let mapping = claude_identity::mappings(root)?;
    let mut ids = HashMap::new();
    for (desktop, cli) in &mapping {
        // Reject ambiguity in either direction, and other account registries.
        if account.join(format!("{desktop}.json")).is_file()
            && mapping.iter().filter(|(d, _)| d == desktop).count() == 1
            && mapping.iter().filter(|(_, c)| c == cli).count() == 1
        {
            let record = read_registry(&account.join(format!("{desktop}.json")))?;
            if record["sessionId"] != *desktop || record["cliSessionId"] != *cli {
                return None;
            }
            ids.insert(desktop.as_str(), cli.as_str());
            if let Some(title) = record["title"].as_str().filter(|s| !s.trim().is_empty()) {
                out.titles
                    .insert(("claude".into(), cli.clone()), title.to_owned());
            }
            if let Some(starred) = record.get("isStarred") {
                let pinned = starred.as_bool()?;
                if pinned {
                    out.pinned.insert(("claude".into(), cli.clone()));
                }
            }
        }
    }
    let mut assigned = HashSet::new();
    for s in entries {
        let id = s["id"].as_str()?;
        if id.len() > 256 || id.is_empty() {
            return None;
        }
        let kind = s["kind"].as_str()?;
        if !matches!(kind, "manual" | "pinned" | "sessions") {
            continue;
        }
        let title = s["name"]
            .as_str()
            .filter(|s| !s.is_empty())
            .unwrap_or(if kind == "pinned" {
                "Pinned"
            } else {
                "Recents"
            });
        let section_id = format!("{scope}:{id}");
        let mut metadata = section(
            "claude",
            section_id.clone(),
            title.into(),
            s["order"].as_i64().unwrap_or(0),
            s["collapsed"] == true,
        );
        metadata.kind = match kind {
            "pinned" => "pin",
            "sessions" => "recent",
            _ => "section",
        }
        .into();
        out.sections.push(metadata);
        // pinnedOrder is presentation order, never proof of membership.
        let members = if kind == "pinned" {
            None
        } else {
            s["members"].as_array()
        };
        for (pos, member) in members.into_iter().flatten().enumerate().take(10000) {
            let Some(desktop) = member
                .as_str()
                .map(|s| s.strip_prefix("code:").unwrap_or(s))
            else {
                continue;
            };
            if let Some(cli) = ids.get(desktop) {
                if assigned.insert(*cli) {
                    out.members.insert(
                        ("claude".into(), (*cli).into()),
                        (section_id.clone(), pos as i64),
                    );
                }
            }
        }
        if kind == "sessions" {
            for cli in ids.values() {
                if assigned.insert(*cli) {
                    out.members.insert(
                        ("claude".into(), (*cli).into()),
                        (section_id.clone(), i64::MAX),
                    );
                }
            }
        }
    }
    Some(out)
}

#[cfg(test)]
mod tests {
    use super::*;
    const A: &str = "11111111-1111-4111-8111-111111111111";
    const B: &str = "22222222-2222-4222-8222-222222222222";
    const USER: &str = "33333333-3333-4333-8333-333333333333";
    const ORG: &str = "44444444-4444-4444-8444-444444444444";
    #[test]
    fn unavailable_provider_keeps_display_snapshot_but_successful_removal_clears_membership() {
        let key = ("claude".into(), A.into());
        let previous = Catalog {
            sections: vec![section(
                "claude",
                "group".into(),
                "Example".into(),
                0,
                false,
            )],
            members: HashMap::from([(key.clone(), ("group".into(), 3))]),
            pinned: HashSet::from([key]),
            ..Default::default()
        };
        let mut failed = Catalog {
            notices: vec![("claude".into(), "unavailable".into())],
            ..Default::default()
        };
        failed.retain_unavailable(&previous);
        assert_eq!(failed.sections, previous.sections);
        let mut session = AgentSession {
            id: A.into(),
            provider: "claude".into(),
            ..Default::default()
        };
        failed.apply(&mut session);
        assert_eq!(session.section_id, "group");
        assert!(session.pinned);
        let mut successful = Catalog::default();
        successful.retain_unavailable(&previous);
        successful.apply(&mut session);
        assert!(session.section_id.is_empty());
        assert!(!session.pinned);
        assert_eq!(session.section_position, 0);
    }
    #[test]
    fn codex_membership_is_by_uuid_and_keeps_duplicate_section_labels() {
        let dir = tempfile::tempdir().unwrap();
        let db = Connection::open(dir.path().join("state_5.sqlite")).unwrap();
        db.execute_batch("CREATE TABLE thread_sections(id TEXT,name TEXT); CREATE TABLE projects(id TEXT,name TEXT,position INTEGER); CREATE TABLE threads(id TEXT,thread_section_id TEXT,project_id TEXT,section_position INTEGER,archived INTEGER);
          INSERT INTO thread_sections VALUES('one','Same'),('two','Same'); INSERT INTO projects VALUES('project','Same',0);").unwrap();
        db.execute("INSERT INTO threads VALUES(?1,'two','project',7,0)", [A])
            .unwrap();
        db.execute("INSERT INTO threads VALUES(?1,NULL,'project',NULL,0)", [B])
            .unwrap();
        let c = codex(dir.path()).unwrap();
        assert_eq!(c.sections.len(), 3);
        assert_eq!(
            c.members[&("codex".into(), A.into())],
            ("section:two".into(), 7)
        );
        assert_eq!(c.members[&("codex".into(), B.into())].0, "project:project");
    }
    #[test]
    fn codex_projects_use_local_uuid_assignments_and_unambiguous_full_paths() {
        let dir = tempfile::tempdir().unwrap();
        let db = Connection::open(dir.path().join("state_5.sqlite")).unwrap();
        db.execute_batch("CREATE TABLE thread_sections(id TEXT,name TEXT); CREATE TABLE projects(id TEXT,name TEXT,position INTEGER); CREATE TABLE project_roots(project_id TEXT,path TEXT); CREATE TABLE threads(id TEXT,thread_section_id TEXT,project_id TEXT,section_position INTEGER,archived INTEGER,cwd TEXT);
        INSERT INTO projects VALUES('one','Same',0),('two','Same',1);
        INSERT INTO project_roots VALUES('one','/workspace/one/app'),('two','/workspace/two/app'),('one','/workspace/shared'),('two','/workspace/shared');").unwrap();
        let ids: Vec<_> = (0..8)
            .map(|i| format!("10000000-0000-4000-8000-{i:012}"))
            .collect();
        for (id, path) in ids.iter().zip([
            "/workspace/one/app",
            "/workspace/two/app",
            "/workspace/shared",
            "/workspace/one/app",
            "/workspace/one/app",
            "/workspace/one/app",
            "/workspace/moved",
            "/workspace/one/app",
        ]) {
            db.execute(
                "INSERT INTO threads VALUES(?1,NULL,NULL,NULL,0,?2)",
                [id.as_str(), path],
            )
            .unwrap();
        }
        let host = format!("local:{}", dir.path().display());
        let state = serde_json::json!({
            "app-server-project-id-by-legacy-project-id-by-host":{host:{"legacy":"two"}},
            "thread-project-assignments":{&ids[0]:{"projectKind":"local","projectId":"legacy"},&ids[4]:{"projectKind":"remote","projectId":"one"},&ids[7]:{"projectKind":"local","projectId":"one","hostId":"remote-host"}},
            "projectless-thread-ids":[ids[3]],
            "thread-project-membership-host-ids":{&ids[5]:"remote-host"},
            "thread-workspace-root-hints":{&ids[6]:"/workspace/one/app"}
        });
        std::fs::write(
            dir.path().join(".codex-global-state.json"),
            state.to_string(),
        )
        .unwrap();
        let c = codex(dir.path()).unwrap();
        assert_eq!(c.members.len(), 3);
        for (i, expected) in [(0, "project:two"), (1, "project:two"), (6, "project:one")] {
            assert_eq!(c.members[&("codex".into(), ids[i].clone())].0, expected);
        }
    }
    #[test]
    fn claude_membership_rejects_remote_other_account_and_ambiguous_ids() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();
        let account = root.join(ORG).join(USER);
        std::fs::create_dir_all(&account).unwrap();
        let desktop = format!("local_{A}");
        std::fs::write(
            account.join(format!("{desktop}.json")),
            serde_json::json!({"sessionId":desktop,"cliSessionId":B}).to_string(),
        )
        .unwrap();
        let scope = format!("{ORG}/{USER}");
        let state = serde_json::json!({"lastSidebarScopeKey":scope,"codeSidebarByScope":{&scope:{"sections":[{"id":"group","kind":"manual","name":"Same","order":0,"members":[format!("code:{desktop}"),"session_remote"]},{"id":"empty","kind":"manual","name":"Same","members":[]},{"id":"sessions","kind":"sessions"}]}}});
        let c = claude_state(root, &state).unwrap();
        assert_eq!(c.sections.len(), 3);
        assert_eq!(c.members.len(), 1);
        assert!(
            c.members[&("claude".into(), B.into())]
                .0
                .ends_with(":group")
        );
        let other = root.join(ORG).join(A);
        std::fs::create_dir_all(&other).unwrap();
        std::fs::write(
            other.join(format!("{desktop}.json")),
            serde_json::json!({"sessionId":desktop,"cliSessionId":B}).to_string(),
        )
        .unwrap();
        assert!(claude_state(root, &state).unwrap().members.is_empty());
    }
    #[test]
    fn codex_pin_metadata_survives_without_section_membership() {
        let dir = tempfile::tempdir().unwrap();
        let db = Connection::open(dir.path().join("state_5.sqlite")).unwrap();
        db.execute_batch("CREATE TABLE thread_sections(id TEXT,name TEXT); CREATE TABLE projects(id TEXT,name TEXT,position INTEGER); CREATE TABLE threads(id TEXT,thread_section_id TEXT,project_id TEXT,section_position INTEGER,archived INTEGER,is_pinned INTEGER);").unwrap();
        db.execute("INSERT INTO threads VALUES(?1,NULL,NULL,NULL,0,1)", [A])
            .unwrap();
        db.execute("INSERT INTO threads VALUES(?1,NULL,NULL,NULL,1,1)", [B])
            .unwrap();
        let c = codex(dir.path()).unwrap();
        let mut a = AgentSession {
            id: A.into(),
            provider: "codex".into(),
            ..Default::default()
        };
        c.apply(&mut a);
        assert!(a.pinned);
        assert!(a.section_id.is_empty());
        assert!(!c.pinned.contains(&("codex".into(), B.into())));
    }

    #[test]
    fn claude_registry_star_is_authoritative_over_pinned_order() {
        let dir = tempfile::tempdir().unwrap();
        let account = dir.path().join(ORG).join(USER);
        std::fs::create_dir_all(&account).unwrap();
        let desktop = format!("local_{A}");
        let file = account.join(format!("{desktop}.json"));
        let scope = format!("{ORG}/{USER}");
        let mut state = serde_json::json!({"lastSidebarScopeKey":scope,"pinnedOrder":[],
            "codeSidebarByScope":{&scope:{"sections":[{"id":"pinned","kind":"pinned"},{"id":"sessions","kind":"sessions"}]}}});
        let mut record = serde_json::json!({"sessionId":desktop,"cliSessionId":B,"isStarred":true});
        std::fs::write(&file, record.to_string()).unwrap();
        assert!(
            claude_state(dir.path(), &state)
                .unwrap()
                .pinned
                .contains(&("claude".into(), B.into()))
        );
        state["pinnedOrder"] = serde_json::json!([format!("code:{desktop}")]);
        record["isStarred"] = serde_json::json!(false);
        std::fs::write(&file, record.to_string()).unwrap();
        assert!(
            !claude_state(dir.path(), &state)
                .unwrap()
                .pinned
                .contains(&("claude".into(), B.into()))
        );
        record.as_object_mut().unwrap().remove("isStarred");
        std::fs::write(&file, record.to_string()).unwrap();
        assert!(
            !claude_state(dir.path(), &state)
                .unwrap()
                .pinned
                .contains(&("claude".into(), B.into()))
        );
        record["isStarred"] = serde_json::json!("invalid");
        std::fs::write(&file, record.to_string()).unwrap();
        assert!(claude_state(dir.path(), &state).is_none());
    }
    #[test]
    fn claude_pin_is_independent_of_custom_membership_and_visibility() {
        let dir = tempfile::tempdir().unwrap();
        let account = dir.path().join(ORG).join(USER);
        std::fs::create_dir_all(&account).unwrap();
        let desktop = format!("local_{A}");
        std::fs::write(
            account.join(format!("{desktop}.json")),
            serde_json::json!({"sessionId": desktop, "cliSessionId": B, "isStarred":true})
                .to_string(),
        )
        .unwrap();
        let scope = format!("{ORG}/{USER}");
        let state = serde_json::json!({"lastSidebarScopeKey": scope,
        "pinnedOrder": [format!("code:{desktop}"), "code:session_remote"],
        "codeSidebarByScope": {&scope: {"sections": [
            {"id":"pinned","kind":"pinned","shown":false},
            {"id":"custom","kind":"manual","name":"Recents","members":[format!("code:{desktop}")]},
            {"id":"sessions","kind":"sessions"}
        ]}}});
        let c = claude_state(dir.path(), &state).unwrap();
        let key = ("claude".into(), B.into());
        assert_eq!(c.pinned, HashSet::from([key.clone()]));
        assert!(c.members[&key].0.ends_with(":custom"));
        assert_eq!(
            c.sections
                .iter()
                .map(|s| s.kind.as_str())
                .collect::<Vec<_>>(),
            ["pin", "section", "recent"]
        );
        let mut unpinned = state.clone();
        unpinned["pinnedOrder"] = serde_json::json!([]);
        let mut s = AgentSession {
            id: B.into(),
            provider: "claude".into(),
            ..Default::default()
        };
        c.apply(&mut s);
        assert!(s.pinned);
        claude_state(dir.path(), &unpinned).unwrap().apply(&mut s);
        assert!(s.pinned); // Reordering does not unpin a native session.
        std::fs::write(
            account.join(format!("{desktop}.json")),
            serde_json::json!({"sessionId":desktop,"cliSessionId":B,"isStarred":false}).to_string(),
        )
        .unwrap();
        claude_state(dir.path(), &state).unwrap().apply(&mut s);
        assert!(!s.pinned);
        assert!(s.section_id.ends_with(":custom"));
    }

    #[test]
    fn local_storage_reads_latest_live_value_without_writing_source() {
        let dir = tempfile::tempdir().unwrap();
        let mut db = DB::open(dir.path(), Options::default()).unwrap();
        let key = b"_https://claude.ai\0\x01dframe-store";
        db.put(key, b"\x01{\"state\":{\"version\":1}}").unwrap();
        db.flush().unwrap();
        db.put(key, b"\x01{\"state\":{\"version\":2}}").unwrap();
        db.flush().unwrap();
        let before = stamp(dir.path()).unwrap();
        assert_eq!(local_storage(dir.path()).unwrap()["state"]["version"], 2);
        assert_eq!(stamp(dir.path()).unwrap(), before);
        db.delete(key).unwrap();
        db.flush().unwrap();
        assert!(local_storage(dir.path()).is_none());
    }
}
