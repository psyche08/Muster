use muster_agent::Error;
use muster_agent::orca::{Action, Reply, Rpc, Source, fingerprint};
use muster_proto::v1::AgentMessagesRequest;
use serde_json::{Value, json};
use std::sync::{Arc, Mutex};

struct Fake {
    state: Mutex<State>,
}
struct State {
    runtime: String,
    items: Vec<Value>,
    calls: Vec<(String, Value)>,
    fail: bool,
    receipt: &'static str,
}
impl Fake {
    fn new() -> Arc<Self> {
        Arc::new(Self {
            state: Mutex::new(State {
                runtime: "r1".into(),
                items: vec![item(
                    "a",
                    1,
                    json!({"kind":"message","role":"assistant","blocks":[{"type":"text","text":"Hello **Muster**"}]}),
                )],
                calls: vec![],
                fail: false,
                receipt: "accepted",
            }),
        })
    }
}
fn item(id: &str, rev: u64, body: Value) -> Value {
    json!({"itemId":id,"revision":rev,"sequence":1,"observedAt":1700000000000u64,"body":body})
}
#[async_trait::async_trait]
impl Rpc for Fake {
    async fn call(
        &self,
        runtime: Option<&str>,
        method: &str,
        params: Value,
        mutation: bool,
    ) -> Result<Reply, Error> {
        let mut s = self.state.lock().unwrap();
        if s.fail || runtime.is_some_and(|r| r != s.runtime) {
            return Err(if mutation {
                Error::UnknownOutcome("lost receipt".into())
            } else {
                Error::Unavailable("offline".into())
            });
        }
        s.calls.push((method.into(), params.clone()));
        let result = match method {
            "session.tabs.listAll" => {
                json!({"snapshots":[{"worktree":"/work","tabs":[{"type":"agent-session","sessionId":"s1","agent":"codex","title":"Test"},{"type":"terminal","id":"shell","title":"codex"}]}]})
            }
            "agentSession.history" => {
                json!({"ok":true,"page":{"sessionId":"s1","epoch":"e1","fence":7,"items":s.items,"removedItemIds":[],"window":{"oldest":{"epoch":"e1","sequence":1},"nextCursor":{"epoch":"e1","sequence":10}},"hasOlder":false,"hasNewer":false,"latestTurn":{"turn":{"state":"running"}}}})
            }
            "agentSession.send" => {
                assert!(mutation);
                assert_eq!(params["envelope"]["expectedRuntimeFence"], 7);
                let operation = params["envelope"]["clientOperationId"].as_str().unwrap();
                assert_eq!(operation.len(), 46);
                assert_eq!(
                    params["envelope"]["payloadFingerprint"],
                    fingerprint(method, "s1", &json!({"body":params["body"]}))
                );
                json!({"ok":true,"fence":7,"value":{"clientMessageId":operation,"submission":{"dispatchState":s.receipt}}})
            }
            "agentSession.cancel" => json!({"ok":true,"fence":7,"value":{"cancelled":true}}),
            "agentSession.respondToApproval" => {
                json!({"ok":true,"fence":7,"value":{"itemId":params["itemId"],"revision":params["expectedRevision"].as_u64().unwrap()+1,"resolution":{"state":"resolved"}}})
            }
            _ => panic!("unexpected method {method}"),
        };
        Ok(Reply {
            runtime: s.runtime.clone(),
            result,
        })
    }
}
async fn setup() -> (Arc<Fake>, Arc<Source>, String) {
    let fake = Fake::new();
    let (events, _) = tokio::sync::broadcast::channel(64);
    let source = Source::new(fake.clone(), events);
    source.discover().await.unwrap();
    let id = source.list()[0].id.clone();
    (fake, source, id)
}
#[test]
fn upstream_fingerprint_vector() {
    assert_eq!(
        fingerprint(
            "agentSession.send",
            "session-1",
            &json!({"body":{"kind":"message","role":"user","blocks":[{"type":"text","text":"You have 1 orchestration message."}]}})
        ),
        "bf73e28368c0837ade16328692116535c6d97fc9924c70aa342a3cf18817d047"
    );
}
#[tokio::test]
async fn structured_history_send_and_runtime_restart_are_isolated() {
    let (fake, source, id) = setup().await;
    assert_eq!(
        source.list().len(),
        1,
        "a shell titled codex is not an agent"
    );
    let request = AgentMessagesRequest {
        session: id.clone(),
        ..Default::default()
    };
    let history = source.history(&request).await.unwrap();
    assert_eq!(history.messages[0].text, "Hello **Muster**");
    source
        .operation(&id, "op-one", Action::Send("hello".into()))
        .await
        .unwrap();
    source
        .operation(&id, "op-stop", Action::Interrupt)
        .await
        .unwrap();
    fake.state.lock().unwrap().runtime = "r2".into();
    assert!(
        source
            .operation(&id, "stale", Action::Send("wrong runtime".into()))
            .await
            .is_err()
    );
    source.discover().await.unwrap();
    assert!(source.session(&id).unwrap().read_only);
    assert!(source.list().iter().any(|s| s.id != id && !s.read_only));
}
#[tokio::test]
async fn revisions_reset_cached_history_and_unknown_send_never_counts_as_accepted() {
    let (fake, source, id) = setup().await;
    let first = source
        .history(&AgentMessagesRequest {
            session: id.clone(),
            ..Default::default()
        })
        .await
        .unwrap();
    {
        let mut state = fake.state.lock().unwrap();
        state.items[0]["revision"] = json!(2);
        state.items[0]["body"]["blocks"][0]["text"] = json!("Updated");
        state.receipt = "unknown";
    }
    let page = source
        .history(&muster_proto::history::request(&id, &first.messages))
        .await
        .unwrap();
    assert_eq!(page.messages[0].text, "Updated");
    assert!(matches!(
        source
            .operation(&id, "uncertain", Action::Send("hello".into()))
            .await,
        Err(Error::UnknownOutcome(_))
    ));
    fake.state.lock().unwrap().fail = true;
    assert!(source.discover().await.is_err());
    assert!(source.session(&id).unwrap().read_only);
}
#[tokio::test]
async fn approvals_bind_the_displayed_revision_and_exact_option() {
    let (fake, source, id) = setup().await;
    fake.state.lock().unwrap().items.push(item("approve",3,json!({"kind":"approval","title":"Run tests?","options":[{"id":"allow-once","label":"Allow once"},{"id":"deny","label":"Deny"}],"resolution":{"state":"pending"}})));
    source.refresh(&id).await.unwrap();
    let pending = source.pending(&id)[0].clone();
    assert!(
        source
            .operation(
                &id,
                "bad-choice",
                Action::Respond {
                    interaction: pending.id.clone(),
                    option: "Allow forever".into()
                }
            )
            .await
            .is_err()
    );
    source
        .operation(
            &id,
            "approve",
            Action::Respond {
                interaction: pending.id.clone(),
                option: "Allow once".into(),
            },
        )
        .await
        .unwrap();
    fake.state.lock().unwrap().items[1]["revision"] = json!(4);
    assert!(
        source
            .operation(
                &id,
                "stale-choice",
                Action::Respond {
                    interaction: pending.id,
                    option: "Allow once".into()
                }
            )
            .await
            .is_err()
    );
    let s = fake.state.lock().unwrap();
    let calls: Vec<_> = s
        .calls
        .iter()
        .filter(|(m, _)| m == "agentSession.respondToApproval")
        .collect();
    assert_eq!(calls.len(), 1);
    assert_eq!(calls[0].1["optionId"], "allow-once");
    assert_eq!(calls[0].1["expectedRevision"], 3);
}
