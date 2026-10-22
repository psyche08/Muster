//! Unsent text is local data, never authority or evidence of delivery.
use crate::cache::Cache;
use muster_proto::{app::ChatDraft, v1::op_result::Status};

/// Which actions can leave the composer. This policy belongs to Shared Rust;
/// shells attach an input snapshot without interpreting navigation semantics.
pub(crate) fn saves_before(action: &muster_proto::app::intent::It) -> bool {
    use muster_proto::app::{desktop_setup_action, desktop_update_action, intent::It};
    match action {
        It::Navigate(_)
        | It::SelectDevice(_)
        | It::OpenAgents(_)
        | It::OpenAgent(_)
        | It::OpenDesktopChats(_)
        | It::DesktopChatOpen(_)
        | It::OpenTerminals(_)
        | It::OpenTerminal(_)
        | It::OpenSsh(_)
        | It::OpenFiles(_)
        | It::OpenFile(_)
        | It::OpenGit(_)
        | It::OpenChange(_)
        | It::OpenFileLink(_)
        | It::OpenBrowser(_)
        | It::ShowInvite(_)
        | It::JoinNetwork(_) => true,
        It::DesktopSetupAction(a) => a.action == desktop_setup_action::Action::Restart as i32,
        It::DesktopUpdateAction(a) => a.action == desktop_update_action::Action::Install as i32,
        _ => false,
    }
}

/// A receipt may have cleared this revision before its last UI snapshot was
/// rendered. Do not resurrect that text when a queued navigation arrives.
pub(crate) fn reconcile(cache: &Cache, draft: ChatDraft) -> ChatDraft {
    if let Some(saved) = cache.get::<ChatDraft>(&draft.key)
        && saved.revision == draft.revision
        && saved.text.is_empty()
    {
        return saved;
    }
    draft
}

pub(crate) fn clear_sent(
    cache: &Cache,
    submitted: &ChatDraft,
    status: i32,
) -> std::io::Result<Option<ChatDraft>> {
    if !matches!(
        Status::try_from(status),
        Ok(Status::Applied | Status::Duplicate)
    ) {
        return Ok(None);
    }
    let Some(current) = cache.get::<ChatDraft>(&submitted.key) else {
        return Ok(None);
    };
    if current != *submitted {
        return Ok(None);
    }
    let cleared = ChatDraft {
        text: String::new(),
        ..current
    };
    cache.put_durable(cleared.key.clone(), &cleared)?;
    Ok(Some(cleared))
}

#[cfg(test)]
mod tests {
    use super::*;
    #[tokio::test]
    async fn submitting_does_not_write_and_stale_navigation_cannot_restore_sent_text() {
        let dir = tempfile::tempdir().unwrap();
        let cache = Cache::new(dir.path().into());
        let draft = ChatDraft {
            key: "draft".into(),
            text: "send me".into(),
            revision: "1".into(),
        };
        cache.remember(draft.key.clone(), &draft);
        assert!(!dir.path().join(&draft.key).exists());
        let cleared = clear_sent(&cache, &draft, Status::Applied as i32)
            .unwrap()
            .unwrap();
        assert_eq!(reconcile(&cache, draft.clone()), cleared);
        let newer = ChatDraft {
            revision: "2".into(),
            ..draft
        };
        assert_eq!(reconcile(&cache, newer.clone()), newer);
    }
    #[tokio::test]
    async fn draft_survives_reopening_and_failed_or_unknown_sends() {
        let dir = tempfile::tempdir().unwrap();
        let draft = ChatDraft {
            key: Cache::key(&["draft", "host", "session"]),
            text: "未发送的草稿\nsecond line".into(),
            revision: "edit".into(),
        };
        let cache = Cache::new(dir.path().into());
        cache.put_durable(draft.key.clone(), &draft).unwrap();
        drop(cache);
        let reopened = Cache::new(dir.path().into());
        assert_eq!(reopened.get::<ChatDraft>(&draft.key), Some(draft.clone()));
        for status in [Status::Rejected as i32, Status::Unknown as i32, -1] {
            assert!(clear_sent(&reopened, &draft, status).unwrap().is_none());
            assert_eq!(reopened.get::<ChatDraft>(&draft.key), Some(draft.clone()));
        }
        assert!(
            clear_sent(&reopened, &draft, Status::Applied as i32)
                .unwrap()
                .unwrap()
                .text
                .is_empty()
        );
        drop(reopened);
        let reopened = Cache::new(dir.path().into());
        assert!(
            reopened
                .get::<ChatDraft>(&draft.key)
                .unwrap()
                .text
                .is_empty()
        );
    }
    #[tokio::test]
    async fn late_success_clears_its_session_but_preserves_newer_edits() {
        let dir = tempfile::tempdir().unwrap();
        let cache = Cache::new(dir.path().into());
        let a = ChatDraft {
            key: Cache::key(&["draft", "host", "a"]),
            text: "first".into(),
            revision: "1".into(),
        };
        let b = ChatDraft {
            key: Cache::key(&["draft", "host", "b"]),
            text: "other session".into(),
            revision: "2".into(),
        };
        cache.put(a.key.clone(), &a);
        cache.put(b.key.clone(), &b);
        let edited = ChatDraft {
            text: "next message".into(),
            revision: "3".into(),
            ..a.clone()
        };
        cache.put(a.key.clone(), &edited);
        assert!(
            clear_sent(&cache, &a, Status::Applied as i32)
                .unwrap()
                .is_none()
        );
        assert_eq!(cache.get::<ChatDraft>(&a.key), Some(edited.clone()));
        assert_eq!(
            clear_sent(&cache, &edited, Status::Duplicate as i32)
                .unwrap()
                .unwrap()
                .text,
            ""
        );
        assert_eq!(cache.get::<ChatDraft>(&b.key), Some(b));
    }
}
