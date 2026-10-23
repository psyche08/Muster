//! Read-only settlement after the owner has returned its result. One-shot
//! owners return only after their OS-work barrier, relock and cover cleanup.
//! This holds no console permit and never calls a controller or ledger.begin.
use super::*;
use muster_proto::Message;

#[derive(Clone)]
pub(super) struct ReceiptWait {
    daemon: Weak<Daemon>,
    peer: DeviceId,
    request: v1::AgentSendRequest,
    hash: [u8; 32],
}
impl ReceiptWait {
    pub fn new(daemon: &Arc<Daemon>, peer: DeviceId, req: &Req) -> Option<Self> {
        let Req::AgentSend(request) = req else {
            return None;
        };
        if request.op_id.is_empty()
            || muster_agent::desktop_chat::app_of(&request.session).is_some()
        {
            return None;
        }
        Some(Self {
            daemon: Arc::downgrade(daemon),
            peer,
            hash: muster_store::Ledger::body_hash(&request.encode_to_vec()),
            request: request.clone(),
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
        log::info!(
            "Agent receipt op={:?}: native work ended; waiting read-only",
            self.request.op_id
        );
        loop {
            if cancel.is_cancelled() {
                return result;
            }
            let Some(daemon) = self.daemon.upgrade() else {
                return result;
            };
            // Reading an outcome must not survive membership/provider revocation.
            let permission = crate::handler::locked_operation_permission(
                &daemon,
                &Req::AgentSend(self.request.clone()),
            )?;
            if muster_core::authorize(
                daemon.roster().read().unwrap().state(),
                &self.peer,
                &permission,
                &daemon.policy().read().unwrap(),
                &muster_core::TargetState { locked: false },
            ) != muster_core::Decision::Allow
            {
                return Err(Error::Unauthorized("Send receipt access revoked".into()));
            }
            if let Some(bytes) = daemon.store().ledger().confirmed_result(
                &self.request.op_id,
                "agent.send",
                &self.hash,
            )? {
                let session = String::from_utf8(bytes)
                    .map_err(|_| Error::Unavailable("Invalid send receipt".into()))?;
                log::info!(
                    "Agent receipt op={:?}: confirmed after native cleanup elapsed_ms={}",
                    self.request.op_id,
                    started.elapsed().as_millis()
                );
                return Ok(Resp::OpResult(v1::OpResult {
                    status: v1::op_result::Status::Applied as i32,
                    result_id: session,
                    ..Default::default()
                }));
            }
            if started.elapsed() >= Duration::from_secs(18) {
                return result;
            }
            tokio::select! {
                _ = cancel.cancelled() => return result,
                _ = tokio::time::sleep(Duration::from_millis(100)) => {}
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
            request: request.clone(),
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
