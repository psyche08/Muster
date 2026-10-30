//! Scheduler protocol adapters. Native identity/delivery remains behind Host.
use crate::{Host, MAX_FRAME, StopReason, TurnEvent, Writer};
use serde_json::{Value, json};
use std::sync::Arc;
use tokio::io::{AsyncBufReadExt, AsyncRead, AsyncReadExt, AsyncWrite, BufReader};

fn text(input: &Value) -> Result<String, String> {
    if let Some(s) = input.as_str() {
        return Ok(s.into());
    }
    let blocks = input.as_array().ok_or("Expected text input")?;
    let mut out = Vec::new();
    for b in blocks {
        if b["type"] != "text" {
            return Err("Native scheduler input currently supports text only".into());
        }
        out.push(b["text"].as_str().ok_or("Missing text")?);
    }
    if out.is_empty() {
        return Err("Empty prompt".into());
    }
    Ok(out.join("\n"))
}
fn explicit_options(p: &Value) -> Result<(), String> {
    for key in [
        "model",
        "modelProvider",
        "profile",
        "config",
        "baseInstructions",
        "developerInstructions",
        "effort",
        "serviceTier",
    ] {
        if key == "model" && p[key] == "native-app" {
            continue;
        }
        if p.get(key)
            .is_some_and(|v| !v.is_null() && v != "" && v != &json!({}))
        {
            return Err(format!(
                "Native app keeps its own settings; unsupported scheduler option: {key}"
            ));
        }
    }
    Ok(())
}
fn failed(id: Value, message: String) -> Value {
    json!({"id":id,"error":{"code":-32000,"message":message}})
}

struct TurnTask {
    handle: tokio::task::JoinHandle<()>,
    host: Arc<dyn Host>,
    session: String,
    completed: Arc<std::sync::atomic::AtomicBool>,
}
impl TurnTask {
    fn is_finished(&self) -> bool {
        self.completed.load(std::sync::atomic::Ordering::SeqCst) || self.handle.is_finished()
    }
}
impl Drop for TurnTask {
    fn drop(&mut self) {
        if !self.is_finished() {
            self.handle.abort();
            let host = self.host.clone();
            let session = self.session.clone();
            tokio::spawn(async move {
                let _ = host.interrupt(&session).await;
            });
        }
    }
}
struct ReaderTask(tokio::task::JoinHandle<()>);
impl Drop for ReaderTask {
    fn drop(&mut self) {
        self.0.abort();
    }
}

