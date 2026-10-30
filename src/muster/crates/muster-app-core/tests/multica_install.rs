//! Installation progress travels through the real multiplexed local RPC ABI.
use muster_app_core::{AppConfig, AppCore, Mode, app};
use muster_proto::v1;
use muster_rpc::{Context, EventSink, Handler, HandlerFuture, Principal};
use std::sync::{
    Arc,
    atomic::{AtomicBool, AtomicUsize, Ordering},
};
use tokio::sync::Notify;
mod common;
use common::TestShell;

#[derive(Default)]
struct Server {
    installing: AtomicBool,
    release: Notify,
    queries: AtomicUsize,
    installs: AtomicUsize,
}
impl Server {
    fn status(&self) -> v1::MulticaStatus {
        let busy = self.installing.load(Ordering::SeqCst);
        v1::MulticaStatus {
            supported: true,
            data_dir: "/example/Multica".into(),
            busy,
            installation: Some(v1::MulticaInstallation {
                busy,
                finished: !busy,
                succeeded: !busy,
                log: if busy {
                    "Downloading package"
                } else {
                    "Installed"
                }
                .into(),
                ..Default::default()
            }),
            ..Default::default()
        }
    }
}
impl Handler for Server {
    fn call(&self, _: Context, request: v1::request::Req, events: EventSink) -> HandlerFuture<'_> {
        use v1::{multica_request::Action, request::Req, response::Resp};
        Box::pin(async move {
            match request {
                Req::Multica(request) => {
                    match Action::try_from(request.action).unwrap() {
                        Action::Install => {
                            self.installs.fetch_add(1, Ordering::SeqCst);
                            self.installing.store(true, Ordering::SeqCst);
                            self.release.notified().await;
                            self.installing.store(false, Ordering::SeqCst);
                        }
                        Action::Progress => {
                            self.queries.fetch_add(1, Ordering::SeqCst);
                            return Resp::Multica(v1::MulticaStatus {
                                installation: self.status().installation,
                                ..Default::default()
                            });
                        }
                        _ => {}
                    }
                    Resp::Multica(self.status())
                }
                Req::DeviceWatch(_) => {
                    events.cancelled().cancelled().await;
                    Resp::Ack(v1::Ack {})
                }
                _ => Resp::Ack(v1::Ack {}),
            }
        })
    }
}

async fn exercise(recover: bool) {
    let dir = tempfile::tempdir().unwrap();
    let socket = dir.path().join("rpc.sock");
    let listener = tokio::net::UnixListener::bind(&socket).unwrap();
    let server = Arc::new(Server::default());
    server.installing.store(recover, Ordering::SeqCst);
    let handler = server.clone();
    let task = tokio::spawn(async move {
        let (stream, _) = listener.accept().await.unwrap();
        muster_rpc::serve(stream, Principal::Local, handler).await;
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
    shell
        .wait(|v| v.multica.as_ref().is_some_and(|m| m.supported))
        .await;
    if !recover {
        core.intent(app::Intent {
            it: Some(app::intent::It::Multica(v1::MulticaRequest {
                action: v1::multica_request::Action::Install as i32,
                ..Default::default()
            })),
        });
    }
    let view = shell
        .wait(|v| {
            v.multica.as_ref().is_some_and(|m| {
                m.busy
                    && m.installation
                        .as_ref()
                        .is_some_and(|p| p.log == "Downloading package")
            })
        })
        .await;
    let status = view.multica.unwrap();
    assert!(status.supported);
    assert_eq!(status.data_dir, "/example/Multica");
    if recover {
        server.installing.store(false, Ordering::SeqCst);
    } else {
        server.release.notify_one();
    }
    shell
        .wait(|v| {
            v.multica.as_ref().is_some_and(|m| {
                !m.busy
                    && m.installation
                        .as_ref()
                        .is_some_and(|p| p.succeeded && p.log == "Installed")
            })
        })
        .await;
    let queries = server.queries.load(Ordering::SeqCst);
    tokio::time::sleep(std::time::Duration::from_millis(1200)).await;
    assert_eq!(
        server.queries.load(Ordering::SeqCst),
        queries,
        "no idle polling"
    );
    assert_eq!(
        server.installs.load(Ordering::SeqCst),
        usize::from(!recover),
        "never replay installation"
    );
    drop(core);
    task.abort();
}
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn live_installation_keeps_configuration_and_stops_polling() {
    exercise(false).await;
}
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn reconnect_exposes_settings_and_recovers_without_reinstalling() {
    exercise(true).await;
}
