//! Desktop Chat-mode conversations in the service: what the platform
//! observes on screen becomes sessions, messages and events.

use std::time::Duration;

use muster_agent::desktop_chat::{ChatApp, ChatKey, ChatLine};
use muster_agent::model::Role;
use muster_agent::{AgentConfig, AgentService, Error, Event};

const UUID: &str = "0f0e0d0c-1111-4222-8333-444455556666";

fn service() -> std::sync::Arc<AgentService> {
    AgentService::start(AgentConfig {
        codex_home: None,
        claude_home: None,
        catdesk_home: None,
        deepseek_home: None,
        claude_desktop: None,
        poll: Duration::from_secs(60),
        rescan: Duration::from_secs(60),
        ..AgentConfig::default()
    })
}

fn line(role: Role, text: &str) -> ChatLine {
    ChatLine {
        role,
        text: text.into(),
    }
}

fn drain(rx: &mut tokio::sync::broadcast::Receiver<Event>) -> Vec<Event> {
    let mut out = Vec::new();
    while let Ok(event) = rx.try_recv() {
        out.push(event);
    }
    out
}

#[tokio::test]
async fn observed_conversation_becomes_a_chat_session_with_growing_reply() {
    let service = service();
    let mut rx = service.subscribe();
    let id = service
        .chat_observed(
            ChatApp::Claude,
            None,
            ChatKey::Id(UUID.into()),
            "Kyoto trip",
            vec![line(Role::User, "Plan Kyoto"), line(Role::Assistant, "Day")],
        )
        .unwrap();
    assert_eq!(id, format!("claude-chat-{UUID}"));
    let events = drain(&mut rx);
    let messages: Vec<_> = events
        .iter()
        .filter_map(|e| match e {
            Event::Message { session, message } if session == &id => Some(message.clone()),
            _ => None,
        })
        .collect();
    assert_eq!(messages.len(), 2);
    assert_eq!(messages[1].id, "ax0-00001");
    assert!(
        events
            .iter()
            .any(|e| matches!(e, Event::State(s) if s.id == id))
    );

    // The reply grows under the same id; nothing else is re-sent.
    service.chat_observed(
        ChatApp::Claude,
        Some(&id),
        ChatKey::Id(UUID.into()),
        "Kyoto trip",
        vec![
            line(Role::User, "Plan Kyoto"),
            line(Role::Assistant, "Day one: Fushimi Inari."),
        ],
    );
    let grown: Vec<_> = drain(&mut rx)
        .into_iter()
        .filter_map(|e| match e {
            Event::Message { message, .. } => Some(message),
            _ => None,
        })
        .collect();
    assert_eq!(grown.len(), 1);
    assert_eq!(grown[0].id, "ax0-00001");
    assert_eq!(grown[0].text, "Day one: Fushimi Inari.");

    let session = service.session(&id).unwrap();
    assert_eq!(session.mode, "chat");
    assert_eq!(session.provider, "claude");
    assert_eq!(session.title, "Kyoto trip");
    assert_eq!(session.cwd, "");
    assert_eq!(session.message_count, 2);
    assert_eq!(session.controller, "none");
    // Chats are reached from the desktop chat list, not the transcript list.
    assert!(service.list(None, 0).is_empty());

    let (page, more) = service.messages(&id, 1, None).await.unwrap();
    assert_eq!(page.len(), 1);
    assert!(more);
    let (earlier, more) = service.messages(&id, 5, Some(&page[0].id)).await.unwrap();
    assert_eq!(earlier[0].text, "Plan Kyoto");
    assert!(!more);

    // A different page under the same id (an edited turn) renumbers.
    service.chat_observed(
        ChatApp::Claude,
        Some(&id),
        ChatKey::Id(UUID.into()),
        "Kyoto trip",
        vec![line(Role::User, "Plan Osaka")],
    );
    let events = drain(&mut rx);
    assert!(
        events
            .iter()
            .any(|e| matches!(e, Event::Reset { session } if session == &id))
    );
    let (page, _) = service.messages(&id, 50, None).await.unwrap();
    assert_eq!(page.len(), 1);
    assert_eq!(page[0].id, "ax1-00000");
}

#[tokio::test]
async fn title_only_conversations_keep_their_session_across_a_rename_on_screen() {
    let service = service();
    let opening = vec![line(Role::User, "Plan Kyoto")];
    let first = service
        .chat_observed(
            ChatApp::ChatGpt,
            None,
            ChatKey::Title("Travel".into()),
            "Travel",
            opening.clone(),
        )
        .unwrap();
    assert!(first.starts_with("codex-chat-t"));
    let mut more = opening.clone();
    more.push(line(Role::Assistant, "Sure."));
    let renamed = service
        .chat_observed(
            ChatApp::ChatGpt,
            Some(&first),
            ChatKey::Title("Kyoto itinerary".into()),
            "Kyoto itinerary",
            more,
        )
        .unwrap();
    assert_eq!(renamed, first);
    assert_eq!(
        service.chat_session_for(ChatApp::ChatGpt, &ChatKey::Title("Kyoto itinerary".into())),
        first
    );
    assert_eq!(service.session(&first).unwrap().title, "Kyoto itinerary");

    // Another conversation that happens to be shown next is not a rename.
    let other = service
        .chat_observed(
            ChatApp::ChatGpt,
            Some(&first),
            ChatKey::Title("Groceries".into()),
            "Groceries",
            vec![line(Role::User, "Milk")],
        )
        .unwrap();
    assert_ne!(other, first);

    // The empty new-chat page is not a conversation yet.
    assert!(
        service
            .chat_observed(ChatApp::ChatGpt, Some(&other), ChatKey::New, "", Vec::new())
            .is_none()
    );
}

#[tokio::test]
async fn chat_sessions_without_a_controller_explain_how_to_open_them() {
    let service = service();
    let new = muster_agent::desktop_chat::new_session_id(ChatApp::Claude);
    let session = service.session(&new).unwrap();
    assert_eq!(session.title, "New Claude chat");
    assert_eq!(session.state, "unknown");
    let (messages, more) = service.messages(&new, 0, None).await.unwrap();
    assert!(messages.is_empty() && !more);
    match service.send(&new, "hello").await {
        Err(Error::Unavailable(message)) => assert!(message.contains("desktop chat list")),
        other => panic!("unexpected {other:?}"),
    }
    assert!(matches!(
        service.files(&new, "").await,
        Err(Error::Unavailable(_))
    ));
    // Transcript ids never look like chat ids.
    assert!(service.session(UUID).is_none());
}
