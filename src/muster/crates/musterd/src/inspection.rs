//! Local CLI bridge for one signed, read-only inspection on an enrolled peer.
use crate::{Daemon, Error};
use muster_core::{DeviceId, locked_use::Challenge};
use muster_proto::v1::{self, request::Req, response::Resp};
use std::{sync::Arc, time::Duration};

#[derive(serde::Deserialize)]
#[serde(deny_unknown_fields)]
struct Request {
    target: String,
    session: String,
    op_id: String,
}

pub async fn inspect(
    daemon: &Arc<Daemon>,
    json: &str,
    cancel: &tokio_util::sync::CancellationToken,
) -> Result<Resp, Error> {
    let r: Request = serde_json::from_str(json).map_err(|e| Error::Invalid(e.to_string()))?;
    let target = DeviceId::from_hex(&r.target)?;
    if target == daemon.device_id() || r.session.is_empty() || r.session.len() > 256 {
        return Err(Error::Invalid(
            "inspection requires a remote target and bounded session".into(),
        ));
    }
    let operation =
        muster_proto::locked_use::operation(&Req::AgentInspect(v1::AgentInspectRequest {
            session: r.session,
            op_id: r.op_id,
        }))
        .unwrap();
    let turn = muster_proto::locked_use::turn(&operation)
        .ok_or_else(|| Error::Invalid("invalid inspection operation id".into()))?;
    let digest = muster_proto::locked_use::digest(&operation);
    let network = daemon
        .roster()
        .read()
        .unwrap()
        .state()
        .network_id
        .ok_or(Error::NoNetwork)?
        .0;
    let (send, recv) = daemon
        .node()
        .open(target, crate::SERVICE, crate::SERVICE_VERSION)
        .await?;
    let client = muster_rpc::Client::new(tokio::io::join(recv, send));
    let prepared = tokio::time::timeout(
        Duration::from_secs(8),
        client.call(Req::LockedUsePrepare(v1::LockedUsePrepareRequest {
            operation: Some(operation.clone()),
        })),
    )
    .await
    .map_err(|_| Error::Timeout("inspection preparation timed out".into()))??;
    let Resp::LockedUseChallenge(prepared) = prepared else {
        return Err(Error::Unavailable(
            "target did not issue an inspection challenge".into(),
        ));
    };
    let challenge = Challenge::decode(&prepared.challenge)?;
    let message =
        challenge.signing_message_for(&network, target, daemon.device_id(), turn, &digest)?;
    let signer = daemon.signer().clone();
    let signature = tokio::task::spawn_blocking(move || signer.sign(&message))
        .await
        .map_err(|e| Error::Unavailable(e.to_string()))??;
    let mut call = client
        .start(Req::LockedUseExecute(v1::LockedUseExecuteRequest {
            challenge: prepared.challenge,
            signature,
        }))
        .await?;
    // Keep the stream until cleanup finishes. Cancellation is forwarded and
    // the target owner retains its watchdog until lock is observed.
    tokio::select! {
        _ = cancel.cancelled() => {
            call.cancel().await;
            Err(Error::Unavailable("inspection cancelled; target is relocking".into()))
        }
        _ = tokio::time::sleep(Duration::from_secs(25)) => {
            call.cancel().await;
            Err(Error::Timeout("inspection timed out; target is relocking".into()))
        }
        _ = async { while call.events.recv().await.is_some() {} } => Ok(call.finish().await?),
    }
}
