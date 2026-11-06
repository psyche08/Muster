//! muster-rpc transport (ARCHITECTURE §3.6.1): the same envelopes over the
//! local Unix socket and over a service stream between devices. A call is
//! a Request with an id; Events with that id may follow; the Response with
//! that id ends it. A Cancel from the caller ends a streaming call early.

use std::collections::HashMap;
use std::future::Future;
use std::pin::Pin;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex};

use muster_core::DeviceId;
use muster_proto::v1::{self, Envelope, Request, Response, envelope::Msg};
use muster_proto::{frame, unframe};
use tokio::io::{AsyncRead, AsyncReadExt, AsyncWrite, AsyncWriteExt};
use tokio::sync::{mpsc, oneshot};
use tokio_util::sync::CancellationToken;

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("io: {0}")]
    Io(String),
    #[error("protocol: {0}")]
    Protocol(String),
    #[error("closed")]
    Closed,
    #[error("remote error {code:?}: {message}")]
    Remote {
        code: v1::error::Code,
        message: String,
    },
}

/// Who is calling: the desktop's own user over the socket, or a member over the link.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Principal {
    Local,
    Remote(DeviceId),
}

/// Who is calling and what they proved.
#[derive(Clone, Debug)]
pub struct Context {
    pub principal: Principal,
    pub id: u64,
    /// The presence proof the envelope carried (empty when none).
    pub presence: Vec<u8>,
    pub presence_enrollment: Vec<u8>,
}

impl Context {
    pub fn local() -> Self {
        Self {
            principal: Principal::Local,
            id: 0,
            presence: Vec::new(),
            presence_enrollment: Vec::new(),
        }
    }
    /// The bytes a presence proof for this call must sign.
    pub fn presence_message(&self, req: &v1::request::Req) -> Vec<u8> {
        muster_proto::presence_message(self.id, req)
    }
}

/// Where a handler puts events for a streaming call.
#[derive(Clone)]
pub struct EventSink {
    id: u64,
    tx: mpsc::Sender<Envelope>,
    cancel: CancellationToken,
}

impl EventSink {
    pub async fn send(&self, ev: v1::event::Ev) -> Result<(), Error> {
        self.tx
            .send(Envelope::event(self.id, ev))
            .await
            .map_err(|_| Error::Closed)
    }
    pub fn cancelled(&self) -> &CancellationToken {
        &self.cancel
    }
    pub fn is_cancelled(&self) -> bool {
        self.cancel.is_cancelled()
    }
}

pub type HandlerFuture<'a> = Pin<Box<dyn Future<Output = v1::response::Resp> + Send + 'a>>;

/// Any byte stream a server can serve or a client can use.
pub trait Stream: AsyncRead + AsyncWrite + Unpin + Send {}
impl<T: AsyncRead + AsyncWrite + Unpin + Send> Stream for T {}

pub type TargetFuture<'a> =
    Pin<Box<dyn Future<Output = Result<Box<dyn Stream>, Error>> + Send + 'a>>;

/// The application side of a server. One call at a time per id; calls run
/// concurrently. Return the final response; stream events through the sink.
pub type TimingObserver = Arc<dyn Fn(v1::RpcTiming) + Send + Sync>;

pub trait Handler: Send + Sync + 'static {
    /// The local daemon's verified public binding, attached when proxying its app.
    fn forwarding_enrollment(&self) -> Vec<u8> {
        Vec::new()
    }

    fn call(&self, ctx: Context, request: v1::request::Req, events: EventSink)
    -> HandlerFuture<'_>;

    /// A stream to `target`'s own service, for envelopes addressed there
    /// (a device's daemon proxying for its app). Default: no proxying.
    fn open_target(&self, _target: &str) -> TargetFuture<'_> {
        Box::pin(async { Err(Error::Protocol("this endpoint does not proxy".into())) })
    }
    fn open_target_observed(
        &self,
        target: &str,
        _observer: Option<TimingObserver>,
    ) -> TargetFuture<'_> {
        self.open_target(target)
    }
}

