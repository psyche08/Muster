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
fn parent<'a>(
    s: &AgentSession,
    all: &'a HashMap<String, &AgentSession>,
) -> Option<&'a AgentSession> {
    // Families are the listed sessions a provider links: Claude's subsessions
    // and forks, DeepSeek Harness forks. Subagents are never catalog rows.
    let family = match s.provider.as_str() {
        "claude" => matches!(s.relationship.as_str(), "subsession" | "fork"),
        "deepseek" => s.relationship == "fork",
        _ => false,
    };
    if !family {
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
fn native_section<'a>(
    state: &'a State,
    s: &AgentSession,
) -> Option<&'a muster_proto::v1::AgentSection> {
    state
        .agents
        .native_sections
        .iter()
        .find(|g| g.device_id == s.device_id && g.provider == s.provider && g.id == s.section_id)
}
fn category(state: &State, s: &AgentSession) -> usize {
    let kind = native_section(state, s)
        .map(|g| g.kind.as_str())
        .unwrap_or("");
    // Compatibility with the typed IDs emitted by older Claude readers. Never
    // use a section's user-editable label to infer pin or recent semantics.
    let legacy_claude = s.provider == "claude" && kind.is_empty();
    if s.pinned || kind == "pin" || (legacy_claude && s.section_id.ends_with(":pinned")) {
        0
    } else if s.section_id.is_empty()
        || kind == "recent"
        || (legacy_claude && s.section_id.ends_with(":sessions"))
    {
        2
    } else {
        1
    }
}
pub(crate) fn project(state: &State, sessions: &[AgentSession]) -> Vec<AgentCatalogSection> {
    let searching = !state.agents.query.trim().is_empty();
    let all: HashMap<_, _> = sessions.iter().map(|s| (row_key(s), s)).collect();
    let mut groups: Vec<_> = [("pin", "Pin"), ("section", "Section"), ("recent", "Recent")]
        .into_iter()
        .map(|(id, title)| {
            let key = format!("catalog:{id}");
            AgentCatalogSection {
                expanded: searching || *state.agent_disclosures.get(&key).unwrap_or(&true),
                key,
                title: title.into(),
                ..Default::default()
            }
        })
        .collect();
    let mut roots = Vec::new();
    let mut children: HashMap<String, Vec<&AgentSession>> = HashMap::new();
    for s in sessions {
        // Keep native families within a category. A pinned child must remain
        // visible under Pin even when its unpinned parent's family is collapsed.
        let chain = ancestors(s, &all);
        if !searching && !chain.is_empty() && category(state, s) == category(state, chain[0]) {
            children.entry(row_key(chain[0])).or_default().push(s);
        } else {
            roots.push(s);
        }
    }
    let sort = |list: &mut Vec<&AgentSession>| {
        list.sort_by(|a, b| {
            b.updated_unix
                .cmp(&a.updated_unix)
                .then_with(|| row_key(a).cmp(&row_key(b)))
        })
    };
    sort(&mut roots);
    for list in children.values_mut() {
        sort(list);
    }
    for root in roots {
        let idx = category(state, root);
        append(
            root,
            0,
            &children,
            &all,
            &state.agent_disclosures,
            &mut groups[idx],
            searching,
        );
    }
    for group in &mut groups {
        for row in &mut group.rows {
            let Some(session) = &row.session else {
                continue;
            };
            let device = if session.device_name.is_empty() {
                &session.device_id
            } else {
                &session.device_name
            };
            let mut origin = vec![provider_title(&session.provider)];
            if !device.is_empty() {
                origin.push(device.clone());
            }
            if let Some(section) = native_section(state, session)
                && section.kind != "recent"
                && section.kind != "pin"
                && !(section.kind.is_empty()
                    && session.provider == "claude"
                    && (section.id.ends_with(":sessions") || section.id.ends_with(":pinned")))
                && !section.title.is_empty()
            {
                origin.push(section.title.clone());
            }
            row.source_label = origin.join(" · ");
        }
    }
    groups
}
fn provider_title(provider: &str) -> String {
    match provider {
        "codex" => "Codex",
        "claude" => "Claude",
        "deepseek" => "DeepSeek",
        "catdesk" => "Catdesk",
        "orca" => "Orca",
        other => other,
    }
    .into()
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
            ..Default::default()
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
    fn deepseek_forks_join_a_family_but_subagents_and_other_providers_do_not() {
        let mut p = s("p", "");
        p.provider = "deepseek".into();
        let mut fork = s("f", "p");
        fork.provider = "deepseek".into();
        fork.relationship = "fork".into();
        let map = HashMap::from([(row_key(&p), &p)]);
        assert_eq!(parent(&fork, &map).map(|p| p.id.as_str()), Some("p"));
        let mut subagent = fork.clone();
        subagent.relationship = "subagent".into();
        assert!(parent(&subagent, &map).is_none());
        let mut subsession = fork.clone();
        subsession.relationship = "subsession".into();
        assert!(parent(&subsession, &map).is_none());
        let mut codex = fork.clone();
        codex.provider = "codex".into();
        p.provider = "codex".into();
        let map = HashMap::from([(row_key(&p), &p)]);
        assert!(parent(&codex, &map).is_none());
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
    fn three_categories_preserve_device_identities_and_native_labels() {
        let mut state = State::new(false, Some("host".into()));
        state.agents.device_id = "*".into();
        state.agents.native_sections = ["host", "second"]
            .into_iter()
            .map(|device| AgentSection {
                id: "custom".into(),
                provider: "claude".into(),
                device_id: device.into(),
                title: "Same group".into(),
                ..Default::default()
            })
            .collect();
        let mut old = session("same-id", "");
        old.device_name = "Mac A".into();
        old.updated_unix = 1;
        old.section_position = 0;
        let mut recent = old.clone();
        recent.device_id = "second".into();
        recent.device_name = "Mac B".into();
        recent.updated_unix = 10;
        recent.section_position = 99;
        let mut codex = old.clone();
        codex.provider = "codex".into();
        let sessions = [old, recent, codex];
        for query in ["", "same"] {
            state.agents.query = query.into();
            let groups = project(&state, &sessions);
            assert_eq!(
                groups.iter().map(|g| g.title.as_str()).collect::<Vec<_>>(),
                ["Pin", "Section", "Recent"]
            );
            let claude = &groups[1];
            assert_eq!(claude.count, 3);
            assert_eq!(claude.rows[0].session.as_ref().unwrap().device_id, "second");
            assert_ne!(claude.rows[0].key, claude.rows[1].key);
            assert_eq!(claude.rows[0].source_label, "Claude · Mac B · Same group");
            assert_eq!(claude.rows[1].source_label, "Claude · Mac A · Same group");
        }
        state.agents.query.clear();
        state.agents.device_id = "host".into();
        let groups = project(&state, &sessions[..1]);
        assert_eq!(groups.len(), 3);
        assert_eq!(groups[1].title, "Section");
        assert_eq!(
            groups[1].rows[0].source_label,
            "Claude · Mac A · Same group"
        );
    }

    #[test]
    fn claude_family_is_flat_and_native_sections_do_not_add_subdivisions() {
        let mut state = State::new(false, Some("host".into()));
        state.agents.device_id = "host".into();
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
        assert_eq!(groups.len(), 3);
        let group = &groups[1];
        assert_eq!(group.count, 3);
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
        assert!(project(&state, &[]).iter().all(|g| g.count == 0));
    }
    #[test]
    fn native_pin_precedes_section_and_recent_is_not_a_custom_group() {
        let mut state = State::new(false, Some("host".into()));
        state.agents.native_sections = vec![
            AgentSection {
                id: "custom".into(),
                provider: "claude".into(),
                device_id: "host".into(),
                title: "Pinned".into(),
                kind: "section".into(),
                collapsed: true,
                ..Default::default()
            },
            AgentSection {
                id: "default".into(),
                provider: "claude".into(),
                device_id: "host".into(),
                title: "Custom label".into(),
                kind: "recent".into(),
                ..Default::default()
            },
        ];
        let root = session("root", "");
        let mut child = session("child", "root");
        child.pinned = true;
        let mut recent = session("recent", "");
        recent.section_id = "default".into();
        let mut plain = session("plain", "");
        plain.section_id.clear();
        state.agent_disclosures.insert(row_key(&root), false);
        let sessions = [root, child, recent, plain];
        for device in ["*", "host"] {
            state.agents.device_id = device.into();
            for query in ["", "needle"] {
                state.agents.query = query.into();
                let groups = project(&state, &sessions);
                assert_eq!(
                    groups.iter().map(|g| g.count).collect::<Vec<_>>(),
                    [1, 1, 2]
                );
                assert_eq!(groups[0].rows[0].session.as_ref().unwrap().id, "child");
                assert_eq!(groups[0].rows[0].depth, 0);
                assert!(groups[1].expanded); // native collapsed flag is not a subdivision
                assert_eq!(groups[1].rows[0].session.as_ref().unwrap().id, "root");
            }
        }
        state.agents.query.clear();
        state
            .agent_disclosures
            .insert("catalog:section".into(), false);
        let groups = project(&state, &sessions);
        assert_eq!(groups[1].count, 1);
        assert!(groups[1].rows.is_empty());
        assert_eq!(groups[0].rows.len(), 1);
    }

    #[test]
    fn older_peers_use_native_ids_not_section_titles() {
        let state = State::new(false, Some("host".into()));
        let mut s = session("one", "");
        s.section_id = "scope:sessions".into();
        assert_eq!(category(&state, &s), 2);
        s.section_id = "scope:pinned".into();
        assert_eq!(category(&state, &s), 0);
        s.provider = "codex".into();
        assert_eq!(category(&state, &s), 1);
    }
}
