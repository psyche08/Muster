//! Attaches Accessibility controllers to sessions visible in desktop apps.

use std::collections::HashMap;
use std::sync::Arc;
#[cfg(target_os = "macos")]
use std::time::Duration;

use muster_agent::AgentService;
#[cfg(target_os = "macos")]
use muster_agent::controller::Controller;
use muster_agent::profile::{APPS, App};
use muster_platform::Platform;
#[cfg(target_os = "macos")]
use muster_platform::macos::native_app::Resolver;

#[cfg(target_os = "macos")]
const SCAN: Duration = Duration::from_secs(3);

/// Sessions shown in a desktop app (Claude Code in Claude, Codex in ChatGPT,
/// CatDesk) get an Accessibility controller (macOS only).
/// `queues` lets the scan stand aside while an operation drives the app:
/// a concurrent AX walk of the same app only slows that operation down.
#[cfg(target_os = "macos")]
pub(crate) async fn native_apps(agent: Arc<AgentService>, queues: ProviderQueues) {
    use muster_platform::macos::native_app::{NativeAppController, app_pids, discover, profiles};
    let mut last_error: std::collections::HashMap<String, String> =
        std::collections::HashMap::new();
    let mut warned = false;
    let resolvers: Vec<_> = profiles()
        .iter()
        .map(|profile| resolver(profile, &agent))
        .collect();
    let mut scan = tokio::time::interval(SCAN);
    scan.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);
    loop {
        // The first tick is immediate: warm visible sessions at daemon startup.
        scan.tick().await;
        let files = muster_platform::macos::procs::codex_runtime_files().await;
        let observer = agent.clone();
        let _ = tokio::task::spawn_blocking(move || {
            let claude = muster_platform::macos::procs::claude_runtime_sessions();
            let catdesk = muster_platform::macos::native_app::catdesk_runtime();
            observer.observe_runtime(files, claude, catdesk);
        })
        .await;
        if !muster_platform::macos::ax::trusted() {
            if !warned {
                log::warn!(
                    "Accessibility not granted to musterd: desktop agent apps stay view-only \
                     (Muster › Settings › Accessibility, or System Settings › Privacy & Security)"
                );
                warned = true;
            }
            continue;
        }
        warned = false;
        for (profile, resolver) in profiles().iter().zip(resolvers.iter()) {
            if queues.busy(profile.app.provider) {
                continue;
            }
            // The app's main process by executable path (pgrep does not
            // list it); the controller verifies its signature.
            let pids = app_pids(profile);
            for pid in pids {
                for session in discover(pid, resolver) {
                    if agent.retains_native_controller(&session) {
                        continue;
                    }
                    if agent.session(&session).is_none() {
                        agent.rescan().await;
                        if agent.session(&session).is_none() {
                            continue;
                        }
                    }
                    match NativeAppController::attach(
                        profile,
                        pid,
                        &session,
                        resolver.clone(),
                        agent.controller_events(),
                        agent.question_source(&session),
                        agent.send_receipt_source(&session),
                        agent.queue_source(&session),
                        agent.controller_runtime(),
                    )
                    .await
                    {
                        Ok(controller) => {
                            if agent.attach_native(controller as Arc<dyn Controller>) {
                                log::info!(
                                    "{} desktop shows session {}; AX controller attached",
                                    profile.app.provider,
                                    &session[..8]
                                );
                            } else {
                                log::info!(
                                    "{} desktop shows session {}; kept the controller an operation attached",
                                    profile.app.provider,
                                    &session[..8]
                                );
                            }
                        }
                        Err(error) => {
                            let text = error.to_string();
                            if last_error.get(&session) != Some(&text) {
                                log::warn!("AX attach {}: {text}", &session[..8]);
                                last_error.insert(session.clone(), text);
                            }
                        }
                    }
                }
            }
        }
    }
}