/// Serves one connection until it closes.
pub async fn serve<S>(stream: S, principal: Principal, handler: Arc<dyn Handler>)
where
    S: AsyncRead + AsyncWrite + Unpin + Send + 'static,
{
    let (mut reader, mut writer) = tokio::io::split(stream);
    let (tx, mut rx) = mpsc::channel::<Envelope>(256);
    let writer_task = tokio::spawn(async move {
        while let Some(envelope) = rx.recv().await {
            if writer.write_all(&frame(&envelope)).await.is_err() {
                break;
            }
        }
        let _ = writer.shutdown().await;
    });
    let in_flight: Arc<Mutex<HashMap<u64, CancellationToken>>> = Arc::default();
    let forwarded: ForwardedCalls = Arc::default();
    let mut buf = Vec::with_capacity(8192);
    let mut chunk = vec![0u8; 65536];
    loop {
        let n = match reader.read(&mut chunk).await {
            Ok(0) | Err(_) => break,
            Ok(n) => n,
        };
        buf.extend_from_slice(&chunk[..n]);
        loop {
            let envelope = match unframe(&mut buf) {
                Ok(Some(envelope)) => envelope,
                Ok(None) => break,
                Err(error) => {
                    log::info!("rpc: {error}; closing");
                    for (_, token) in in_flight.lock().unwrap().drain() {
                        token.cancel();
                    }
                    for token in forwarded.lock().unwrap().values() {
                        token.cancel();
                    }
                    writer_task.abort();
                    return;
                }
            };
            if !envelope.target.is_empty() {
                if principal != Principal::Local {
                    let _ = tx
                        .send(Envelope::error(
                            envelope.id,
                            v1::error::Code::Unauthorized,
                            "only the device's own app may address other devices",
                        ))
                        .await;
                    continue;
                }
                let key = (envelope.target.clone(), envelope.id);
                if matches!(envelope.msg, Some(Msg::Cancel(_))) {
                    if let Some(token) = forwarded.lock().unwrap().get(&key) {
                        token.cancel();
                    }
                    continue;
                }
                if !matches!(envelope.msg, Some(Msg::Request(_))) {
                    continue;
                }
                let cancel = CancellationToken::new();
                let admitted = {
                    let mut calls = forwarded.lock().unwrap();
                    if calls.len() >= MAX_FORWARDED_CALLS || calls.contains_key(&key) {
                        false
                    } else {
                        calls.insert(key.clone(), cancel.clone());
                        true
                    }
                };
                if !admitted {
                    let _ = tx
                        .send(Envelope::error(
                            envelope.id,
                            v1::error::Code::Unavailable,
                            "too many forwarded calls or duplicate request id",
                        ))
                        .await;
                    continue;
                }
                let (handler, forwarded, tx) = (handler.clone(), forwarded.clone(), tx.clone());
                tokio::spawn(async move {
                    proxy(&handler, envelope, &tx, &cancel).await;
                    forwarded.lock().unwrap().remove(&key);
                });
                continue;
            }
            match envelope.msg {
                Some(Msg::Request(Request { req: Some(req) })) => {
                    let cancel = CancellationToken::new();
                    in_flight
                        .lock()
                        .unwrap()
                        .insert(envelope.id, cancel.clone());
                    let sink = EventSink {
                        id: envelope.id,
                        tx: tx.clone(),
                        cancel: cancel.clone(),
                    };
                    let handler = handler.clone();
                    let tx = tx.clone();
                    let in_flight = in_flight.clone();
                    let id = envelope.id;
                    let ctx = Context {
                        principal,
                        id,
                        presence: envelope.presence,
                        presence_enrollment: envelope.presence_enrollment,
                    };
                    tokio::spawn(async move {
                        // A cancelled handler gets a moment to answer on its
                        // own (a streaming call ending with Ack); then it is
                        // answered for it.
                        let diagnostic_id =
                            muster_proto::diagnostics::request_id(&req).map(str::to_owned);
                        let started = std::time::Instant::now();
                        let timing = |phase| {
                            if let Some(diagnostic_id) = &diagnostic_id {
                                let _ = tx.try_send(Envelope::event(
                                    id,
                                    v1::event::Ev::RpcTiming(v1::RpcTiming {
                                        phase: phase as i32,
                                        elapsed_us: started.elapsed().as_micros() as u64,
                                        diagnostic_id: diagnostic_id.clone(),
                                        reused_connection: false,
                                    }),
                                ));
                            }
                        };
                        timing(v1::rpc_timing::Phase::ServerReceived);
                        let mut fut = handler.call(ctx, req, sink);
                        let resp = tokio::select! {
                            resp = &mut fut => resp,
                            _ = cancel.cancelled() => match tokio::time::timeout(std::time::Duration::from_secs(2), &mut fut).await {
                                Ok(resp) => resp,
                                Err(_) => v1::response::Resp::Error(v1::Error { code: v1::error::Code::Timeout as i32, message: "cancelled".into() }),
                            },
                        };
                        timing(v1::rpc_timing::Phase::ServerFinished);
                        in_flight.lock().unwrap().remove(&id);
                        let _ = tx.send(Envelope::response(id, resp)).await;
                    });
                }
                Some(Msg::Cancel(_)) => {
                    if let Some(token) = in_flight.lock().unwrap().get(&envelope.id) {
                        token.cancel();
                    }
                }
                Some(Msg::Request(_)) => {
                    let _ = tx
                        .send(Envelope::error(
                            envelope.id,
                            v1::error::Code::Invalid,
                            "empty request",
                        ))
                        .await;
                }
                _ => {}
            }
        }
    }
    for (_, token) in in_flight.lock().unwrap().drain() {
        token.cancel();
    }
    for token in forwarded.lock().unwrap().values() {
        token.cancel();
    }
    drop(tx);
    writer_task.abort();
    let _ = writer_task.await;
}

const MAX_FORWARDED_CALLS: usize = 256;
type ForwardedCalls = Arc<Mutex<HashMap<(String, u64), CancellationToken>>>;

/// Forwards one addressed envelope to its target and relays what comes back.
async fn proxy(
    handler: &Arc<dyn Handler>,
    mut envelope: Envelope,
    tx: &mpsc::Sender<Envelope>,
    cancel: &CancellationToken,
) {
    let id = envelope.id;
    let target = envelope.target.clone();
    let started = std::time::Instant::now();
    // One request owns one service stream. The handler/Node reuses the
    // authenticated QUIC connection; no other operation shares this reader.
    let diagnostic_id = match &envelope.msg {
        Some(Msg::Request(Request { req: Some(req) })) => {
            muster_proto::diagnostics::request_id(req).map(str::to_owned)
        }
        _ => None,
    };
    let observer: Option<TimingObserver> = diagnostic_id.map(|diagnostic_id| {
        let tx = tx.clone();
        Arc::new(move |mut timing: v1::RpcTiming| {
            timing.diagnostic_id = diagnostic_id.clone();
            let _ = tx.try_send(Envelope::event(id, v1::event::Ev::RpcTiming(timing)));
        }) as TimingObserver
    });
    let connect = async {
        let client = Client::new(
            handler
                .open_target_observed(&target, observer.clone())
                .await?,
        );
        Ok::<_, Error>(client.with_timing_observer(observer))
    };
    let client = tokio::select! {
        biased;
        _ = cancel.cancelled() => {
            let _ = tx.send(Envelope::error(id, v1::error::Code::Timeout,
                "forward cancelled before dispatch")).await;
            return;
        }
        result = connect => match result {
            Ok(client) => client,
            Err(error) => {
                log::warn!("rpc proxy target {target} id={id}: connection failed after {} ms: {error}", started.elapsed().as_millis());
                let _ = tx.send(Envelope::error(id, v1::error::Code::Unavailable,
                    format!("{target}: {error}"))).await;
                return;
            }
        },
    };
    if started.elapsed() >= std::time::Duration::from_secs(1) {
        log::warn!(
            "rpc proxy target {target}: connection ready after {} ms",
            started.elapsed().as_millis()
        );
    }
    envelope.presence_enrollment = handler.forwarding_enrollment();
    // forward reserves queue capacity before publishing Pending; cancelling
    // before capacity is available cannot leave an unsent request behind.
    let mut call = tokio::select! {
        biased;
        _ = cancel.cancelled() => {
            let _ = tx.send(Envelope::error(id, v1::error::Code::Timeout,
                "forward cancelled before dispatch")).await;
            return;
        }
        result = client.forward(envelope) => match result {
            Ok(call) => call,
            Err(error) => {
                let _ = tx.send(Envelope::error(id, v1::error::Code::Unavailable,
                    error.to_string())).await;
                return;
            }
        },
    };
    let mut events_open = true;
    let response = loop {
        tokio::select! {
            biased;
            _ = cancel.cancelled() => break None,
            ev = call.events.recv(), if events_open => match ev {
                Some(ev) => {
                    let sent = tokio::select! {
                        biased;
                        _ = cancel.cancelled() => false,
                        result = tx.send(Envelope::event(id, ev)) => result.is_ok(),
                    };
                    if !sent { break None; }
                }
                None => events_open = false,
            },
            result = &mut call.response => break Some(result),
        }
    };
    let response = match response {
        Some(result) => {
            drop(call);
            result.map_err(|_| Error::Closed)
        }
        None => {
            // Match the server's bounded cancellation grace: a cooperative
            // watch may Ack and a delivered mutation may have a real receipt.
            // Never replace either with an invented timeout or replay a write.
            tokio::time::timeout(std::time::Duration::from_secs(2), async move {
                call.cancel().await;
                call.finish_raw().await
            })
            .await
            .unwrap_or(Err(Error::Closed))
        }
    };
    // Closing this operation's service stream leaves the peer QUIC connection
    // and every other request/subscription intact.
    client.close();
    let response = match response {
        Ok(response) => Envelope {
            id,
            msg: Some(Msg::Response(response)),
            ..Default::default()
        },
        Err(_) => Envelope::error(
            id,
            v1::error::Code::Timeout,
            "forward ended without a response after dispatch; operation outcome may be unknown",
        ),
    };
    let _ = tx.send(response).await;
}

