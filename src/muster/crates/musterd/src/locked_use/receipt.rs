//! Read-only settlement after the owner has returned its result. One-shot
//! owners return only after their OS-work barrier, relock and cover cleanup.
//! This holds no console permit and never calls a controller or ledger.begin.
//! A send settles on its confirmed ledger row. A desktop session creation
//! also settles on its transcript receipt, the unique new session showing
//! its first message, which it records in its own ledger row (ADR 0150).
use super::*;
use muster_proto::Message;

#[derive(Clone)]
enum Awaited {
    Send(v1::AgentSendRequest),
    /// `lookup` names the directory as transcripts do; `baseline` holds the
    /// sessions already there, which cannot be this creation's receipt.
    Create {
        request: v1::AgentNativeCreateRequest,
        lookup: v1::AgentNativeCreateRequest,
        baseline: Vec<String>,
    },
}

impl Awaited {
    fn op_id(&self) -> &str {
        match self {
            Awaited::Send(r) => &r.op_id,
            Awaited::Create { request, .. } => &request.op_id,
        }
    }

    fn kind(&self) -> &'static str {
        match self {
            Awaited::Send(_) => "agent.send",
            Awaited::Create { .. } => "agent.native_create",
        }
    }

    fn request(&self) -> Req {
        match self {
            Awaited::Send(r) => Req::AgentSend(r.clone()),
            Awaited::Create { request, .. } => Req::AgentNativeCreate(request.clone()),
        }
    }

    /// How long, and how often, the receipt is looked for. A new session
    /// gets the scheduler's read-only window (ADR 0097).
    fn wait(&self) -> (Duration, Duration) {
        match self {
            Awaited::Send(_) => (Duration::from_secs(18), Duration::from_millis(100)),
            Awaited::Create { .. } => (crate::scheduler::AFTER_RECEIPT, Duration::from_millis(500)),
        }
    }
}

#[derive(Clone)]
pub(super) struct ReceiptWait {
    daemon: Weak<Daemon>,
    peer: DeviceId,
    awaited: Awaited,
    hash: [u8; 32],
}
impl ReceiptWait {
    pub fn new(daemon: &Arc<Daemon>, peer: DeviceId, req: &Req) -> Option<Self> {
        let (awaited, body) = match req {
            Req::AgentSend(request) => {
                if request.op_id.is_empty()
                    || muster_agent::desktop_chat::app_of(&request.session).is_some()
                {
                    return None;
                }
                (Awaited::Send(request.clone()), request.encode_to_vec())
            }
            Req::AgentNativeCreate(request) if !request.op_id.is_empty() => {
                // Taken before the creation runs, so no session it makes is in it.
                let directory = std::fs::canonicalize(&request.directory)
                    .ok()?
                    .to_str()?
                    .to_owned();
                let lookup = v1::AgentNativeCreateRequest {
                    directory,
                    ..request.clone()
                };
                let baseline =
                    crate::native_create::baseline(daemon, &lookup.provider, &lookup.directory);
                (
                    Awaited::Create {
                        request: request.clone(),
                        lookup,
                        baseline,
                    },
                    request.encode_to_vec(),
                )
            }
            _ => return None,
        };
        Some(Self {
            daemon: Arc::downgrade(daemon),
            peer,
            hash: muster_store::Ledger::body_hash(&body),
            awaited,
        })
    }

    pub fn peer_matches(&self, peer: DeviceId) -> bool {
        self.peer == peer
    }