pub async fn codex<R, W>(
    read: R,
    write: W,
    host: Arc<dyn Host>,
    cwd: String,
    args: &[String],
) -> anyhow::Result<()>
where
    R: AsyncRead + Unpin + Send + 'static,
    W: AsyncWrite + Unpin + Send + 'static,
{
    let launch_supported =
        args.is_empty() || args == ["app-server"] || args == ["app-server", "--listen", "stdio://"];
    let writer = Arc::new(Writer {
        inner: tokio::sync::Mutex::new(write),
    });
    let mut reader = BufReader::new(read);
    let mut active: Option<TurnTask> = None;
    let mut session = String::new();
    let mut resume_failed = false;
    let mut initialized = false;
    loop {
        let mut line = String::new();
        let n = (&mut reader)
            .take(MAX_FRAME + 1)
            .read_line(&mut line)
            .await?;
        if n == 0 {
            break;
        }
        if n as u64 > MAX_FRAME {
            anyhow::bail!("Scheduler frame too large")
        }
        let frame: Value = serde_json::from_str(&line)?;
        let id = frame.get("id").cloned();
        let p = &frame["params"];
        let Some(method) = frame["method"].as_str() else {
            continue;
        };
        if method == "initialized" {
            continue;
        }
        let Some(id) = id else { continue };
        let result:Result<Value,String>=async {
            if method=="initialize"{if !launch_supported{return Err("Native app keeps its own settings; unsupported launch options".into())}initialized=true;return Ok(json!({"userAgent":"Muster native runtime","platformFamily":"unix","platformOs":"macos"}))}
            if !initialized{return Err("Initialize first".into())}
            match method {
                "model/list"=>Ok(json!({"data":[{"id":"native-app","model":"native-app","displayName":"ChatGPT.app settings","description":"Uses the native application model and permissions","isDefault":true,"defaultReasoningEffort":"none","supportedReasoningEfforts":[]}],"nextCursor":null})),
                "thread/start"=>{
                    if resume_failed{return Err("Resume failed; refusing a replacement conversation".into())}
                    explicit_options(p)?;
                    if p["cwd"].as_str()!=Some(cwd.as_str()){return Err("Scheduler cwd differs from its launch directory".into())}
                    if !session.is_empty(){return Err("This runtime already owns a session".into())}
                    session=host.create(&cwd).await?;Ok(json!({"thread":{"id":session,"cwd":cwd,"turns":[]},"model":"native-app","modelProvider":"native-app","cwd":cwd}))
                },
                "thread/resume"=>{
                    explicit_options(p)?;
                    if p["cwd"].as_str()!=Some(cwd.as_str()){return Err("Resume cwd differs from its launch directory".into())}
                    let requested=p["threadId"].as_str().ok_or("Missing thread identity")?;
                    match host.resume(requested).await {Ok(s)=>{session=s;Ok(json!({"thread":{"id":session,"cwd":cwd,"turns":[]},"model":"native-app","cwd":cwd}))},Err(e)=>{resume_failed=true;Err(e)}}
                },
                "thread/name/set"=>Err("Native title changes are not supported".into()),
                "turn/start"=>{
                    explicit_options(p)?;
                    if session.is_empty() || p["threadId"].as_str()!=Some(session.as_str()){return Err("Wrong native session".into())}
                    if active.as_ref().is_some_and(|t|!t.is_finished()){return Err("A native turn is already active".into())}
                    let prompt=text(&p["input"])?;let turn=format!("turn-{}",super::native_host::nonce());
                    let output=writer.clone();let executor=host.clone();let sid=session.clone();let tid=turn.clone();
                    let completed=Arc::new(std::sync::atomic::AtomicBool::new(false));let completion=completed.clone();
                    active=Some(TurnTask {host:host.clone(),session:session.clone(),completed,handle:tokio::spawn(async move {
                        output.send(json!({"method":"turn/started","params":{"threadId":sid,"turn":{"id":tid,"status":"inProgress"}}})).await;
                        let mut body=String::new();let mut reason=StopReason::Error("Provider turn ended without a completion receipt".into());
                        match executor.prompt(&sid,&prompt).await {
                            Err(e)=>reason=StopReason::Error(e),
                            Ok(mut events)=>while let Some(event)=events.recv().await {match event {
                                TurnEvent::Text(delta)=>{body.push_str(&delta);output.send(json!({"method":"item/agentMessage/delta","params":{"threadId":sid,"turnId":tid,"itemId":tid,"delta":delta}})).await;},
                                TurnEvent::Settled(r)=>{reason=r;break},_=>{}
                            }}
                        }
                        // A failed submission is not a final answer. Some scheduler
                        // clients act on this item before reading turn/completed.
                        if reason == StopReason::EndTurn {
                            output.send(json!({"method":"item/completed","params":{"threadId":sid,"turnId":tid,"item":{"id":tid,"type":"agentMessage","text":body,"phase":"final_answer"}}})).await;
                        }
                        let (status,error)=match reason {StopReason::EndTurn=>("completed",Value::Null),StopReason::Cancelled=>("interrupted",Value::Null),StopReason::Error(e)=>("failed",json!({"message":e})),_=>("failed",json!({"message":"Provider refused the turn"}))};
                        if status == "failed" {
                            output.send(json!({"method":"error","params":{"threadId":sid,"turnId":tid,"error":error,"willRetry":false}})).await;
                        }
                        completion.store(true,std::sync::atomic::Ordering::SeqCst);
                        output.send(json!({"method":"turn/completed","params":{"threadId":sid,"turn":{"id":tid,"status":status,"error":error}}})).await;
                    })});Ok(json!({"turn":{"id":turn,"status":"inProgress"}}))
                },
                "turn/interrupt"=>{if p["threadId"].as_str()!=Some(session.as_str()) || session.is_empty(){return Err("Wrong native session".into())}host.interrupt(&session).await?;Ok(json!({}))},
                _=>Err(format!("Unsupported native method: {method}"))
            }
        }.await;
        writer
            .send(match result {
                Ok(result) => json!({"id":id,"result":result}),
                Err(e) => failed(id, e),
            })
            .await;
    }
    drop(active);
    Ok(())
}

