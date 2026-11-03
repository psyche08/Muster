//! Display-only native sections and family trees. Identity is (device, provider, UUID).
use crate::state::State;
use muster_proto::{
    app::{AgentCatalogRow, AgentCatalogSection},
    v1::AgentSession,
};
use std::collections::HashMap;
#[cfg(test)]
use std::collections::HashSet;
pub(crate) fn row_key(s: &AgentSession) -> String {
    serde_json::to_string(&(&s.device_id, &s.provider, &s.id)).unwrap()
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
#[cfg(test)]
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
pub(crate) fn project(state: &State, sessions: &[AgentSession]) -> Vec<AgentCatalogSection> {
    use muster_agent::catalog::{PAGE_SIZE, display_key, family_roots, group};
    let searching = !state.agents.query.trim().is_empty();
    let all: HashMap<_, _> = sessions.iter().map(|s| (row_key(s), s)).collect();
    let families = family_roots(sessions);
    let mut projected = std::collections::BTreeMap::<String, AgentCatalogSection>::new();
    let mut add = |g: muster_proto::v1::AgentCatalogGroup, remote: bool| {
        let key = display_key(&g);
        let p = projected
            .entry(key.clone())
            .or_insert_with(|| AgentCatalogSection {
                key: key.clone(),
                title: g.title.clone(),
                kind: g.kind.clone(),
                expanded: searching || *state.agent_disclosures.get(&key).unwrap_or(&true),
                loading_more: state
                    .agent_catalog_pending
                    .get(&key)
                    .is_some_and(|n| *n > 0),
                ..Default::default()
            });
        if remote {
            p.count += g.count;
            p.has_more |= !g.next_cursor.is_empty();
        }
        if p.kind != g.kind && !matches!(p.kind.as_str(), "pin" | "recent") {
            p.kind = "mixed".into();
        }
    };
    for (kind, title) in [("pin", "Pin"), ("recent", "Recent")] {
        add(
            muster_proto::v1::AgentCatalogGroup {
                kind: kind.into(),
                title: title.into(),
                ..Default::default()
            },
            false,
        );
    }
    if !searching {
        for g in &state.agent_catalog_groups {
            if !state.offline(&g.device_id)
                && (state.agents.device_id.is_empty()
                    || state.agents.device_id == "*"
                    || state.agents.device_id == g.device_id)
            {
                add(g.clone(), true);
            }
        }
        for sec in &state.agents.native_sections {
            if state.offline(&sec.device_id)
                || !(state.agents.device_id.is_empty()
                    || state.agents.device_id == "*"
                    || state.agents.device_id == sec.device_id)
            {
                continue;
            }
            add(
                group(
                    &AgentSession {
                        provider: sec.provider.clone(),
                        section_id: sec.id.clone(),
                        device_id: sec.device_id.clone(),
                        ..Default::default()
                    },
                    &state.agents.native_sections,
                ),
                false,
            );
        }
    }
    let mut roots = Vec::new();
    let mut children: HashMap<String, Vec<&AgentSession>> = HashMap::new();
    for s in sessions {
        let own = row_key(s);
        if !searching && families[&own] != own {
            if let Some(p) = parent(s, &all) {
                children.entry(row_key(p)).or_default().push(s);
            }
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
    let mut seen: HashMap<String, usize> = HashMap::new();
    for root in roots {
        let g = group(root, &state.agents.native_sections);
        let key = display_key(&g);
        add(g, false);
        *seen.entry(key).or_default() += 1;
    }
    let mut shown: HashMap<String, usize> = HashMap::new();
    let mut roots: Vec<_> = sessions
        .iter()
        .filter(|s| searching || families[&row_key(s)] == row_key(s))
        .collect();
    sort(&mut roots);
    for root in roots {
        let key = display_key(&group(root, &state.agents.native_sections));
        let limit = *state.agent_catalog_limits.get(&key).unwrap_or(&PAGE_SIZE);
        let used = shown.entry(key.clone()).or_default();
        if *used >= limit {
            continue;
        }
        *used += 1;
        append(
            root,
            0,
            &children,
            &all,
            &state.agent_disclosures,
            projected.get_mut(&key).unwrap(),
            searching,
        );
    }
    for (key, g) in &mut projected {
        let available = seen.get(key).copied().unwrap_or(0);
        if searching || state.agent_catalog_groups.is_empty() {
            g.count = available as u32;
        }
        g.has_more |= available > *state.agent_catalog_limits.get(key).unwrap_or(&PAGE_SIZE);
        g.subtitle = match g.kind.as_str() {
            "section" => "Section group",
            "project" => "Project Group",
            "mixed" => "Section group / Project Group",
            _ => "",
        }
        .into();
        for row in &mut g.rows {
            let Some(s) = &row.session else {
                continue;
            };
            let device = if s.device_name.is_empty() {
                &s.device_id
            } else {
                &s.device_name
            };
            let mut origin = vec![provider_title(&s.provider)];
            if !device.is_empty() {
                origin.push(device.clone());
            }
            if let Some(sec) = native_section(state, s)
                && matches!(sec.kind.as_str(), "section" | "project")
                && !sec.title.is_empty()
            {
                origin.push(sec.title.clone());
            }
            row.source_label = origin.join(" · ");
            row.can_organize = can_organize(state, s);
            row.organizing = state.agent_organizing.contains(&row.key);
            row.destinations = destinations(state, s);
            row.removable_section = row
                .destinations
                .iter()
                .find(|d| d.section_id == s.section_id)
                .map(|d| d.section_id.clone())
                .unwrap_or_default();
        }
    }
    for sec in &state.agents.native_sections {
        if !matches!(sec.kind.as_str(), "section" | "project") {
            continue;
        }
        let Some(anchor) = state.agents.sessions.iter().find(|s| {
            s.device_id == sec.device_id
                && s.provider == sec.provider
                && can_organize(state, s)
                && destinations(state, s)
                    .iter()
                    .any(|d| d.section_id == sec.id)
        }) else {
            continue;
        };
        let key = display_key(&group(
            &AgentSession {
                provider: sec.provider.clone(),
                section_id: sec.id.clone(),
                device_id: sec.device_id.clone(),
                ..Default::default()
            },
            &state.agents.native_sections,
        ));
        if let Some(g) = projected.get_mut(&key) {
            g.native_targets
                .push(muster_proto::app::AgentOrganizationTarget {
                    key: serde_json::to_string(&(&anchor.device_id, &anchor.provider, &sec.id))
                        .unwrap(),
                    row_key: row_key(anchor),
                    section_id: sec.id.clone(),
                    title: format!(
                        "{} · {} · {}",
                        provider_title(&sec.provider),
                        anchor.device_name,
                        sec.title
                    ),
                });
        }
    }
    let mut out: Vec<_> = projected.into_values().collect();
    let rank = |g: &AgentCatalogSection| match g.kind.as_str() {
        "pin" => 0,
        "recent" => 2,
        _ => 1,
    };
    out.sort_by(|a, b| rank(a).cmp(&rank(b)).then_with(|| a.title.cmp(&b.title)));
    out
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
    use muster_proto::v1::{AgentCatalogGroup, AgentSection};
    fn session(id: &str, parent: &str, device: &str) -> AgentSession {
        AgentSession {
            id: id.into(),
            title: "Same".into(),
            provider: "claude".into(),
            device_id: device.into(),
            section_id: "custom".into(),
            relationship: "subsession".into(),
            parent_session_id: parent.into(),
            ..Default::default()
        }
    }
    #[test]
    fn equal_names_aggregate_without_collapsing_session_or_native_identity() {
        let mut state = State::new(false, Some("host".into()));
        state.agents.device_id = "*".into();
        state.agents.native_sections = ["host", "second"]
            .into_iter()
            .map(|d| AgentSection {
                id: "custom".into(),
                provider: "claude".into(),
                device_id: d.into(),
                title: "Workspace".into(),
                kind: if d == "host" { "section" } else { "project" }.into(),
                ..Default::default()
            })
            .collect();
        let rows = [session("same", "", "host"), session("same", "", "second")];
        let groups = project(&state, &rows);
        assert_eq!(
            groups.iter().map(|g| g.title.as_str()).collect::<Vec<_>>(),
            ["Pin", "Workspace", "Recent"]
        );
        assert_eq!(groups[1].kind, "mixed");
        assert_eq!(groups[1].count, 2);
        assert_ne!(groups[1].rows[0].key, groups[1].rows[1].key);
        assert_eq!(state.agents.native_sections.len(), 2);
    }
    #[test]
    fn ten_roots_are_shown_with_all_children_and_load_more_advances_ten() {
        let mut state = State::new(false, Some("host".into()));
        state.agents.native_sections = vec![AgentSection {
            id: "custom".into(),
            provider: "claude".into(),
            device_id: "host".into(),
            title: "Work".into(),
            kind: "section".into(),
            ..Default::default()
        }];
        let mut rows = Vec::new();
        for i in 0..21 {
            let id = format!("r{i:02}");
            rows.push(session(&id, "", "host"));
            for j in 0..3 {
                rows.push(session(&format!("c{i:02}-{j}"), &id, "host"));
            }
        }
        let groups = project(&state, &rows);
        let g = &groups[1];
        assert_eq!(g.count, 21);
        assert_eq!(g.rows.len(), 40);
        assert!(g.has_more);
        assert_eq!(g.rows.iter().filter(|r| r.depth == 0).count(), 10);
        state.agent_catalog_limits.insert(g.key.clone(), 20);
        let groups = project(&state, &rows);
        assert_eq!(groups[1].rows.len(), 80);
        assert_eq!(groups[1].rows.iter().filter(|r| r.depth == 0).count(), 20);
    }
    #[test]
    fn server_totals_and_continuations_include_unloaded_roots() {
        let mut state = State::new(false, Some("host".into()));
        state.agent_catalog_groups = vec![AgentCatalogGroup {
            key: "recent".into(),
            title: "Recent".into(),
            kind: "recent".into(),
            device_id: "host".into(),
            count: 42,
            next_cursor: "cursor".into(),
            ..Default::default()
        }];
        let mut s = session("one", "", "host");
        s.section_id.clear();
        let groups = project(&state, &[s]);
        let g = groups.iter().find(|g| g.kind == "recent").unwrap();
        assert_eq!(g.count, 42);
        assert!(g.has_more);
        assert_eq!(g.rows.len(), 1);
    }
}

pub(crate) fn can_organize(state: &State, s: &AgentSession) -> bool {
    !s.read_only
        && s.mode == "code"
        && muster_agent::viewer::is_uuid(&s.id)
        && matches!(s.provider.as_str(), "codex" | "claude")
        && !state.offline(&s.device_id)
}
pub(crate) fn destinations(
    state: &State,
    s: &AgentSession,
) -> Vec<muster_proto::app::AgentOrganizationTarget> {
    let scope = if s.provider == "claude" {
        s.section_id.rsplit_once(':').map(|(scope, _)| scope)
    } else {
        None
    };
    state
        .agents
        .native_sections
        .iter()
        .filter(|g| {
            g.device_id == s.device_id
                && g.provider == s.provider
                && matches!(g.kind.as_str(), "section" | "project")
                && (s.provider != "claude"
                    || scope.is_some_and(|scope| g.id.starts_with(&format!("{scope}:"))))
        })
        .map(|g| muster_proto::app::AgentOrganizationTarget {
            key: serde_json::to_string(&(&s.device_id, &s.provider, &g.id)).unwrap(),
            row_key: row_key(s),
            section_id: g.id.clone(),
            title: g.title.clone(),
        })
        .collect()
}
