//! Shared by the integration tests: a shell that keeps the latest view, and
//! the transcript a chat view shows. Each test crate uses part of it.
#![allow(dead_code)]

use std::sync::Mutex;
use std::time::Duration;

use muster_app_core::{Shell, app};
use muster_proto::{Message as _, v1};
use tokio::sync::Notify;

/// Keeps the latest view model; signs nothing.
#[derive(Default)]
pub struct TestShell {
    pub latest: Mutex<app::ViewModel>,
    changed: Notify,
}

impl Shell for TestShell {
    fn on_view(&self, bytes: &[u8]) {
        *self.latest.lock().unwrap() = app::ViewModel::decode(bytes).unwrap();
        self.changed.notify_one();
    }
    fn on_effect(&self, _: u64, _: &[u8]) {}
    fn hw_public_key(&self) -> Option<Vec<u8>> {
        None
    }
    fn hw_sign(&self, _: &[u8]) -> Option<Vec<u8>> {
        None
    }
}

impl TestShell {
    /// The first view within five seconds that passes `check`.
    pub async fn wait(&self, check: impl Fn(&app::ViewModel) -> bool) -> app::ViewModel {
        self.wait_for("the expected view", check).await
    }

    pub async fn wait_for(
        &self,
        what: &str,
        check: impl Fn(&app::ViewModel) -> bool,
    ) -> app::ViewModel {
        self.wait_within(Duration::from_secs(5), what, check).await
    }

    /// For a view that waits on one of the client's own longer deadlines, such
    /// as the chat history snapshot timeout.
    pub async fn wait_within(
        &self,
        patience: Duration,
        what: &str,
        check: impl Fn(&app::ViewModel) -> bool,
    ) -> app::ViewModel {
        tokio::time::timeout(patience, async {
            loop {
                let view = self.latest.lock().unwrap().clone();
                if check(&view) {
                    return view;
                }
                self.changed.notified().await;
            }
        })
        .await
        .unwrap_or_else(|_| panic!("view never showed {what}"))
    }
}

/// The transcript messages a chat's timeline shows, in order. The timeline
/// leaves out empty text records; `message_count` still counts them.
pub fn messages(chat: &app::ChatView) -> Vec<v1::AgentMessage> {
    fn walk(blocks: &[app::ChatBlock], out: &mut Vec<v1::AgentMessage>) {
        for block in blocks {
            out.extend(block.message.clone());
            out.extend(block.activity.iter().cloned());
            walk(&block.children, out);
        }
    }
    let mut out = Vec::new();
    walk(&chat.timeline, &mut out);
    out
}
