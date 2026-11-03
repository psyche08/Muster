//! Native group identity, Claude family boundaries and bounded catalog pages.
//! Labels aggregate presentation only; mutations retain device/provider/UUID.
use crate::Error;
use muster_proto::v1::{AgentCatalogGroup, AgentSection, AgentSession, AgentSessionPage};
use std::collections::{BTreeMap, HashMap, HashSet};

pub const PAGE_SIZE: usize = 10;
pub const CODEX_PINNED_SECTION: &str = "01984de2-8f74-7c91-a3b2-5c5e937cf318";

pub fn session_key(s: &AgentSession) -> String {
    serde_json::to_string(&(&s.device_id, &s.provider, &s.id)).unwrap()
}

pub fn group(s: &AgentSession, sections: &[AgentSection]) -> AgentCatalogGroup {
    let native = sections
        .iter()
        .find(|g| g.device_id == s.device_id && g.provider == s.provider && g.id == s.section_id);
    let kind = native.map(|g| g.kind.as_str()).unwrap_or("");
    let legacy_claude = s.provider == "claude" && kind.is_empty();
    let (kind, title) =
        if s.pinned || kind == "pin" || (legacy_claude && s.section_id.ends_with(":pinned")) {
            ("pin", "Pin")
        } else if s.section_id.is_empty()
            || kind == "recent"
            || (legacy_claude && s.section_id.ends_with(":sessions"))
        {
            ("recent", "Recent")
        } else {
            (
                if kind == "project" || s.section_id.starts_with("project:") {
                    "project"
                } else {
                    "section"
                },
                native
                    .map(|g| g.title.as_str())
                    .filter(|s| !s.is_empty())
                    .unwrap_or(&s.section_id),
            )
        };
    let key = if matches!(kind, "pin" | "recent") {
        kind.to_owned()
    } else {
        serde_json::to_string(&(&s.provider, &s.section_id)).unwrap()
    };
    AgentCatalogGroup {
        key,
        kind: kind.into(),
        title: title.into(),
        provider: if matches!(kind, "pin" | "recent") {
            String::new()
        } else {
            s.provider.clone()
        },
        section_id: if matches!(kind, "pin" | "recent") {
            String::new()
        } else {
            s.section_id.clone()
        },
        device_id: s.device_id.clone(),
        ..Default::default()
    }
}

pub fn display_key(group: &AgentCatalogGroup) -> String {
    if matches!(group.kind.as_str(), "pin" | "recent") {
        format!("catalog:{}", group.kind)
    } else {
        serde_json::to_string(&("group", &group.title)).unwrap()
    }
}

/// Orphans are independent roots; cycles never hide conversations. Cross-device
/// or cross-provider parent IDs cannot join a family.
pub fn family_roots(sessions: &[AgentSession]) -> HashMap<String, String> {
    let all: HashMap<_, _> = sessions.iter().map(|s| (session_key(s), s)).collect();
    sessions
        .iter()
        .map(|s| {
            let own = session_key(s);
            let mut at = s;
            let mut seen = HashSet::from([own.clone()]);
            let root = loop {
                if at.provider != "claude"
                    || !matches!(at.relationship.as_str(), "subsession" | "fork")
                {
                    break session_key(at);
                }
                let parent =
                    serde_json::to_string(&(&at.device_id, &at.provider, &at.parent_session_id))
                        .unwrap();
                let Some(next) = all.get(&parent) else {
                    break session_key(at);
                };
                if !seen.insert(parent) || seen.len() > 32 {
                    break own.clone();
                }
                at = next;
            };
            (own, root)
        })
        .collect()
}

type Cursor = (String, u64, String, String);
fn boundary(s: &AgentSession) -> (std::cmp::Reverse<u64>, &str, &str) {
    (std::cmp::Reverse(s.updated_unix), &s.provider, &s.id)
}
fn cursor(key: &str, s: &AgentSession) -> String {
    serde_json::to_string(&(key, s.updated_unix, &s.provider, &s.id)).unwrap()
}

