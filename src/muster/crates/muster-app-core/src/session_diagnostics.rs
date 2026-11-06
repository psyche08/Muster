//! Bounded, content-free observations; never a source of product state.
use crate::Shell;
use muster_proto::{Message, app, v1};
use serde_json::{Value, json};
use std::sync::{
    Arc,
    atomic::{AtomicBool, AtomicU8, AtomicU64, AtomicUsize, Ordering},
};
use std::time::{Instant, SystemTime, UNIX_EPOCH};

#[derive(Clone)]
pub(crate) struct Trace(Arc<Inner>);
struct Inner {
    id: String,
    target: String,
    session: String,
    started: Instant,
    shell: Arc<dyn Shell>,
    records: AtomicUsize,
    native: AtomicU8,
    published: AtomicU8,
    history_ready: AtomicBool,
}
pub(crate) fn emit(shell: &dyn Shell, mut row: Value) {
    row["schema_version"] = json!(1);
    row["timestamp"] = json!(
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs_f64()
    );
    shell.on_effect(
        0,
        &app::Effect {
            ef: Some(app::effect::Ef::SendDiagnostic(app::SendDiagnostic {
                json: serde_json::to_vec(&row).expect("fixed diagnostic fields"),
            })),
        }
        .encode_to_vec(),
    );
}
impl Trace {
    pub fn new(shell: Arc<dyn Shell>, device: &str, session: &str) -> Self {
        static NEXT: AtomicU64 = AtomicU64::new(1);
        let id = format!(
            "{:x}-{:x}",
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap_or_default()
                .as_nanos(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        );
        let trace = Self(Arc::new(Inner {
            id,
            target: crate::cache::Cache::key(&[device])[..16].into(),
            session: crate::cache::Cache::key(&[device, session])[..16].into(),
            started: Instant::now(),
            shell,
            records: AtomicUsize::new(0),
            native: AtomicU8::new(0),
            published: AtomicU8::new(0),
            history_ready: AtomicBool::new(false),
        }));
        trace.record("entered", json!({}));
        trace
    }
    pub fn history_ready(&self) -> bool {
        self.0.history_ready.load(Ordering::Relaxed)
    }
    pub fn history_applied(&self) {
        self.0.history_ready.store(true, Ordering::Relaxed);
    }
    pub fn id(&self) -> &str {
        &self.0.id
    }
    pub fn record(&self, event: &'static str, fields: Value) {
        if self.0.records.fetch_add(1, Ordering::Relaxed) >= 128 {
            return;
        }
        let mut row = json!({"record_type":"session_load", "trace_id":self.0.id,
            "target_ref":self.0.target,"session_ref":self.0.session,"event":event,
            "elapsed_us":self.0.started.elapsed().as_micros() as u64});
        if let (Some(row), Some(fields)) = (row.as_object_mut(), fields.as_object()) {
            row.extend(fields.clone());
        }
        emit(self.0.shell.as_ref(), row);
    }
    pub fn observer(&self, request: String) -> muster_rpc::TimingObserver {
        let trace = self.clone();
        Arc::new(move |timing| {
            let Ok(phase) = v1::rpc_timing::Phase::try_from(timing.phase) else {
                return;
            };
            trace.record("rpc_timing", json!({"request":request,"phase":phase.as_str_name(),
                "source_elapsed_us":timing.elapsed_us,"reused_connection":timing.reused_connection,
                "source":if matches!(phase,v1::rpc_timing::Phase::ServerReceived|v1::rpc_timing::Phase::ServerFinished) {"target"} else {"transport"}}));
        })
    }
    pub fn published(&self, ready: bool, bytes: usize) {
        let bit = if ready { 2 } else { 1 };
        if self.0.published.fetch_or(bit, Ordering::Relaxed) & bit == 0 {
            self.record(
                "view_published",
                json!({"history_ready":ready,"bytes":bytes}),
            );
        }
    }
    pub fn native(&self, ack: &app::SessionViewApplied) {
        if ack.trace_id != self.id() {
            return;
        }
        let bit = 1 << (u8::from(ack.history_ready) * 2 + u8::from(!ack.decoded_only));
        if self.0.native.fetch_or(bit, Ordering::Relaxed) & bit != 0 {
            return;
        }
        self.record(if ack.decoded_only {"native_decoded"} else {"native_applied"},
            json!({"history_ready":ack.history_ready,"decode_us":ack.decode_us,"main_queue_us":ack.queue_us,"apply_us":ack.apply_us}));
    }
}
pub(crate) fn open_timing(timing: muster_net::node::OpenTiming) -> v1::RpcTiming {
    use muster_net::node::OpenPhase;
    v1::RpcTiming {
        phase: match timing.phase {
            OpenPhase::PoolReady => 1,
            OpenPhase::StreamAllocated => 2,
            OpenPhase::ServiceReady => 3,
        },
        elapsed_us: timing.elapsed_us,
        reused_connection: timing.reused,
        diagnostic_id: String::new(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[derive(Default)]
    struct Sink(std::sync::Mutex<Vec<Value>>);
    impl Shell for Sink {
        fn on_view(&self, _: &[u8]) {}
        fn on_effect(&self, _: u64, bytes: &[u8]) {
            if let Some(app::effect::Ef::SendDiagnostic(d)) = app::Effect::decode(bytes).unwrap().ef
            {
                self.0
                    .lock()
                    .unwrap()
                    .push(serde_json::from_slice(&d.json).unwrap());
            }
        }
        fn hw_public_key(&self) -> Option<Vec<u8>> {
            None
        }
        fn hw_sign(&self, _: &[u8]) -> Option<Vec<u8>> {
            None
        }
    }
    #[test]
    fn bounded_private_records_and_once_only_matching_native_ack() {
        let sink = Arc::new(Sink::default());
        let trace = Trace::new(
            sink.clone(),
            "synthetic-device-secret",
            "synthetic-session-secret",
        );
        assert!(!trace.history_ready());
        trace.history_applied();
        assert!(trace.history_ready());
        let next = Trace::new(sink.clone(), "another-device", "another-session");
        assert!(!next.history_ready());
        sink.0.lock().unwrap().pop();
        let mut ack = app::SessionViewApplied {
            trace_id: "stale".into(),
            ..Default::default()
        };
        trace.native(&ack);
        assert_eq!(sink.0.lock().unwrap().len(), 1);
        ack.trace_id = trace.id().into();
        trace.native(&ack);
        trace.native(&ack);
        assert_eq!(sink.0.lock().unwrap().len(), 2);
        for _ in 0..200 {
            trace.record("retry", json!({}));
        }
        let rows = sink.0.lock().unwrap();
        assert_eq!(rows.len(), 128);
        let text = serde_json::to_string(&*rows).unwrap();
        assert!(
            !text.contains("synthetic-device-secret") && !text.contains("synthetic-session-secret")
        );
    }
}