/// A call's incoming side: events, then the final response.
pub struct Call {
    pub id: u64,
    pub events: mpsc::Receiver<v1::event::Ev>,
    response: oneshot::Receiver<Response>,
    cancel_tx: mpsc::Sender<Envelope>,
    target: String,
    pending: Arc<Mutex<HashMap<u64, Pending>>>,
    closed: CancellationToken,
}

impl Drop for Call {
    fn drop(&mut self) {
        // A dropped deadline/future must not leave a queued mutation to run
        // later. After dispatch cancellation cannot establish non-execution.
        if self.pending.lock().unwrap().remove(&self.id).is_some() {
            let mut cancel = Envelope::cancel(self.id);
            cancel.target = self.target.clone();
            if self.cancel_tx.try_send(cancel).is_err() {
                // Bounded fallback: tear down this stuck connection instead of
                // spawning an unbounded queue of cancellation tasks.
                self.closed.cancel();
            }
        }
    }
}

impl Call {
    /// Waits for the final response (draining any remaining events).
    pub async fn finish(self) -> Result<v1::response::Resp, Error> {
        into_result(self.finish_raw().await?)
    }

    pub async fn cancel(&self) {
        let mut envelope = Envelope::cancel(self.id);
        envelope.target = self.target.clone();
        let _ = self.cancel_tx.send(envelope).await;
    }

    /// The response as it arrived, errors included.
    pub async fn finish_raw(mut self) -> Result<Response, Error> {
        let mut events_open = true;
        loop {
            tokio::select! {
                result = &mut self.response => return result.map_err(|_| Error::Closed),
                event = self.events.recv(), if events_open => events_open = event.is_some(),
            }
        }
    }
}

fn into_result(response: Response) -> Result<v1::response::Resp, Error> {
    match response.resp {
        Some(v1::response::Resp::Error(error)) => Err(Error::Remote {
            code: v1::error::Code::try_from(error.code).unwrap_or(v1::error::Code::Unknown),
            message: error.message,
        }),
        Some(resp) => Ok(resp),
        None => Err(Error::Protocol("empty response".into())),
    }
}

struct Pending {
    diagnostic_id: Option<String>,
    timing_observer: Option<TimingObserver>,
    timing_count: usize,
    events: mpsc::Sender<v1::event::Ev>,
    response: oneshot::Sender<Response>,
    target: String,
}

/// The calling side, usable from many tasks at once.
#[derive(Clone)]
pub struct Client {
    timing_observer: Option<TimingObserver>,
    closed: CancellationToken,
    tx: mpsc::Sender<Envelope>,
    pending: Arc<Mutex<HashMap<u64, Pending>>>,
    next_id: Arc<AtomicU64>,
    /// Device id (hex) every envelope is addressed to; empty = this endpoint.
    target: String,
}

impl Client {
    pub fn new<S>(stream: S) -> Self
    where
        S: AsyncRead + AsyncWrite + Unpin + Send + 'static,
    {
        let (mut reader, mut writer) = tokio::io::split(stream);
        let (tx, mut rx) = mpsc::channel::<Envelope>(256);
        let closed = CancellationToken::new();
        let writer_closed = closed.clone();
        tokio::spawn(async move {
            loop {
                let envelope = tokio::select! {
                    _ = writer_closed.cancelled() => break,
                    value = rx.recv() => match value { Some(value) => value, None => break },
                };
                let bytes = frame(&envelope);
                let written = tokio::select! {
                    _ = writer_closed.cancelled() => break,
                    result = writer.write_all(&bytes) => result,
                };
                if written.is_err() {
                    break;
                }
            }
            writer_closed.cancel();
            let _ = writer.shutdown().await;
        });
        let pending: Arc<Mutex<HashMap<u64, Pending>>> = Arc::default();
        let reader_pending = pending.clone();
        let reader_closed = closed.clone();
        let reader_cancel_tx = tx.downgrade();
        tokio::spawn(async move {
            let mut buf = Vec::with_capacity(8192);
            let mut chunk = vec![0u8; 65536];
            loop {
                let read = tokio::select! {
                    _ = reader_closed.cancelled() => break,
                    result = reader.read(&mut chunk) => result,
                };
                let n = match read {
                    Ok(0) | Err(_) => break,
                    Ok(n) => n,
                };
                buf.extend_from_slice(&chunk[..n]);
                loop {
                    let envelope = match unframe(&mut buf) {
                        Ok(Some(envelope)) => envelope,
                        Ok(None) => break,
                        Err(_) => {
                            reader_closed.cancel();
                            reader_pending.lock().unwrap().clear();
                            return;
                        }
                    };
                    match envelope.msg {
                        Some(Msg::Response(response)) => {
                            if let Some(pending) =
                                reader_pending.lock().unwrap().remove(&envelope.id)
                            {
                                let _ = pending.response.send(response);
                            }
                        }
                        Some(Msg::Event(v1::Event {
                            ev: Some(v1::event::Ev::RpcTiming(timing)),
                        })) => {
                            // Diagnostics cannot satisfy watch initialization or fill its business queue.
                            let observer = {
                                let mut pending = reader_pending.lock().unwrap();
                                pending.get_mut(&envelope.id).and_then(|p| {
                                    if p.timing_count >= 16 || p.diagnostic_id.as_deref() != Some(timing.diagnostic_id.as_str())
                                        || !matches!(v1::rpc_timing::Phase::try_from(timing.phase), Ok(phase) if phase != v1::rpc_timing::Phase::Unspecified)
                                    { return None; }
                                    p.timing_count += 1;
                                    p.timing_observer.clone()
                                })
                            };
                            if let Some(observer) = observer {
                                observer(timing);
                            }
                        }
                        Some(Msg::Event(v1::Event { ev: Some(ev) })) => {
                            let sender = reader_pending
                                .lock()
                                .unwrap()
                                .get(&envelope.id)
                                .map(|p| p.events.clone());
                            if let Some(sender) = sender {
                                // One slow subscriber must not stop the shared
                                // reader from delivering other calls' replies.
                                // Preserve a bounded queue and fail explicitly
                                // instead of hiding lost stream events.
                                if let Err(mpsc::error::TrySendError::Full(_)) = sender.try_send(ev)
                                {
                                    let pending =
                                        reader_pending.lock().unwrap().remove(&envelope.id);
                                    if let Some(pending) = pending {
                                        log::warn!(
                                            "rpc call {} event backlog exceeded; resynchronize",
                                            envelope.id
                                        );
                                        let _ = pending.response.send(Response {
                                            resp: Some(err(v1::error::Code::Unavailable,
                                                "RPC event backlog exceeded; resynchronize. Operation outcome may be unknown")),
                                        });
                                        let mut cancel = Envelope::cancel(envelope.id);
                                        cancel.target = pending.target;
                                        if reader_cancel_tx
                                            .upgrade()
                                            .is_none_or(|tx| tx.try_send(cancel).is_err())
                                        {
                                            reader_closed.cancel();
                                        }
                                    }
                                }
                            }
                        }
                        _ => {}
                    }
                    // Parsing a buffered burst has no I/O await. Give active
                    // consumers a chance to drain before treating them as slow.
                    tokio::task::consume_budget().await;
                }
            }
            reader_closed.cancel();
            reader_pending.lock().unwrap().clear();
        });
        Self {
            timing_observer: None,
            closed,
            tx,
            pending,
            next_id: Arc::new(AtomicU64::new(1)),
            target: String::new(),
        }
    }