    pub async fn settle(
        self,
        result: Result<Resp, Error>,
        cancel: CancellationToken,
    ) -> Result<Resp, Error> {
        if !matches!(&result, Ok(Resp::OpResult(r)) if r.status == v1::op_result::Status::Unknown as i32)
        {
            return result;
        }
        let started = Instant::now();
        let (op_id, kind) = (self.awaited.op_id(), self.awaited.kind());
        let (budget, interval) = self.awaited.wait();
        log::info!("Agent receipt op={op_id:?}: native work ended; waiting read-only");
        loop {
            if cancel.is_cancelled() {
                return result;
            }
            let Some(daemon) = self.daemon.upgrade() else {
                return result;
            };
            // Reading an outcome must not survive membership/provider revocation.
            let permission =
                crate::handler::locked_operation_permission(&daemon, &self.awaited.request())?;
            if muster_core::authorize(
                daemon.roster().read().unwrap().state(),
                &self.peer,
                &permission,
                &daemon.policy().read().unwrap(),
                &muster_core::TargetState { locked: false },
            ) != muster_core::Decision::Allow
            {
                return Err(Error::Unauthorized(match self.awaited {
                    Awaited::Send(_) => "Send receipt access revoked".into(),
                    Awaited::Create { .. } => "Creation receipt access revoked".into(),
                }));
            }
            let ledger = daemon.store().ledger();
            let mut confirmed = ledger.confirmed_result(op_id, kind, &self.hash)?;
            if confirmed.is_none()
                && let Awaited::Create {
                    lookup, baseline, ..
                } = &self.awaited
                // Only a creation that reached its still undecided ledger row
                // can have a receipt.
                && matches!(ledger.status(op_id)?.as_deref(), Some("pending" | "unknown"))
            {
                match crate::native_create::receipt(&daemon, lookup, baseline).await {
                    Ok(Some(session)) => {
                        if ledger
                            .confirm(op_id, kind, &self.hash, session.as_bytes())
                            .is_ok()
                        {
                            confirmed = Some(session.into_bytes());
                        }
                    }
                    Ok(None) => {}
                    Err(error) => {
                        log::warn!("Agent receipt op={op_id:?}: not binding a receipt: {error}");
                        return result;
                    }
                }
            }
            if let Some(bytes) = confirmed {
                let session = String::from_utf8(bytes)
                    .map_err(|_| Error::Unavailable("Invalid receipt".into()))?;
                log::info!(
                    "Agent receipt op={op_id:?}: confirmed after native cleanup elapsed_ms={}",
                    started.elapsed().as_millis()
                );
                return Ok(Resp::OpResult(v1::OpResult {
                    status: v1::op_result::Status::Applied as i32,
                    result_id: session,
                    ..Default::default()
                }));
            }
            if started.elapsed() >= budget {
                return result;
            }
            tokio::select! {
                _ = cancel.cancelled() => return result,
                _ = tokio::time::sleep(interval) => {}
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use muster_agent::desktop_chat::{ChatApp, ChatKey};

    #[tokio::test]
    async fn receipt_wait_is_read_only_after_cleanup_and_bound_to_original_request() {
        let (_dir, daemon, owner, _os, c, _key, peer, job) = super::super::tests::start().await;
        owner.cancel(peer, c.nonce(), None).await.unwrap();
        job.await.unwrap();
        assert!(owner.writes.clone().try_write_owned().is_ok());
        let session = daemon
            .agent()
            .unwrap()
            .chat_observed(
                ChatApp::ChatGpt,
                None,
                ChatKey::Id("receipt-fixture".into()),
                "Receipt fixture",
                Vec::new(),
            )
            .unwrap();
        let request = v1::AgentSendRequest {
            session,
            text: "synthetic receipt".into(),
            op_id: "receipt-1".into(),
            ..Default::default()
        };
        let body = request.encode_to_vec();
        let hash = muster_store::Ledger::body_hash(&body);
        daemon
            .store()
            .ledger()
            .begin(&request.op_id, "agent.send", &body)
            .unwrap();
        let wait = ReceiptWait {
            daemon: Arc::downgrade(&daemon),
            peer,
            awaited: Awaited::Send(request.clone()),
            hash,
        };
        let receipt = tokio::spawn(wait.clone().settle(
            Ok(unknown_result("pending".into())),
            CancellationToken::new(),
        ));
        tokio::time::sleep(Duration::from_millis(120)).await;
        assert!(!receipt.is_finished());
        assert!(owner.current.lock().unwrap().is_none());
        assert!(owner.writes.clone().try_write_owned().is_ok());
        assert!(
            daemon
                .store()
                .ledger()
                .confirm(&request.op_id, "agent.send", &[0; 32], b"wrong")
                .is_err()
        );
        daemon
            .store()
            .ledger()
            .confirm(
                &request.op_id,
                "agent.send",
                &hash,
                request.session.as_bytes(),
            )
            .unwrap();
        let Resp::OpResult(result) = receipt.await.unwrap().unwrap() else {
            panic!()
        };
        assert_eq!(result.status, v1::op_result::Status::Applied as i32);
        assert_eq!(result.result_id, request.session);
        assert!(matches!(
            daemon
                .store()
                .ledger()
                .begin(&request.op_id, "agent.send", &body)
                .unwrap(),
            muster_store::Begin::Duplicate(_)
        ));
        let cancel = CancellationToken::new();
        cancel.cancel();
        let Resp::OpResult(result) = wait
            .clone()
            .settle(Ok(unknown_result("pending".into())), cancel)
            .await
            .unwrap()
        else {
            panic!()
        };
        assert_eq!(result.status, v1::op_result::Status::Unknown as i32);
        // A read permission check still runs even when the receipt is already durable.
        daemon.policy().write().unwrap().offer_agent = false;
        assert!(
            wait.settle(
                Ok(unknown_result("pending".into())),
                CancellationToken::new()
            )
            .await
            .is_err()
        );
        daemon.shutdown().await;
    }
}
