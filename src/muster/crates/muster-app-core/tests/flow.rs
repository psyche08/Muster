//! The core end to end: a desktop core over musterd's socket creates a
//! network and invites; a phone core with its own embedded link joins,
//! gets approved, reaches the desktop over the link (agents list), is made
//! admin and changes the desktop's policy with a presence proof.

use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use muster_app_core::app::{self, Screen, intent::It};
use muster_app_core::{AppConfig, AppCore, Mode, Shell};
use muster_core::{HwSigner, SoftSigner};
use muster_proto::Message as _;
use musterd::{Daemon, DaemonConfig};
use tokio::sync::Notify;

mod common;

struct TestShell {
    name: &'static str,
    views: Mutex<Vec<app::ViewModel>>,
    changed: Notify,
    effects: Mutex<Vec<(u64, app::Effect)>>,
    presence: SoftSigner,
    core: Mutex<Option<std::sync::Weak<AppCore>>>,
}

impl TestShell {
    fn new(name: &'static str) -> Arc<Self> {
        Arc::new(Self {
            name,
            views: Mutex::new(Vec::new()),
            changed: Notify::new(),
            effects: Mutex::new(Vec::new()),
            presence: SoftSigner::generate(),
            core: Mutex::new(None),
        })
    }

    fn latest(&self) -> app::ViewModel {
        self.views
            .lock()
            .unwrap()
            .last()
            .cloned()
            .unwrap_or_default()
    }

    async fn try_wait(
        &self,
        secs: u64,
        check: impl Fn(&app::ViewModel) -> bool,
    ) -> Option<app::ViewModel> {
        let started = Instant::now();
        loop {
            let vm = self.latest();
            if check(&vm) {
                return Some(vm);
            }
            if started.elapsed() > Duration::from_secs(secs) {
                return None;
            }
            let _ = tokio::time::timeout(Duration::from_millis(200), self.changed.notified()).await;
        }
    }

    async fn wait(&self, what: &str, check: impl Fn(&app::ViewModel) -> bool) -> app::ViewModel {
        match self.try_wait(30, check).await {
            Some(vm) => vm,
            None => panic!(
                "{}: timed out waiting for {what}; last: {:?}",
                self.name,
                self.latest()
            ),
        }
    }
}

impl Shell for TestShell {
    fn on_view(&self, view_model: &[u8]) {
        let vm = app::ViewModel::decode(view_model).unwrap();
        self.views.lock().unwrap().push(vm);
        self.changed.notify_waiters();
    }
    fn on_effect(&self, id: u64, effect: &[u8]) {
        let effect = app::Effect::decode(effect).unwrap();
        // Authentication: "the user is here", sign the message with the presence key.
        if let Some(app::effect::Ef::Authenticate(a)) = &effect.ef {
            let signature = self.presence.sign(&a.message).unwrap();
            if let Some(core) = self.core.lock().unwrap().as_ref().and_then(|c| c.upgrade()) {
                core.effect_result(
                    id,
                    app::EffectResult {
                        ok: true,
                        data: signature,
                        ..Default::default()
                    },
                );
            }
        }
        self.effects.lock().unwrap().push((id, effect));
    }
    fn hw_public_key(&self) -> Option<Vec<u8>> {
        None
    }
    fn hw_sign(&self, _message: &[u8]) -> Option<Vec<u8>> {
        None
    }
    fn presence_public_key(&self) -> Option<Vec<u8>> {
        Some(self.presence.public_key().raw().to_vec())
    }
}

