//! Explicit host-owned cover. Remote state below is display-only; the Mac
//! validates a bound signature and each later operation's current grants.
use super::*;

pub(crate) fn views(state: &State) -> Vec<app::DeviceLockedUseView> {
    state
        .network
        .iter()
        .flat_map(|n| &n.members)
        .filter(|m| !m.is_self && m.kind == "desktop")
        .filter_map(|m| {
            let info = state.manual_devices.get(&m.device_id);
            if info.is_some_and(|i| i.platform != "macos") {
                return None;
            }
            let device = info.and_then(|i| i.state.as_ref());
            let active = device.is_some_and(|s| s.manual_locked_use_active);
            let own = device.is_some_and(|s| s.manual_locked_use_owner == state.self_id);
            let supported =
                info.is_some_and(|i| i.services.iter().any(|s| s == "locked.device.v1"));
            let busy = state.manual_busy.contains(&m.device_id);
            let foreign_owner = device
                .filter(|s| !s.manual_locked_use_owner.is_empty() && !own)
                .map(|s| {
                    state
                        .network
                        .as_ref()
                        .and_then(|n| {
                            n.members
                                .iter()
                                .find(|m| m.device_id == s.manual_locked_use_owner)
                        })
                        .map(|m| m.name.as_str())
                        .filter(|name| !name.is_empty())
                        .unwrap_or("另一台设备")
                });
            Some(app::DeviceLockedUseView {
                device_id: m.device_id.clone(),
                on: active,
                busy,
                enabled: m.connected
                    && supported
                    && !busy
                    && device.is_some_and(|s| s.manual_locked_use_owner.is_empty() || own),
                status: if let Some(owner) = foreign_owner {
                    format!("Locked Use 由 {owner} 持有；请先在该设备结束，再从本设备开启")
                } else {
                    if !m.connected {
                        "设备离线"
                    } else if busy {
                        "正在切换…"
                    } else if info.is_none() {
                        "正在读取状态…"
                    } else if !supported {
                        "请更新 Mac 上的 Muster"
                    } else if active {
                        "遮罩下常驻 · 关闭或本地接管后锁屏"
                    } else if device.is_some_and(|s| !s.manual_locked_use_owner.is_empty()) {
                        "正在开启或锁屏…"
                    } else {
                        "独立于 Accessibility · 开启后保持遮罩"
                    }
                    .into()
                },
            })
        })
        .collect()
}

pub(super) fn watch(state: &mut State, ctx: &Ctx) {
    let ids: Vec<_> = state
        .network
        .iter()
        .flat_map(|n| &n.members)
        .filter(|m| !m.is_self && m.kind == "desktop" && m.connected)
        .map(|m| m.device_id.clone())
        .collect();
    state.manual_watches.retain(|id, _| ids.contains(id));
    state.manual_devices.retain(|id, _| ids.contains(id));
    for id in ids {
        if state.manual_watches.contains_key(&id) {
            continue;
        }
        let (watch, token) = state.new_watch();
        state.manual_watches.insert(id.clone(), watch);
        let backend = ctx.backend.clone();
        let tx = ctx.tx.clone();
        let me = state.self_id.clone();
        tokio::spawn(async move {
            loop {
                let read = async {
                    let client = backend.client_for(&id, &me).await?;
                    match unary(&client, Req::DeviceInfo(v1::DeviceInfoRequest {})).await? {
                        Resp::DeviceInfo(info) => Ok(info),
                        _ => Err(unexpected()),
                    }
                };
                let result = tokio::select! {
                    _ = token.cancelled() => return,
                    r = tokio::time::timeout(Duration::from_secs(5), read) => r.unwrap_or_else(|_| Err(Error::Unavailable("device status timed out".into()))),
                };
                let _ = tx.send(loop_message(Msg::ManualDevice(id.clone(), result)));
                tokio::select! { _ = token.cancelled() => return, _ = tokio::time::sleep(Duration::from_secs(3)) => {} }
            }
        });
    }
}

