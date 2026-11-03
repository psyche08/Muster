//! Native organization policy. AX executes; provider files confirm persistence.
use crate::{AgentConfig, Error, sections};
use muster_proto::v1::{AgentOrganizeRequest, AgentSession, agent_organize_request::Action};
use std::{collections::HashSet, path::PathBuf};

pub struct Plan {
    pub request: AgentOrganizeRequest,
    pub action: Action,
    pub provider: String,
    pub native_id: String,
    pub title: String,
    pub section_title: String,
    pub section_kind: String,
    pub no_op: bool,
    scope: String,
    source: Option<PathBuf>,
    before: HashSet<String>,
    previous_section: String,
    group_sessions: Vec<String>,
}

fn claude_source(config: &AgentConfig, session: &str) -> Result<(String, PathBuf, String), Error> {
    let root = config
        .claude_desktop
        .as_ref()
        .ok_or_else(|| Error::Unavailable("Claude local registry is unavailable".into()))?;
    let native = crate::claude_identity::desktop_for_cli(root, session)
        .ok_or_else(|| Error::Conflict("Claude session has no unique local UUID mapping".into()))?;
    let mut matches = Vec::new();
    for org in std::fs::read_dir(root)
        .map_err(|e| Error::Io(e.to_string()))?
        .flatten()
    {
        if !org.file_type().is_ok_and(|t| t.is_dir()) {
            continue;
        }
        for account in std::fs::read_dir(org.path())
            .into_iter()
            .flatten()
            .flatten()
        {
            let path = account.path().join(format!("{native}.json"));
            if path.is_file() {
                let scope = format!(
                    "{}/{}",
                    org.file_name().to_string_lossy(),
                    account.file_name().to_string_lossy()
                );
                matches.push((native.clone(), path, scope));
            }
        }
    }
    match matches.as_slice() {
        [one] => Ok(one.clone()),
        _ => Err(Error::Conflict("Claude local account is ambiguous".into())),
    }
}

pub fn prepare(
    config: &AgentConfig,
    session: &AgentSession,
    request: &AgentOrganizeRequest,
) -> Result<Plan, Error> {
    if request.session != session.id
        || !crate::viewer::is_uuid(&session.id)
        || session.read_only
        || session.mode != "code"
    {
        return Err(Error::Invalid(
            "Choose a local native session by UUID".into(),
        ));
    }
    let action = Action::try_from(request.action)
        .map_err(|_| Error::Invalid("Unknown organization action".into()))?;
    if !matches!(session.provider.as_str(), "codex" | "claude") {
        return Err(Error::Unavailable(
            "This provider has no native organization controls".into(),
        ));
    }
    if request.name.len() > 512
        || request.name.chars().any(char::is_control)
        || (action == Action::CreateSection && request.name.trim().is_empty())
    {
        return Err(Error::Invalid(
            "Enter a section name of at most 512 bytes".into(),
        ));
    }
    if action != Action::CreateSection && !request.name.is_empty() {
        return Err(Error::Invalid(
            "A name is only valid when creating a section".into(),
        ));
    }
    if !matches!(action, Action::Move | Action::DeleteSection) && !request.section_id.is_empty() {
        return Err(Error::Invalid("Unexpected native section target".into()));
    }
    let (native_id, source, scope) = if session.provider == "claude" {
        let (id, path, scope) = claude_source(config, &session.id)?;
        (id, Some(path), scope)
    } else {
        (session.id.clone(), None, String::new())
    };
    let catalog = sections::read(config);
    if catalog
        .notices
        .iter()
        .any(|(provider, _)| provider == &session.provider)
    {
        return Err(Error::Unavailable(
            "Native organization metadata is unavailable".into(),
        ));
    }
    let candidates: Vec<_> = catalog
        .sections
        .iter()
        .filter(|g| {
            g.provider == session.provider
                && (scope.is_empty() || g.id.starts_with(&format!("{scope}:")))
        })
        .collect();
    if session.provider == "claude" && candidates.is_empty() {
        return Err(Error::Unavailable(
            "Open this local Claude account before managing its sections".into(),
        ));
    }
    let mut request = request.clone();
    request.name = request.name.trim().into();
    let key = (session.provider.clone(), session.id.clone());
    let pinned = catalog.pinned.contains(&key);
    let current = catalog
        .members
        .get(&key)
        .map(|(id, _)| id.as_str())
        .unwrap_or("");
    let target = if action == Action::Remove {
        current
    } else {
        &request.section_id
    };
    let section = candidates.iter().find(|g| g.id == target);
    let mut no_op =
        matches!(action, Action::Pin) && pinned || matches!(action, Action::Unpin) && !pinned;
    if matches!(
        action,
        Action::Move | Action::DeleteSection | Action::Remove
    ) {
        if action == Action::Remove && target.is_empty() {
            no_op = true;
        } else {
            let g = section.ok_or_else(|| {
                Error::Conflict("Native section is no longer available in this account".into())
            })?;
            if !matches!(g.kind.as_str(), "section" | "project") {
                return Err(Error::Invalid("Use Pin/Unpin for the pinned group".into()));
            }
            if candidates
                .iter()
                .filter(|other| other.title == g.title)
                .count()
                != 1
            {
                return Err(Error::Conflict(
                    "Native group labels are ambiguous; no group was changed".into(),
                ));
            }
            if action == Action::Move && current == g.id {
                no_op = true;
            }
        }
    }
    if action == Action::CreateSection && candidates.iter().any(|g| g.title == request.name) {
        return Err(Error::Conflict(
            "A section with this name already exists on this native source".into(),
        ));
    }
    let title = sections::native_title(config, &session.provider, &session.id)
        .unwrap_or_else(|| session.title.clone());
    Ok(Plan {
        action,
        provider: session.provider.clone(),
        native_id,
        title,
        section_title: section.map(|g| g.title.clone()).unwrap_or_default(),
        section_kind: section.map(|g| g.kind.clone()).unwrap_or_default(),
        no_op,
        scope,
        source,
        previous_section: current.into(),
        group_sessions: catalog
            .members
            .iter()
            .filter(|((provider, _), (section, _))| {
                provider == &session.provider && section == target
            })
            .map(|((_, id), _)| id.clone())
            .collect(),
        before: candidates.iter().map(|g| g.id.clone()).collect(),
        request,
    })
}

