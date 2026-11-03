//! Debounced cancellable per-host searches, separate from the live catalog.
use super::*;
pub(super) fn intent(state: &mut State, ctx: &Ctx, a: app::AgentCatalogAction) {
    use app::agent_catalog_action::Kind;
    match Kind::try_from(a.kind) {
        Ok(Kind::Search) => {
            if a.value.len() > 1024 {
                state.toast("Search text is too long", true);
                return;
            }
            state.agents.query = a.value;
            restart_search(state, ctx);
        }
        Ok(Kind::SearchMore) if !state.agents.searching && state.agents.search_has_more => {
            start_search(state, ctx, true)
        }
        Ok(Kind::ToggleSection | Kind::ToggleFamily) => {
            let sections = crate::catalog::project(state, &state.agents.sessions);
            let expanded = if a.kind == Kind::ToggleSection as i32 {
                sections
                    .iter()
                    .find(|g| g.key == a.value)
                    .map(|g| g.expanded)
            } else {
                sections
                    .iter()
                    .flat_map(|g| &g.rows)
                    .find(|r| r.key == a.value && r.expandable)
                    .map(|r| r.expanded)
            };
            if let Some(expanded) = expanded {
                if state.agent_disclosures.len() > 4096 {
                    state.agent_disclosures.clear();
                }
                state.agent_disclosures.insert(a.value, !expanded);
            }
        }
        _ => {}
    }
}
pub(super) fn restart_search(state: &mut State, ctx: &Ctx) {
    start_search(state, ctx, false);
}
fn start_search(state: &mut State, ctx: &Ctx, more: bool) {
    state.agents_search = None;
    state.agents_search_epoch += 1;
    if !more {
        state.agents_search_sessions.clear();
        state.agents.search_notices.clear();
        state.agents_search_cursors.clear();
        state.agents.search_has_more = false;
    }
    let query = state.agents.query.trim().to_string();
    state.agents.searching = !query.is_empty();
    if query.is_empty() {
        return;
    }
    let epoch = state.agents_search_epoch;
    let devices: Vec<_> = state
        .reachable_service_devices()
        .into_iter()
        .filter(|m| {
            state.agents.device_id.is_empty()
                || state.agents.device_id == "*"
                || state.agents.device_id == m.device_id
        })
        .filter(|m| !more || state.agents_search_cursors.contains_key(&m.device_id))
        .collect();
    state.agents_search_pending = devices.len();
    state.agents.searching = !devices.is_empty();
    let (watch, token) = state.new_watch();
    state.agents_search = Some(watch);
    for member in devices {
        let cursor = state
            .agents_search_cursors
            .get(&member.device_id)
            .cloned()
            .unwrap_or_default();
        let token = token.clone();
        let backend = ctx.backend.clone();
        let tx = ctx.tx.clone();
        let query = query.clone();
        let self_id = if member.is_self {
            member.device_id.clone()
        } else {
            state.self_id.clone()
        };
        tokio::spawn(async move {
            let run = async {
                tokio::time::sleep(Duration::from_millis(350)).await;
                let client = backend.client_for(&member.device_id, &self_id).await?;
                let response = client
                    .call(Req::AgentList(v1::AgentListRequest {
                        query: query.clone(),
                        search_cursor: cursor,
                        limit: 100,
                        ..Default::default()
                    }))
                    .await?;
                match response {
                    Resp::AgentSessions(mut page) if page.query == query => {
                        for s in &mut page.sessions {
                            s.device_id = member.device_id.clone();
                            s.device_name = member.name.clone();
                        }
                        Ok(page)
                    }
                    Resp::AgentSessions(_) => Err(Error::Unavailable(
                        "This device needs an update to search conversation content".into(),
                    )),
                    _ => Err(Error::Unavailable("Unexpected search response".into())),
                }
            };
            let result = tokio::select! {_=token.cancelled()=>None,r=tokio::time::timeout(Duration::from_secs(10),run)=>Some(r.unwrap_or_else(|_|Err(Error::Unavailable("Search timed out".into()))))};
            if let Some(result) = result {
                let _ = tx.send(loop_message(Msg::AgentSearch {
                    epoch,
                    device: member.device_id,
                    result,
                }));
            }
        });
    }
}
pub(super) fn result(
    state: &mut State,
    epoch: u64,
    device: String,
    result: Result<v1::AgentSessionPage, Error>,
) {
    if epoch != state.agents_search_epoch || state.agents.query.trim().is_empty() {
        return;
    }
    state.agents_search_pending = state.agents_search_pending.saturating_sub(1);
    state.agents.searching = state.agents_search_pending > 0;
    if state.offline(&device) {
        return;
    }
    match result {
        Ok(page) if page.query == state.agents.query.trim() => {
            state.agents_search_cursors.remove(&device);
            if !page.next_cursor.is_empty() {
                state
                    .agents_search_cursors
                    .insert(device.clone(), page.next_cursor);
            }
            state.agents.search_has_more = !state.agents_search_cursors.is_empty();
            let existing: std::collections::HashSet<_> = state
                .agents_search_sessions
                .iter()
                .map(|s| (s.device_id.clone(), s.id.clone()))
                .collect();
            state
                .agents_search_sessions
                .extend(page.sessions.into_iter().filter(|s| {
                    s.device_id == device
                        && !s.archived
                        && !existing.contains(&(s.device_id.clone(), s.id.clone()))
                }));
            if !page.notice.is_empty() && !state.agents.search_notices.contains(&page.notice) {
                state.agents.search_notices.push(page.notice);
            }
        }
        Ok(_) => {}
        Err(error) => state
            .agents
            .search_notices
            .push(format!("{device}: {error}")),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn stale_searches_and_wrong_devices_cannot_replace_results() {
        let mut state = State::new(false, Some("host".into()));
        state.agents.query = "new".into();
        state.agents_search_epoch = 2;
        state.agents_search_pending = 1;
        state.agents.searching = true;
        let page = || v1::AgentSessionPage {
            query: "new".into(),
            sessions: vec![v1::AgentSession {
                id: "same".into(),
                device_id: "host".into(),
                ..Default::default()
            }],
            ..Default::default()
        };
        result(&mut state, 1, "host".into(), Ok(page()));
        assert!(state.agents_search_sessions.is_empty());
        assert!(state.agents.searching);
        result(&mut state, 2, "other".into(), Ok(page()));
        assert!(state.agents_search_sessions.is_empty());
        result(&mut state, 2, "host".into(), Ok(page()));
        assert_eq!(state.agents_search_sessions.len(), 1);
        state.agents.query.clear();
        result(&mut state, 2, "host".into(), Ok(page()));
        assert_eq!(state.agents_search_sessions.len(), 1);
    }
}
