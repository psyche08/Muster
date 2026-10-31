//! DeepSeek Harness sessions through the service: discovery from its
//! session logs, history over the record grammar, and change push as the
//! app appends Zstandard frames. The records are format 4 as the app writes
//! them (synthetic content).

use std::io::Write;
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

use muster_agent::{AgentConfig, AgentService, Event};
use serde_json::{Value, json};

const SESSION: &str = "session-0b6c4a52-3d1e-4f8a-9c2b-6e7d8f901a23";
const ARCHIVED: &str = "session-1c7d5b63-4e2f-4a9b-8d3c-7f8e9a012b34";
const CHILD: &str = "session-2d8e6c74-5f3a-4b0c-9e4d-8a9fab123c45";
const NEWER: &str = "session-3e9f7d85-6a4b-4c1d-8f5e-9bafbc234d56";

fn frame(records: &[Value]) -> Vec<u8> {
    use ruzstd::encoding::{CompressionLevel, compress_to_vec};
    let text: String = records.iter().map(|r| format!("{r}\n")).collect();
    compress_to_vec(text.as_bytes(), CompressionLevel::Fastest)
}

fn header(id: &str, cwd: &Path, extra: Value) -> Value {
    let mut header = json!({"type":"session","version":4,"id":id,"createdAt":1_793_362_458_582_u64,
        "cwd":cwd,"isSeeded":false,"delegationDepth":0,"agentPreset":"standard"});
    header
        .as_object_mut()
        .unwrap()
        .extend(extra.as_object().unwrap().clone());
    header
}

fn event(seq: u64, kind: &str, data: Value) -> Value {
    json!({"type":kind,"seq":seq,"time":1_793_362_469_000_u64 + seq,"data":data})
}

fn user(seq: u64, text: &str, kind: &str) -> Value {
    let mut record = event(
        seq,
        "user/message",
        json!({"role":"user","id":format!("u{seq}"),"content":[{"type":"text","text":text}],
            "source":{"kind":kind}}),
    );
    record["surfaceOp"] = "append".into();
    record
}

fn assistant(seq: u64, content: Value) -> Value {
    event(
        seq,
        "assistant/message",
        json!({"turn":1,"step":1,"message":{"role":"assistant","id":format!("a{seq}"),"content":content,
            "source":{"kind":"model","provider":"deepseek-official","model":"deepseek-v4.1-flash"}},
            "stream":[],"usage":{"inputTokens":100,"outputTokens":5,"cacheReadTokens":20,"cacheWriteTokens":0}}),
    )
}

/// A session directory under the app's sessions root, with its log.
fn log(home: &Path, id: &str, name: &str) -> PathBuf {
    let dir = home.join("sessions/--tmp-project--").join(id);
    std::fs::create_dir_all(&dir).unwrap();
    std::fs::write(dir.join("session.lock"), "").unwrap();
    dir.join(name)
}