pub async fn claude<R, W>(
    read: R,
    write: W,
    host: Arc<dyn Host>,
    cwd: String,
    args: &[String],
) -> anyhow::Result<()>
where
    R: AsyncRead + Unpin + Send + 'static,
    W: AsyncWrite + Unpin + Send + 'static,
{
    let writer = Arc::new(Writer {
        inner: tokio::sync::Mutex::new(write),
    });
    // A native application cannot inherit CLI-only model/MCP/hook options.
    for flag in [
        "--model",
        "--effort",
        "--settings",
        "--mcp-config",
        "--append-system-prompt",
        "--system-prompt",
        "--max-turns",
    ] {
        if flag == "--model"
            && args
                .windows(2)
                .any(|a| a[0] == flag && a[1] == "native-app")
        {
            continue;
        }
        if args
            .iter()
            .any(|a| a == flag || a.starts_with(&format!("{flag}=")))
        {
            anyhow::bail!("Unsupported native scheduler option: {flag}")
        }
    }
    let resume = args
        .windows(2)
        .find(|a| a[0] == "--resume")
        .map(|a| a[1].as_str());
    let session = match resume {
        Some(id) => host.resume(id).await,
        None => host.create(&cwd).await,
    }
    .map_err(anyhow::Error::msg)?;
    writer.send(json!({"type":"system","subtype":"init","session_id":session,"tools":[],"model":"native-app","permissionMode":"native"})).await;

    // Keep reading while a turn runs: EOF and interrupt must not strand a
    // desktop turn or retain its task credentials indefinitely.
    let (frames, mut input) = tokio::sync::mpsc::channel(16);
    let _reader = ReaderTask(tokio::spawn(async move {
        let mut reader = BufReader::new(read);
        loop {
            let mut line = String::new();
            let n = match (&mut reader).take(MAX_FRAME + 1).read_line(&mut line).await {
                Ok(n) => n,
                Err(_) => break,
            };
            if n == 0 || n as u64 > MAX_FRAME {
                break;
            }
            let Ok(frame) = serde_json::from_str::<Value>(&line) else {
                break;
            };
            if frames.send(frame).await.is_err() {
                break;
            }
        }
    }));
    let (done_tx, mut done_rx) = tokio::sync::mpsc::channel(1);
    let mut active: Option<TurnTask> = None;
    loop {
        tokio::select! {
            _=done_rx.recv(),if active.is_some()=>return Ok(()),
            frame=input.recv()=>{
                let Some(frame)=frame else{return Ok(())};
                if frame["type"]=="control_request" {
                    let response=if frame["request"]["subtype"]=="interrupt" {
                        match host.interrupt(&session).await {
                            Ok(())=>json!({"subtype":"success","request_id":frame["request_id"],"response":{}}),
                            Err(e)=>json!({"subtype":"error","request_id":frame["request_id"],"error":e})
                        }
                    }else{json!({"subtype":"error","request_id":frame["request_id"],"error":"Native runtime does not implement CLI SDK hooks"})};
                    writer.send(json!({"type":"control_response","response":response})).await;continue
                }
                if frame["type"]!="user"{continue}
                if active.is_some(){anyhow::bail!("Native mid-turn input is unsupported")}
                let prompt=text(&frame["message"]["content"]).map_err(anyhow::Error::msg)?;
                let output=writer.clone();let executor=host.clone();let sid=session.clone();let done=done_tx.clone();
                let completed=Arc::new(std::sync::atomic::AtomicBool::new(false));let completion=completed.clone();
                active=Some(TurnTask{host:host.clone(),session:session.clone(),completed,handle:tokio::spawn(async move{
                    let mut body=String::new();let started=std::time::Instant::now();let mut reason=StopReason::Error("No provider completion receipt".into());
                    match executor.prompt(&sid,&prompt).await {
                        Err(e)=>reason=StopReason::Error(e),
                        Ok(mut events)=>while let Some(event)=events.recv().await{match event{
                            TurnEvent::Text(t)=>{body.push_str(&t);output.send(json!({"type":"assistant","session_id":sid,"message":{"role":"assistant","content":[{"type":"text","text":t}]}})).await;},
                            TurnEvent::Settled(r)=>{reason=r;break},_=>{}
                        }}
                    }
                    let success=reason==StopReason::EndTurn;let result=match reason{StopReason::Error(e)=>e,_=>body};
                    output.send(json!({"type":"result","subtype":if success{"success"}else{"error_during_execution"},"is_error":!success,"result":result,"session_id":sid,"duration_ms":started.elapsed().as_millis(),"num_turns":1})).await;
                    completion.store(true,std::sync::atomic::Ordering::SeqCst);
                    let _=done.send(()).await;
                })});
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicUsize, Ordering};
    use tokio::{io::AsyncWriteExt, sync::mpsc};
    struct Fake {
        creates: AtomicUsize,
        stop: StopReason,
        reject: bool,
    }
    #[async_trait::async_trait]
    impl Host for Fake {
        async fn create(&self, _: &str) -> Result<String, String> {
            self.creates.fetch_add(1, Ordering::SeqCst);
            Ok("fixture-session".into())
        }
        async fn resume(&self, _: &str) -> Result<String, String> {
            Err("unconfirmed".into())
        }
        async fn prompt(&self, _: &str, _: &str) -> Result<mpsc::Receiver<TurnEvent>, String> {
            if self.reject {
                return Err("fixture submission rejected".into());
            }
            let (tx, rx) = mpsc::channel(4);
            tx.send(TurnEvent::Text("fixture reply".into()))
                .await
                .unwrap();
            tx.send(TurnEvent::Settled(self.stop.clone()))
                .await
                .unwrap();
            Ok(rx)
        }
        async fn interject(&self, _: &str, _: &str) -> Result<(), String> {
            Err("unsupported".into())
        }
        async fn interrupt(&self, _: &str) -> Result<(), String> {
            Ok(())
        }
    }
    async fn send(w: &mut tokio::io::WriteHalf<tokio::io::DuplexStream>, frame: Value) {
        w.write_all(format!("{frame}\n").as_bytes()).await.unwrap();
    }
    async fn next(r: &mut BufReader<tokio::io::ReadHalf<tokio::io::DuplexStream>>) -> Value {
        let mut s = String::new();
        tokio::time::timeout(std::time::Duration::from_secs(2), r.read_line(&mut s))
            .await
            .unwrap()
            .unwrap();
        serde_json::from_str(&s).unwrap()
    }
    #[tokio::test]
    async fn codex_failed_resume_cannot_fall_through_to_fresh_thread() {
        let host = Arc::new(Fake {
            creates: AtomicUsize::new(0),
            stop: StopReason::EndTurn,
            reject: false,
        });
        let (a, b) = tokio::io::duplex(8192);
        let (r, w) = tokio::io::split(a);
        let (r2, mut w2) = tokio::io::split(b);
        let mut r2 = BufReader::new(r2);
        let server = tokio::spawn(codex(r, w, host.clone(), "/tmp/fixture".into(), &[]));
        send(&mut w2, json!({"id":1,"method":"initialize"})).await;
        assert!(next(&mut r2).await["result"].is_object());
        send(&mut w2,json!({"id":2,"method":"thread/resume","params":{"threadId":"prior","cwd":"/tmp/fixture"}})).await;
        assert!(next(&mut r2).await["error"].is_object());
        send(
            &mut w2,
            json!({"id":3,"method":"thread/start","params":{"cwd":"/tmp/fixture"}}),
        )
        .await;
        assert!(next(&mut r2).await["error"].is_object());
        assert_eq!(host.creates.load(Ordering::SeqCst), 0);
        w2.shutdown().await.unwrap();
        drop(w2);
        drop(r2);
        server.await.unwrap().unwrap();
    }
    #[tokio::test]
    async fn codex_terminal_frames_never_turn_failure_or_cancellation_into_a_final_answer() {
        for (stop, reject, expected) in [
            (StopReason::EndTurn, false, "completed"),
            (StopReason::Error("fixture failure".into()), false, "failed"),
            (StopReason::Cancelled, false, "interrupted"),
            (StopReason::EndTurn, true, "failed"),
        ] {
            let host = Arc::new(Fake {
                creates: AtomicUsize::new(0),
                stop,
                reject,
            });
            let (a, b) = tokio::io::duplex(8192);
            let (r, w) = tokio::io::split(a);
            let (r2, mut w2) = tokio::io::split(b);
            let mut r2 = BufReader::new(r2);
            let server = tokio::spawn(codex(r, w, host, "/tmp/fixture".into(), &[]));
            send(&mut w2, json!({"id":1,"method":"initialize"})).await;
            next(&mut r2).await;
            send(
                &mut w2,
                json!({"id":2,"method":"thread/start","params":{"cwd":"/tmp/fixture"}}),
            )
            .await;
            next(&mut r2).await;
            send(&mut w2,json!({"id":3,"method":"turn/start","params":{"threadId":"fixture-session","input":[{"type":"text","text":"hello"}]}})).await;
            let mut finals = 0;
            let mut errors = 0;
            loop {
                let frame = next(&mut r2).await;
                if frame["method"] == "item/completed" {
                    assert_eq!(expected, "completed", "failed turn emitted a final answer");
                    assert_eq!(frame["params"]["item"]["text"], "fixture reply");
                    finals += 1;
                }
                if frame["method"] == "error" {
                    assert_eq!(frame["params"]["willRetry"], false);
                    assert!(
                        frame["params"]["error"]["message"]
                            .as_str()
                            .is_some_and(|s| !s.is_empty())
                    );
                    errors += 1;
                }
                if frame["method"] == "turn/completed" {
                    assert_eq!(frame["params"]["turn"]["status"], expected);
                    assert_eq!(finals, usize::from(expected == "completed"));
                    assert_eq!(errors, usize::from(expected == "failed"));
                    break;
                }
            }
            w2.shutdown().await.unwrap();
            drop(w2);
            drop(r2);
            server.await.unwrap().unwrap();
        }
    }
    #[test]
    fn native_settings_and_nontext_requests_are_not_silently_accepted() {
        assert!(explicit_options(&json!({"model":"native-app"})).is_ok());
        assert!(explicit_options(&json!({"model":"different-model"})).is_err());
        assert!(text(&json!([{"type":"image","url":"file:///tmp/fixture"}])).is_err());
    }
}