    /// The same connection, with every call addressed to `target` (a member
    /// device id in hex) for the daemon to forward.
    pub fn with_target(&self, target: &str) -> Client {
        Client {
            timing_observer: self.timing_observer.clone(),
            closed: self.closed.clone(),
            tx: self.tx.clone(),
            pending: self.pending.clone(),
            next_id: self.next_id.clone(),
            target: target.to_string(),
        }
    }

    pub fn with_timing_observer(mut self, observer: Option<TimingObserver>) -> Self {
        self.timing_observer = observer;
        self
    }

    pub fn is_closed(&self) -> bool {
        self.closed.is_cancelled() || self.tx.is_closed()
    }

    /// Whether both handles use one connection (`with_target` copies do).
    pub fn same_connection(&self, other: &Client) -> bool {
        Arc::ptr_eq(&self.pending, &other.pending)
    }

    /// Discard a stale transport so cached clients cannot keep reusing it.
    pub fn close(&self) {
        self.closed.cancel();
    }

    /// Sends an envelope as is (id, presence and all) and tracks its call.
    /// The caller guarantees the id is unused on this connection.
    pub async fn forward(&self, mut envelope: Envelope) -> Result<Call, Error> {
        if self.is_closed() {
            return Err(Error::Closed);
        }
        let diagnostic_id = match &envelope.msg {
            Some(Msg::Request(Request { req: Some(req) })) => {
                muster_proto::diagnostics::request_id(req).map(str::to_owned)
            }
            _ => None,
        };
        let id = envelope.id;
        envelope.target = String::new();
        let permit = self.tx.reserve().await.map_err(|_| Error::Closed)?;
        let (events_tx, events_rx) = mpsc::channel(256);
        let (response_tx, response_rx) = oneshot::channel();
        self.pending.lock().unwrap().insert(
            id,
            Pending {
                diagnostic_id,
                timing_observer: self.timing_observer.clone(),
                timing_count: 0,
                events: events_tx,
                response: response_tx,
                target: String::new(),
            },
        );
        if self.is_closed() {
            self.pending.lock().unwrap().remove(&id);
            return Err(Error::Closed);
        }
        permit.send(envelope);
        Ok(Call {
            id,
            events: events_rx,
            response: response_rx,
            cancel_tx: self.tx.clone(),
            pending: self.pending.clone(),
            closed: self.closed.clone(),
            target: String::new(),
        })
    }

    pub async fn cancel_id(&self, id: u64) {
        let _ = self.tx.send(Envelope::cancel(id)).await;
    }

    /// Starts a call; events arrive on `Call::events`.
    pub async fn start(&self, req: v1::request::Req) -> Result<Call, Error> {
        let id = self.next_id.fetch_add(1, Ordering::Relaxed);
        self.start_with(id, req, Vec::new()).await
    }

    /// Reserves the id of a future call, so its presence message can be
    /// signed asynchronously (local authentication) before `start_presigned`.
    pub fn next_id(&self) -> u64 {
        self.next_id.fetch_add(1, Ordering::Relaxed)
    }

    /// Starts the call reserved by `next_id` with a presence proof made
    /// over `muster_proto::presence_message(id, &req)`.
    pub async fn start_presigned(
        &self,
        id: u64,
        req: v1::request::Req,
        presence: Vec<u8>,
    ) -> Result<Call, Error> {
        self.start_with(id, req, presence).await
    }

    /// Starts a call carrying a presence proof made by `sign` over the
    /// call's presence message (the caller has just authenticated locally).
    pub async fn start_signed(
        &self,
        req: v1::request::Req,
        sign: &dyn Fn(&[u8]) -> Vec<u8>,
    ) -> Result<Call, Error> {
        let id = self.next_id.fetch_add(1, Ordering::Relaxed);
        let presence = sign(&muster_proto::presence_message(id, &req));
        self.start_with(id, req, presence).await
    }