/// How a desktop app's web areas map to the session ids the service knows:
/// the engine's implementation of the profile's `SessionKey` strategy.
#[cfg(target_os = "macos")]
fn resolver(profile: &App, agent: &Arc<AgentService>) -> Resolver {
    use muster_agent::profile::SessionKey;
    use muster_platform::macos::native_app::{catdesk_resolver, claude_resolver};
    match profile.session_key {
        Some(SessionKey::Url) => claude_resolver(),
        Some(SessionKey::Runtime) => catdesk_resolver(),
        // An app that is not driven names no session in any window.
        None => Arc::new(|_| None),
        Some(SessionKey::Title) => {
            let agent = agent.clone();
            let provider = profile.provider();
            Arc::new(move |key| {
                key.title
                    .as_deref()
                    .and_then(|title| agent.native_session_by_title(provider, title))
            })
        }
    }
}

/// Each daemon is one host. Every profiled app has its own FIFO queue;
/// chat/code modes of the same app share the queue and its app controls.
#[derive(Clone)]
pub(crate) struct ProviderQueues {
    queues: Arc<HashMap<&'static str, Arc<tokio::sync::Mutex<()>>>>,
}
impl Default for ProviderQueues {
    fn default() -> Self {
        Self {
            queues: Arc::new(
                APPS.iter()
                    .map(|app| (app.provider(), Arc::new(tokio::sync::Mutex::new(()))))
                    .collect(),
            ),
        }
    }
}
impl ProviderQueues {
    fn queue(&self, app: &App) -> &Arc<tokio::sync::Mutex<()>> {
        // Every `App` is one of `APPS`, which the map was built from.
        self.queues
            .get(app.provider())
            .expect("every profiled app has a queue")
    }
    /// Whether an operation currently holds this provider's queue.
    #[cfg_attr(not(target_os = "macos"), allow(dead_code))]
    fn busy(&self, provider: &str) -> bool {
        self.queues
            .get(provider)
            .is_some_and(|queue| queue.try_lock().is_err())
    }
    async fn acquire(
        &self,
        app: &App,
        wait: std::time::Duration,
    ) -> Result<tokio::sync::OwnedMutexGuard<()>, crate::Error> {
        let queue = self.queue(app);
        tokio::time::timeout(wait, queue.clone().lock_owned())
            .await
            .map_err(|_| {
                crate::Error::Timeout(format!(
                    "{} provider queue timed out; operation was not started",
                    app.lock
                ))
            })
    }
}
pub struct ProviderGuard {
    permits: Option<ProviderPermits>,
    work: Option<muster_agent::control_guard::WorkQueue>,
}
struct ProviderPermits {
    _file: Option<muster_agent::desktop_lock::LockGuard>,
    _queue: tokio::sync::OwnedMutexGuard<()>,
}
impl ProviderGuard {
    fn new(
        queue: tokio::sync::OwnedMutexGuard<()>,
        file: Option<muster_agent::desktop_lock::LockGuard>,
    ) -> Self {
        Self {
            permits: Some(ProviderPermits {
                _queue: queue,
                _file: file,
            }),
            work: muster_agent::control_guard::current_work_queue(),
        }
    }
}
impl Drop for ProviderGuard {
    fn drop(&mut self) {
        if let Some(permits) = self.permits.take() {
            if let Some(work) = &self.work {
                work.retain_until_idle(permits);
            }
        }
    }
}

/// Holds `app` on this Mac for one operation: the file lock every daemon of
/// this user shares (keyed by the profile's `lock` name), released when the
/// guard drops and its outstanding OS workers finish, whether the operation
/// succeeded or not (and by its TTL, at most five minutes, if the daemon dies
/// holding it). The FIFO permit is
/// acquired before the cross-process app lock. Platforms without desktop
/// control still use the queue, without a file lock.
pub async fn hold(
    daemon: &crate::Daemon,
    app: &App,
    kind: muster_agent::desktop_lock::LockKind,
    detail: &str,
) -> Result<Option<ProviderGuard>, crate::Error> {
    use muster_agent::desktop_lock::{MAX_TTL, patience};
    let queue = daemon
        .agent_operations
        .acquire(app, std::time::Duration::from_secs(30))
        .await?;
    let file = if let Some(locks) = daemon.desktop_locks.as_ref() {
        Some(
            locks
                .acquire(app.lock, kind, detail, MAX_TTL, |holder| {
                    patience(kind, holder)
                })
                .await
                .map_err(|busy| crate::Error::Busy(format!("{}: {busy}", daemon.name())))?,
        )
    } else {
        None
    };
    Ok(Some(ProviderGuard::new(queue, file)))
}