pub(super) fn set(state: &mut State, ctx: &Ctx, r: app::SetDeviceLockedUse) {
    if !views(state)
        .iter()
        .any(|v| v.device_id == r.device_id && v.enabled)
    {
        return;
    }
    if r.on && (state.presence.is_some() || state.locked.is_some() || !state.foreground) {
        state.toast(
            "Finish the current authentication and keep Muster in the foreground",
            true,
        );
        return;
    }
    let phone = state.embedded && state.self_info.as_ref().is_some_and(|i| i.kind == "phone");
    state.manual_busy.insert(r.device_id.clone());
    let backend = ctx.backend.clone();
    let me = state.self_id.clone();
    let network = state.network.as_ref().map(|n| n.network_id.clone());
    let op_id = state.op_id();
    spawn(ctx, async move {
        let result = async {
            let client = backend.client_for(&r.device_id, &me).await?;
            let Resp::DeviceInfo(info) =
                unary(&client, Req::DeviceInfo(v1::DeviceInfoRequest {})).await?
            else {
                return Err(unexpected());
            };
            let status = info.state.ok_or_else(unexpected)?;
            if !r.on {
                if status.manual_locked_use_owner != me {
                    return Err(Error::Unauthorized(
                        "this device does not own Locked Use".into(),
                    ));
                }
                match unary(
                    &client,
                    Req::LockedUseCancel(v1::LockedUseCancelRequest {
                        nonce: status.manual_locked_use_nonce,
                        reason: "user".into(),
                    }),
                )
                .await?
                {
                    Resp::Ack(_) => return Ok(None),
                    _ => return Err(unexpected()),
                }
            }
            let operation = v1::LockedUseOperation {
                provider_queue: String::new(),
                operation: Some(v1::locked_use_operation::Operation::Device(
                    v1::DeviceLeaseRequest {
                        op_id: op_id.clone(),
                    },
                )),
            };
            let digest = muster_proto::locked_use::digest(&operation);
            let network: [u8; 32] = hex::decode(network.ok_or_else(unexpected)?)
                .map_err(|_| unexpected())?
                .try_into()
                .map_err(|_| unexpected())?;
            let target = muster_core::DeviceId::from_hex(&r.device_id).map_err(|_| unexpected())?;
            let requester = muster_core::DeviceId::from_hex(&me).map_err(|_| unexpected())?;
            let Resp::LockedUseChallenge(first) = unary(
                &client,
                Req::LockedUsePrepare(v1::LockedUsePrepareRequest {
                    operation: Some(operation),
                }),
            )
            .await?
            else {
                return Err(unexpected());
            };
            let challenge = muster_core::locked_use::Challenge::decode(&first.challenge)
                .map_err(|e| Error::Invalid(e.to_string()))?;
            let message = challenge
                .signing_message_for(&network, target, requester, &op_id, &digest)
                .map_err(|e| Error::Invalid(e.to_string()))?;
            if !phone {
                return Ok(Some(Msg::PresenceNeeded {
                    response_origin: None,
                    what: "manual",
                    device: r.device_id.clone(),
                    call_id: client.next_id(),
                    request: Box::new(Req::LockedUseExecute(v1::LockedUseExecuteRequest {
                        challenge: first.challenge,
                        signature: Vec::new(),
                    })),
                    message,
                }));
            }
            let signature = backend
                .signer()
                .ok_or_else(|| Error::Unavailable("device signing key unavailable".into()))?
                .sign(&message)
                .map_err(|e| Error::Unauthorized(e.to_string()))?;
            match unary(
                &client,
                Req::LockedUseExecute(v1::LockedUseExecuteRequest {
                    challenge: first.challenge,
                    signature,
                }),
            )
            .await?
            {
                Resp::OpResult(r) if r.status == v1::op_result::Status::Applied as i32 => Ok(None),
                Resp::OpResult(r) => Err(Error::Unavailable(r.message)),
                _ => Err(unexpected()),
            }
        }
        .await;
        match result {
            Ok(Some(message)) => message,
            Ok(None) => Msg::ManualDone(r.device_id, Ok(())),
            Err(error) => Msg::ManualDone(r.device_id, Err(error)),
        }
    });
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn switch_is_a_rust_projection_independent_of_accessibility_and_respects_owner() {
        let mut s = State::new(true, Some("phone".into()));
        s.network = Some(v1::NetworkStatus {
            members: vec![v1::MemberView {
                device_id: "mac".into(),
                kind: "desktop".into(),
                connected: true,
                ..Default::default()
            }],
            ..Default::default()
        });
        assert!(!views(&s)[0].enabled);
        s.manual_devices.insert(
            "mac".into(),
            v1::DeviceInfo {
                platform: "macos".into(),
                services: vec!["locked.device.v1".into()],
                state: Some(v1::DeviceState {
                    accessibility: "missing".into(),
                    ..Default::default()
                }),
                ..Default::default()
            },
        );
        assert!(views(&s)[0].enabled);
        s.manual_devices
            .get_mut("mac")
            .unwrap()
            .state
            .as_mut()
            .unwrap()
            .accessibility = "granted".into();
        assert!(views(&s)[0].enabled);
        let d = s
            .manual_devices
            .get_mut("mac")
            .unwrap()
            .state
            .as_mut()
            .unwrap();
        d.manual_locked_use_owner = "other-phone".into();
        d.manual_locked_use_active = true;
        assert!(views(&s)[0].on);
        assert!(!views(&s)[0].enabled);
        assert!(views(&s)[0].status.contains("另一台设备"));
        s.embedded = false;
        assert!(
            views(&s)[0].on,
            "desktop exposes the same Rust-owned manual control"
        );
        assert!(
            !views(&s)[0].enabled,
            "desktop cannot cancel a foreign owner"
        );
        s.manual_devices
            .get_mut("mac")
            .unwrap()
            .state
            .as_mut()
            .unwrap()
            .manual_locked_use_owner = "phone".into();
        assert!(views(&s)[0].enabled);
        s.manual_busy.insert("mac".into());
        assert!(!views(&s)[0].enabled);
        s.manual_busy.clear();
        s.network.as_mut().unwrap().members[0].connected = false;
        assert!(!views(&s)[0].enabled);
    }
}
