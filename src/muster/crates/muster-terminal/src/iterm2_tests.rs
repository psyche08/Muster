//! Deterministic protocol/event fixtures. No native apps or permissions required.
use super::*;
use crate::proto::*;

struct Fixture {
    client: Arc<Iterm2>,
    requests: mpsc::Receiver<Vec<u8>>,
    close: watch::Sender<bool>,
}
impl Fixture {
    fn new(capacity: usize) -> Self {
        let (outbound, requests) = mpsc::channel(capacity);
        let (close, closed) = watch::channel(false);
        Self {
            client: Arc::new(Iterm2 {
                outbound,
                closed,
                pending: Arc::default(),
                next_id: AtomicI64::new(1),
                notifications: broadcast::channel(8).0,
            }),
            requests,
            close,
        }
    }
    async fn next(&mut self) -> ClientOriginatedMessage {
        let bytes = tokio::time::timeout(Duration::from_secs(1), self.requests.recv())
            .await
            .unwrap()
            .unwrap();
        ClientOriginatedMessage::decode(&bytes[..]).unwrap()
    }
    fn reply(&self, request: ClientOriginatedMessage, response: Resp) {
        if let Some(tx) = self
            .client
            .pending
            .lock()
            .unwrap()
            .remove(&request.id.unwrap())
        {
            let _ = tx.send(ServerOriginatedMessage {
                id: request.id,
                submessage: Some(response),
            });
        }
    }
    fn notify(&self, session: &str, end: bool) {
        let n = if end {
            Notification {
                terminate_session_notification: Some(TerminateSessionNotification {
                    session_id: Some(session.into()),
                }),
                ..Default::default()
            }
        } else {
            Notification {
                screen_update_notification: Some(ScreenUpdateNotification {
                    session: Some(session.into()),
                }),
                ..Default::default()
            }
        };
        let _ = self.client.notifications.send(Arc::new(n));
    }
    async fn buffer(&mut self, lines: &[&str], x: i32) {
        let request = self.next().await;
        let Some(Req::GetBufferRequest(ref r)) = request.submessage else {
            panic!("{request:?}")
        };
        assert_eq!(r.session.as_deref(), Some("fixture"));
        self.reply(request, buffer(lines, x));
    }
    async fn attach(
        &mut self,
    ) -> (
        mpsc::Receiver<TerminalFrame>,
        tokio::task::JoinHandle<Result<(), Error>>,
    ) {
        let client = self.client.clone();
        let (tx, mut rx) = mpsc::channel(8);
        let task = tokio::spawn(async move { client.attach("fixture", 40, tx).await });
        for _ in 0..2 {
            let r = self.next().await;
            assert!(matches!(r.submessage, Some(Req::NotificationRequest(_))));
            self.reply(
                r,
                Resp::NotificationResponse(NotificationResponse::default()),
            );
        }
        self.buffer(&["first", "second"], 0).await;
        let frame = rx.recv().await.unwrap();
        assert!(frame.full);
        assert_eq!(frame.rev, 41);
        (rx, task)
    }
}
fn buffer(lines: &[&str], x: i32) -> Resp {
    Resp::GetBufferResponse(GetBufferResponse {
        contents: lines
            .iter()
            .map(|text| LineContents {
                text: Some((*text).into()),
                ..Default::default()
            })
            .collect(),
        cursor: Some(Coord {
            x: Some(x),
            y: Some(0),
        }),
        ..Default::default()
    })
}
async fn done(task: tokio::task::JoinHandle<Result<(), Error>>) -> Result<(), Error> {
    tokio::time::timeout(Duration::from_secs(1), task)
        .await
        .unwrap()
        .unwrap()
}

#[tokio::test(start_paused = true)]
async fn screen_bursts_cursor_only_changes_and_shrinking_screens_are_delivered() {
    let mut f = Fixture::new(8);
    let (mut rx, task) = f.attach().await;
    f.notify("unrelated", false);
    tokio::time::sleep(COALESCE * 2).await;
    assert!(f.requests.try_recv().is_err());
    for _ in 0..6 {
        f.notify("fixture", false);
    }
    f.buffer(&["first", "second"], 3).await;
    let cursor = rx.recv().await.unwrap();
    assert_eq!((cursor.rev, cursor.cursor_x, cursor.full), (42, 3, false));
    assert!(cursor.lines.is_empty());
    assert!(f.requests.try_recv().is_err(), "burst must use one fetch");
    f.notify("fixture", false);
    f.buffer(&["first", "changed"], 3).await;
    let delta = rx.recv().await.unwrap();
    assert_eq!(delta.lines.len(), 1);
    assert_eq!(delta.lines[0].index, 1);
    assert!(!delta.full);
    f.notify("fixture", false);
    f.buffer(&["short"], 1).await;
    let full = rx.recv().await.unwrap();
    assert!(full.full);
    assert_eq!((full.rev, full.rows), (44, 1));
    f.notify("fixture", false);
    f.buffer(&["short"], 1).await;
    tokio::task::yield_now().await;
    assert!(rx.try_recv().is_err(), "unchanged output emits no frame");
    drop(rx);
    done(task).await.unwrap();
}