/// Holds the app a session lives in: a desktop chat's app for a chat
/// operation, an agent session's app (Claude, or ChatGPT for Codex) for an
/// agent operation.
pub async fn hold_session(
    daemon: &crate::Daemon,
    agent: &AgentService,
    session: &str,
    what: &str,
) -> Result<Option<ProviderGuard>, crate::Error> {
    use muster_agent::desktop_chat::app_of;
    use muster_agent::desktop_lock::LockKind;
    let short: String = session.chars().take(20).collect();
    let (app, kind) = match app_of(session) {
        Some(app) => (app.profile(), LockKind::Chat),
        None => match agent
            .session(session)
            .and_then(|s| muster_agent::profile::app(&s.provider))
        {
            Some(app) => (app, LockKind::Agent),
            None => return Ok(None),
        },
    };
    hold(daemon, app, kind, &format!("{what} {short}")).await
}

/// Who holds `app` on this Mac now, if anyone.
pub fn holder(daemon: &crate::Daemon, app: &App) -> Option<muster_agent::desktop_lock::Holder> {
    daemon.desktop_locks.as_ref()?.holder(app.lock)
}

/// Whether the daemon's platform drives other apps' controls. Every
/// Accessibility entry point below asks the injected platform, not the
/// build target: a daemon started with `DefaultPlatform` (a Linux server,
/// a test) never touches the host's desktop apps.
pub(crate) fn drives_desktop(platform: &dyn Platform) -> bool {
    platform.accessibility().is_some()
}

fn no_desktop_control() -> crate::Error {
    crate::Error::Unavailable("this device has no desktop Accessibility control".into())
}

/// What a desktop app's Chat mode lists (read-only).
pub async fn chat_catalog(
    platform: &dyn Platform,
    agent: Arc<AgentService>,
    app: muster_agent::desktop_chat::ChatApp,
    full_history: bool,
) -> Result<muster_proto::v1::AgentChatCatalog, crate::Error> {
    if !drives_desktop(platform) {
        return Err(no_desktop_control());
    }
    // Shared Rust chooses the typed source and merges only live OS state.
    let local = if app == muster_agent::desktop_chat::ChatApp::ChatGpt {
        let reader = agent.clone();
        Some(
            tokio::task::spawn_blocking(move || reader.local_chatgpt_catalog())
                .await
                .map_err(|e| crate::Error::Unavailable(e.to_string()))??,
        )
    } else {
        None
    };
    #[cfg(target_os = "macos")]
    {
        let live = muster_platform::macos::native_chat::catalog(
            app,
            &agent,
            full_history && local.is_none(),
        )
        .await?;
        if let Some(mut catalog) = local {
            catalog.app_running = live.app_running;
            catalog.chat_mode = live.chat_mode;
            catalog.current = live.current;
            for chat in &mut catalog.chats {
                chat.current = chat.id == catalog.current;
            }
            Ok(catalog)
        } else {
            Ok(live)
        }
    }
    #[cfg(not(target_os = "macos"))]
    {
        let _ = (agent, app, full_history, local);
        Err(crate::Error::Unavailable("desktop chats need a Mac".into()))
    }
}