#[tokio::test]
async fn lists_reads_and_follows_deepseek_sessions() {
    let home = tempfile::tempdir().unwrap();
    let home = home.path();
    let project = home.join("project");
    std::fs::create_dir(&project).unwrap();
    std::fs::write(project.join("README.md"), "x").unwrap();

    let path = log(home, SESSION, "session.v4.jsonl.zstd");
    let mut file = std::fs::File::create(&path).unwrap();
    file.write_all(&frame(&[
        header(SESSION, &project, json!({})),
        event(0, "permission/preset", json!({"preset":"workspace-write"})),
    ]))
    .unwrap();
    file.write_all(&frame(&[
        event(1, "agent/inbox/spliced", json!({"target":"next-turn","start":0,"inserted":[]})),
        event(2, "turn/start", json!({"turn":1})),
        event(3, "step/start", json!({"turn":1,"step":1})),
        event(4, "system/message", json!({"turn":1,"step":1,
            "message":{"role":"system","id":"s","content":[{"type":"text","text":"You are"}],"source":{"kind":"system-prompt"}}})),
        user(5, "Read the README", "user"),
        user(6, "Current runtime context.", "runtime-context"),
        user(7, "Time sampled while preparing turn 1", "time-context"),
        event(8, "session/title", json!({"title":"Read the README","messageSeqs":[5],"source":{"kind":"fallback"}})),
        assistant(9, json!([
            {"type":"reasoning","text":"Look at it first."},
            {"type":"text","text":"Reading it."},
            {"type":"tool-call","id":"call-1","name":"read_file","arguments":"{\"path\":\"README.md\"}"}
        ])),
        event(10, "tool/call", json!({"turn":1,"step":1,"callId":"call-1","name":"read_file","arguments":"{}"})),
        event(11, "tool/result", json!({"turn":1,"step":1,"message":{"role":"tool","id":"r","toolCallId":"call-1",
            "content":[{"type":"text","text":"x"}],"source":{"kind":"tool","callId":"call-1"}}})),
    ]))
    .unwrap();
    // The last frame of this flush is still being written.
    let pending = frame(&[
        assistant(12, json!([{"type":"text","text":"It says x."}])),
        event(13, "step/end", json!({"turn":1,"step":1})),
        event(
            14,
            "turn/end",
            json!({"turn":1,"reason":{"kind":"completed"}}),
        ),
        event(
            15,
            "session/title",
            json!({"title":"README contents","messageSeqs":[5],
            "source":{"kind":"provider","provider":"session-title-first-prompt-llm"}}),
        ),
    ]);
    file.write_all(&pending[..pending.len() / 2]).unwrap();

    // An archived session, uncompressed, titled by the app's cache.
    std::fs::write(
        log(home, ARCHIVED, "session.v4.jsonl"),
        format!("{}\n", header(ARCHIVED, &project, json!({}))),
    )
    .unwrap();
    // A subagent's session names its parent.
    std::fs::write(
        log(home, CHILD, "session.v4.jsonl.zstd"),
        frame(&[header(
            CHILD,
            &project,
            json!({"parentSession":SESSION,"origin":"subagent","delegationDepth":1}),
        )]),
    )
    .unwrap();
    // A generation this build does not read is left alone.
    std::fs::write(
        log(home, NEWER, "session.v5.jsonl.zstd"),
        frame(&[json!({"type":"session","version":5,"id":NEWER})]),
    )
    .unwrap();
    std::fs::write(
        log(home, NEWER, "session.v4.jsonl.zstd"),
        frame(&[header(NEWER, &project, json!({}))]),
    )
    .unwrap();
    std::fs::create_dir_all(home.join("storages/session_projcache/sessions")).unwrap();
    std::fs::write(
        home.join("storages/workspace.json"),
        json!({"global":{"archivedSessionIds":[ARCHIVED],"pinnedSessionIds":[]}}).to_string(),
    )
    .unwrap();
    std::fs::write(
        home.join(format!(
            "storages/session_projcache/sessions/{ARCHIVED}.json"
        )),
        json!({"version":7,"record":{"rows":{"title":{"ver":1,"seq":3,"val":"Old work"}}}})
            .to_string(),
    )
    .unwrap();

    let service = AgentService::start(AgentConfig {
        codex_home: None,
        claude_home: None,
        claude_desktop: None,
        catdesk_home: None,
        deepseek_home: Some(home.into()),
        poll: Duration::from_millis(50),
        rescan: Duration::from_millis(200),
        ..Default::default()
    });
    let deadline = Instant::now() + Duration::from_secs(5);
    while service.session(SESSION).is_none() || service.session(ARCHIVED).is_none() {
        assert!(Instant::now() < deadline, "the logs were not listed");
        tokio::time::sleep(Duration::from_millis(20)).await;
    }
    assert!(service.session(NEWER).is_none(), "format 5 is not read");
    let child = service.session(CHILD).expect("the subagent is known");
    assert!(child.read_only);
    assert_eq!(child.parent_session_id, SESSION);
    let listed = service.list(None, 0);
    assert!(
        listed.iter().all(|s| s.id != CHILD),
        "subagents stay out of lists"
    );
    let archived = service.session(ARCHIVED).unwrap();
    assert!(archived.archived);
    assert_eq!(archived.title, "Old work");

    let session = service.session(SESSION).unwrap();
    assert_eq!(session.provider, "deepseek");
    assert_eq!(session.cwd, project.to_string_lossy());
    // No process or window is observed: nothing is claimed about either.
    assert_eq!(session.process_state, "unknown");
    assert_eq!(session.state, "unknown");
    assert_eq!(session.controller, "none");
    let files = service.files(SESSION, "").await.unwrap();
    assert!(files.entries.iter().any(|e| e.name == "README.md"));

    let (messages, _) = service.messages(SESSION, 50, None).await.unwrap();
    let texts: Vec<(&str, &str, &str)> = messages
        .iter()
        .map(|m| (m.role.as_str(), m.kind.as_str(), m.text.as_str()))
        .collect();
    assert_eq!(
        texts,
        [
            ("user", "text", "Read the README"),
            ("assistant", "text", "Reading it."),
            (
                "assistant",
                "tool_call",
                "read_file {\"path\":\"README.md\"}"
            ),
            ("tool", "tool_result", "read_file: x"),
        ]
    );
    assert_eq!(messages[2].tool_call_id, "call-1");
    assert_eq!(messages[0].ts_unix, 1_793_362_469);
    assert_eq!(service.session(SESSION).unwrap().title, "Read the README");

    // The rest of the frame lands: the reply, the turn's usage and the
    // model's title are pushed.
    let mut events = service.subscribe();
    file.write_all(&pending[pending.len() / 2..]).unwrap();
    let pushed = tokio::time::timeout(Duration::from_secs(5), async {
        let mut seen = Vec::new();
        while seen.len() < 2 {
            if let Ok(Event::Message { session, message }) = events.recv().await
                && session == SESSION
            {
                seen.push(message);
            }
        }
        seen
    })
    .await
    .expect("the completed frame was not pushed");
    assert_eq!(pushed[0].text, "It says x.");
    assert_eq!(pushed[1].kind, "turn_summary");
    let usage = pushed[1].usage.as_ref().unwrap();
    assert_eq!((usage.input_tokens, usage.cache_read_tokens), (240, 40));
    assert_eq!(usage.model, "deepseek-v4.1-flash");
    let deadline = Instant::now() + Duration::from_secs(5);
    while service.session(SESSION).unwrap().title != "README contents" {
        assert!(Instant::now() < deadline, "the model's title was not taken");
        tokio::time::sleep(Duration::from_millis(20)).await;
    }
}

