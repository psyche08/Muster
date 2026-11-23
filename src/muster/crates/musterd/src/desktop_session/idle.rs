//! A parked resource has one taker and a deadline measured from disconnect.
use std::{future::Future, sync::Mutex, time::Duration};
use tokio::{
    sync::oneshot,
    time::{Instant, timeout_at},
};

const GRACE: Duration = Duration::from_secs(30);

pub(crate) struct IdleSlot<T> {
    next: Mutex<Option<oneshot::Sender<oneshot::Sender<T>>>>,
}

impl<T> Default for IdleSlot<T> {
    fn default() -> Self {
        Self {
            next: Mutex::new(None),
        }
    }
}

impl<T: Send + 'static> IdleSlot<T> {
    pub(super) fn park(&self, reset: impl Future<Output = Option<T>> + Send + 'static) {
        let deadline = Instant::now() + GRACE;
        let (offer, next) = oneshot::channel::<oneshot::Sender<T>>();
        *self.next.lock().unwrap() = Some(offer);
        tokio::spawn(async move {
            let handoff = async {
                if let Some(resource) = reset.await
                    && let Ok(reply) = next.await
                    && Instant::now() < deadline
                {
                    let _ = reply.send(resource);
                }
            };
            let _ = timeout_at(deadline, handoff).await;
        });
    }

    pub(super) async fn take(&self) -> Option<T> {
        let offer = self.next.lock().unwrap().take()?;
        let (reply, resource) = oneshot::channel();
        offer.send(reply).ok()?;
        resource.await.ok()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn idle_begins_on_disconnect_and_each_resource_is_claimed_once() {
        let slot = IdleSlot::default();
        assert!(slot.take().await.is_none());
        slot.park(async { Some(1) });
        assert_eq!(slot.take().await, Some(1));
        assert!(slot.take().await.is_none());
        slot.park(async { None });
        assert!(slot.take().await.is_none());
    }

    #[tokio::test]
    async fn expired_resource_is_dropped_without_a_new_request() {
        let slot = IdleSlot::default();
        let (held, dropped) = oneshot::channel::<()>();
        let start = Instant::now();
        slot.park(async { Some(held) });
        assert!(
            tokio::time::timeout(GRACE + Duration::from_secs(5), dropped)
                .await
                .unwrap()
                .is_err()
        );
        assert!(start.elapsed() >= GRACE);
        assert!(slot.take().await.is_none());
    }
}