/// Shows a desktop chat (or a new one, `None`) in its app's Chat mode and
/// attaches its controller. Callers hold the provider queue.
pub async fn open_chat(
    platform: &dyn Platform,
    agent: Arc<AgentService>,
    app: muster_agent::desktop_chat::ChatApp,
    chat: Option<&str>,
) -> Result<String, crate::Error> {
    if !drives_desktop(platform) {
        return Err(no_desktop_control());
    }
    if app == muster_agent::desktop_chat::ChatApp::ChatGpt
        && let Some(id) = chat.filter(|s| !muster_agent::desktop_chat::is_new_session(s))
        && agent.chat_entry(id).is_none()
    {
        let reader = agent.clone();
        let catalog = tokio::task::spawn_blocking(move || reader.local_chatgpt_catalog())
            .await
            .map_err(|e| crate::Error::Unavailable(e.to_string()))??;
        if !catalog.chats.iter().any(|c| c.id == id) {
            return Err(crate::Error::Unavailable(
                "This is not a current ChatGPT chat in the local catalog; refresh the list".into(),
            ));
        }
    }
    #[cfg(target_os = "macos")]
    {
        Ok(muster_platform::macos::native_chat::open(app, &agent, chat).await?)
    }
    #[cfg(not(target_os = "macos"))]
    {
        let _ = (agent, app, chat);
        Err(crate::Error::Unavailable("desktop chats need a Mac".into()))
    }
}

/// Shows a desktop chat and reads its model / effort choices, first
/// choosing `set` (model, effort; empty keeps) when given. Callers hold
/// the provider queue.
pub async fn chat_options(
    platform: &dyn Platform,
    agent: Arc<AgentService>,
    app: muster_agent::desktop_chat::ChatApp,
    session: &str,
    set: Option<(&str, &str)>,
) -> Result<muster_proto::v1::AgentChatOptions, crate::Error> {
    if !drives_desktop(platform) {
        return Err(no_desktop_control());
    }
    #[cfg(target_os = "macos")]
    {
        Ok(muster_platform::macos::native_chat::options(app, &agent, session, set).await?)
    }
    #[cfg(not(target_os = "macos"))]
    {
        let _ = (agent, app, session, set);
        Err(crate::Error::Unavailable("desktop chats need a Mac".into()))
    }
}

/// Local diagnosis: list the model / effort menus next to the prompt.
pub async fn inspect_chat_options(
    platform: &dyn Platform,
    app: muster_agent::desktop_chat::ChatApp,
) -> Result<String, crate::Error> {
    if !drives_desktop(platform) {
        return Err(no_desktop_control());
    }
    #[cfg(target_os = "macos")]
    {
        Ok(muster_platform::macos::native_chat::inspect_options(app).await?)
    }
    #[cfg(not(target_os = "macos"))]
    {
        let _ = app;
        Err(crate::Error::Unavailable("desktop chats need a Mac".into()))
    }
}

/// Local diagnosis: switch an app to its Chat mode or back.
pub async fn set_chat_mode(
    platform: &dyn Platform,
    app: muster_agent::desktop_chat::ChatApp,
    chat: bool,
) -> Result<(), crate::Error> {
    if !drives_desktop(platform) {
        return Err(no_desktop_control());
    }
    #[cfg(target_os = "macos")]
    {
        Ok(muster_platform::macos::native_chat::set_mode(app, chat).await?)
    }
    #[cfg(not(target_os = "macos"))]
    {
        let _ = (app, chat);
        Err(crate::Error::Unavailable("desktop chats need a Mac".into()))
    }
}

