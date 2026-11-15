//! The forced mask beside Locked Use in the Settings device list (ADR 0171,
//! ADR 0212). The Mac persists the preference, refuses to turn it on while
//! locked and alone reports whether every screen is covered; this core only
//! bridges the tap and the next status poll with a short pending state.
use super::*;
use std::time::Instant;

/// How long a requested change shows as pending without the Mac reporting it.
const PENDING: Duration = Duration::from_secs(12);
const IDLE_HINT: &str = "开启后遮盖所有屏幕 · 禁止休眠";

pub(crate) fn views(state: &State) -> Vec<app::DeviceBlackoutView> {
    if !state.embedded {
        return Vec::new();
    }
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
            let mode = device.and_then(|s| s.blackout.as_ref());
            let locked = device.is_some_and(|s| s.locked);
            let pending = state.blackout_pending.get(&m.device_id).map(|p| p.0);
            let on = pending.unwrap_or(mode.is_some_and(|b| b.enabled));
            let busy = pending.is_some();
            let status = if !m.connected {
                "设备离线".into()
            } else if info.is_none() {
                "正在读取状态…".into()
            } else if mode.is_none() {
                "请更新 Mac 上的 Muster".into()
            } else if let Some(desired) = pending {
                if desired {
                    "正在开启强制遮罩…"
                } else {
                    "正在解除强制遮罩…"
                }
                .into()
            } else if !on && locked {
                "Mac 已锁屏；解锁后才能开启".into()
            } else {
                match mode {
                    Some(b) if b.enabled || b.status != "已关闭" => b.status.clone(),
                    _ => IDLE_HINT.into(),
                }
            };
            Some(app::DeviceBlackoutView {
                device_id: m.device_id.clone(),
                on,
                busy,
                // Turning it off is always allowed; the Mac refuses turning it
                // on while locked, so the switch does too.
                enabled: m.connected && mode.is_some() && !busy && (on || !locked),
                status,
            })
        })
        .collect()
}

/// Both the device page and the Settings row send through here.
pub(super) fn set(state: &mut State, ctx: &Ctx, r: app::SetCover) {
    let allowed = views(state)
        .iter()
        .any(|v| v.device_id == r.device_id && v.enabled && v.on != r.on);
    if !allowed {
        return;
    }
    state
        .blackout_pending
        .insert(r.device_id.clone(), (r.on, Instant::now()));
    network::admin_request(
        state,
        ctx,
        "Forced mask",
        r.device_id,
        Req::DeviceSetBlackout(v1::DeviceSetCoverRequest { on: r.on }),
    );
}

/// A fresh status poll settles the pending state once the Mac reports the
/// requested preference, or when it never does.
pub(super) fn observed(state: &mut State, device: &str) {
    let Some(&(desired, since)) = state.blackout_pending.get(device) else {
        return;
    };
    let reported = state
        .manual_devices
        .get(device)
        .and_then(|i| i.state.as_ref()?.blackout.as_ref())
        .map(|b| b.enabled);
    if reported == Some(desired) || reported.is_none() || since.elapsed() > PENDING {
        state.blackout_pending.remove(device);
    }
}

/// The request failed or its authentication was cancelled.
pub(super) fn failed(state: &mut State, device: Option<&str>) {
    match device {
        Some(device) => {
            state.blackout_pending.remove(device);
        }
        None => state.blackout_pending.clear(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn phone_with_mac() -> State {
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
        s.manual_devices.insert(
            "mac".into(),
            v1::DeviceInfo {
                device_id: "mac".into(),
                platform: "macos".into(),
                state: Some(v1::DeviceState {
                    blackout: Some(v1::DisplayBlackout {
                        enabled: false,
                        status: "已关闭".into(),
                    }),
                    ..Default::default()
                }),
                ..Default::default()
            },
        );
        s
    }

    fn mode(s: &mut State) -> &mut v1::DisplayBlackout {
        s.manual_devices
            .get_mut("mac")
            .unwrap()
            .state
            .as_mut()
            .unwrap()
            .blackout
            .as_mut()
            .unwrap()
    }

    #[test]
    fn forced_mask_row_follows_the_mac_and_requires_it_unlocked() {
        let mut s = phone_with_mac();
        let row = &views(&s)[0];
        assert!(row.enabled && !row.on && !row.busy);
        assert_eq!(row.status, IDLE_HINT);

        s.manual_devices
            .get_mut("mac")
            .unwrap()
            .state
            .as_mut()
            .unwrap()
            .locked = true;
        let row = &views(&s)[0];
        assert!(!row.enabled, "a locked Mac cannot turn the mask on");
        assert!(row.status.contains("解锁"));

        *mode(&mut s) = v1::DisplayBlackout {
            enabled: true,
            status: "所有屏幕已遮盖 · Mac 保持唤醒".into(),
        };
        let row = &views(&s)[0];
        assert!(
            row.on && row.enabled,
            "turning it off never needs an unlock"
        );
        assert_eq!(row.status, "所有屏幕已遮盖 · Mac 保持唤醒");

        s.network.as_mut().unwrap().members[0].connected = false;
        assert!(!views(&s)[0].enabled);
        assert_eq!(views(&s)[0].status, "设备离线");

        s.embedded = false;
        assert!(views(&s).is_empty(), "only a phone sets the forced mask");
    }

    #[test]
    fn pending_change_settles_on_the_reported_preference() {
        let mut s = phone_with_mac();
        s.blackout_pending
            .insert("mac".into(), (true, Instant::now()));
        let row = &views(&s)[0];
        assert!(row.on && row.busy && !row.enabled);
        assert_eq!(row.status, "正在开启强制遮罩…");

        observed(&mut s, "mac");
        assert!(s.blackout_pending.contains_key("mac"), "not reported yet");

        mode(&mut s).enabled = true;
        observed(&mut s, "mac");
        assert!(s.blackout_pending.is_empty());

        s.blackout_pending.insert(
            "mac".into(),
            (false, Instant::now() - PENDING - Duration::from_secs(1)),
        );
        observed(&mut s, "mac");
        assert!(s.blackout_pending.is_empty(), "a stale request expires");

        s.blackout_pending
            .insert("mac".into(), (false, Instant::now()));
        failed(&mut s, Some("mac"));
        assert!(s.blackout_pending.is_empty());
    }

    #[test]
    fn older_mac_without_the_mask_is_listed_but_disabled() {
        let mut s = phone_with_mac();
        s.manual_devices
            .get_mut("mac")
            .unwrap()
            .state
            .as_mut()
            .unwrap()
            .blackout = None;
        let row = &views(&s)[0];
        assert!(!row.enabled);
        assert_eq!(row.status, "请更新 Mac 上的 Muster");
    }
}
