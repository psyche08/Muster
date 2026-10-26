use super::*;
pub(super) fn request(state: &mut State, ctx: &Ctx, request: v1::MulticaRequest) {
    if state.embedded || state.multica.busy {
        return;
    }
    state.multica.busy = true;
    let client = ctx.backend.local().clone();
    spawn(ctx, async move {
        let result = async {
            match unary(&client, Req::Multica(request)).await? {
                Resp::Multica(status) => Ok(status),
                _ => Err(unexpected()),
            }
        }
        .await;
        Msg::Multica(result)
    });
}

pub(super) fn browse(state: &mut State, ctx: &Ctx, request: v1::MulticaBrowseRequest) {
    if state.embedded {
        return;
    }
    state.multica_generation = state.multica_generation.wrapping_add(1);
    let generation = state.multica_generation;
    if state.multica_page.workspace_id != request.workspace_id && !request.workspace_id.is_empty() {
        let workspaces = std::mem::take(&mut state.multica_page.workspaces);
        state.multica_page = v1::MulticaPage {
            workspaces,
            workspace_id: request.workspace_id.clone(),
            ..Default::default()
        };
    }
    state.multica_page.issue = None;
    state.multica_page.agent = None;
    state.multica_page.busy = true;
    state.multica_page.error.clear();
    state.multica_query = request.clone();
    let client = ctx.backend.local().clone();
    spawn(ctx, async move {
        let result = async {
            match unary(&client, Req::MulticaBrowse(request)).await? {
                Resp::MulticaPage(page) => Ok(page),
                _ => Err(unexpected()),
            }
        }
        .await;
        Msg::MulticaPage { generation, result }
    });
}
pub(super) fn apply_page(
    state: &mut State,
    generation: u64,
    result: Result<v1::MulticaPage, Error>,
) {
    if generation != state.multica_generation {
        return;
    }
    state.multica_page.busy = false;
    match result {
        Ok(mut page) => {
            state.multica_query.workspace_id = page.workspace_id.clone();
            if page.issue.is_some() || page.agent.is_some() {
                state.multica_page.issue = page.issue.take();
                state.multica_page.agent = page.agent.take();
                state.multica_page.error.clear();
            } else {
                state.multica_page = page;
            }
        }
        Err(error) => state.multica_page.error = error.to_string(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn late_workspace_and_detail_responses_cannot_replace_the_current_selection() {
        let mut state = State::new(false, Some("local".into()));
        state.multica_generation = 2;
        state.multica_page = v1::MulticaPage {
            workspace_id: "workspace-b".into(),
            busy: true,
            ..Default::default()
        };
        apply_page(
            &mut state,
            1,
            Ok(v1::MulticaPage {
                workspace_id: "workspace-a".into(),
                issue: Some(v1::MulticaIssue {
                    id: "old-issue".into(),
                    ..Default::default()
                }),
                ..Default::default()
            }),
        );
        assert_eq!(state.multica_page.workspace_id, "workspace-b");
        assert!(state.multica_page.issue.is_none());
        assert!(state.multica_page.busy);
        apply_page(
            &mut state,
            2,
            Ok(v1::MulticaPage {
                workspace_id: "workspace-b".into(),
                ..Default::default()
            }),
        );
        assert!(!state.multica_page.busy);
        apply_page(&mut state, 1, Err(Error::Rpc("old error".into())));
        assert!(state.multica_page.error.is_empty());
    }
    #[test]
    fn a_detail_does_not_replace_the_selected_list_page() {
        let mut state = State::new(false, Some("local".into()));
        state.multica_generation = 7;
        state.multica_page = v1::MulticaPage {
            workspace_id: "w".into(),
            offset: 50,
            has_more: true,
            issues: vec![v1::MulticaIssue {
                id: "visible".into(),
                ..Default::default()
            }],
            ..Default::default()
        };
        apply_page(
            &mut state,
            7,
            Ok(v1::MulticaPage {
                workspace_id: "w".into(),
                issue: Some(v1::MulticaIssue {
                    id: "selected".into(),
                    ..Default::default()
                }),
                ..Default::default()
            }),
        );
        assert_eq!(state.multica_page.offset, 50);
        assert!(state.multica_page.has_more);
        assert_eq!(state.multica_page.issues[0].id, "visible");
        assert_eq!(state.multica_page.issue.unwrap().id, "selected");
    }
}