/// A missing read is never treated as proof of deletion or unpinning.
pub fn confirmed(config: &AgentConfig, plan: &Plan) -> Result<Option<String>, Error> {
    if plan.action == Action::Archive {
        let archived = if plan.provider == "codex" {
            let home = config
                .codex_home
                .as_ref()
                .ok_or_else(|| Error::Unavailable("Codex metadata unavailable".into()))?;
            let db = rusqlite::Connection::open_with_flags(
                home.join("state_5.sqlite"),
                rusqlite::OpenFlags::SQLITE_OPEN_READ_ONLY,
            )
            .map_err(|e| Error::Io(e.to_string()))?;
            db.busy_timeout(std::time::Duration::from_millis(100))
                .map_err(|e| Error::Io(e.to_string()))?;
            db.query_row(
                "SELECT archived FROM threads WHERE id=?1",
                [&plan.request.session],
                |r| r.get::<_, bool>(0),
            )
            .map_err(|e| Error::Io(e.to_string()))?
        } else {
            let path = plan
                .source
                .as_ref()
                .ok_or_else(|| Error::Unavailable("Claude native registry unavailable".into()))?;
            if std::fs::metadata(path)
                .map_err(|e| Error::Io(e.to_string()))?
                .len()
                > 4 << 20
            {
                return Err(Error::Unavailable(
                    "Claude registry exceeds read limit".into(),
                ));
            }
            let v: serde_json::Value =
                serde_json::from_slice(&std::fs::read(path).map_err(|e| Error::Io(e.to_string()))?)
                    .map_err(|e| Error::Io(e.to_string()))?;
            v["sessionId"] == plan.native_id
                && v["cliSessionId"] == plan.request.session
                && v["isArchived"] == true
        };
        return Ok(archived.then(|| plan.request.session.clone()));
    }
    let catalog = sections::read(config);
    if catalog.notices.iter().any(|(p, _)| p == &plan.provider) {
        return Err(Error::Unavailable(
            "Native organization readback is unavailable".into(),
        ));
    }
    let key = (plan.provider.clone(), plan.request.session.clone());
    let pinned = catalog.pinned.contains(&key);
    let member = catalog
        .members
        .get(&key)
        .map(|(id, _)| id.as_str())
        .unwrap_or("");
    let ok = match plan.action {
        Action::Pin => pinned,
        Action::Unpin => !pinned,
        Action::Move => member == plan.request.section_id,
        Action::Remove => member != plan.previous_section,
        Action::DeleteSection => {
            if catalog
                .sections
                .iter()
                .any(|g| g.provider == plan.provider && g.id == plan.request.section_id)
                || (!plan.scope.is_empty()
                    && !catalog.sections.iter().any(|g| {
                        g.provider == plan.provider && g.id.starts_with(&format!("{}:", plan.scope))
                    }))
            {
                false
            } else {
                // Group removal must not silently archive/delete its sessions.
                for id in &plan.group_sessions {
                    if plan.provider == "codex" {
                        let home = config.codex_home.as_ref().ok_or_else(|| {
                            Error::Unavailable("Codex metadata unavailable".into())
                        })?;
                        let db = rusqlite::Connection::open_with_flags(
                            home.join("state_5.sqlite"),
                            rusqlite::OpenFlags::SQLITE_OPEN_READ_ONLY,
                        )
                        .map_err(|e| Error::Io(e.to_string()))?;
                        db.busy_timeout(std::time::Duration::from_millis(100))
                            .map_err(|e| Error::Io(e.to_string()))?;
                        if !db
                            .query_row("SELECT archived=0 FROM threads WHERE id=?1", [id], |r| {
                                r.get::<_, bool>(0)
                            })
                            .unwrap_or(false)
                        {
                            return Ok(None);
                        }
                    } else if claude_source(config, id).is_err() {
                        return Ok(None);
                    }
                }
                true
            }
        }
        Action::CreateSection => {
            let new: Vec<_> = catalog
                .sections
                .iter()
                .filter(|g| {
                    g.provider == plan.provider
                        && g.title == plan.request.name
                        && g.kind == "section"
                        && !plan.before.contains(&g.id)
                        && (plan.scope.is_empty() || g.id.starts_with(&format!("{}:", plan.scope)))
                })
                .collect();
            return Ok(match new.as_slice() {
                [one] => Some(one.id.clone()),
                _ => None,
            });
        }
        Action::Archive => unreachable!(),
    };
    Ok(ok.then(|| plan.request.session.clone()))
}

