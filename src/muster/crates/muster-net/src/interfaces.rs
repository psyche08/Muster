//! One short-lived OS interface snapshot shared by candidate exchange and DNS.
//! Network-change notifications invalidate it immediately; TTL covers missed events.
use std::{
    io,
    sync::{Arc, Mutex, OnceLock},
    time::{Duration, Instant},
};

struct Snapshot<T> {
    value: Option<(Instant, Arc<T>)>,
}
impl<T> Default for Snapshot<T> {
    fn default() -> Self {
        Self { value: None }
    }
}
impl<T> Snapshot<T> {
    fn get(&mut self, now: Instant, read: impl FnOnce() -> io::Result<T>) -> io::Result<Arc<T>> {
        if let Some((at, value)) = &self.value
            && now.saturating_duration_since(*at) < Duration::from_secs(5)
        {
            return Ok(value.clone());
        }
        let value = Arc::new(read()?);
        self.value = Some((now, value.clone()));
        Ok(value)
    }
}
fn cache() -> &'static Mutex<Snapshot<Vec<if_addrs::Interface>>> {
    static CACHE: OnceLock<Mutex<Snapshot<Vec<if_addrs::Interface>>>> = OnceLock::new();
    CACHE.get_or_init(Mutex::default)
}
pub(crate) fn snapshot() -> io::Result<Arc<Vec<if_addrs::Interface>>> {
    // Serialize refreshes too: simultaneous peer exchanges make one OS call.
    cache()
        .lock()
        .unwrap()
        .get(Instant::now(), if_addrs::get_if_addrs)
}
pub(crate) fn invalidate() {
    cache().lock().unwrap().value = None;
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn shares_enumeration_expires_and_refreshes_after_network_change() {
        let mut cache = Snapshot::default();
        let now = Instant::now();
        let mut calls = 0;
        for _ in 0..100 {
            assert_eq!(
                *cache
                    .get(now, || {
                        calls += 1;
                        Ok(vec!["en0"])
                    })
                    .unwrap(),
                vec!["en0"]
            );
        }
        assert_eq!(calls, 1);
        cache.value = None;
        assert_eq!(
            *cache
                .get(now, || {
                    calls += 1;
                    Ok(vec!["en5"])
                })
                .unwrap(),
            vec!["en5"]
        );
        assert_eq!(calls, 2);
        assert_eq!(
            *cache
                .get(now + Duration::from_secs(5), || {
                    calls += 1;
                    Ok(vec![])
                })
                .unwrap(),
            Vec::<&str>::new()
        );
        assert_eq!(calls, 3);
    }
    #[test]
    fn failures_are_not_cached_or_replaced_by_stale_interfaces() {
        let mut cache = Snapshot::default();
        let now = Instant::now();
        cache.get(now, || Ok(vec![1])).unwrap();
        assert!(
            cache
                .get(now + Duration::from_secs(6), || Err(io::Error::other(
                    "offline"
                )))
                .is_err()
        );
        assert_eq!(
            *cache
                .get(now + Duration::from_secs(6), || Ok(vec![2]))
                .unwrap(),
            vec![2]
        );
    }
}
