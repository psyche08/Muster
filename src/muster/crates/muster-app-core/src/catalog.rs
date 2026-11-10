//! Display-only native sections and family trees. Identity is (device, provider, UUID).
use crate::state::State;
use muster_proto::{
    app::{AgentCatalogRow, AgentCatalogSection},
    v1::AgentSession,
};
use std::collections::HashMap;
#[cfg(test)]
use std::collections::HashSet;
pub(crate) const PROVIDER_LIMIT: &str = "catalog:providers";

pub(crate) fn row_key(s: &AgentSession) -> String {
    serde_json::to_string(&(&s.device_id, &s.provider, &s.id)).unwrap()
}
/// Replace a source-qualified session projection without losing search context
/// or a detailed chat's child list when the input is a catalog summary.
pub(crate) fn reconcile_session(
    target: &mut AgentSession,
    fresh: &AgentSession,
    summary: bool,
) -> bool {
    if target.id != fresh.id
        || target.provider != fresh.provider
        || target.device_id != fresh.device_id
    {
        return false;
    }
    let snippet = std::mem::take(&mut target.search_snippet);
    let children = summary.then(|| {
        (
            std::mem::take(&mut target.subagents),
            std::mem::take(&mut target.subsessions),
        )
    });
    *target = fresh.clone();
    target.search_snippet = snippet;
    if let Some((subagents, subsessions)) = children {
        target.subagents = subagents;
        target.subsessions = subsessions;
    }
    true
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
                title: if g.kind == "pin" { "Pin" } else { "Recent" }.into(),
                kind: if g.kind == "pin" { "pin" } else { "recent" }.into(),
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
                if g.kind == "provider" {
                    for (kind, count) in [
                        ("pin", g.pinned_count),
                        ("recent", g.count.saturating_sub(g.pinned_count)),
                    ] {
                        let mut display = g.clone();
                        display.kind = kind.into();
                        display.count = count;
                        if count == 0 {
                            display.next_cursor.clear();
                        }
                        add(display, true);
                    }
                } else {
                    add(g.clone(), true);
                }
            }
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
    let mut sources: HashMap<String, usize> = HashMap::new();
    let mut roots: Vec<_> = sessions
        .iter()
        .filter(|s| searching || families[&row_key(s)] == row_key(s))
        .collect();
    sort(&mut roots);
    for root in roots {
        let key = display_key(&group(root, &state.agents.native_sections));
        let budget_key = if searching {
            key.as_str()
        } else {
            PROVIDER_LIMIT
        };
        let limit = *state
            .agent_catalog_limits
            .get(budget_key)
            .unwrap_or(&PAGE_SIZE);
        let source = if searching {
            key.clone()
        } else {
            serde_json::to_string(&(&root.device_id, &root.provider)).unwrap()
        };
        let used = sources.entry(source).or_default();
        if *used >= limit {
            continue;
        }
        *used += 1;
        *shown.entry(key.clone()).or_default() += 1;
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
        g.has_more |= available > shown.get(key).copied().unwrap_or(0);
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
            row.source_label = origin.join(" · ");
            row.can_organize = can_organize(state, s);
            row.organizing = state.agent_organizing.contains(&row.key);
        }
    }
    // Pin is the owner's own list: with nothing in it there is nothing to say,
    // so it is left out entirely rather than shown empty. A group the daemon
    // still counts, is still paging or is still loading stays, so the section
    // does not flicker away before its rows arrive. Recent is the catalog
    // itself and remains, empty or not.
    projected.retain(|_, g| {
        g.kind != "pin" || !g.rows.is_empty() || g.count > 0 || g.has_more || g.loading_more
    });
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
        "orca" => "CLI",
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
    #[test]
    fn every_device_provider_gets_ten_and_twenty_across_display_sections() {
        let mut state = State::new(false, None);
        let mut sessions = Vec::new();
        for device in ["device-a", "device-b"] {
            for provider in ["claude", "codex"] {
                for i in 0..25 {
                    sessions.push(AgentSession {
                        id: format!("s-{i:02}"),
                        device_id: device.into(),
                        provider: provider.into(),
                        pinned: i % 2 == 0,
                        updated_unix: 100 - i,
                        ..Default::default()
                    });
                }
            }
        }
        for limit in [10, 20] {
            state
                .agent_catalog_limits
                .insert(PROVIDER_LIMIT.into(), limit);
            let sections = project(&state, &sessions);
            let rows: Vec<_> = sections
                .iter()
                .flat_map(|g| &g.rows)
                .filter_map(|r| r.session.as_ref())
                .collect();
            assert_eq!(rows.len(), 4 * limit);
            assert!(sections.iter().all(|g| g.has_more));
            for device in ["device-a", "device-b"] {
                assert_eq!(visible_ids(&sessions, device, limit).len(), 2 * limit);
                for provider in ["claude", "codex"] {
                    assert_eq!(
                        rows.iter()
                            .filter(|s| s.device_id == device && s.provider == provider)
                            .count(),
                        limit
                    );
                }
            }
        }
    }
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
    use muster_proto::v1::{AgentCatalogGroup, AgentSection};
    /// Pin is absent whenever nothing is pinned, so find Recent by kind.
    fn recent(groups: &[AgentCatalogSection]) -> &AgentCatalogSection {
        groups.iter().find(|g| g.kind == "recent").expect("Recent")
    }
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
    fn native_sections_and_projects_flatten_without_losing_session_identity() {
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
            ["Recent"]
        );
        let shown = recent(&groups);
        assert!(groups.iter().all(|g| g.native_targets.is_empty()));
        assert!(shown.rows.iter().all(|r| r.destinations.is_empty()
            && r.removable_section.is_empty()
            && !r.source_label.contains("Workspace")));
        assert_eq!(shown.count, 2);
        assert_ne!(shown.rows[0].key, shown.rows[1].key);
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
        let g = recent(&groups);
        assert_eq!(g.count, 21);
        assert_eq!(g.rows.len(), 40);
        assert!(g.has_more);
        assert_eq!(g.rows.iter().filter(|r| r.depth == 0).count(), 10);
        state.agent_catalog_limits.insert(PROVIDER_LIMIT.into(), 20);
        let groups = project(&state, &rows);
        let g = recent(&groups);
        assert_eq!(g.rows.len(), 80);
        assert_eq!(g.rows.iter().filter(|r| r.depth == 0).count(), 20);
    }
    #[test]
    fn old_peer_section_pages_merge_into_recent_and_remain_fetchable() {
        let mut state = State::new(false, Some("host".into()));
        state.agent_catalog_groups = ["section", "project"]
            .into_iter()
            .map(|kind| AgentCatalogGroup {
                key: format!("old-{kind}"),
                title: "Old native name".into(),
                kind: kind.into(),
                device_id: "host".into(),
                count: 12,
                next_cursor: format!("next-{kind}"),
                ..Default::default()
            })
            .collect();
        let groups = project(&state, &[session("one", "", "host")]);
        assert_eq!(groups.len(), 1);
        let recent = recent(&groups);
        assert_eq!(recent.title, "Recent");
        assert_eq!(recent.count, 24);
        assert_eq!(recent.rows.len(), 1);
        assert!(recent.has_more);
        assert!(recent.native_targets.is_empty());
        assert_eq!(
            state
                .agent_catalog_groups
                .iter()
                .filter(|g| muster_agent::catalog::display_key(g) == recent.key
                    && !g.next_cursor.is_empty())
                .count(),
            2
        );
        assert_eq!(state.agent_catalog_groups[0].key, "old-section");
        state.agents.query = "same".into();
        let search = project(&state, &[session("one", "", "host")]);
        assert_eq!(search.len(), 1);
        assert_eq!(search[0].count, 1);
    }

    #[test]
    fn pin_appears_only_while_it_has_something_to_show() {
        let mut state = State::new(false, Some("host".into()));
        let plain = session("one", "", "host");
        let titles = |groups: &[AgentCatalogSection]| {
            groups.iter().map(|g| g.kind.clone()).collect::<Vec<_>>()
        };
        assert_eq!(
            titles(&project(&state, std::slice::from_ref(&plain))),
            ["recent"]
        );
        assert_eq!(titles(&project(&state, &[])), ["recent"]);

        let mut pinned = plain.clone();
        pinned.id = "two".into();
        pinned.pinned = true;
        let groups = project(&state, &[plain.clone(), pinned.clone()]);
        assert_eq!(titles(&groups), ["pin", "recent"]);
        assert_eq!(groups[0].rows.len(), 1);
        assert_eq!(groups[0].rows[0].session.as_ref().unwrap().id, "two");

        // Searching hides a Pin whose rows no longer match.
        state.agents.query = "nothing matches".into();
        assert_eq!(
            titles(&project(&state, std::slice::from_ref(&plain))),
            ["recent"]
        );
        state.agents.query.clear();

        // A page the daemon still counts keeps its place until its rows land.
        state.agent_catalog_groups = vec![AgentCatalogGroup {
            key: "catalog:pin".into(),
            kind: "pin".into(),
            title: "Pin".into(),
            device_id: "host".into(),
            count: 3,
            ..Default::default()
        }];
        let groups = project(&state, std::slice::from_ref(&plain));
        assert_eq!(titles(&groups), ["pin", "recent"]);
        assert!(groups[0].rows.is_empty());
        state.agent_catalog_groups[0].count = 0;
        state.agent_catalog_groups[0].next_cursor = "more".into();
        assert_eq!(
            titles(&project(&state, std::slice::from_ref(&plain))),
            ["pin", "recent"]
        );
        state.agent_catalog_groups[0].next_cursor.clear();
        assert_eq!(
            titles(&project(&state, std::slice::from_ref(&plain))),
            ["recent"]
        );
        state.agent_catalog_pending.insert("catalog:pin".into(), 1);
        assert_eq!(
            titles(&project(&state, std::slice::from_ref(&plain))),
            ["pin", "recent"]
        );
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

#[cfg(test)]
mod reconcile_tests {
    use super::*;
    #[test]
    fn metadata_joins_require_source_identity_and_keep_view_context() {
        let mut old = AgentSession {
            id: "same-uuid".into(),
            provider: "claude".into(),
            device_id: "host-a".into(),
            title: "old".into(),
            search_snippet: "match".into(),
            subagents: vec![AgentSession::default()],
            ..Default::default()
        };
        let mut fresh = AgentSession {
            title: "new".into(),
            updated_unix: 42,
            subagents: vec![],
            ..old.clone()
        };
        fresh.device_id = "host-b".into();
        assert!(!reconcile_session(&mut old, &fresh, true));
        fresh.device_id = "host-a".into();
        fresh.provider = "codex".into();
        assert!(!reconcile_session(&mut old, &fresh, true));
        fresh.provider = "claude".into();
        fresh.search_snippet.clear();
        assert!(reconcile_session(&mut old, &fresh, true));
        assert_eq!(old.title, "new");
        assert_eq!(old.updated_unix, 42);
        assert_eq!(old.search_snippet, "match");
        assert_eq!(old.subagents.len(), 1);
        assert!(reconcile_session(&mut old, &fresh, false));
        assert!(old.subagents.is_empty());
    }
}

/// Keep only families currently within each source budget when restoring a cache.
pub(crate) fn visible_ids(sessions: &[AgentSession], device: &str, limit: usize) -> Vec<String> {
    let families = muster_agent::catalog::family_roots(sessions);
    let mut roots: Vec<_> = sessions
        .iter()
        .filter(|s| s.device_id == device && families[&row_key(s)] == row_key(s))
        .collect();
    roots.sort_by(|a, b| {
        b.updated_unix
            .cmp(&a.updated_unix)
            .then_with(|| row_key(a).cmp(&row_key(b)))
    });
    let mut counts = HashMap::<String, usize>::new();
    let chosen: std::collections::HashSet<_> = roots
        .into_iter()
        .filter_map(|s| {
            let count = counts.entry(s.provider.clone()).or_default();
            if *count >= limit {
                return None;
            }
            *count += 1;
            Some(row_key(s))
        })
        .collect();
    sessions
        .iter()
        .filter(|s| s.device_id == device && chosen.contains(&families[&row_key(s)]))
        .map(|s| s.id.clone())
        .take(2000)
        .collect()
}
