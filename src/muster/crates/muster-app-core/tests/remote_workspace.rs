use std::sync::Arc;

use muster_app_core::{AppConfig, AppCore, Mode, app};
use muster_proto::v1::{self, event::Ev, request::Req, response::Resp};
use muster_rpc::{Context, EventSink, Handler, HandlerFuture, Principal};

mod common;
use common::TestShell;

struct Desktop;

impl Handler for Desktop {
    fn call(&self, _: Context, request: Req, events: EventSink) -> HandlerFuture<'_> {
        Box::pin(async move {
            match request {
                Req::DeviceInfo(_) => Resp::DeviceInfo(v1::DeviceInfo {
                    device_id: "local".into(),
                    state: Some(v1::DeviceState {
                        desktop_viewer_available: true,
                        ..Default::default()
                    }),
                    ..Default::default()
                }),
                Req::NetworkStatus(_) => Resp::NetworkStatus(v1::NetworkStatus {
                    has_network: true,
                    self_id: "local".into(),
                    members: vec![v1::MemberView {
                        device_id: "host".into(),
                        name: "Test desktop".into(),
                        kind: "desktop".into(),
                        connected: true,
                        ..Default::default()
                    }],
                    ..Default::default()
                }),
                Req::DesktopSession(request) => {
                    assert_eq!(request.device, "host");
                    events
                        .send(Ev::DesktopSession(v1::DesktopSessionProgress {
                            phase: v1::desktop_session_progress::Phase::Visible as i32,
                            ..Default::default()
                        }))
                        .await
                        .unwrap();
                    events.cancelled().cancelled().await;
                    Resp::Ack(v1::Ack {})
                }
                _ => Resp::Ack(v1::Ack {}),
            }
        })
    }
}

#[tokio::test]
async fn embedded_lease_survives_navigation_until_disconnect() {
    use app::intent::It;
    let dir = tempfile::tempdir().unwrap();
    let socket = dir.path().join("rpc.sock");
    let listener = muster_local::Listener::bind(&socket).unwrap();
    tokio::spawn(async move {
        while let Ok((stream, _)) = listener.accept().await {
            tokio::spawn(muster_rpc::serve(
                stream,
                Principal::Local,
                Arc::new(Desktop),
            ));
        }
    });
    let shell = Arc::new(TestShell::default());
    let core = AppCore::start(
        AppConfig {
            dir: dir.path().into(),
            mode: Mode::Socket { path: socket },
            allow_loopback: true,
        },
        shell.clone(),
    )
    .await
    .unwrap();
    let send = |it| core.intent(app::Intent { it: Some(it) });
    let navigate = |screen| {
        send(It::Navigate(app::Navigate {
            screen: screen as i32,
            ..Default::default()
        }))
    };
    navigate(app::Screen::Remote);
    shell
        .wait(|v| {
            v.remote_workspace
                .as_ref()
                .is_some_and(|r| !r.devices.is_empty())
        })
        .await;
    send(It::OpenRemote(app::SelectDevice {
        device_id: "host".into(),
    }));
    shell
        .wait(|v| v.remote_workspace.as_ref().is_some_and(|r| r.can_connect))
        .await;
    send(It::DesktopSession(v1::DesktopSessionRequest {
        device: "host".into(),
        action: Some(v1::desktop_session_request::Action::ViewInWindow(1)),
    }));
    let viewing = |v: &app::ViewModel| {
        v.remote_workspace
            .as_ref()
            .is_some_and(|r| r.session_open && r.status == "Viewing desktop")
    };
    shell.wait(viewing).await;
    navigate(app::Screen::Terminals);
    shell
        .wait(|v| v.screen == app::Screen::Terminals as i32)
        .await;
    navigate(app::Screen::Remote);
    shell.wait(viewing).await;
    send(It::EndDesktopSession("host".into()));
    shell
        .wait(|v| {
            v.remote_workspace
                .as_ref()
                .is_some_and(|r| !r.session_open && r.can_connect)
        })
        .await;
}