    async fn start_with(
        &self,
        id: u64,
        req: v1::request::Req,
        presence: Vec<u8>,
    ) -> Result<Call, Error> {
        if self.is_closed() {
            return Err(Error::Closed);
        }
        let diagnostic_id = muster_proto::diagnostics::request_id(&req).map(str::to_owned);
        let permit = self.tx.reserve().await.map_err(|_| Error::Closed)?;
        let (events_tx, events_rx) = mpsc::channel(256);
        let (response_tx, response_rx) = oneshot::channel();
        self.pending.lock().unwrap().insert(
            id,
            Pending {
                diagnostic_id,
                timing_observer: self.timing_observer.clone(),
                timing_count: 0,
                events: events_tx,
                response: response_tx,
                target: self.target.clone(),
            },
        );
        let mut envelope = Envelope::request(id, req);
        envelope.presence = presence;
        envelope.target = self.target.clone();
        if self.is_closed() {
            self.pending.lock().unwrap().remove(&id);
            return Err(Error::Closed);
        }
        permit.send(envelope);
        Ok(Call {
            id,
            events: events_rx,
            response: response_rx,
            cancel_tx: self.tx.clone(),
            pending: self.pending.clone(),
            closed: self.closed.clone(),
            target: self.target.clone(),
        })
    }

    /// A unary call.
    pub async fn call(&self, req: v1::request::Req) -> Result<v1::response::Resp, Error> {
        self.start(req).await?.finish().await
    }
}

