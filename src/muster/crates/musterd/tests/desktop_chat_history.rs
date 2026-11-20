//! A fresh desktop daemon knows the chat ID format, while its history
//! becomes available only after the platform observes that conversation.
use std::sync::Arc;

use muster_agent::desktop_chat::{ChatApp, ChatKey, ChatLine, new_session_id, session_id};
use muster_agent::model::Role;
use muster_core::SoftSigner;
use muster_proto::v1::{self, request::Req, response::Resp};
use muster_rpc::Client;
use musterd::{Daemon, DaemonConfig, DefaultPlatform};

#[tokio::test]
async fn desktop_chat_history_recovers_after_observation_over_local_rpc() {
    let dir = tempfile::tempdir().unwrap();
    let mut config = DaemonConfig::new(dir.path());
    config.bind = "127.0.0.1:0".parse().unwrap();
    config.cloud = false;
    config.agent = Some(muster_agent::AgentConfig {
        codex_home: None,
        claude_home: None,
        claude_desktop: None,
        catdesk_home: None,
        deepseek_home: None,
        ..Default::default()
    });
    let socket = config.socket_path();
    let daemon = Daemon::start_with(
        config,
        Arc::new(DefaultPlatform),
        Some(Arc::new(SoftSigner::generate())),
    )
    .await
    .unwrap();
    let client = Client::new(muster_local::connect(&socket).await.unwrap());
    let service = daemon.agent().unwrap();
    let app = ChatApp::ChatGpt;
    let key = ChatKey::Id("example-conversation".into());
    let stable = session_id(app, &key);
    let receipt = session_id(app, &ChatKey::Title("Example conversation".into()));
    let query = |id: &str, turns, before_id: &str| {
        Req::AgentMessages(v1::AgentMessagesRequest {
            session: id.into(),
            turns,
            before_id: before_id.into(),
            ..Default::default()
        })
    };

    for id in [&receipt, &stable] {
        assert!(matches!(
            service.messages(id, 20, None).await,
            Err(muster_agent::Error::Unavailable(_))
        ));
        for turns in [0, 2] {
            assert!(matches!(client.call(query(id, turns, "")).await,
                Err(muster_rpc::Error::Remote {code: v1::error::Code::Unavailable, message})
                    if message.contains("desktop Chats list")));
        }
    }
    let Resp::AgentMessages(blank) = client
        .call(query(&new_session_id(app), 0, ""))
        .await
        .unwrap()
    else {
        panic!("expected new chat page")
    };
    assert!(blank.messages.is_empty());

    let observe =
        |lines| service.chat_observed(app, None, key.clone(), "Example conversation", lines);
    assert_eq!(observe(vec![]).as_deref(), Some(stable.as_str()));
    let Resp::AgentMessages(blank) = client.call(query(&stable, 2, "")).await.unwrap() else {
        panic!("expected observed empty page")
    };
    assert!(blank.messages.is_empty());
    observe(vec![
        ChatLine {
            role: Role::User,
            text: "Example question".into(),
        },
        ChatLine {
            role: Role::Assistant,
            text: "Example reply".into(),
        },
    ]);
    let (rows, more) = service.messages(&stable, 10, None).await.unwrap();
    assert_eq!(rows.len(), 2);
    assert!(!more);
    for turns in [0, 2] {
        let Resp::AgentMessages(page) = client.call(query(&stable, turns, "")).await.unwrap()
        else {
            panic!("expected restored page")
        };
        assert_eq!(page.messages, rows);
    }
    let Resp::AgentMessages(older) = client.call(query(&stable, 0, &rows[1].id)).await.unwrap()
    else {
        panic!("expected earlier page")
    };
    assert_eq!(older.messages, rows[..1]);
    assert!(service.messages(&receipt, 10, None).await.is_err());
    daemon.shutdown().await;
}