fn send(core: &AppCore, it: It) {
    core.intent(app::Intent { it: Some(it) });
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn invitations_require_admin_and_disappear_after_demotion() {
    use muster_proto::v1::{self, request::Req};
    let relay = muster_relay::RelayServer::bind("127.0.0.1:0".parse().unwrap(), Default::default())
        .await
        .unwrap();
    let address = relay.udp_addr().unwrap().to_string();
    tokio::spawn(async move {
        let _ = relay.serve().await;
    });
    let dirs = [tempfile::tempdir().unwrap(), tempfile::tempdir().unwrap()];
    let shells = [TestShell::new("admin"), TestShell::new("member")];
    let mut cores = Vec::new();
    for i in 0..2 {
        cores.push(
            AppCore::start(
                AppConfig {
                    dir: dirs[i].path().to_path_buf(),
                    mode: Mode::Embedded {
                        relay: Some(address.clone()),
                        name: shells[i].name.into(),
                        // Remote RPC targets must be desktops or servers.
                        kind: if i == 0 { "desktop" } else { "phone" }.into(),
                        bind: "127.0.0.1:0".parse().unwrap(),
                        version: String::new(),
                    },
                    allow_loopback: true,
                },
                shells[i].clone(),
            )
            .await
            .unwrap(),
        );
        shells[i].wait("initial", |vm| vm.self_.is_some()).await;
        assert!(!shells[i].latest().can_invite);
    }
    let a = cores[0].backend().daemon().unwrap().clone();
    let b = cores[1].backend().daemon().unwrap().clone();
    a.create_network("invite permissions", "192.168.13.0/24")
        .await
        .unwrap();
    shells[0]
        .wait("admin invite capability", |vm| vm.can_invite)
        .await;
    send(&cores[0], It::ShowInvite(app::ShowInvite { ttl_secs: 60 }));
    let invite = shells[0]
        .wait("admin invitation", |vm| vm.invite.is_some())
        .await
        .invite
        .unwrap()
        .text;
    let joining = b.clone();
    let task =
        tokio::spawn(async move { joining.join(&invite, "member", "phone", &|_, _| {}).await });
    tokio::time::timeout(Duration::from_secs(10), async {
        while a.pending_joins().joins.is_empty() {
            tokio::time::sleep(Duration::from_millis(20)).await;
        }
    })
    .await
    .unwrap();
    // Granting network.admin scope alone must not replace the admin role.
    a.decide(&b.device_id().to_hex(), true, &["network.admin".into()], "")
        .unwrap();
    task.await.unwrap().unwrap();
    shells[1]
        .wait("joined", |vm| {
            vm.network.as_ref().is_some_and(|n| n.has_network)
        })
        .await;
    assert!(!shells[1].latest().can_invite);
    assert!(matches!(
        b.make_invite(60),
        Err(musterd::Error::Unauthorized(_))
    ));
    let request = Req::NetworkInvite(v1::NetworkInviteRequest { ttl_secs: 60 });
    let error = cores[1]
        .backend()
        .local()
        .call(request.clone())
        .await
        .unwrap_err();
    assert!(
        error.to_string().contains("only an admin can invite"),
        "{error}"
    );
    let (send_stream, recv_stream) = b
        .node()
        .open(a.device_id(), musterd::SERVICE, musterd::SERVICE_VERSION)
        .await
        .unwrap();
    let remote = muster_rpc::Client::new(tokio::io::join(recv_stream, send_stream));
    let error = remote.call(request).await.unwrap_err();
    assert!(
        error.to_string().contains("only an admin can invite"),
        "{error}"
    );
    send(&cores[1], It::ShowInvite(app::ShowInvite { ttl_secs: 60 }));
    let denied = shells[1]
        .wait("invite intent denied", |vm| {
            vm.toasts.iter().any(|t| t.text.contains("Only an admin"))
        })
        .await;
    assert_ne!(denied.screen, Screen::Invite as i32);
    assert!(denied.invite.is_none());
    send(
        &cores[1],
        It::Navigate(app::Navigate {
            screen: Screen::Invite as i32,
            back: false,
            ..Default::default()
        }),
    );
    shells[1]
        .wait("invite navigation denied", |vm| vm.toasts.len() >= 2)
        .await;
    assert_ne!(shells[1].latest().screen, Screen::Invite as i32);
    a.set_scopes(&b.device_id().to_hex(), &[], "admin").unwrap();
    shells[1].wait("promoted", |vm| vm.can_invite).await;
    send(&cores[1], It::ShowInvite(app::ShowInvite { ttl_secs: 60 }));
    shells[1]
        .wait("promoted invitation", |vm| vm.invite.is_some())
        .await;
    a.set_scopes(&b.device_id().to_hex(), &[], "member")
        .unwrap();
    let demoted = shells[1].wait("demoted", |vm| !vm.can_invite).await;
    assert!(demoted.invite.is_none());
    assert_ne!(demoted.screen, Screen::Invite as i32);
    for core in &cores {
        core.backend().shutdown().await;
    }
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn network_activation_is_bounded_and_does_not_assert_connectivity() {
    let dir = tempfile::tempdir().unwrap();
    let shell = TestShell::new("activation");
    let core = AppCore::start(
        AppConfig {
            dir: dir.path().to_path_buf(),
            mode: Mode::Embedded {
                relay: None,
                name: "phone".into(),
                kind: "phone".into(),
                bind: "127.0.0.1:0".parse().unwrap(),
                version: String::new(),
            },
            allow_loopback: true,
        },
        shell.clone(),
    )
    .await
    .unwrap();
    shell.wait("initial view", |vm| vm.self_.is_some()).await;
    let effects = shell.effects.lock().unwrap().clone();
    let (id, request) = effects
        .iter()
        .find_map(|(id, effect)| match &effect.ef {
            Some(app::effect::Ef::ActivateNetwork(request)) => Some((*id, request.clone())),
            _ => None,
        })
        .unwrap();
    assert_eq!(request.timeout_ms, 3000);
    core.effect_result(
        id,
        app::EffectResult {
            ok: true,
            ..Default::default()
        },
    );
    for _ in 0..8 {
        core.lifecycle(app::Lifecycle::NetworkChanged);
    }
    core.lifecycle(app::Lifecycle::Background);
    core.lifecycle(app::Lifecycle::NetworkChanged);
    tokio::time::sleep(Duration::from_millis(100)).await;
    assert_eq!(
        shell
            .effects
            .lock()
            .unwrap()
            .iter()
            .filter(|(_, e)| matches!(e.ef, Some(app::effect::Ef::ActivateNetwork(_))))
            .count(),
        1
    );
    assert!(
        shell
            .latest()
            .network
            .is_none_or(|n| n.network_id.is_empty())
    );
    core.backend().shutdown().await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn desktop_invites_phone_joins_and_administers_with_presence() {
    let _ = env_logger::Builder::from_env(env_logger::Env::default().default_filter_or("info"))
        .is_test(true)
        .try_init();
    let relay = muster_relay::RelayServer::bind(
        "127.0.0.1:0".parse().unwrap(),
        muster_relay::RelayOptions::default(),
    )
    .await
    .unwrap();
    let relay_addr = relay.udp_addr().unwrap().to_string();
    tokio::spawn(async move {
        let _ = relay.serve().await;
    });

    // The desktop daemon, with the agent service on temp homes.
    let dir_a = tempfile::tempdir().unwrap();
    let mut config = DaemonConfig::new(dir_a.path());
    config.bind = "127.0.0.1:0".parse().unwrap();
    config.relay = Some(relay_addr.clone());
    config.name = "desk".into();
    config.allow_loopback = true;
    config.agent = Some(muster_agent::AgentConfig {
        codex_home: Some(dir_a.path().join("codex")),
        claude_home: Some(dir_a.path().join("claude")),
        catdesk_home: None,
        claude_desktop: None,
        ..Default::default()
    });
    config.terminal = None;
    // One Claude session on the desktop whose cwd is a small git repo.
    let repo = dir_a.path().join("repo");
    std::fs::create_dir_all(&repo).unwrap();
    std::fs::write(repo.join("README.md"), "hello\n").unwrap();
    let git = std::process::Command::new("git")
        .args(["init", "-q"])
        .current_dir(&repo)
        .status()
        .is_ok_and(|s| s.success())
        && std::process::Command::new("git")
            .args(["add", "README.md"])
            .current_dir(&repo)
            .status()
            .is_ok_and(|s| s.success())
        && std::process::Command::new("git")
            .args([
                "-c",
                "user.name=t",
                "-c",
                "user.email=t@t",
                "commit",
                "-q",
                "-m",
                "init",
            ])
            .current_dir(&repo)
            .status()
            .is_ok_and(|s| s.success());
    let claude_id = "a1b2c3d4-e5f6-4a7b-8c9d-0e1f2a3b4c5d";
    let project = dir_a.path().join("claude/projects/-repo");
    std::fs::create_dir_all(&project).unwrap();
    std::fs::write(
        project.join(format!("{claude_id}.jsonl")),
        format!(
            "{{\"type\":\"user\",\"cwd\":\"{}\",\"message\":{{\"role\":\"user\",\"content\":\"hi claude\"}}}}\n",
            repo.display()
        ),
    )
    .unwrap();
    // This exercises the app protocol, not the host's live console. A locked
    // developer Mac must not silently change the fixture's authorization state.
    let daemon = Daemon::start_with(config, Arc::new(musterd::DefaultPlatform), None)
        .await
        .unwrap();

    // The desktop app core over the socket.
    let shell_a = TestShell::new("desk");
    let core_a = Arc::new(
        AppCore::start(
            AppConfig {
                dir: dir_a.path().join("app"),
                mode: Mode::Socket {
                    path: daemon.socket_path(),
                },
                allow_loopback: true,
            },
            shell_a.clone(),
        )
        .await
        .unwrap(),
    );
    *shell_a.core.lock().unwrap() = Some(Arc::downgrade(&core_a));
    let vm = shell_a
        .wait("onboarding", |vm| {
            vm.self_.is_some() && vm.screen == Screen::Onboarding as i32
        })
        .await;
    assert_eq!(vm.self_.as_ref().unwrap().name, "desk");
    assert!(!vm.embedded);

    send(
        &core_a,
        It::CreateNetwork(app::CreateNetwork {
            name: "home".into(),
            vnet_cidr: String::new(),
        }),
    );
    shell_a
        .wait("home", |vm| {
            vm.screen == Screen::Agents as i32 && vm.network.as_ref().is_some_and(|n| n.has_network)
        })
        .await;
    send(&core_a, It::ShowInvite(app::ShowInvite { ttl_secs: 300 }));
    let vm = shell_a
        .wait("invite", |vm| {
            vm.invite
                .as_ref()
                .is_some_and(|i| i.text.starts_with("muster1:"))
        })
        .await;
    let invite = vm.invite.unwrap().text;

    // The phone: its own link, no relay configured until the invite says.
    let dir_b = tempfile::tempdir().unwrap();
    let shell_b = TestShell::new("phone");
    let core_b = Arc::new(
        AppCore::start(
            AppConfig {
                dir: dir_b.path().to_path_buf(),
                mode: Mode::Embedded {
                    relay: None,
                    name: "phone".into(),
                    kind: "phone".into(),
                    bind: "127.0.0.1:0".parse().unwrap(),
                    version: String::new(),
                },
                allow_loopback: true,
            },
            shell_b.clone(),
        )
        .await
        .unwrap(),
    );
    *shell_b.core.lock().unwrap() = Some(Arc::downgrade(&core_b));
    let vm = shell_b
        .wait("phone onboarding", |vm| vm.self_.is_some())
        .await;
    assert!(vm.embedded);
    let phone_id = vm.self_.unwrap().device_id;

    send(
        &core_b,
        It::JoinNetwork(app::JoinNetwork {
            invite_text: invite,
            device_name: "phone".into(),
            device_kind: "phone".into(),
        }),
    );
    shell_b
        .wait("waiting approval", |vm| {
            vm.join
                .as_ref()
                .is_some_and(|j| j.phase == "waiting_approval")
        })
        .await;

    // The desktop sees the request and approves it.
    let vm = shell_a
        .wait("pending join", |vm| {
            vm.pending.iter().any(|p| p.device_id == phone_id)
        })
        .await;
    assert_eq!(vm.pending[0].name, "phone");
    send(
        &core_a,
        It::DecideJoin(app::DecideJoin {
            device_id: phone_id.clone(),
            approve: true,
            scopes: vec![
                "agents".into(),
                "agents:codex".into(),
                "agents:claude".into(),
                "net:tcp/22".into(),
            ],
            reason: String::new(),
        }),
    );
    let vm = shell_b
        .wait("joined", |vm| {
            vm.screen == Screen::Agents as i32
                && vm.network.as_ref().is_some_and(|n| n.members.len() == 2)
        })
        .await;
    let desk_id = vm
        .network
        .as_ref()
        .unwrap()
        .members
        .iter()
        .find(|m| !m.is_self)
        .unwrap()
        .device_id
        .clone();
    let home = shell_b
        .wait("default all-device catalog", |vm| {
            vm.agents.as_ref().is_some_and(|a| {
                a.device_id == "*" && !a.loading && a.sessions.iter().any(|s| s.id == claude_id)
            })
        })
        .await;
    let row = home
        .agents
        .unwrap()
        .sessions
        .into_iter()
        .find(|s| s.id == claude_id)
        .unwrap();
    assert_eq!(row.device_id, desk_id);
    assert_eq!(row.device_name, "desk");
    // A device tab filters in Rust; rapid changes must ignore older completions.
    send(
        &core_b,
        It::OpenAgents(app::OpenAgents {
            device_id: phone_id.clone(),
        }),
    );
    send(
        &core_b,
        It::OpenAgents(app::OpenAgents {
            device_id: "*".into(),
        }),
    );
    shell_b
        .wait("all-device catalog after tab switch", |vm| {
            vm.agents
                .as_ref()
                .is_some_and(|a| a.device_id == "*" && !a.loading && a.sessions.len() == 1)
        })
        .await;
    shell_a
        .wait("two members", |vm| {
            vm.network.as_ref().is_some_and(|n| n.members.len() == 2) && vm.pending.is_empty()
        })
        .await;

    // The phone reaches the desktop over the link: agent sessions (none yet).
    send(
        &core_b,
        It::SelectDevice(app::SelectDevice {
            device_id: desk_id.clone(),
        }),
    );
    send(&core_b, It::OpenAgents(app::OpenAgents::default()));
    let vm = shell_b
        .wait("agents", |vm| {
            vm.screen == Screen::Agents as i32
                && vm
                    .agents
                    .as_ref()
                    .is_some_and(|a| !a.loading && a.device_id == desk_id)
        })
        .await;
    let agents = vm.agents.unwrap();
    assert_eq!(agents.device_id, desk_id);
    assert!(vm.toasts.is_empty(), "{:?}", vm.toasts);
    assert!(
        agents.sessions.iter().any(|s| s.id == claude_id),
        "{agents:?}"
    );

    // Files and git of the session's working tree, over the link.
    send(
        &core_b,
        It::OpenFiles(app::OpenFiles {
            path: String::new(),
        }),
    );
    shell_b
        .wait("toast without a chat", |vm| {
            vm.toasts
                .iter()
                .any(|t| t.text.contains("open a session first"))
        })
        .await;
    send(
        &core_b,
        It::OpenAgent(app::OpenAgent {
            session: claude_id.into(),
            ..Default::default()
        }),
    );
    shell_b
        .wait("chat", |vm| {
            vm.screen == Screen::Chat as i32
                && vm
                    .chat
                    .as_ref()
                    .is_some_and(|c| !c.sending && c.message_count == 1)
        })
        .await;
    // Drop the actual phone RPC transport while the chat stays open. Messages
    // committed during the gap must arrive through reconnect + history catch-up,
    // without a navigation or lifecycle event, and repeated text stays distinct.
    let client = core_b.backend().client_for(&desk_id, "").await.unwrap();
    client.close();
    {
        use std::io::Write;
        let mut file = std::fs::OpenOptions::new()
            .append(true)
            .open(project.join(format!("{claude_id}.jsonl")))
            .unwrap();
        writeln!(
            file,
            r#"{{"type":"user","uuid":"u2","message":{{"role":"user","content":"repeat"}}}}"#
        )
        .unwrap();
        writeln!(
            file,
            r#"{{"type":"assistant","uuid":"a2","message":{{"role":"assistant","content":"OK"}}}}"#
        )
        .unwrap();
    }
    let vm = shell_b
        .wait("messages after broken watch", |vm| {
            vm.chat.as_ref().is_some_and(|c| c.message_count == 3)
        })
        .await;
    let messages = common::messages(&vm.chat.unwrap());
    assert_eq!(messages[1].role, "user");
    assert_eq!(messages[2].role, "assistant");
    // Rapid foreground/network callbacks replace watches; late results from
    // cancelled watches must not remove the active subscription or duplicate rows.
    core_b.lifecycle(app::Lifecycle::Foreground);
    core_b.lifecycle(app::Lifecycle::NetworkChanged);
    {
        use std::io::Write;
        let mut file = std::fs::OpenOptions::new()
            .append(true)
            .open(project.join(format!("{claude_id}.jsonl")))
            .unwrap();
        writeln!(
            file,
            r#"{{"type":"user","uuid":"u3","message":{{"role":"user","content":"repeat"}}}}"#
        )
        .unwrap();
        writeln!(
            file,
            r#"{{"type":"assistant","uuid":"a3","message":{{"role":"assistant","content":"OK"}}}}"#
        )
        .unwrap();
    }
    let vm = shell_b
        .wait("repeated message after foreground", |vm| {
            vm.chat.as_ref().is_some_and(|c| c.message_count == 6)
        })
        .await;
    let messages = common::messages(&vm.chat.unwrap());
    assert_eq!(
        messages.iter().map(|m| m.role.as_str()).collect::<Vec<_>>(),
        ["user", "user", "assistant", "system", "user", "assistant"]
    );
    assert_ne!(messages[1].id, messages[4].id);
    // The previous response had no completion marker. Its partial footer stays
    // before the next prompt across reconnects, without duplicating either.
    assert_eq!(messages[3].kind, "turn_summary");
    assert!(!messages[3].usage.as_ref().unwrap().tokens_known);
    send(
        &core_b,
        It::OpenFiles(app::OpenFiles {
            path: String::new(),
        }),
    );
    let vm = shell_b
        .wait("files", |vm| {
            vm.screen == Screen::Files as i32 && vm.files.as_ref().is_some_and(|f| !f.loading)
        })
        .await;
    let files = vm.files.unwrap();
    assert_eq!(files.device_id, desk_id);
    assert!(
        files
            .entries
            .iter()
            .any(|e| e.name == "README.md" && !e.dir),
        "{files:?}"
    );
    send(
        &core_b,
        It::OpenFile(app::OpenFile {
            path: "README.md".into(),
        }),
    );
    let vm = shell_b
        .wait("file", |vm| {
            vm.screen == Screen::File as i32 && vm.file.as_ref().is_some_and(|f| !f.loading)
        })
        .await;
    assert_eq!(vm.file.unwrap().content, b"hello\n");
    send(
        &core_b,
        It::OpenFile(app::OpenFile {
            path: "../secret".into(),
        }),
    );
    shell_b
        .wait("escape refused", |vm| {
            vm.file.as_ref().is_some_and(|f| {
                !f.loading && f.error.contains("path escapes") && f.content.is_empty()
            })
        })
        .await;
    if git {
        send(
            &core_b,
            It::OpenGit(app::OpenGit {
                diff_commit: String::new(),
            }),
        );
        let vm = shell_b
            .wait("git log", |vm| {
                vm.screen == Screen::Git as i32
                    && vm
                        .git
                        .as_ref()
                        .is_some_and(|g| !g.loading && !g.commits.is_empty())
            })
            .await;
        let log = vm.git.unwrap();
        assert_eq!(log.commits[0].subject, "init");
        send(
            &core_b,
            It::OpenGit(app::OpenGit {
                diff_commit: log.commits[0].hash.clone(),
            }),
        );
        let vm = shell_b
            .wait("git diff", |vm| {
                vm.git
                    .as_ref()
                    .is_some_and(|g| !g.loading && !g.diff.is_empty())
            })
            .await;
        assert!(vm.git.unwrap().diff.contains("README.md"));
    }
    // Back out to the agents list; the working-tree views go with it.
    for _ in 0..8 {
        if shell_b.latest().screen == Screen::Agents as i32 {
            break;
        }
        send(
            &core_b,
            It::Navigate(app::Navigate {
                screen: 0,
                back: true,
                ..Default::default()
            }),
        );
        tokio::time::sleep(Duration::from_millis(100)).await;
    }
    shell_b
        .wait("agents again", |vm| vm.screen == Screen::Agents as i32)
        .await;
    // A network change while a chat is open re-arms its watch quietly.
    core_b.lifecycle(app::Lifecycle::NetworkChanged);

    // Not an admin: changing the desktop's policy is refused at the door,
    // after the phone authenticated locally (presence).
    send(
        &core_b,
        It::SetPolicy(app::SetPolicy {
            policy: Some(muster_proto::v1::Policy {
                open_ports: vec!["tcp/22".into(), "tcp/5900".into()],
                offer_agent: true,
                offer_terminal: true,
                ..Default::default()
            }),
        }),
    );
    let vm = shell_b
        .wait("refused", |vm| {
            vm.toasts.iter().any(|t| t.text.contains("policy"))
        })
        .await;
    assert!(
        vm.toasts
            .iter()
            .any(|t| t.error && t.text.contains("network.admin")),
        "{:?}",
        vm.toasts
    );
    assert!(
        shell_b
            .effects
            .lock()
            .unwrap()
            .iter()
            .any(|(_, e)| matches!(e.ef, Some(app::effect::Ef::Authenticate(_))))
    );

    // Made admin by the desktop, the same change goes through.
    send(
        &core_a,
        It::SetScopes(app::SetScopes {
            device_id: phone_id.clone(),
            scopes: vec![
                "agents".into(),
                "agents:codex".into(),
                "agents:claude".into(),
                "net:tcp/22".into(),
            ],
            role: "admin".into(),
        }),
    );
    shell_b
        .wait("admin", |vm| {
            vm.network
                .as_ref()
                .is_some_and(|n| n.members.iter().any(|m| m.is_self && m.role == "admin"))
        })
        .await;
    for toast in shell_b.latest().toasts {
        send(&core_b, It::Dismiss(app::Dismiss { toast_id: toast.id }));
    }
    shell_b.wait("toasts gone", |vm| vm.toasts.is_empty()).await;
    send(
        &core_b,
        It::SetPolicy(app::SetPolicy {
            policy: Some(muster_proto::v1::Policy {
                open_ports: vec!["tcp/22".into(), "tcp/5900".into()],
                offer_agent: true,
                offer_terminal: true,
                ..Default::default()
            }),
        }),
    );
    // Success shows on the desktop, whose policy the phone changed.
    if shell_a
        .try_wait(20, |vm| {
            vm.policy.as_ref().is_some_and(|p| p.open_ports.len() == 2)
        })
        .await
        .is_none()
    {
        panic!(
            "desk policy unchanged; phone: {:?}",
            shell_b.latest().toasts
        );
    }
    assert!(
        !shell_b.latest().toasts.iter().any(|t| t.error),
        "{:?}",
        shell_b.latest().toasts
    );

    // A direct segment tap opens the combined list, without choosing a host.
    send(
        &core_b,
        It::OpenTerminals(app::OpenTerminals {
            device_id: "*".into(),
        }),
    );
    let combined = shell_b
        .wait("all terminals loaded", |vm| {
            vm.screen == Screen::Terminals as i32
                && vm
                    .terminals
                    .as_ref()
                    .is_some_and(|t| t.device_id == "*" && !t.loading)
        })
        .await;
    assert!(
        !combined.terminals.unwrap().ssh_available,
        "creating requires an explicit host"
    );

    // In-app SSH: the forward stream reaches the desktop, which has no sshd
    // on 22 in this test, so the session ends with a status rather than hanging.
    // The top-level terminal entry chooses a target without going through
    // Device details, even when the previous selected device was the phone.
    send(
        &core_b,
        It::SelectDevice(app::SelectDevice {
            device_id: phone_id.clone(),
        }),
    );
    send(
        &core_b,
        It::OpenTerminals(app::OpenTerminals {
            device_id: desk_id.clone(),
        }),
    );
    let vm = shell_b
        .wait("terminals", |vm| {
            vm.screen == Screen::Terminals as i32
                && vm
                    .terminals
                    .as_ref()
                    .is_some_and(|t| t.device_id == desk_id)
        })
        .await;
    assert!(vm.terminals.as_ref().unwrap().ssh_available);
    assert_eq!(vm.terminals.as_ref().unwrap().device_id, desk_id);
    send(
        &core_b,
        It::OpenSsh(app::OpenSsh {
            port: 22,
            user: "me".into(),
            cols: 40,
            rows: 10,
        }),
    );
    let vm = shell_b
        .wait("ssh ended", |vm| {
            vm.terminal.as_ref().is_some_and(|t| t.ended)
        })
        .await;
    let terminal = vm.terminal.unwrap();
    assert_eq!(terminal.source, "ssh");
    assert!(!terminal.status.is_empty());
    send(&core_b, It::CloseTerminal(app::CloseTerminal {}));
    shell_b.wait("back", |vm| vm.terminal.is_none()).await;

    // Removing the phone empties its network view.
    send(
        &core_a,
        It::RemoveDevice(app::RemoveDevice {
            device_id: phone_id.clone(),
        }),
    );
    shell_b
        .wait("removed", |vm| vm.screen == Screen::Onboarding as i32)
        .await;
    shell_a
        .wait("one member", |vm| {
            vm.network.as_ref().is_some_and(|n| n.members.len() == 1)
        })
        .await;

    core_b.backend().shutdown().await;
    daemon.shutdown().await;
}