#[tokio::test(start_paused = true)]
async fn termination_during_coalescing_is_not_swallowed_or_refetched() {
    let mut f = Fixture::new(8);
    let (mut rx, task) = f.attach().await;
    f.notify("fixture", false);
    tokio::task::yield_now().await;
    f.notify("other", true);
    f.notify("fixture", true);
    let frame = tokio::time::timeout(Duration::from_secs(1), rx.recv())
        .await
        .unwrap()
        .unwrap();
    assert!(frame.ended);
    assert_eq!(frame.rev, 42);
    done(task).await.unwrap();
    assert!(f.requests.try_recv().is_err());
}

#[tokio::test(start_paused = true)]
async fn lost_screen_notifications_resync_even_when_only_unrelated_events_remain() {
    let mut f = Fixture::new(8);
    let (mut rx, task) = f.attach().await;
    f.notify("fixture", false);
    for _ in 0..16 {
        f.notify("other", false);
    }
    f.buffer(&["resynchronized"], 2).await;
    assert_eq!(rx.recv().await.unwrap().lines[0].text, "resynchronized");
    drop(rx);
    done(task).await.unwrap();
}

#[tokio::test(start_paused = true)]
async fn buffer_failure_ends_view_and_cancel_releases_inflight_waiter() {
    let mut f = Fixture::new(8);
    let (rx, task) = f.attach().await;
    f.notify("fixture", false);
    let request = f.next().await;
    f.reply(
        request,
        Resp::GetBufferResponse(GetBufferResponse {
            status: Some(get_buffer_response::Status::SessionNotFound as i32),
            ..Default::default()
        }),
    );
    assert!(matches!(done(task).await, Err(Error::NotFound(_))));
    drop(rx);
    let (rx, task) = f.attach().await;
    f.notify("fixture", false);
    let late = f.next().await;
    drop(rx);
    done(task).await.unwrap();
    assert!(f.client.pending.lock().unwrap().is_empty());
    f.reply(late, buffer(&["late"], 0));
}

#[tokio::test(start_paused = true)]
async fn cancellation_and_timeout_bound_waiters_even_with_a_full_outbound_queue() {
    let f = Fixture::new(1);
    for cancel in [true, false] {
        let client = f.client.clone();
        let task = tokio::spawn(async move { client.send_text("fixture", "once").await });
        tokio::task::yield_now().await;
        assert_eq!(f.client.pending.lock().unwrap().len(), 1);
        if cancel {
            task.abort();
            assert!(task.await.unwrap_err().is_cancelled());
        } else {
            // First cancelled request still occupies the queue. This request
            // times out while queueing; it must not wait indefinitely.
            tokio::time::advance(REQUEST_TIMEOUT).await;
            assert!(matches!(task.await.unwrap(), Err(Error::Protocol(_))));
        }
        assert!(f.client.pending.lock().unwrap().is_empty());
    }
}

#[tokio::test(start_paused = true)]
async fn concurrent_responses_are_correlated_and_disconnect_fails_all_waiters() {
    let mut f = Fixture::new(8);
    let client = f.client.clone();
    let a = tokio::spawn(async move { client.send_text("a", "one").await });
    let client = f.client.clone();
    let b = tokio::spawn(async move { client.send_text("b", "two").await });
    let first = f.next().await;
    let second = f.next().await;
    f.reply(second, Resp::SendTextResponse(SendTextResponse::default()));
    f.reply(first, Resp::SendTextResponse(SendTextResponse::default()));
    done(a).await.unwrap();
    done(b).await.unwrap();
    let client = f.client.clone();
    let a = tokio::spawn(async move { client.send_text("a", "three").await });
    let client = f.client.clone();
    let b = tokio::spawn(async move { client.sessions().await.map(|_| ()) });
    let _ = f.next().await;
    let _ = f.next().await;
    f.close.send(true).unwrap();
    assert!(matches!(done(a).await, Err(Error::Unavailable(_))));
    assert!(matches!(done(b).await, Err(Error::Unavailable(_))));
    assert!(f.client.pending.lock().unwrap().is_empty());
    assert!(f.requests.try_recv().is_err(), "no replay after disconnect");
}

