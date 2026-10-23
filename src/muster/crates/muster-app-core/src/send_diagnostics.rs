//! Content-free client diagnostics. Never a delivery decision or retry source.
use crate::{Error, Shell};
use muster_proto::{
    Message, app,
    v1::{self, request::Req, response::Resp},
};
use std::{
    sync::Arc,
    time::{Instant, SystemTime, UNIX_EPOCH},
};

pub(crate) struct Trace {
    shell: Arc<dyn Shell>,
    op: Option<String>,
    target: String,
    operation: &'static str,
    attempt: String,
    started: Instant,
    phase_started: Instant,
    phase: &'static str,
}
impl Trace {
    pub fn new(shell: Arc<dyn Shell>, what: &'static str, device: &str, req: &Req) -> Self {
        let op = match req {
            Req::AgentSend(r) => Some(r.op_id.clone()),
            Req::AgentRespond(r) => Some(r.op_id.clone()),
            Req::LockedUseExecute(r) if matches!(what, "send" | "respond" | "terminal_result") => {
                muster_core::locked_use::Challenge::decode(&r.challenge)
                    .ok()
                    .map(|c| c.turn().to_owned())
            }
            _ => None,
        }
        .filter(|id| {
            !id.is_empty()
                && id.len() <= 128
                && id
                    .bytes()
                    .all(|b| b.is_ascii_alphanumeric() || b"-_.:".contains(&b))
        });
        let operation = match what {
            "send" => "send",
            "respond" => "respond",
            "terminal_result" => "terminal_result",
            _ => "other",
        };
        let now = Instant::now();
        let trace = Self {
            shell,
            op,
            target: crate::cache::Cache::key(&[device])[..16].into(),
            operation,
            attempt: format!(
                "{}",
                SystemTime::now()
                    .duration_since(UNIX_EPOCH)
                    .unwrap_or_default()
                    .as_nanos()
            ),
            started: now,
            phase_started: now,
            phase: "created",
        };
        trace.record("entered", None, None);
        trace
    }
    pub fn stage(&mut self, phase: &'static str) {
        self.record("stage_finished", None, None);
        self.phase = phase;
        self.phase_started = Instant::now();
        self.record("entered", None, None);
    }
    pub fn event(&self, event: &'static str) {
        self.record(event, None, None);
    }
    pub fn rejection(&self, code: &'static str) {
        self.record("rejected_before_dispatch", Some(code), None);
    }
    pub fn failure(&self, error: &Error) {
        let code = match error {
            Error::Invalid(_) => "invalid",
            Error::Unavailable(_) => "unavailable",
            Error::Unauthorized(_) => "unauthorized",
            Error::Rpc(_) => "rpc",
            Error::Busy(_) => "busy",
        };
        self.record("failed", Some(code), None);
    }
    pub async fn call(
        &mut self,
        client: &muster_rpc::Client,
        request: Req,
        phase: &'static str,
    ) -> Result<Resp, muster_rpc::Error> {
        self.stage(phase);
        let result = client.call(request).await;
        match &result {
            Err(muster_rpc::Error::Remote { code, .. }) => {
                self.record("rpc_error", Some("remote"), Some(*code as i32))
            }
            Err(muster_rpc::Error::Io(_)) => self.record("rpc_error", Some("io"), None),
            Err(muster_rpc::Error::Protocol(_)) => self.record("rpc_error", Some("protocol"), None),
            Err(muster_rpc::Error::Closed) => self.record("rpc_error", Some("closed"), None),
            Ok(Resp::OpResult(r)) => self.record(
                match v1::op_result::Status::try_from(r.status) {
                    Ok(v1::op_result::Status::Applied) => "applied",
                    Ok(v1::op_result::Status::Duplicate) => "duplicate",
                    Ok(v1::op_result::Status::Rejected) => "rejected",
                    _ => "unknown",
                },
                None,
                None,
            ),
            Ok(_) => self.record("rpc_response", None, None),
        }
        result
    }
    fn record(&self, event: &'static str, error: Option<&str>, remote_code: Option<i32>) {
        let Some(op) = &self.op else { return };
        let data = serde_json::to_vec(&serde_json::json!({
            "record_type":"send_diagnostic", "schema_version":1,
            "timestamp":SystemTime::now().duration_since(UNIX_EPOCH).unwrap_or_default().as_secs_f64(),
            "op_id":op, "attempt_id":self.attempt, "target_ref":self.target,
            "operation":self.operation, "phase":self.phase, "event":event,
            "elapsed_ms":self.started.elapsed().as_millis() as u64,
            "phase_elapsed_ms":self.phase_started.elapsed().as_millis() as u64,
            "error_category":error, "remote_error_code":remote_code,
        })).expect("fixed diagnostic schema");
        // ID zero is reserved for fire-and-forget telemetry. No product effect
        // bookkeeping, wait, acknowledgement or retry depends on its storage.
        self.shell.on_effect(
            0,
            &app::Effect {
                ef: Some(app::effect::Ef::SendDiagnostic(app::SendDiagnostic {
                    json: data,
                })),
            }
            .encode_to_vec(),
        );
    }
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;
    use std::sync::Mutex;
    #[derive(Default)]
    pub struct Capture(pub Mutex<Vec<serde_json::Value>>);
    impl Shell for Capture {
        fn on_view(&self, _: &[u8]) {}
        fn hw_public_key(&self) -> Option<Vec<u8>> {
            None
        }
        fn hw_sign(&self, _: &[u8]) -> Option<Vec<u8>> {
            None
        }
        fn on_effect(&self, id: u64, bytes: &[u8]) {
            let effect = app::Effect::decode(bytes).unwrap();
            if let Some(app::effect::Ef::SendDiagnostic(row)) = effect.ef {
                assert_eq!(id, 0);
                assert!(row.json.len() < 4096);
                self.0
                    .lock()
                    .unwrap()
                    .push(serde_json::from_slice(&row.json).unwrap());
            }
        }
    }
    #[test]
    fn privacy_identity_and_monotonic_phase_measurements() {
        let sink = Arc::new(Capture::default());
        let request = Req::AgentSend(v1::AgentSendRequest {
            op_id: "app-123-1".into(),
            text: "secret-text".into(),
            session: "private-session".into(),
            release_unconfirmed: "private-token".into(),
            ..Default::default()
        });
        let mut trace = Trace::new(sink.clone(), "send", "private-device", &request);
        trace.stage("connect");
        trace.failure(&Error::Unavailable("secret-error-address".into()));
        let rows = sink.0.lock().unwrap();
        let text = serde_json::to_string(&*rows).unwrap();
        for secret in [
            "secret-text",
            "private-session",
            "private-token",
            "private-device",
            "secret-error-address",
        ] {
            assert!(!text.contains(secret));
        }
        assert!(rows.iter().all(|r| r["op_id"] == "app-123-1"));
        assert_eq!(rows.last().unwrap()["error_category"], "unavailable");
        assert_eq!(rows.last().unwrap()["phase"], "connect");
        assert!(
            rows.windows(2)
                .all(|w| w[0]["elapsed_ms"].as_u64() <= w[1]["elapsed_ms"].as_u64())
        );
    }
    #[test]
    fn invalid_ids_cannot_inject_content_and_challenge_only_exports_turn() {
        let sink = Arc::new(Capture::default());
        for id in ["bad\ncontent", "", &"x".repeat(129)] {
            Trace::new(
                sink.clone(),
                "send",
                "device",
                &Req::AgentSend(v1::AgentSendRequest {
                    op_id: id.into(),
                    ..Default::default()
                }),
            );
        }
        assert!(sink.0.lock().unwrap().is_empty());
        let mut bytes = muster_core::canonical::Writer::new();
        bytes
            .u8(1)
            .fixed(&[1; 32])
            .fixed(&[2; 32])
            .fixed(&[3; 32])
            .fixed(&[4; 32])
            .fixed(&[5; 32])
            .u32(501)
            .u32(1)
            .str("app-123-1")
            .fixed(&[6; 32])
            .u64(10000);
        let request = Req::LockedUseExecute(v1::LockedUseExecuteRequest {
            challenge: bytes.finish(),
            signature: b"secret-signature".to_vec(),
        });
        let mut trace = Trace::new(sink.clone(), "send", "device", &request);
        trace.stage("authentication_sign");
        trace.event("authenticated");
        let rows = sink.0.lock().unwrap();
        assert!(!rows.is_empty());
        assert!(rows.iter().all(|r| r["op_id"] == "app-123-1"));
        let text = serde_json::to_string(&*rows).unwrap();
        assert!(!text.contains("signature"));
        assert!(!text.contains("challenge"));
    }
}
