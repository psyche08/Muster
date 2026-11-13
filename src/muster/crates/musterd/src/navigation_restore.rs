//! One outstanding return per provider. Planning and cleanup serialize with
//! ordinary provider work; timers own no provider or console permits.
use crate::{Daemon, Error, agent_bridge};
use muster_agent::{
    control_guard,
    controller::ProcessIdentity,
    native_navigation::restore::{Navigation, Page},
    profile::App,
};
use muster_platform::macos::navigation_restore::Snapshot;
use std::{
    collections::HashMap,
    sync::{Arc, Mutex, Weak},
    time::Duration,
};

pub(crate) struct Pending {
    snapshot: Arc<Snapshot>,
    page: Page,
    front: ProcessIdentity,
    allowed: Arc<dyn Fn() -> bool + Send + Sync>,
}
type Slot = Arc<Pending>;
#[derive(Default)]
pub(crate) struct Returns(Mutex<HashMap<&'static str, Slot>>);

impl Returns {
    fn take(&self, provider: &str) -> Option<Slot> {
        self.0.lock().unwrap().remove(provider)
    }
    fn take_if(&self, provider: &'static str, token: &Slot) -> Option<Slot> {
        muster_agent::native_navigation::restore::take_ticket(
            &mut *self.0.lock().unwrap(),
            &provider,
            token,
        )
    }
}
async fn restore(token: Slot) {
    let allowed = token.allowed.clone();
    let inherited = control_guard::current_check();
    let result = control_guard::spawn_blocking_async(control_guard::scope(
        Arc::new(move || allowed() && inherited.as_ref().is_none_or(|c| c())),
        async move { token.snapshot.restore(&token.page, token.front).await },
    ))
    .await;
    match result {
        Ok(Ok(())) => log::info!("native navigation restoration complete"),
        Ok(Err(e)) => log::info!("native navigation restoration skipped or stopped: {e}"),
        Err(e) => log::warn!("native navigation restoration worker ended: {e}"),
    }
}

struct Operation {
    daemon: Weak<Daemon>,
    app: &'static App,
    scope: Navigation,
    original: Option<Arc<Snapshot>>,
    target: Option<Page>,
    previous: Option<Slot>,
    covered: bool,
    ownership: Arc<dyn Send + Sync>,
    cleaned: bool,
    allowed: Arc<dyn Fn() -> bool + Send + Sync>,
}
impl Operation {
    fn pending(&mut self) -> Option<Slot> {
        if !self.scope.dispatched() {
            return self.previous.take().map(|old| {
                Arc::new(Pending {
                    snapshot: old.snapshot.clone(),
                    page: old.page.clone(),
                    front: old.front,
                    allowed: old.allowed.clone(),
                })
            });
        }
        self.previous.take();
        let snapshot = self.original.take().or_else(|| {
            log::info!("native navigation restoration skipped: original desktop unavailable");
            None
        })?;
        let page = self
            .scope
            .created()
            .map(Page::Code)
            .or_else(|| self.target.take())
            .or_else(|| {
                snapshot
                    .creation
                    .lock()
                    .unwrap()
                    .as_ref()
                    .map(|_| Page::Creation(self.app.provider().into()))
            });
        let Some(page) = page else {
            log::info!("native navigation restoration skipped: no verified creation destination");
            return None;
        };
        let front = snapshot.expected_front(self.scope.may_activate(), self.scope.foreground());
        Some(Arc::new(Pending {
            snapshot,
            page,
            front,
            allowed: self.allowed.clone(),
        }))
    }
    fn defer(&self, token: Slot, delay: Duration) {
        let Some(d) = self.daemon.upgrade() else {
            return;
        };
        let app = self.app;
        d.navigation_returns
            .0
            .lock()
            .unwrap()
            .insert(app.provider(), token.clone());
        let weak = self.daemon.clone();
        tokio::spawn(async move {
            tokio::time::sleep(delay).await;
            let Some(d) = weak.upgrade() else { return };
            // Acquire in the same order as direct operations, never wait for
            // or borrow a subsequent Locked Use owner's console.
            let Ok(_console) = d.locked_use.writes.clone().try_read_owned() else {
                if d.navigation_returns
                    .take_if(app.provider(), &token)
                    .is_some()
                {
                    log::info!("native navigation restoration skipped: console owned");
                }
                return;
            };
            let Ok(_provider) = agent_bridge::hold(
                &d,
                app,
                muster_agent::desktop_lock::LockKind::Agent,
                "restore native session",
            )
            .await
            else {
                d.navigation_returns.take_if(app.provider(), &token);
                log::info!("native navigation restoration skipped: provider busy");
                return;
            };
            let Some(token) = d.navigation_returns.take_if(app.provider(), &token) else {
                return;
            };
            if !d
                .policy()
                .read()
                .unwrap()
                .ax_allowlist
                .iter()
                .any(|id| id == app.app.bundle_id)
            {
                log::info!("native navigation restoration skipped: provider revoked");
                return;
            }
            restore(token).await;
        });
    }
}
impl Drop for Operation {
    fn drop(&mut self) {
        if self.cleaned {
            return;
        }
        if self.scope.busy() {
            // Cancellation may precede dispatch on an already registered worker.
            // Copy its journal, retain provider ownership, then decide after drain.
            let mut tail = Operation {
                daemon: self.daemon.clone(),
                app: self.app,
                scope: self.scope.clone(),
                original: self.original.take(),
                target: self.target.take(),
                previous: self.previous.take(),
                covered: self.covered,
                ownership: self.ownership.clone(),
                allowed: self.allowed.clone(),
                cleaned: false,
            };
            control_guard::spawn_blocking_async(async move {
                tail.scope.drain().await;
                if tail.covered
                    && let Some(token) = tail.pending()
                {
                    restore(token).await;
                }
                if tail.covered {
                    tail.cleaned = true;
                }
                drop(tail);
            });
            return;
        }
        if let Some(token) = self.pending() {
            if self.covered {
                // Registered OS work inherits the live actor check and the
                // actor keeps the cover until the worker drains.
                let ownership = self.ownership.clone();
                control_guard::spawn_blocking_async(async move {
                    let _held = ownership;
                    if let Err(e) = token.snapshot.restore(&token.page, token.front).await {
                        log::info!("covered navigation cleanup skipped: {e}")
                    }
                });
            } else {
                self.defer(token, Duration::from_secs(35));
            }
        }
    }
}

/// Enter only while holding the provider; an inspection of a visible session
/// can extend a pending return without ever taking another snapshot as origin.
pub(crate) async fn run<T>(
    daemon: &Arc<Daemon>,
    app: &'static App,
    target: Option<&str>,
    covered: bool,
    ownership: Arc<dyn Send + Sync>,
    operation: impl Future<Output = Result<T, Error>>,
) -> Result<T, Error> {
    if !agent_bridge::drives_desktop(daemon.platform().as_ref()) {
        return operation.await;
    }
    let desired = target.map(|s| {
        if muster_agent::desktop_chat::app_of(s).is_some()
            && let Some((_, id)) = s.split_once("-chat-")
            && muster_agent::viewer::is_uuid(id)
        {
            Page::Chat(id.to_owned())
        } else {
            Page::Code(s.to_owned())
        }
    });
    let previous = daemon.navigation_returns.take(app.provider());
    let previous = if let Some(p) = previous {
        if Some(&p.page) == desired.as_ref() && p.snapshot.owns(&p.page, p.front).is_ok() {
            Some(p)
        } else {
            restore(p).await;
            None
        }
    } else {
        None
    };
    let original = match &previous {
        Some(p) => Some(p.snapshot.clone()),
        None => control_guard::spawn_blocking(move || Snapshot::capture(app))
            .await
            .ok()
            .flatten()
            .map(Arc::new),
    };
    let scope = Navigation::default();
    let mut op = Operation {
        daemon: Arc::downgrade(daemon),
        app,
        scope: scope.clone(),
        original,
        target: desired,
        previous,
        covered,
        ownership,
        cleaned: false,
        allowed: control_guard::current_check().unwrap_or_else(|| Arc::new(|| true)),
    };
    let form = op
        .original
        .as_ref()
        .map(|s| s.creation.clone())
        .unwrap_or_default();
    let result = scope
        .scope(muster_platform::macos::navigation_restore::track_form(
            form, operation,
        ))
        .await;
    if covered {
        scope.drain().await;
        if let Some(token) = op.pending() {
            restore(token).await;
        }
        op.cleaned = true;
    }
    result
}
