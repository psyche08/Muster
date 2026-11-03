//! Display-only native sections and family trees. Identity is (device, provider, UUID).
use crate::state::State;
use muster_proto::{
    app::{AgentCatalogRow, AgentCatalogSection},
    v1::AgentSession,
};
use std::collections::{HashMap, HashSet};
pub(crate) fn row_key(s: &AgentSession) -> String {
    serde_json::to_string(&(&s.device_id, &s.provider, &s.id)).unwrap()
}
fn group_key(s: &AgentSession) -> String {
    serde_json::to_string(&(&s.device_id, &s.provider, &s.section_id)).unwrap()
}
fn parent<'a>(
    s: &AgentSession,
    all: &'a HashMap<String, &AgentSession>,
) -> Option<&'a AgentSession> {
    if s.provider != "claude" || !matches!(s.relationship.as_str(), "subsession" | "fork") {
        return None;
    }
    let key = serde_json::to_string(&(&s.device_id, &s.provider, &s.parent_session_id)).unwrap();
    all.get(&key).copied().filter(|p| p.id != s.id)
}
fn ancestors<'a>(
    s: &'a AgentSession,
    all: &'a HashMap<String, &AgentSession>,
) -> Vec<&'a AgentSession> {
    let mut seen = HashSet::from([row_key(s)]);
    let mut out = Vec::new();
    let mut at = s;
    while let Some(p) = parent(at, all) {
        if !seen.insert(row_key(p)) || out.len() >= 32 {
            return Vec::new();
        }
        out.push(p);
        at = p;
    }
    out
}
pub(crate) fn project(state: &State, sessions: &[AgentSession]) -> Vec<AgentCatalogSection> {
    let searching = !state.agents.query.trim().is_empty();
    let all: HashMap<_, _> = sessions.iter().map(|s| (row_key(s), s)).collect();
    let mut groups = Vec::<(i64, AgentCatalogSection)>::new();
    let mut positions = HashMap::new();
    if !searching {
        for s in &state.agents.native_sections {
            if state.offline(&s.device_id)
                || (!state.agents.device_id.is_empty()
                    && state.agents.device_id != "*"
                    && state.agents.device_id != s.device_id)
            {
                continue;
            }
            let key = serde_json::to_string(&(&s.device_id, &s.provider, &s.id)).unwrap();
            if positions.contains_key(&key) {
                continue;
            }
            positions.insert(key.clone(), groups.len());
            groups.push((
                s.position,
                AgentCatalogSection {
                    key: key.clone(),
                    title: s.title.clone(),
                    subtitle: format!("{} · {}", s.device_name, s.provider),
                    expanded: *state.agent_disclosures.get(&key).unwrap_or(&!s.collapsed),
                    ..Default::default()
                },
            ));
        }
    }
    let mut roots = Vec::new();
    let mut children: HashMap<String, Vec<&AgentSession>> = HashMap::new();
    for s in sessions {
        let chain = ancestors(s, &all);
        if !searching && !chain.is_empty() {
            children.entry(row_key(chain[0])).or_default().push(s);
        } else {
            roots.push(s);
        }
    }
    let sort = |list: &mut Vec<&AgentSession>| {
        list.sort_by(|a, b| {
            a.section_position
                .cmp(&b.section_position)
                .then_with(|| b.updated_unix.cmp(&a.updated_unix))
                .then_with(|| a.id.cmp(&b.id))
        })
    };
    sort(&mut roots);
    for list in children.values_mut() {
        sort(list);
    }
    for root in roots {
        let key = group_key(root);
        let idx = *positions.entry(key.clone()).or_insert_with(|| {
            let metadata = state.agents.native_sections.iter().find(|g| {
                g.device_id == root.device_id
                    && g.provider == root.provider
                    && g.id == root.section_id
            });
            let idx = groups.len();
            groups.push((
                metadata.map_or(i64::MAX, |g| g.position),
                AgentCatalogSection {
                    key: key.clone(),
                    title: metadata
                        .map(|g| g.title.clone())
                        .unwrap_or_else(|| "Recents".into()),
                    subtitle: format!("{} · {}", root.device_name, root.provider),
                    expanded: searching || *state.agent_disclosures.get(&key).unwrap_or(&true),
                    ..Default::default()
                },
            ));
            idx
        });
        append(
            root,
            0,
            &children,
            &all,
            &state.agent_disclosures,
            &mut groups[idx].1,
            searching,
        );
    }
    groups.sort_by(|(ap, a), (bp, b)| {
        a.subtitle
            .cmp(&b.subtitle)
            .then_with(|| ap.cmp(bp))
            .then_with(|| a.key.cmp(&b.key))
    });
    groups.into_iter().map(|(_, g)| g).collect()
}
fn append(
    s: &AgentSession,
    depth: u32,
    children: &HashMap<String, Vec<&AgentSession>>,
    all: &HashMap<String, &AgentSession>,
    disclosures: &HashMap<String, bool>,
    group: &mut AgentCatalogSection,
    searching: bool,
) {
    let key = row_key(s);
    let nested = children.get(&key);
    let expanded = depth > 0 || *disclosures.get(&key).unwrap_or(&true);
    group.count += 1;
    let parent_title = parent(s, all).map(|p| p.title.clone()).unwrap_or_default();
    if group.expanded {
        group.rows.push(AgentCatalogRow {
            key,
            session: Some(s.clone()),
            depth: depth.min(1),
            expandable: depth == 0 && nested.is_some_and(|c| !c.is_empty()),
            expanded,
            parent_title,
        });
    }
    if !searching
        && depth < 32
        && let Some(nested) = nested
    {
        // Count and retain hierarchy even while disclosure hides its rows.
        for child in nested {
            let shown = group.expanded;
            group.expanded &= expanded;
            append(child, depth + 1, children, all, disclosures, group, false);
            group.expanded = shown;
        }
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    fn s(id: &str, parent: &str) -> AgentSession {
        AgentSession {
            id: id.into(),
            provider: "claude".into(),
            device_id: "host".into(),
            relationship: if parent.is_empty() { "" } else { "subsession" }.into(),
            parent_session_id: parent.into(),
            ..Default::default()
        }
    }
    #[test]
    fn hierarchy_uses_ids_not_titles_and_rejects_cycles() {
        let a = s("a", "");
        let b = s("b", "a");
        let c = s("c", "b");
        let rows = [a, b, c];
        let map = rows.iter().map(|s| (row_key(s), s)).collect();
        assert_eq!(ancestors(&rows[2], &map).len(), 2);
        let rows = [s("a", "b"), s("b", "a")];
        let map = rows.iter().map(|s| (row_key(s), s)).collect();
        assert!(ancestors(&rows[0], &map).is_empty());
    }
    #[test]
    fn parent_never_crosses_device() {
        let mut p = s("a", "");
        p.device_id = "other".into();
        let c = s("c", "a");
        let map = HashMap::from([(row_key(&p), &p)]);
        assert!(parent(&c, &map).is_none());
    }
}

#[cfg(test)]
mod projection_tests {
    use super::*;
    use muster_proto::v1::AgentSection;
    fn session(id: &str, parent: &str) -> AgentSession {
        AgentSession {
            id: id.into(),
            title: "Same".into(),
            provider: "claude".into(),
            device_id: "host".into(),
            section_id: "custom".into(),
            relationship: "subsession".into(),
            parent_session_id: parent.into(),
            ..Default::default()
        }
    }
    #[test]
    fn claude_family_is_flat_under_root_with_one_disclosure_and_same_name_sections_stay_separate() {
        let mut state = State::new(false, Some("host".into()));
        state.agents.native_sections = vec![
            AgentSection {
                id: "custom".into(),
                provider: "claude".into(),
                device_id: "host".into(),
                title: "Same".into(),
                ..Default::default()
            },
            AgentSection {
                id: "other".into(),
                provider: "claude".into(),
                device_id: "host".into(),
                title: "Same".into(),
                ..Default::default()
            },
        ];
        let sessions = vec![
            session("root", ""),
            session("child", "root"),
            session("grandchild", "child"),
        ];
        let groups = project(&state, &sessions);
        assert_eq!(groups.len(), 2);
        let group = groups.iter().find(|g| g.count == 3).unwrap();
        assert_eq!(
            group.rows.iter().map(|r| r.depth).collect::<Vec<_>>(),
            vec![0, 1, 1]
        );
        assert!(group.rows[0].expandable);
        assert!(!group.rows[1].expandable);
        state.agent_disclosures.insert(row_key(&sessions[0]), false);
        let groups = project(&state, &sessions);
        let group = groups.iter().find(|g| g.count == 3).unwrap();
        assert_eq!(group.rows.len(), 1);
        state.agents.device_id = "other-device".into();
        assert!(project(&state, &[]).is_empty());
    }
}