/// Convenience for handlers: an error response.
pub fn err(code: v1::error::Code, message: impl Into<String>) -> v1::response::Resp {
    v1::response::Resp::Error(v1::Error {
        code: code as i32,
        message: message.into(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn timings_are_correlated_bounded_and_never_business_events() {
        let (a, mut peer) = tokio::io::duplex(65536);
        let observed = Arc::new(Mutex::new(Vec::new()));
        let sink = observed.clone();
        let client = Client::new(a)
            .with_timing_observer(Some(Arc::new(move |t| sink.lock().unwrap().push(t))));
        let diagnostic_id = "0123456789abcdef-1";
        let mut call = client
            .start(v1::request::Req::AgentWatch(v1::AgentWatchRequest {
                diagnostic_id: diagnostic_id.into(),
                ..Default::default()
            }))
            .await
            .unwrap();
        for i in 0..32 {
            peer.write_all(&frame(&Envelope::event(
                call.id,
                v1::event::Ev::RpcTiming(v1::RpcTiming {
                    phase: 4,
                    diagnostic_id: if i < 2 { "wrong" } else { diagnostic_id }.into(),
                    elapsed_us: i,
                    ..Default::default()
                }),
            )))
            .await
            .unwrap();
        }
        peer.write_all(&frame(&Envelope::event(
            call.id,
            v1::event::Ev::AgentEvent(v1::AgentEvent::default()),
        )))
        .await
        .unwrap();
        let event = tokio::time::timeout(std::time::Duration::from_secs(1), call.events.recv())
            .await
            .unwrap()
            .unwrap();
        assert!(matches!(event, v1::event::Ev::AgentEvent(_)));
        let got = observed.lock().unwrap();
        assert_eq!(got.len(), 16);
        assert!(got.iter().all(|t| t.diagnostic_id == diagnostic_id));
    }

    #[tokio::test]
    async fn stalled_watch_does_not_block_other_calls_and_cancels_its_target() {
        let (a, mut peer) = tokio::io::duplex(65536);
        let client = Client::new(a);
        let stalled = client
            .with_target("peer-a")
            .start(v1::request::Req::DeviceWatch(v1::DeviceWatchRequest {}))
            .await
            .unwrap();
        let unrelated = client
            .start(v1::request::Req::DeviceInfo(v1::DeviceInfoRequest {}))
            .await
            .unwrap();
        let mut other_watch = client
            .start(v1::request::Req::DeviceWatch(v1::DeviceWatchRequest {}))
            .await
            .unwrap();
        for i in 0..257 {
            peer.write_all(&frame(&Envelope::event(
                stalled.id,
                v1::event::Ev::DeviceState(v1::DeviceState {
                    uptime_secs: i,
                    ..Default::default()
                }),
            )))
            .await
            .unwrap();
        }
        // A late success must not overwrite the explicit overflow failure.
        peer.write_all(&frame(&Envelope::response(
            stalled.id,
            v1::response::Resp::Ack(v1::Ack {}),
        )))
        .await
        .unwrap();
        peer.write_all(&frame(&Envelope::response(
            unrelated.id,
            v1::response::Resp::DeviceInfo(v1::DeviceInfo {
                name: "unblocked".into(),
                ..Default::default()
            }),
        )))
        .await
        .unwrap();
        peer.write_all(&frame(&Envelope::event(
            other_watch.id,
            v1::event::Ev::DeviceState(v1::DeviceState::default()),
        )))
        .await
        .unwrap();
        assert!(
            tokio::time::timeout(std::time::Duration::from_secs(1), other_watch.events.recv())
                .await
                .expect("one full event queue blocked another watch")
                .is_some()
        );
        let response = tokio::time::timeout(std::time::Duration::from_secs(1), unrelated.finish())
            .await
            .expect("one full event queue blocked another call")
            .unwrap();
        assert!(
            matches!(response, v1::response::Resp::DeviceInfo(info) if info.name == "unblocked")
        );
        let stalled_id = stalled.id;
        assert!(matches!(stalled.finish().await,
            Err(Error::Remote { code: v1::error::Code::Unavailable, message })
                if message.contains("resynchronize")));
        assert!(!client.is_closed());
        let mut bytes = Vec::new();
        let cancel = tokio::time::timeout(std::time::Duration::from_secs(1), async {
            loop {
                while let Some(envelope) = unframe(&mut bytes).unwrap() {
                    if matches!(envelope.msg, Some(Msg::Cancel(_))) {
                        return envelope;
                    }
                }
                let mut chunk = [0; 4096];
                let n = peer.read(&mut chunk).await.unwrap();
                assert_ne!(n, 0);
                bytes.extend_from_slice(&chunk[..n]);
            }
        })
        .await
        .unwrap();
        assert_eq!(cancel.id, stalled_id);
        assert_eq!(cancel.target, "peer-a");
        client.close();
    }

    struct EchoHandler;

    impl Handler for EchoHandler {
        fn call(
            &self,
            ctx: Context,
            request: v1::request::Req,
            events: EventSink,
        ) -> HandlerFuture<'_> {
            Box::pin(async move {
                assert_eq!(ctx.principal, Principal::Local);
                if let v1::request::Req::NetworkRemove(_) = &request {
                    assert_eq!(ctx.presence, ctx.presence_message(&request));
                    return v1::response::Resp::Ack(v1::Ack {});
                }
                match request {
                    v1::request::Req::DeviceInfo(_) => {
                        v1::response::Resp::DeviceInfo(v1::DeviceInfo {
                            name: "test".into(),
                            ..Default::default()
                        })
                    }
                    v1::request::Req::DeviceWatch(_) => {
                        for i in 0..3u64 {
                            events
                                .send(v1::event::Ev::DeviceState(v1::DeviceState {
                                    uptime_secs: i,
                                    ..Default::default()
                                }))
                                .await
                                .unwrap();
                        }
                        v1::response::Resp::Ack(v1::Ack {})
                    }
                    _ => err(v1::error::Code::Invalid, "unsupported"),
                }
            })
        }
    }

    struct DelayedTarget {
        entered: Arc<tokio::sync::Notify>,
        release: Arc<tokio::sync::Notify>,
        forwarded: Arc<std::sync::atomic::AtomicUsize>,
    }

    #[tokio::test]
    async fn each_forwarded_request_owns_an_independent_stream() {
        let opened = Arc::new(std::sync::atomic::AtomicUsize::new(0));
        let handler = Arc::new(DelayedTarget {
            entered: Arc::new(tokio::sync::Notify::new()),
            release: Arc::new(tokio::sync::Notify::new()),
            forwarded: opened.clone(),
        });
        let (a, b) = tokio::io::duplex(65536);
        let server = tokio::spawn(serve(b, Principal::Local, handler));
        let client = Client::new(a).with_target("peer");
        for round in 0..3 {
            let (info, watch) = tokio::join!(
                client.call(v1::request::Req::DeviceInfo(v1::DeviceInfoRequest {})),
                client.call(v1::request::Req::DeviceWatch(v1::DeviceWatchRequest {}))
            );
            assert!(matches!(info.unwrap(), v1::response::Resp::DeviceInfo(_)));
            assert!(matches!(watch.unwrap(), v1::response::Resp::Ack(_)));
            assert_eq!(
                opened.load(Ordering::SeqCst),
                (round + 1) * 2,
                "each forwarded operation must own a service stream"
            );
        }
        client.close();
        server.abort();
    }

    impl Handler for DelayedTarget {
        fn call(
            &self,
            ctx: Context,
            req: v1::request::Req,
            events: EventSink,
        ) -> HandlerFuture<'_> {
            EchoHandler.call(ctx, req, events)
        }
        fn open_target(&self, target: &str) -> TargetFuture<'_> {
            let slow = target == "slow";
            let watch = target == "watch";
            Box::pin(async move {
                if slow {
                    self.entered.notify_one();
                    self.release.notified().await;
                }
                self.forwarded.fetch_add(1, Ordering::SeqCst);
                let (a, b) = tokio::io::duplex(65536);
                let handler: Arc<dyn Handler> = if watch {
                    Arc::new(WaitingWatch(self.forwarded.clone()))
                } else {
                    Arc::new(EchoHandler)
                };
                tokio::spawn(serve(b, Principal::Local, handler));
                Ok(Box::new(a) as Box<dyn Stream>)
            })
        }
    }
    /// A target whose link accepts frames and never answers (a half-dead path).
    struct MuteTarget {
        opened: Arc<std::sync::atomic::AtomicUsize>,
        held: Mutex<Vec<tokio::io::DuplexStream>>,
    }
    impl Handler for MuteTarget {
        fn call(
            &self,
            ctx: Context,
            req: v1::request::Req,
            events: EventSink,
        ) -> HandlerFuture<'_> {
            EchoHandler.call(ctx, req, events)
        }
        fn open_target(&self, _target: &str) -> TargetFuture<'_> {
            Box::pin(async move {
                self.opened.fetch_add(1, Ordering::SeqCst);
                let (a, b) = tokio::io::duplex(65536);
                self.held.lock().unwrap().push(b);
                Ok(Box::new(a) as Box<dyn Stream>)
            })
        }
    }
    #[tokio::test]
    async fn silent_target_link_is_not_reused_after_a_cancelled_call() {
        let opened = Arc::new(std::sync::atomic::AtomicUsize::new(0));
        let (a, b) = tokio::io::duplex(65536);
        let server = tokio::spawn(serve(
            b,
            Principal::Local,
            Arc::new(MuteTarget {
                opened: opened.clone(),
                held: Mutex::default(),
            }),
        ));
        let client = Client::new(a);
        let target = client.with_target("mute");
        let call = target
            .start(v1::request::Req::DeviceInfo(Default::default()))
            .await
            .unwrap();
        while opened.load(Ordering::SeqCst) == 0 {
            tokio::time::sleep(std::time::Duration::from_millis(10)).await;
        }
        tokio::time::sleep(std::time::Duration::from_millis(50)).await;
        call.cancel().await;
        let response = call.finish_raw().await.unwrap();
        assert!(matches!(
            response.resp,
            Some(v1::response::Resp::Error(ref e)) if e.code == v1::error::Code::Timeout as i32
        ));
        let again = target
            .start(v1::request::Req::DeviceInfo(Default::default()))
            .await
            .unwrap();
        tokio::time::sleep(std::time::Duration::from_millis(100)).await;
        assert_eq!(
            opened.load(Ordering::SeqCst),
            2,
            "the silent link was reused"
        );
        drop(again);
        client.close();
        let _ = server.await;
    }
    #[tokio::test]
    async fn slow_target_does_not_block_local_or_other_target_calls() {
        let entered = Arc::new(tokio::sync::Notify::new());
        let release = Arc::new(tokio::sync::Notify::new());
        let (a, b) = tokio::io::duplex(65536);
        let server = tokio::spawn(serve(
            b,
            Principal::Local,
            Arc::new(DelayedTarget {
                entered: entered.clone(),
                release: release.clone(),
                forwarded: Arc::default(),
            }),
        ));
        let client = Client::new(a);
        let slow = client
            .with_target("slow")
            .start(v1::request::Req::DeviceInfo(Default::default()))
            .await
            .unwrap();
        entered.notified().await;
        for target in ["", "healthy"] {
            let result = tokio::time::timeout(
                std::time::Duration::from_millis(200),
                client
                    .with_target(target)
                    .call(v1::request::Req::DeviceInfo(Default::default())),
            )
            .await;
            assert!(result.is_ok(), "a slow peer blocked target {target:?}");
            assert!(result.unwrap().is_ok());
        }
        slow.cancel().await;
        release.notify_one();
        client.close();
        tokio::time::timeout(std::time::Duration::from_secs(3), server)
            .await
            .unwrap()
            .unwrap();
    }
    fn delayed() -> Arc<DelayedTarget> {
        Arc::new(DelayedTarget {
            entered: Arc::default(),
            release: Arc::default(),
            forwarded: Arc::default(),
        })
    }

    #[tokio::test]
    async fn cancelled_connection_never_dispatches_and_can_be_reused() {
        let handler = delayed();
        let (a, b) = tokio::io::duplex(65536);
        let server = tokio::spawn(serve(b, Principal::Local, handler.clone()));
        let client = Client::new(a);
        let call = client
            .with_target("slow")
            .start_signed(v1::request::Req::NetworkRemove(Default::default()), &|m| {
                m.to_vec()
            })
            .await
            .unwrap();
        handler.entered.notified().await;
        call.cancel().await;
        let result = tokio::time::timeout(std::time::Duration::from_secs(1), call.finish())
            .await
            .unwrap();
        assert!(matches!(
            result,
            Err(Error::Remote {
                code: v1::error::Code::Timeout,
                ..
            })
        ));
        assert_eq!(handler.forwarded.load(Ordering::SeqCst), 0);
        handler.release.notify_one();
        let target = client.with_target("slow");
        let result = target.call(v1::request::Req::DeviceInfo(Default::default()));
        assert!(
            tokio::time::timeout(std::time::Duration::from_secs(1), result)
                .await
                .unwrap()
                .is_ok()
        );
        assert_eq!(handler.forwarded.load(Ordering::SeqCst), 1);
        client.close();
        server.await.unwrap();
    }

    #[tokio::test]
    async fn dropped_deadline_cancels_connection_setup() {
        let handler = delayed();
        let (a, b) = tokio::io::duplex(65536);
        let server = tokio::spawn(serve(b, Principal::Local, handler.clone()));
        let client = Client::new(a);
        let target = client.with_target("slow");
        assert!(
            tokio::time::timeout(
                std::time::Duration::from_millis(30),
                target.call(v1::request::Req::DeviceInfo(Default::default()))
            )
            .await
            .is_err()
        );
        assert!(client.pending.lock().unwrap().is_empty());
        // A local barrier proves the reader processed the preceding Cancel.
        client
            .call(v1::request::Req::DeviceInfo(Default::default()))
            .await
            .unwrap();
        client.close();
        tokio::time::timeout(std::time::Duration::from_secs(1), server)
            .await
            .unwrap()
            .unwrap();
        assert_eq!(handler.forwarded.load(Ordering::SeqCst), 0);
    }

    #[tokio::test]
    async fn disconnect_cancels_stalled_target_setup() {
        let handler = delayed();
        let (a, b) = tokio::io::duplex(65536);
        let server = tokio::spawn(serve(b, Principal::Local, handler.clone()));
        let client = Client::new(a);
        let call = client
            .with_target("slow")
            .start(v1::request::Req::DeviceInfo(Default::default()))
            .await
            .unwrap();
        handler.entered.notified().await;
        client.close();
        tokio::time::timeout(std::time::Duration::from_secs(1), server)
            .await
            .unwrap()
            .unwrap();
        assert!(call.finish().await.is_err());
        assert_eq!(handler.forwarded.load(Ordering::SeqCst), 0);
        // No worker retains the handler (and hence no deferred connection attempt).
        tokio::time::timeout(std::time::Duration::from_secs(1), async {
            while Arc::strong_count(&handler) != 1 {
                tokio::task::yield_now().await;
            }
        })
        .await
        .unwrap();
    }

    #[tokio::test]
    async fn concurrent_signed_calls_own_streams_and_preserve_proof() {
        let handler = delayed();
        let (a, b) = tokio::io::duplex(65536);
        tokio::spawn(serve(b, Principal::Local, handler.clone()));
        let client = Client::new(a);
        let mut tasks = tokio::task::JoinSet::new();
        for _ in 0..16 {
            let target = client.with_target("healthy");
            tasks.spawn(async move {
                let req = v1::request::Req::NetworkRemove(Default::default());
                let id = target.next_id();
                let proof = muster_proto::presence_message(id, &req);
                target
                    .start_presigned(id, req, proof)
                    .await
                    .unwrap()
                    .finish()
                    .await
                    .unwrap()
            });
        }
        while let Some(result) = tasks.join_next().await {
            assert!(matches!(result.unwrap(), v1::response::Resp::Ack(_)));
        }
        assert_eq!(handler.forwarded.load(Ordering::SeqCst), 16);
        client.close();
    }

    #[tokio::test]
    async fn remote_caller_cannot_open_proxy_target() {
        let handler = delayed();
        let (a, b) = tokio::io::duplex(65536);
        tokio::spawn(serve(
            b,
            Principal::Remote(DeviceId([1; 32])),
            handler.clone(),
        ));
        let client = Client::new(a);
        let result = client
            .with_target("healthy")
            .call(v1::request::Req::DeviceInfo(Default::default()))
            .await;
        assert!(matches!(
            result,
            Err(Error::Remote {
                code: v1::error::Code::Unauthorized,
                ..
            })
        ));
        assert_eq!(handler.forwarded.load(Ordering::SeqCst), 0);
        client.close();
    }

    struct WaitingWatch(Arc<std::sync::atomic::AtomicUsize>);
    impl Handler for WaitingWatch {
        fn call(
            &self,
            _ctx: Context,
            req: v1::request::Req,
            events: EventSink,
        ) -> HandlerFuture<'_> {
            Box::pin(async move {
                events
                    .send(v1::event::Ev::DeviceState(Default::default()))
                    .await
                    .unwrap();
                events.cancelled().cancelled().await;
                self.0.fetch_add(1, Ordering::SeqCst);
                match req {
                    v1::request::Req::AgentSend(_) => v1::response::Resp::OpResult(v1::OpResult {
                        status: v1::op_result::Status::Applied as i32,
                        result_id: "confirmed-provider-receipt".into(),
                        ..Default::default()
                    }),
                    _ => v1::response::Resp::Ack(Default::default()),
                }
            })
        }
    }

    #[tokio::test]
    async fn cancelled_forwarded_watch_reaches_handler_and_finishes() {
        let handler = delayed();
        let (a, b) = tokio::io::duplex(65536);
        tokio::spawn(serve(b, Principal::Local, handler.clone()));
        let client = Client::new(a);
        let mut call = client
            .with_target("watch")
            .start(v1::request::Req::DeviceWatch(Default::default()))
            .await
            .unwrap();
        assert!(call.events.recv().await.is_some());
        call.cancel().await;
        let result = tokio::time::timeout(std::time::Duration::from_secs(1), call.finish())
            .await
            .unwrap();
        assert!(matches!(result, Ok(v1::response::Resp::Ack(_))));
        tokio::time::timeout(std::time::Duration::from_secs(1), async {
            while handler.forwarded.load(Ordering::SeqCst) != 2 {
                tokio::task::yield_now().await;
            }
        })
        .await
        .unwrap();
        assert!(client.pending.lock().unwrap().is_empty());
        client.close();
    }

    #[tokio::test]
    async fn cancelled_forward_preserves_confirmed_mutation_receipt() {
        let handler = delayed();
        let (a, b) = tokio::io::duplex(65536);
        tokio::spawn(serve(b, Principal::Local, handler.clone()));
        let client = Client::new(a);
        let mut call = client
            .with_target("watch")
            .start(v1::request::Req::AgentSend(Default::default()))
            .await
            .unwrap();
        assert!(call.events.recv().await.is_some());
        call.cancel().await;
        let response = tokio::time::timeout(std::time::Duration::from_secs(1), call.finish())
            .await
            .unwrap()
            .unwrap();
        let v1::response::Resp::OpResult(receipt) = response else {
            panic!("lost confirmed receipt")
        };
        assert_eq!(receipt.status, v1::op_result::Status::Applied as i32);
        assert_eq!(receipt.result_id, "confirmed-provider-receipt");
        assert_eq!(handler.forwarded.load(Ordering::SeqCst), 2);
        assert!(client.pending.lock().unwrap().is_empty());
        client.close();
    }

    #[tokio::test]
    async fn stalled_calls_have_a_bounded_admission_limit() {
        let handler = delayed();
        let (a, b) = tokio::io::duplex(65536);
        let server = tokio::spawn(serve(b, Principal::Local, handler.clone()));
        let client = Client::new(a);
        let mut calls = Vec::new();
        for _ in 0..MAX_FORWARDED_CALLS {
            calls.push(
                client
                    .with_target("slow")
                    .start(v1::request::Req::DeviceInfo(Default::default()))
                    .await
                    .unwrap(),
            );
        }
        let target = client.with_target("slow");
        let result = tokio::time::timeout(
            std::time::Duration::from_secs(1),
            target.call(v1::request::Req::DeviceInfo(Default::default())),
        )
        .await
        .unwrap();
        assert!(matches!(
            result,
            Err(Error::Remote {
                code: v1::error::Code::Unavailable,
                ..
            })
        ));
        client.close();
        drop(calls);
        tokio::time::timeout(std::time::Duration::from_secs(1), server)
            .await
            .unwrap()
            .unwrap();
        assert_eq!(handler.forwarded.load(Ordering::SeqCst), 0);
    }

    struct ManyEvents;
    impl Handler for ManyEvents {
        fn call(
            &self,
            _ctx: Context,
            _req: v1::request::Req,
            events: EventSink,
        ) -> HandlerFuture<'_> {
            Box::pin(async move {
                for n in 0..300 {
                    events
                        .send(v1::event::Ev::DeviceState(v1::DeviceState {
                            uptime_secs: n,
                            ..Default::default()
                        }))
                        .await
                        .unwrap();
                }
                v1::response::Resp::Ack(Default::default())
            })
        }
    }
    #[tokio::test]
    async fn unary_completion_drains_more_than_one_event_buffer() {
        let (a, b) = tokio::io::duplex(65536);
        tokio::spawn(serve(b, Principal::Local, Arc::new(ManyEvents)));
        let client = Client::new(a);
        let result = tokio::time::timeout(
            std::time::Duration::from_millis(300),
            client.call(v1::request::Req::DeviceInfo(Default::default())),
        )
        .await;
        assert!(result.is_ok(), "unread events blocked unary completion");
        assert!(result.unwrap().is_ok());
        client.close();
    }

    #[tokio::test]
    async fn eof_cancels_owned_target_streams_and_pending_calls() {
        let (local, remote) = tokio::io::duplex(65536);
        let client = Client::new(local);
        let target = client.with_target("peer");
        let call = client
            .start(v1::request::Req::DeviceInfo(v1::DeviceInfoRequest {}))
            .await
            .unwrap();
        drop(remote);
        assert!(matches!(
            tokio::time::timeout(std::time::Duration::from_secs(1), call.finish())
                .await
                .unwrap(),
            Err(Error::Closed)
        ));
        assert!(client.is_closed());
        assert!(target.is_closed());
        assert!(matches!(
            target
                .start(v1::request::Req::DeviceInfo(v1::DeviceInfoRequest {}))
                .await,
            Err(Error::Closed)
        ));
    }

    #[tokio::test]
    async fn explicit_invalidation_cancels_pending_calls() {
        let (local, _remote) = tokio::io::duplex(65536);
        let client = Client::new(local);
        let call = client
            .start(v1::request::Req::DeviceInfo(v1::DeviceInfoRequest {}))
            .await
            .unwrap();
        client.close();
        assert!(client.is_closed());
        assert!(matches!(
            tokio::time::timeout(std::time::Duration::from_secs(1), call.finish())
                .await
                .unwrap(),
            Err(Error::Closed)
        ));
    }

    #[tokio::test]
    async fn unary_and_streaming_over_a_duplex_pipe() {
        let (client_side, server_side) = tokio::io::duplex(65536);
        tokio::spawn(serve(server_side, Principal::Local, Arc::new(EchoHandler)));
        let client = Client::new(client_side);
        match client
            .call(v1::request::Req::DeviceInfo(v1::DeviceInfoRequest {}))
            .await
            .unwrap()
        {
            v1::response::Resp::DeviceInfo(info) => assert_eq!(info.name, "test"),
            other => panic!("{other:?}"),
        }
        let mut call = client
            .start(v1::request::Req::DeviceWatch(v1::DeviceWatchRequest {}))
            .await
            .unwrap();
        let mut seen = Vec::new();
        while let Some(ev) = call.events.recv().await {
            if let v1::event::Ev::DeviceState(state) = ev {
                seen.push(state.uptime_secs);
            }
            if seen.len() == 3 {
                break;
            }
        }
        assert_eq!(seen, vec![0, 1, 2]);
        assert!(matches!(
            call.finish().await.unwrap(),
            v1::response::Resp::Ack(_)
        ));
        let call = client
            .start_signed(
                v1::request::Req::NetworkRemove(v1::NetworkRemoveRequest::default()),
                &|m| m.to_vec(),
            )
            .await
            .unwrap();
        assert!(matches!(
            call.finish().await.unwrap(),
            v1::response::Resp::Ack(_)
        ));
        let error = client
            .call(v1::request::Req::AgentList(v1::AgentListRequest::default()))
            .await
            .unwrap_err();
        assert!(matches!(
            error,
            Error::Remote {
                code: v1::error::Code::Invalid,
                ..
            }
        ));
    }
}