#[tokio::test(start_paused = true)]
async fn native_close_refusal_missing_and_malformed_receipts_never_report_success() {
    for statuses in [vec![0], vec![1], vec![2], vec![], vec![0, 0], vec![999]] {
        let mut f = Fixture::new(8);
        let expected = statuses == [0];
        let client = f.client.clone();
        let task = tokio::spawn(async move { client.close("fixture").await });
        let request = f.next().await;
        let Some(Req::CloseRequest(ref r)) = request.submessage else {
            panic!()
        };
        assert_eq!(r.force, Some(false));
        assert!(
            matches!(&r.target, Some(close_request::Target::Sessions(s)) if s.session_ids == ["fixture"])
        );
        f.reply(request, Resp::CloseResponse(CloseResponse { statuses }));
        assert_eq!(done(task).await.is_ok(), expected);
    }
}

#[tokio::test(start_paused = true)]
async fn input_resize_and_history_use_exact_target_and_surface_protocol_failures() {
    let mut f = Fixture::new(8);
    let client = f.client.clone();
    let task = tokio::spawn(async move { client.send_text("opaque:target", "你好\n").await });
    let r = f.next().await;
    let Some(Req::SendTextRequest(ref input)) = r.submessage else {
        panic!()
    };
    assert_eq!(input.session.as_deref(), Some("opaque:target"));
    assert_eq!(input.text.as_deref(), Some("你好\n"));
    assert_eq!(input.suppress_broadcast, Some(true));
    f.reply(
        r,
        Resp::SendTextResponse(SendTextResponse {
            status: Some(send_text_response::Status::SessionNotFound as i32),
        }),
    );
    assert!(matches!(done(task).await, Err(Error::NotFound(_))));
    let client = f.client.clone();
    let task = tokio::spawn(async move { client.resize("opaque:target", 90, 24).await });
    let r = f.next().await;
    let Some(Req::SetPropertyRequest(ref resize)) = r.submessage else {
        panic!()
    };
    assert_eq!(
        resize.identifier,
        Some(set_property_request::Identifier::SessionId(
            "opaque:target".into()
        ))
    );
    assert_eq!(
        serde_json::from_str::<serde_json::Value>(resize.json_value.as_ref().unwrap()).unwrap(),
        serde_json::json!({"width":90,"height":24})
    );
    f.reply(r, Resp::Error("fixture refusal".into()));
    assert!(matches!(done(task).await, Err(Error::Protocol(_))));
    let client = f.client.clone();
    let task = tokio::spawn(async move { client.buffer("fixture", Some(7)).await });
    let r = f.next().await;
    let Some(Req::GetBufferRequest(ref history)) = r.submessage else {
        panic!()
    };
    assert_eq!(history.line_range.as_ref().unwrap().trailing_lines, Some(7));
    f.reply(r, buffer(&["history"], 0));
    assert_eq!(task.await.unwrap().unwrap().0, ["history"]);
}

#[tokio::test(start_paused = true)]
async fn inventory_notifications_and_lag_refresh_and_cancel_during_fetch() {
    let mut f = Fixture::new(8);
    let service = crate::TerminalService {
        config: crate::TerminalConfig::default(),
        connection: tokio::sync::Mutex::new(Some(f.client.clone())),
    };
    let (tx, mut rx) = mpsc::channel(8);
    let task = tokio::spawn(async move { service.watch_sessions(tx).await });
    for _ in 0..3 {
        let r = f.next().await;
        assert!(matches!(r.submessage, Some(Req::NotificationRequest(_))));
        f.reply(
            r,
            Resp::NotificationResponse(NotificationResponse::default()),
        );
    }
    for round in 0..3 {
        let r = f.next().await;
        assert!(matches!(r.submessage, Some(Req::ListSessionsRequest(_))));
        f.reply(
            r,
            Resp::ListSessionsResponse(ListSessionsResponse::default()),
        );
        assert!(rx.recv().await.unwrap().is_empty());
        if round == 0 {
            let _ = f.client.notifications.send(Arc::new(Notification {
                new_session_notification: Some(NewSessionNotification::default()),
                ..Default::default()
            }));
        } else {
            for _ in 0..16 {
                f.notify("other", false);
            }
        }
    }
    let _pending = f.next().await;
    drop(rx);
    done(task).await.unwrap();
    assert!(f.client.pending.lock().unwrap().is_empty());
}