/// A stand-in for the `muster-dsh-bridge` plugin on a real socket: the
/// state it reports, and every request it was sent.
struct FakePlugin {
    state: std::sync::Arc<std::sync::Mutex<Value>>,
    requests: std::sync::Arc<std::sync::Mutex<Vec<Value>>>,
}

impl FakePlugin {
    fn serve(socket: &Path, state: Value) -> Self {
        use std::io::{BufRead, BufReader};
        std::fs::create_dir_all(socket.parent().unwrap()).unwrap();
        let listener = std::os::unix::net::UnixListener::bind(socket).unwrap();
        let state = std::sync::Arc::new(std::sync::Mutex::new(state));
        let requests = std::sync::Arc::new(std::sync::Mutex::new(Vec::new()));
        let (shown, seen) = (state.clone(), requests.clone());
        std::thread::spawn(move || {
            for stream in listener.incoming() {
                let Ok(stream) = stream else { return };
                let mut line = String::new();
                if BufReader::new(&stream).read_line(&mut line).is_err() {
                    continue;
                }
                let request: Value = serde_json::from_str(&line).unwrap();
                let result = if request["op"] == "state" {
                    shown.lock().unwrap().clone()
                } else {
                    seen.lock().unwrap().push(request);
                    json!({"accepted": true})
                };
                let reply = json!({"id": null, "ok": true, "result": result});
                let _ = (&stream).write_all(format!("{reply}\n").as_bytes());
            }
        });
        Self { state, requests }
    }

    fn sent(&self, op: &str) -> Vec<Value> {
        let requests = self.requests.lock().unwrap();
        requests.iter().filter(|r| r["op"] == op).cloned().collect()
    }
}

/// This test process serves the socket; it stands for the signed app.
struct ThisProcess(std::sync::atomic::AtomicBool);

impl muster_agent::deepseek_app::Host for ThisProcess {
    fn verify(&self, pid: u32) -> Option<muster_agent::controller::ProcessIdentity> {
        (pid == std::process::id())
            .then_some(muster_agent::controller::ProcessIdentity { pid, start: 7 })
    }
    fn exited(&self, _: muster_agent::controller::ProcessIdentity) -> bool {
        self.0.load(std::sync::atomic::Ordering::SeqCst)
    }
}

async fn refresh(service: &std::sync::Arc<AgentService>, host: &std::sync::Arc<ThisProcess>) {
    let (service, host) = (service.clone(), host.clone());
    tokio::task::spawn_blocking(move || {
        let link = service.deepseek_link().expect("the plugin's link");
        muster_agent::deepseek_app::refresh(&service, &link, host.as_ref());
    })
    .await
    .unwrap();
}