#[cfg(test)]
mod tests {
    use super::*;
    const ID: &str = "11111111-1111-4111-8111-111111111111";
    fn fixture() -> (
        tempfile::TempDir,
        AgentConfig,
        AgentSession,
        rusqlite::Connection,
    ) {
        let dir = tempfile::tempdir().unwrap();
        let db = rusqlite::Connection::open(dir.path().join("state_5.sqlite")).unwrap();
        db.execute_batch("CREATE TABLE thread_sections(id TEXT,name TEXT);CREATE TABLE projects(id TEXT,name TEXT,position INTEGER);CREATE TABLE threads(id TEXT,title TEXT,thread_section_id TEXT,project_id TEXT,section_position INTEGER,archived INTEGER,is_pinned INTEGER);").unwrap();
        db.execute(
            "INSERT INTO threads VALUES(?1,'Native title','one',NULL,0,0,0)",
            [ID],
        )
        .unwrap();
        db.execute("INSERT INTO thread_sections VALUES('one','Work')", [])
            .unwrap();
        db.execute(
            "INSERT INTO thread_sections VALUES(?1,'Pinned')",
            [crate::catalog::CODEX_PINNED_SECTION],
        )
        .unwrap();
        let config = AgentConfig {
            codex_home: Some(dir.path().to_owned()),
            claude_home: None,
            claude_desktop: None,
            catdesk_home: None,
            deepseek_home: None,
            ..Default::default()
        };
        let session = AgentSession {
            id: ID.into(),
            provider: "codex".into(),
            mode: "code".into(),
            title: "Old transcript".into(),
            ..Default::default()
        };
        (dir, config, session, db)
    }
    fn request(action: Action) -> AgentOrganizeRequest {
        AgentOrganizeRequest {
            session: ID.into(),
            action: action as i32,
            ..Default::default()
        }
    }
    #[test]
    fn plans_use_native_uuid_and_title_and_do_not_write_provider_files() {
        let (_dir, config, session, db) = fixture();
        let before: i64 = db
            .query_row("PRAGMA data_version", [], |r| r.get(0))
            .unwrap();
        let plan = prepare(&config, &session, &request(Action::Pin)).unwrap();
        assert_eq!(plan.title, "Native title");
        assert!(!plan.no_op);
        assert!(confirmed(&config, &plan).unwrap().is_none());
        assert_eq!(
            before,
            db.query_row::<i64, _, _>("PRAGMA data_version", [], |r| r.get(0))
                .unwrap()
        );
        db.execute(
            "UPDATE threads SET thread_section_id=?1",
            [crate::catalog::CODEX_PINNED_SECTION],
        )
        .unwrap();
        assert_eq!(confirmed(&config, &plan).unwrap(), Some(ID.into()));
    }
    #[test]
    fn stale_metadata_cannot_confirm_unpin_or_delete() {
        let (dir, config, session, _db) = fixture();
        let mut req = request(Action::DeleteSection);
        req.section_id = "section:one".into();
        let plan = prepare(&config, &session, &req).unwrap();
        std::fs::rename(
            dir.path().join("state_5.sqlite"),
            dir.path().join("offline.sqlite"),
        )
        .unwrap();
        assert!(confirmed(&config, &plan).is_err());
        assert!(prepare(&config, &session, &request(Action::Unpin)).is_err());
    }
    #[test]
    fn duplicate_group_labels_and_wrong_uuid_are_rejected() {
        let (_dir, config, session, db) = fixture();
        db.execute("INSERT INTO thread_sections VALUES('two','Work')", [])
            .unwrap();
        let mut req = request(Action::Move);
        req.section_id = "section:one".into();
        assert!(matches!(
            prepare(&config, &session, &req),
            Err(Error::Conflict(_))
        ));
        req = request(Action::Archive);
        req.session = "22222222-2222-4222-8222-222222222222".into();
        assert!(prepare(&config, &session, &req).is_err());
        let mut remote = session.clone();
        remote.id = "cloud:remote".into();
        req.session = remote.id.clone();
        assert!(prepare(&config, &remote, &req).is_err());
    }
    #[test]
    fn native_pinned_section_overrides_obsolete_legacy_flag() {
        let (_dir, config, session, db) = fixture();
        db.execute("UPDATE threads SET is_pinned=1", []).unwrap();
        let plan = prepare(&config, &session, &request(Action::Unpin)).unwrap();
        assert!(plan.no_op);
        assert!(confirmed(&config, &plan).unwrap().is_some());
    }
    #[test]
    fn deleting_a_group_never_confirms_if_its_sessions_were_archived() {
        let (_dir, config, session, db) = fixture();
        let mut req = request(Action::DeleteSection);
        req.section_id = "section:one".into();
        let plan = prepare(&config, &session, &req).unwrap();
        db.execute("DELETE FROM thread_sections WHERE id='one'", [])
            .unwrap();
        db.execute("UPDATE threads SET archived=1", []).unwrap();
        assert!(confirmed(&config, &plan).unwrap().is_none());
        db.execute("UPDATE threads SET archived=0,thread_section_id=NULL", [])
            .unwrap();
        assert!(confirmed(&config, &plan).unwrap().is_some());
    }
    #[test]
    fn create_and_archive_need_exact_persisted_readback() {
        let (_dir, config, session, db) = fixture();
        let mut req = request(Action::CreateSection);
        req.name = "New group".into();
        let plan = prepare(&config, &session, &req).unwrap();
        assert!(confirmed(&config, &plan).unwrap().is_none());
        db.execute("INSERT INTO thread_sections VALUES('new','New group')", [])
            .unwrap();
        assert_eq!(
            confirmed(&config, &plan).unwrap(),
            Some("section:new".into())
        );
        let plan = prepare(&config, &session, &request(Action::Archive)).unwrap();
        assert!(confirmed(&config, &plan).unwrap().is_none());
        db.execute("UPDATE threads SET archived=1 WHERE id=?1", [ID])
            .unwrap();
        assert!(confirmed(&config, &plan).unwrap().is_some());
    }
}