/// Select and verify the requested native session through Accessibility.
/// Never create an owned provider CLI when the desktop is unavailable.
/// A platform without desktop Accessibility selects nothing: only a
/// controller attached in-process (by an embedder or a test) can act.
pub async fn ensure_native(
    platform: &dyn Platform,
    agent: Arc<AgentService>,
    session: &str,
) -> Result<(), crate::Error> {
    if !drives_desktop(platform) {
        return if agent.has_controller(session) {
            Ok(())
        } else {
            Err(no_desktop_control())
        };
    }
    if let Some(app) = muster_agent::desktop_chat::app_of(session) {
        return open_chat(platform, agent, app, Some(session))
            .await
            .map(|_| ());
    }
    #[cfg(target_os = "macos")]
    let preparation_started = std::time::Instant::now();
    #[cfg(target_os = "macos")]
    if agent.native_surface_ready(session)? {
        log::info!(
            "native preparation target={session:?} path=visible_controller ready elapsed_ms={}",
            preparation_started.elapsed().as_millis()
        );
        return Ok(());
    }
    // An agent session lives in its app's Code (Codex) mode: leave Chat
    // mode, the settings or a dialog first.
    #[cfg(target_os = "macos")]
    if let Some(app) = agent
        .session(session)
        .and_then(|s| muster_agent::desktop_chat::ChatApp::from_provider(&s.provider))
        && muster_platform::macos::ax::trusted()
    {
        log::info!("native preparation target={session:?} phase=return_to_code begin");
        muster_platform::macos::native_chat::return_to_code(app).await?;
        log::info!(
            "native preparation target={session:?} phase=return_to_code ready elapsed_ms={}",
            preparation_started.elapsed().as_millis()
        );
    }
    #[cfg(target_os = "macos")]
    {
        use muster_platform::macos::native_app::{
            NativeAppController, app_pids, discover, profiles,
        };
        if !muster_platform::macos::ax::trusted() {
            return Err(crate::Error::Unavailable(
                "Accessibility is not granted to musterd on the target Mac".into(),
            ));
        }
        let target = agent
            .session(session)
            .ok_or_else(|| crate::Error::NotFound(session.into()))?;
        let (navigation_title, title_is_unique) = agent
            .native_navigation_title(session)
            .ok_or_else(|| crate::Error::NotFound(session.into()))?;
        for profile in profiles() {
            if profile.app.provider != target.provider {
                continue;
            }
            let resolver = resolver(profile, &agent);
            for pid in app_pids(profile) {
                log::info!(
                    "native preparation target={session:?} phase=select begin elapsed_ms={}",
                    preparation_started.elapsed().as_millis()
                );
                muster_platform::macos::native_app::select_session(
                    profile,
                    pid,
                    session,
                    &navigation_title,
                    title_is_unique,
                    &resolver,
                )
                .await?;
                log::info!(
                    "native preparation target={session:?} phase=discover begin elapsed_ms={}",
                    preparation_started.elapsed().as_millis()
                );
                if discover(pid, &resolver).iter().any(|s| s == session) {
                    if agent.retains_native_controller(session) {
                        return Ok(());
                    }
                    muster_agent::control_guard::check()?;
                    log::info!(
                        "native preparation target={session:?} phase=attach begin elapsed_ms={}",
                        preparation_started.elapsed().as_millis()
                    );
                    let controller = NativeAppController::attach(
                        profile,
                        pid,
                        session,
                        resolver.clone(),
                        agent.controller_events(),
                        agent.question_source(session),
                        agent.send_receipt_source(session),
                        agent.queue_source(session),
                        agent.controller_runtime(),
                    )
                    .await?;
                    // A background scan may have attached one meanwhile; keep it.
                    agent.attach_native(controller as Arc<dyn Controller>);
                    log::info!(
                        "native preparation target={session:?} phase=attach ready elapsed_ms={}",
                        preparation_started.elapsed().as_millis()
                    );
                    return Ok(());
                }
            }
        }
    }
    let _ = (agent, session);
    Err(crate::Error::Unavailable(
        "this session is not visible in its native app".into(),
    ))
}