#[tokio::test]
async fn drives_a_session_through_the_plugin() {
    let home = tempfile::tempdir().unwrap();
    let home = home.path();
    std::fs::write(
        log(home, SESSION, "session.v4.jsonl.zstd"),
        frame(&[header(SESSION, home, json!({}))]),
    )
    .unwrap();
    let service = AgentService::start(AgentConfig {
        codex_home: None,
        claude_home: None,
        claude_desktop: None,
        catdesk_home: None,
        deepseek_home: Some(home.into()),
        poll: Duration::from_millis(50),
        rescan: Duration::from_millis(200),
        ..Default::default()
    });
    let deadline = Instant::now() + Duration::from_secs(5);
    while service.session(SESSION).is_none() {
        assert!(Instant::now() < deadline, "the log was not listed");
        tokio::time::sleep(Duration::from_millis(20)).await;
    }
    let host = std::sync::Arc::new(ThisProcess(Default::default()));
    // Without the plugin nothing drives the session.
    refresh(&service, &host).await;
    assert_eq!(service.session(SESSION).unwrap().controller, "none");

    let plugin = FakePlugin::serve(
        &home.join("muster/bridge.sock"),
        json!({"protocol": 1, "pid": 1, "sessions": [{"id": SESSION, "running": true}],
            "approvals": [{"id": "a1", "sessionId": SESSION, "toolName": "bash", "reason": "rm -rf build"}],
            "questions": [{"id": "q1", "sessionId": SESSION, "questions": [
                {"id": "x", "question": "Which?", "options": [{"label": "A"}, {"label": "B"}], "multiSelect": false}]}]}),
    );
    refresh(&service, &host).await;
    let session = service.session(SESSION).unwrap();
    assert_eq!(session.controller, "plugin");
    assert_eq!(session.process_state, "running");
    assert_eq!(session.state, "waiting_approval");
    let pending = service.pending(SESSION);
    assert_eq!(pending.len(), 2);
    assert_eq!(pending[0].prompt, "bash: rm -rf build");

    // An approval, once the service has it.
    let deadline = Instant::now() + Duration::from_secs(5);
    while let Err(error) = service
        .respond_in_session(SESSION, "dsh-approval:a1", "Allow once", "")
        .await
    {
        assert!(
            Instant::now() < deadline,
            "the approval was not offered: {error}"
        );
        tokio::time::sleep(Duration::from_millis(20)).await;
    }
    assert_eq!(
        plugin.sent("approve"),
        [json!({"op": "approve", "approvalId": "a1", "outcome": "allowed-once"})]
    );
    assert!(
        service
            .respond_in_session(SESSION, "dsh-approval:a1", "Always", "")
            .await
            .is_err(),
        "only the offered options"
    );
    // A question card answers with the app's own labels.
    service
        .answer_questions(
            SESSION,
            "dsh-question:q1",
            &[muster_proto::v1::QuestionAnswer {
                question_id: "x".into(),
                option_ids: vec!["1".into()],
                text: String::new(),
            }],
        )
        .await
        .unwrap();
    assert_eq!(
        plugin.sent("answer"),
        [json!({"op": "answer", "questionId": "q1", "answers": [{"id": "x", "selected": ["B"]}]})]
    );
    // A prompt and an interrupt go through the app's session controller.
    service.send_to(SESSION, "run the tests").await.unwrap();
    let [prompt] = &plugin.sent("prompt")[..] else {
        panic!("one prompt")
    };
    assert_eq!(prompt["sessionId"], SESSION);
    assert_eq!(prompt["text"], "run the tests");
    assert_eq!(prompt["mode"], "queue");
    assert_eq!(prompt["requestId"].as_str().unwrap().len(), 36);
    service.interrupt(SESSION).await.unwrap();
    assert_eq!(
        plugin.sent("cancel"),
        [json!({"op": "cancel", "sessionId": SESSION})]
    );

    // Answered in the app: nothing waits any more.
    *plugin.state.lock().unwrap() = json!({"protocol": 1, "pid": 1, "sessions": [{"id": SESSION, "running": true}],
            "approvals": [], "questions": []});
    refresh(&service, &host).await;
    assert!(service.pending(SESSION).is_empty());
    assert_eq!(service.session(SESSION).unwrap().state, "running");

    // The app quits: its controllers go with it.
    host.0.store(true, std::sync::atomic::Ordering::SeqCst);
    std::fs::remove_file(home.join("muster/bridge.sock")).unwrap();
    refresh(&service, &host).await;
    let deadline = Instant::now() + Duration::from_secs(5);
    while service.session(SESSION).unwrap().process_state != "ended" {
        assert!(Instant::now() < deadline, "the exit was not seen");
        tokio::time::sleep(Duration::from_millis(20)).await;
    }
    assert!(service.send_to(SESSION, "again").await.is_err());
}
