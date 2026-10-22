//! A session's files and git history.

use super::*;

pub(super) fn open_files(state: &mut State, ctx: &Ctx, path: String) {
    let Some((device, session)) = chat_target(state) else {
        return;
    };
    state.files = Some(app::FilesView {
        device_id: device.clone(),
        session: session.clone(),
        path: path.clone(),
        entries: Vec::new(),
        loading: true,
    });
    state.go(Screen::Files);
    let self_id = state.self_id.clone();
    let backend = ctx.backend.clone();
    spawn(ctx, async move {
        let result = async {
            let client = backend.client_for(&device, &self_id).await?;
            match unary(
                &client,
                Req::AgentFiles(v1::AgentFilesRequest {
                    session: session.clone(),
                    path,
                }),
            )
            .await?
            {
                Resp::FileList(list) => Ok(list),
                _ => Err(unexpected()),
            }
        }
        .await;
        Msg::Files {
            device,
            session,
            result,
        }
    });
}

/// Bytes of a file the core fetches for the shell to show.
pub(super) const FILE_VIEW_MAX: u64 = 512 * 1024;

pub(super) fn session_file_link(cwd: &str, link: &str) -> Result<String, &'static str> {
    if link.starts_with("muster-delivered:") {
        return muster_agent::claude_cards::delivered_key(link)
            .map(|_| link.to_owned())
            .ok_or("Invalid delivered file link");
    }
    let base =
        url::Url::from_directory_path(cwd).map_err(|_| "Session directory is unavailable")?;
    let mut url = base.join(link).map_err(|_| "Invalid file link")?;
    url.set_fragment(None);
    url.set_query(None);
    if url.scheme() != "file" {
        return Err("Unsupported file link");
    }
    let path = url.to_file_path().map_err(|_| "Invalid file link")?;
    let rel = path
        .strip_prefix(cwd)
        .map_err(|_| "File is outside the session directory")?;
    if rel.as_os_str().is_empty() {
        return Err("Select a file to preview");
    }
    Ok(rel.to_string_lossy().into_owned())
}

pub(super) fn open_change(state: &mut State, request: app::OpenChange) {
    let Some(chat) = &state.chat else {
        return;
    };
    let Some(change) = chat
        .messages
        .iter()
        .find(|m| m.id == request.summary_id && m.kind == "turn_summary")
        .and_then(|m| m.usage.as_ref())
        .and_then(|u| u.changes.iter().find(|c| c.path == request.path))
    else {
        state.toast("This recorded change is no longer loaded", true);
        return;
    };
    state.file = Some(app::FileView {
        device_id: chat.device.clone(),
        session: chat.session.id.clone(),
        path: change.path.clone(),
        content: change.diff.as_bytes().to_vec(),
        truncated: change.truncated,
        is_diff: true,
        current_path: change.open_path.clone(),
        error: if change.diff.is_empty() {
            "The provider did not record a diff for this edit.".into()
        } else {
            String::new()
        },
        ..Default::default()
    });
    state.go(Screen::File);
}

pub(super) fn open_file(state: &mut State, ctx: &Ctx, path: String) {
    let Some((device, session)) = chat_target(state) else {
        return;
    };
    state.file = Some(app::FileView {
        device_id: device.clone(),
        session: session.clone(),
        path: path.clone(),
        content: Vec::new(),
        truncated: false,
        loading: true,
        ..Default::default()
    });
    state.go(Screen::File);
    let self_id = state.self_id.clone();
    let backend = ctx.backend.clone();
    spawn(ctx, async move {
        let result = async {
            let client = backend.client_for(&device, &self_id).await?;
            match unary(
                &client,
                Req::AgentFile(v1::AgentFileRequest {
                    session: session.clone(),
                    path: path.clone(),
                    max_bytes: FILE_VIEW_MAX,
                }),
            )
            .await?
            {
                Resp::FileContent(content) => Ok(content),
                _ => Err(unexpected()),
            }
        }
        .await;
        Msg::File {
            device,
            session,
            path,
            result,
        }
    });
}

pub(super) fn open_git(state: &mut State, ctx: &Ctx, diff_commit: String) {
    let Some((device, session)) = chat_target(state) else {
        return;
    };
    state.git = Some(app::GitView {
        device_id: device.clone(),
        session: session.clone(),
        commits: Vec::new(),
        diff_commit: diff_commit.clone(),
        diff: String::new(),
        loading: true,
    });
    state.go(Screen::Git);
    let self_id = state.self_id.clone();
    let backend = ctx.backend.clone();
    spawn(ctx, async move {
        let result = async {
            let client = backend.client_for(&device, &self_id).await?;
            match unary(
                &client,
                Req::AgentGit(v1::AgentGitRequest {
                    session: session.clone(),
                    limit: 50,
                    diff_commit,
                }),
            )
            .await?
            {
                Resp::GitLog(log) => Ok(log),
                _ => Err(unexpected()),
            }
        }
        .await;
        Msg::Git {
            device,
            session,
            result,
        }
    });
}

pub(super) fn intent_open_file_link(state: &mut State, ctx: &Ctx, o: app::OpenFileLink) {
    let Some(chat) = &state.chat else {
        return;
    };
    match session_file_link(&chat.session.cwd, &o.url) {
        Ok(path) => open_file(state, ctx, path),
        Err(error) => state.toast(error, true),
    }
}

pub(super) fn msg_files(
    state: &mut State,
    device: String,
    session: String,
    result: Result<v1::FileList, Error>,
) {
    let Some(files) = &mut state.files else {
        return;
    };
    if files.device_id != device || files.session != session {
        return;
    }
    files.loading = false;
    match result {
        Ok(list) => {
            files.path = list.path;
            files.entries = list.entries;
        }
        Err(error) => state.toast(format!("files: {error}"), true),
    }
}

pub(super) fn msg_file(
    state: &mut State,
    device: String,
    session: String,
    path: String,
    result: Result<v1::FileContent, Error>,
) {
    let Some(file) = &mut state.file else { return };
    if file.device_id != device || file.session != session || file.path != path || file.is_diff {
        return;
    }
    file.loading = false;
    match result {
        Ok(content) => {
            file.path = content.path;
            file.content = content.content;
            file.truncated = content.truncated;
        }
        Err(error) => file.error = error.to_string(),
    }
}

pub(super) fn msg_git(
    state: &mut State,
    device: String,
    session: String,
    result: Result<v1::GitLog, Error>,
) {
    let Some(git) = &mut state.git else { return };
    if git.device_id != device || git.session != session {
        return;
    }
    git.loading = false;
    match result {
        Ok(log) => {
            git.commits = log.commits;
            git.diff = log.diff;
        }
        Err(error) => state.toast(format!("git: {error}"), true),
    }
}
