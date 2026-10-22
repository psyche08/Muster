//! Composer suggestions: asked for once per completed turn of a watched
//! Codex session, carried on its `AgentSession`, and dropped when a new turn
//! starts.

use std::io::Write;
use std::sync::{Arc, Mutex};
use std::time::Duration;

use muster_agent::prediction::{Predictor, Request};
use muster_agent::{AgentConfig, AgentService, AgentSession, Event};

#[derive(Default)]
struct Fake {
    requests: Mutex<Vec<Request>>,
}

#[async_trait::async_trait]
impl Predictor for Fake {
    async fn predict(&self, request: Request) -> Option<String> {
        let text = format!("after {}", request.turn);
        self.requests.lock().unwrap().push(request);
        Some(text)
    }
}

fn turn(file: &mut std::fs::File, id: &str, complete: bool) {
    writeln!(
        file,
        r#"{{"type":"event_msg","payload":{{"type":"task_started","turn_id":"{id}"}}}}"#
    )
    .unwrap();
    writeln!(
        file,
        r#"{{"type":"turn_context","payload":{{"turn_id":"{id}"}}}}"#
    )
    .unwrap();
    writeln!(file, r#"{{"type":"response_item","payload":{{"type":"message","role":"user","content":[{{"type":"input_text","text":"ask {id}"}}]}}}}"#).unwrap();
    if complete {
        writeln!(file, r#"{{"type":"response_item","payload":{{"type":"message","role":"assistant","content":[{{"type":"output_text","text":"done {id}"}}]}}}}"#).unwrap();
        writeln!(file, r#"{{"type":"event_msg","timestamp":"2026-10-17T06:00:00Z","payload":{{"type":"task_complete","turn_id":"{id}"}}}}"#).unwrap();
    }
}

async fn suggestion(
    events: &mut tokio::sync::broadcast::Receiver<Event>,
    id: &str,
    wanted: &str,
) -> AgentSession {
    tokio::time::timeout(Duration::from_secs(5), async {
        loop {
            match events.recv().await {
                Ok(Event::State(s)) if s.id == id && s.composer_suggestion == wanted => break s,
                Ok(_) | Err(tokio::sync::broadcast::error::RecvError::Lagged(_)) => continue,
                Err(_) => panic!("closed"),
            }
        }
    })
    .await
    .unwrap_or_else(|_| panic!("no state with suggestion {wanted:?}"))
}

#[tokio::test]
async fn a_watched_codex_session_gets_one_suggestion_per_completed_turn() {
    let dir = tempfile::tempdir().unwrap();
    let day = dir.path().join("sessions/2026/10/17");
    std::fs::create_dir_all(&day).unwrap();
    let id = "01a14a0a-ccc1-7c90-8f40-d84d2ca7e9df";
    let other = "01a14a0a-ccc1-7c90-8f40-d84d2ca7e9e0";
    let path = day.join(format!("rollout-2026-10-17T12-00-00-{id}.jsonl"));
    let mut file = std::fs::File::create(&path).unwrap();
    writeln!(
        file,
        r#"{{"type":"session_meta","payload":{{"id":"{id}","cwd":"/tmp"}}}}"#
    )
    .unwrap();
    turn(&mut file, "t1", true);
    let mut unwatched =
        std::fs::File::create(day.join(format!("rollout-2026-10-17T12-00-01-{other}.jsonl")))
            .unwrap();
    writeln!(
        unwatched,
        r#"{{"type":"session_meta","payload":{{"id":"{other}","cwd":"/tmp"}}}}"#
    )
    .unwrap();
    turn(&mut unwatched, "u1", true);

    let fake = Arc::new(Fake::default());
    let service = AgentService::start(AgentConfig {
        codex_home: Some(dir.path().into()),
        claude_home: None,
        catdesk_home: None,
        claude_desktop: None,
        poll: Duration::from_millis(50),
        rescan: Duration::from_secs(60),
        predictor: Some(fake.clone()),
        ..AgentConfig::default()
    });
    service.rescan().await;
    let mut events = service.subscribe();

    // Watching asks for the turn that ended before.
    let watch = service.watch_session(id);
    service.messages(id, 10, None).await.unwrap();
    suggestion(&mut events, id, "after t1").await;
    assert_eq!(service.session(id).unwrap().composer_suggestion, "after t1");
    {
        let requests = fake.requests.lock().unwrap();
        assert_eq!(requests.len(), 1);
        assert_eq!(requests[0].thread, id);
        assert_eq!(requests[0].turn, "t1");
        assert_eq!(requests[0].rollout, path);
    }

    // A new turn makes it stale; its completion brings the next one.
    turn(&mut file, "t2", false);
    suggestion(&mut events, id, "").await;
    writeln!(file, r#"{{"type":"response_item","payload":{{"type":"message","role":"assistant","content":[{{"type":"output_text","text":"done t2"}}]}}}}"#).unwrap();
    writeln!(file, r#"{{"type":"event_msg","timestamp":"2026-10-17T06:01:00Z","payload":{{"type":"task_complete","turn_id":"t2"}}}}"#).unwrap();
    suggestion(&mut events, id, "after t2").await;

    // Another watcher reuses it; an unwatched session is never asked about.
    let again = service.watch_session(id);
    tokio::time::sleep(Duration::from_millis(300)).await;
    let turns: Vec<_> = fake
        .requests
        .lock()
        .unwrap()
        .iter()
        .map(|r| r.turn.clone())
        .collect();
    assert_eq!(turns, ["t1", "t2"]);
    assert_eq!(service.session(other).unwrap().composer_suggestion, "");
    drop((watch, again));
}