#[tokio::test(start_paused = true)]
async fn timed_out_response_is_discarded_and_does_not_complete_the_next_request() {
    let mut f = Fixture::new(8);
    let client = f.client.clone();
    let first = tokio::spawn(async move { client.send_text("fixture", "first").await });
    let late = f.next().await;
    tokio::time::advance(REQUEST_TIMEOUT).await;
    assert!(matches!(first.await.unwrap(), Err(Error::Protocol(_))));
    assert!(f.client.pending.lock().unwrap().is_empty());
    let client = f.client.clone();
    let second = tokio::spawn(async move { client.send_text("fixture", "second").await });
    let r = f.next().await;
    f.reply(late, Resp::SendTextResponse(SendTextResponse::default()));
    assert!(!second.is_finished());
    f.reply(r, Resp::SendTextResponse(SendTextResponse::default()));
    done(second).await.unwrap();
}

// Environment-based API credentials are scoped to a child test process: no
// process-global environment mutation can race other tests or touch native UI.
#[tokio::test]
async fn reconnect_fixture_child() {
    let Some(path) = std::env::var_os("MUSTER_TEST_ITERM_SOCKET") else {
        return;
    };
    let service = crate::TerminalService::new(crate::TerminalConfig {
        socket: path.into(),
        ..Default::default()
    });
    service.input("fixture", b"first").await.unwrap();
    let connection = service.connection.lock().await.as_ref().unwrap().clone();
    tokio::time::timeout(Duration::from_secs(2), connection.closed())
        .await
        .unwrap();
    service.input("fixture", b"second").await.unwrap();
}

#[tokio::test]
#[allow(clippy::result_large_err)]
async fn a_new_operation_reconnects_after_socket_loss_without_replaying_input() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("api.sock");
    let listener = tokio::net::UnixListener::bind(&path).unwrap();
    let server = tokio::spawn(async move {
        for expected in ["first", "second"] {
            let (stream, _) = listener.accept().await.unwrap();
            let mut ws = tokio_tungstenite::accept_hdr_async(stream,
                |_: &tokio_tungstenite::tungstenite::handshake::server::Request,
                 mut response: tokio_tungstenite::tungstenite::handshake::server::Response| {
                    response.headers_mut().insert("Sec-WebSocket-Protocol", HeaderValue::from_static(SUBPROTOCOL));
                    Ok(response)
                }).await.unwrap();
            let Message::Binary(bytes) = ws.next().await.unwrap().unwrap() else {
                panic!()
            };
            let request = ClientOriginatedMessage::decode(&bytes[..]).unwrap();
            let Some(Req::SendTextRequest(ref input)) = request.submessage else {
                panic!("no preflight list is expected")
            };
            assert_eq!(input.text.as_deref(), Some(expected));
            ws.send(Message::Binary(
                ServerOriginatedMessage {
                    id: request.id,
                    submessage: Some(Resp::SendTextResponse(SendTextResponse::default())),
                }
                .encode_to_vec()
                .into(),
            ))
            .await
            .unwrap();
            ws.close(None).await.unwrap();
        }
    });
    let child = tokio::process::Command::new(std::env::current_exe().unwrap())
        .args([
            "--exact",
            "iterm2::simulation_tests::reconnect_fixture_child",
            "--nocapture",
        ])
        .env("MUSTER_TEST_ITERM_SOCKET", &path)
        .env("ITERM2_COOKIE", "fixture-cookie")
        .env("ITERM2_KEY", "fixture-key")
        .kill_on_drop(true)
        .output();
    let output = tokio::time::timeout(Duration::from_secs(5), child)
        .await
        .unwrap()
        .unwrap();
    assert!(
        output.status.success(),
        "{} {}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    tokio::time::timeout(Duration::from_secs(1), server)
        .await
        .unwrap()
        .unwrap();
}

#[tokio::test(start_paused = true)]
async fn a_delivered_receipt_wins_over_a_simultaneous_disconnect() {
    let mut f = Fixture::new(8);
    let client = f.client.clone();
    let task = tokio::spawn(async move { client.send_text("fixture", "once").await });
    let request = f.next().await;
    f.reply(request, Resp::SendTextResponse(SendTextResponse::default()));
    f.close.send(true).unwrap();
    done(task).await.unwrap();
    assert!(f.client.send_text("fixture", "later").await.is_err());
    assert!(f.requests.try_recv().is_err());
}

#[tokio::test(start_paused = true)]
async fn cancelling_views_does_not_wait_for_a_connection_or_its_handshake() {
    let service = crate::TerminalService::new(crate::TerminalConfig::default());
    let held = service.connection.lock().await;
    let attach_service = service.clone();
    let (tx, rx) = mpsc::channel(1);
    let attach = tokio::spawn(async move { attach_service.attach("fixture", 0, tx).await });
    let watch_service = service.clone();
    let (out, incoming) = mpsc::channel(1);
    let watch = tokio::spawn(async move { watch_service.watch_sessions(out).await });
    tokio::task::yield_now().await;
    drop(rx);
    drop(incoming);
    done(attach).await.unwrap();
    done(watch).await.unwrap();
    drop(held);
}