#[cfg(test)]
mod queue_tests {
    use super::*;
    use muster_agent::desktop_chat::ChatApp;
    use std::time::Duration;
    #[tokio::test]
    async fn cancelled_provider_operation_holds_queue_until_os_work_finishes() {
        use muster_agent::control_guard::{WorkQueue, spawn_blocking};
        let queues = ProviderQueues::default();
        let permit = queues
            .acquire(ChatApp::Claude.profile(), Duration::from_secs(1))
            .await
            .unwrap();
        let work = WorkQueue::default();
        let dir = tempfile::tempdir().unwrap();
        let locks = muster_agent::desktop_lock::DesktopLocks::new(dir.path());
        let file = locks
            .try_acquire(
                "Claude",
                muster_agent::desktop_lock::LockKind::Agent,
                "cancelled operation",
                Duration::from_secs(60),
            )
            .unwrap();
        let (started, ready) = tokio::sync::oneshot::channel();
        let (finish, wait) = std::sync::mpsc::channel();
        let worker = work.clone();
        let task = tokio::spawn(async move {
            worker
                .scope(async move {
                    let _guard = ProviderGuard::new(permit, Some(file));
                    spawn_blocking(move || {
                        started.send(()).unwrap();
                        wait.recv_timeout(Duration::from_secs(5)).unwrap();
                    })
                    .await
                    .unwrap();
                })
                .await;
        });
        ready.await.unwrap();
        task.abort();
        let _ = task.await;
        assert!(!work.is_idle());
        assert!(
            locks
                .try_acquire(
                    "Claude",
                    muster_agent::desktop_lock::LockKind::Chat,
                    "another process",
                    Duration::from_secs(60),
                )
                .is_err(),
            "cancellation must retain the cross-process lock too"
        );
        let next = queues
            .acquire(ChatApp::Claude.profile(), Duration::from_millis(30))
            .await;
        finish.send(()).unwrap();
        assert!(
            next.is_err(),
            "a cancelled RPC released the provider while its OS worker was still running"
        );
        tokio::time::timeout(Duration::from_secs(1), async {
            while !work.is_idle() {
                tokio::task::yield_now().await;
            }
        })
        .await
        .unwrap();
        assert!(
            queues
                .acquire(ChatApp::Claude.profile(), Duration::from_millis(100))
                .await
                .is_ok()
        );
        assert!(locks.holder("Claude").is_none());
    }

    #[tokio::test]
    async fn background_scan_sees_only_the_held_provider_as_busy() {
        let queues = ProviderQueues::default();
        assert!(!queues.busy("codex") && !queues.busy("claude"));
        let held = queues
            .clone()
            .acquire(ChatApp::ChatGpt.profile(), Duration::from_secs(1))
            .await
            .unwrap();
        assert!(queues.busy("codex"), "a clone shares the operation's queue");
        assert!(!queues.busy("claude") && !queues.busy("catdesk"));
        let catdesk = queues
            .clone()
            .acquire(&muster_agent::profile::CATDESK_APP, Duration::from_secs(1))
            .await
            .unwrap();
        assert!(queues.busy("catdesk"), "CatDesk has its own queue");
        assert!(!queues.busy("unknown"));
        drop(catdesk);
        drop(held);
        assert!(!queues.busy("codex"));
    }
    #[tokio::test]
    async fn provider_queues_are_fifo_and_other_providers_and_hosts_are_independent() {
        let host = Arc::new(ProviderQueues::default());
        let first = host
            .acquire(ChatApp::Claude.profile(), Duration::from_secs(1))
            .await
            .unwrap();
        let (started, mut starts) = tokio::sync::mpsc::channel(2);
        let (finished, mut order) = tokio::sync::mpsc::channel(2);
        for index in 0..2 {
            let host = host.clone();
            let started = started.clone();
            let finished = finished.clone();
            tokio::spawn(async move {
                let next = host.acquire(ChatApp::Claude.profile(), Duration::from_secs(1));
                tokio::pin!(next);
                tokio::select! { biased;
                    _ = &mut next => panic!("same provider must wait"),
                    _ = async {} => {}
                }
                started.send(index).await.unwrap();
                let _guard = next.await.unwrap();
                finished.send(index).await.unwrap();
            });
            assert_eq!(starts.recv().await, Some(index));
        }
        let other_provider = host
            .acquire(ChatApp::ChatGpt.profile(), Duration::from_millis(50))
            .await
            .unwrap();
        let other_host = ProviderQueues::default();
        let other_host_guard = other_host
            .acquire(ChatApp::Claude.profile(), Duration::from_millis(50))
            .await
            .unwrap();
        assert!(matches!(
            host.acquire(ChatApp::Claude.profile(), Duration::from_millis(20))
                .await,
            Err(crate::Error::Timeout(_))
        ));
        drop(first);
        assert_eq!(order.recv().await, Some(0));
        assert_eq!(order.recv().await, Some(1));
        // A timed-out waiter must never run later or retain the provider.
        let _again = host
            .acquire(ChatApp::Claude.profile(), Duration::from_millis(50))
            .await
            .unwrap();
        drop((other_provider, other_host_guard));
    }
}