/// The caller must filter sessions and sections using current read grants.
/// Initial/stream pages return ten roots per group plus explicitly visible
/// families. A continuation returns only the next ten roots and their children.
pub fn page(
    sessions: Vec<AgentSession>,
    sections: Vec<AgentSection>,
    requested: &str,
    after: &str,
    visible: &[String],
) -> Result<AgentSessionPage, Error> {
    if after.len() > 2048 || visible.len() > 2000 {
        return Err(Error::Invalid("Catalog request is too large".into()));
    }
    let boundary_cursor: Option<Cursor> = if after.is_empty() {
        None
    } else {
        let parsed: Cursor = serde_json::from_str(after)
            .map_err(|_| Error::Invalid("Invalid catalog cursor".into()))?;
        if parsed.0 != requested {
            return Err(Error::Invalid(
                "Catalog cursor belongs to another group".into(),
            ));
        }
        Some(parsed)
    };
    let sessions: Vec<_> = sessions.into_iter().filter(|s| !s.archived).collect();
    let roots = family_roots(&sessions);
    let mut families: HashMap<String, Vec<&AgentSession>> = HashMap::new();
    for s in &sessions {
        families
            .entry(roots[&session_key(s)].clone())
            .or_default()
            .push(s);
    }
    let mut groups: BTreeMap<String, (AgentCatalogGroup, Vec<&AgentSession>)> = BTreeMap::new();
    for (key, title) in [("pin", "Pin"), ("recent", "Recent")] {
        groups.insert(
            key.into(),
            (
                AgentCatalogGroup {
                    key: key.into(),
                    kind: key.into(),
                    title: title.into(),
                    ..Default::default()
                },
                vec![],
            ),
        );
    }
    for sec in &sections {
        let g = group(
            &AgentSession {
                provider: sec.provider.clone(),
                section_id: sec.id.clone(),
                device_id: sec.device_id.clone(),
                ..Default::default()
            },
            &sections,
        );
        groups.entry(g.key.clone()).or_insert((g, vec![]));
    }
    for s in &sessions {
        if roots[&session_key(s)] != session_key(s) {
            continue;
        }
        let g = group(s, &sections);
        groups.entry(g.key.clone()).or_insert((g, vec![])).1.push(s);
    }
    if !requested.is_empty() && !groups.contains_key(requested) {
        return Err(Error::NotFound(
            "Native group is no longer available".into(),
        ));
    }
    let visible: HashSet<_> = visible.iter().map(String::as_str).collect();
    let mut out = AgentSessionPage {
        sections,
        ..Default::default()
    };
    for (key, (mut group, mut rows)) in groups {
        if !requested.is_empty() && key != requested {
            continue;
        }
        rows.sort_by_key(|s| boundary(s));
        group.count = rows.len() as u32;
        let eligible: Vec<_> = rows
            .iter()
            .copied()
            .filter(|s| {
                boundary_cursor.as_ref().is_none_or(|(_, ts, p, id)| {
                    boundary(s) > (std::cmp::Reverse(*ts), p.as_str(), id.as_str())
                })
            })
            .collect();
        let mut selected: HashSet<_> = eligible
            .iter()
            .take(PAGE_SIZE)
            .map(|s| session_key(s))
            .collect();
        if requested.is_empty() {
            for s in &rows {
                if families[&session_key(s)]
                    .iter()
                    .any(|s| visible.contains(s.id.as_str()))
                {
                    selected.insert(session_key(s));
                }
            }
        }
        let contiguous = eligible
            .iter()
            .take_while(|s| selected.contains(&session_key(s)))
            .count();
        if contiguous < eligible.len() && contiguous > 0 {
            group.next_cursor = cursor(&key, eligible[contiguous - 1]);
        }
        for s in rows
            .into_iter()
            .filter(|s| selected.contains(&session_key(s)))
        {
            out.sessions
                .extend(families[&session_key(s)].iter().map(|s| (*s).clone()));
        }
        out.groups.push(group);
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;
    fn fixtures() -> Vec<AgentSession> {
        let mut all = Vec::new();
        for i in 0..23 {
            let id = format!("root-{i:02}");
            all.push(AgentSession {
                id: id.clone(),
                provider: "claude".into(),
                updated_unix: 100 - i,
                ..Default::default()
            });
            for j in 0..4 {
                all.push(AgentSession {
                    id: format!("child-{i}-{j}"),
                    provider: "claude".into(),
                    parent_session_id: id.clone(),
                    relationship: "subsession".into(),
                    updated_unix: 200,
                    ..Default::default()
                });
            }
        }
        all
    }
    #[test]
    fn pages_count_roots_and_include_descendants_without_using_page_slots() {
        let first = page(fixtures(), vec![], "", "", &[]).unwrap();
        let group = first.groups.iter().find(|g| g.key == "recent").unwrap();
        assert_eq!(group.count, 23);
        assert_eq!(first.sessions.len(), 50);
        let second = page(fixtures(), vec![], "recent", &group.next_cursor, &[]).unwrap();
        assert_eq!(second.sessions.len(), 50);
        assert!(
            second
                .sessions
                .iter()
                .all(|s| !first.sessions.iter().any(|old| old.id == s.id))
        );
        let last = page(
            fixtures(),
            vec![],
            "recent",
            &second.groups[0].next_cursor,
            &[],
        )
        .unwrap();
        assert_eq!(last.sessions.len(), 15);
        assert!(last.groups[0].next_cursor.is_empty());
        assert!(page(fixtures(), vec![], "pin", &group.next_cursor, &[]).is_err());
    }
    #[test]
    fn watch_refreshes_loaded_families_but_never_reads_hidden_history() {
        let visible = (0..20).map(|i| format!("root-{i:02}")).collect::<Vec<_>>();
        let p = page(fixtures(), vec![], "", "", &visible).unwrap();
        assert_eq!(p.sessions.len(), 100);
        let next = page(
            fixtures(),
            vec![],
            "recent",
            &p.groups
                .iter()
                .find(|g| g.key == "recent")
                .unwrap()
                .next_cursor,
            &[],
        )
        .unwrap();
        assert_eq!(next.sessions.len(), 15);
    }
    #[test]
    fn cycles_and_missing_parents_remain_visible() {
        let mut s = fixtures();
        s[0].parent_session_id = s[1].id.clone();
        s[0].relationship = "subsession".into();
        let roots = family_roots(&s);
        assert_eq!(roots[&session_key(&s[0])], session_key(&s[0]));
        let orphan = AgentSession {
            id: "orphan".into(),
            provider: "claude".into(),
            parent_session_id: "missing".into(),
            relationship: "subsession".into(),
            ..Default::default()
        };
        assert_eq!(
            family_roots(std::slice::from_ref(&orphan))[&session_key(&orphan)],
            session_key(&orphan)
        );
    }
}
