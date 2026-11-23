//! Serialize admission decisions independently from native engine teardown.
use muster_core::DeviceId;
use std::sync::{Arc, Mutex};
use tokio_util::sync::CancellationToken;

#[derive(Clone)]
pub(super) struct Occupant {
    pub peer: DeviceId,
    generation: Arc<()>,
    stop: CancellationToken,
}

#[derive(Default)]
pub(crate) struct Occupancy(Mutex<Option<Occupant>>);

pub(super) struct Reservation(Arc<Occupancy>, Arc<()>);

impl Drop for Reservation {
    fn drop(&mut self) {
        let mut current = self.0.0.lock().unwrap();
        if current
            .as_ref()
            .is_some_and(|v| Arc::ptr_eq(&v.generation, &self.1))
        {
            *current = None;
        }
    }
}

impl Occupancy {
    pub(super) fn reserve(
        self: &Arc<Self>,
        peer: DeviceId,
        stop: &CancellationToken,
        replace: Option<&Occupant>,
    ) -> Result<Reservation, Option<Occupant>> {
        let mut current = self.0.lock().unwrap();
        match (current.as_ref(), replace) {
            (None, None) => {}
            (Some(owner), Some(expected))
                if Arc::ptr_eq(&owner.generation, &expected.generation) =>
            {
                owner.stop.cancel()
            }
            _ => return Err(current.clone()),
        }
        let generation = Arc::new(());
        *current = Some(Occupant {
            peer,
            generation: generation.clone(),
            stop: stop.clone(),
        });
        Ok(Reservation(self.clone(), generation))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn confirmation_can_only_replace_the_observed_session() {
        let slot = Arc::new(Occupancy::default());
        let a = DeviceId([1; 32]);
        let b = DeviceId([2; 32]);
        let c = DeviceId([3; 32]);
        let first = CancellationToken::new();
        let second = CancellationToken::new();
        let third = CancellationToken::new();
        let lease_a = slot.reserve(a, &first, None).ok().unwrap();
        let observed = slot.reserve(b, &second, None).err().flatten().unwrap();
        assert_eq!(observed.peer, a);
        assert!(!first.is_cancelled());
        let lease_b = slot.reserve(b, &second, Some(&observed)).ok().unwrap();
        assert!(first.is_cancelled());
        drop(lease_a);
        assert_eq!(
            slot.reserve(c, &third, Some(&observed))
                .err()
                .flatten()
                .unwrap()
                .peer,
            b
        );
        assert!(!second.is_cancelled());
        drop(lease_b);
        assert!(matches!(
            slot.reserve(c, &third, Some(&observed)),
            Err(None)
        ));
        assert!(slot.reserve(c, &third, None).is_ok());
    }
}
