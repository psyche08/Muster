//! A reply may release only the currently displayed decision for its local viewer.
use crate::Error;
use muster_core::DeviceId;
use std::sync::Mutex;
use tokio::sync::oneshot;

struct Pending {
    peer: DeviceId,
    id: u64,
    reply: oneshot::Sender<()>,
}
#[derive(Default)]
pub(crate) struct Confirmation(Mutex<Option<Pending>>);

pub(super) struct Prompt<'a> {
    owner: &'a Confirmation,
    pub id: u64,
    pub reply: oneshot::Receiver<()>,
}
impl Drop for Prompt<'_> {
    fn drop(&mut self) {
        let mut p = self.owner.0.lock().unwrap();
        if p.as_ref().is_some_and(|v| v.id == self.id) {
            *p = None;
        }
    }
}

impl Confirmation {
    pub(super) fn ask(&self, peer: DeviceId) -> Prompt<'_> {
        let id = rand::random::<std::num::NonZeroU64>().get();
        let (tx, reply) = oneshot::channel();
        *self.0.lock().unwrap() = Some(Pending {
            peer,
            id,
            reply: tx,
        });
        Prompt {
            owner: self,
            id,
            reply,
        }
    }

    pub(super) fn confirm(&self, peer: DeviceId, id: u64) -> Result<(), Error> {
        let stale = || Error::Invalid("desktop confirmation expired".into());
        let mut p = self.0.lock().unwrap();
        if !p.as_ref().is_some_and(|v| v.id == id && v.peer == peer) {
            return Err(stale());
        }
        p.take()
            .ok_or_else(stale)?
            .reply
            .send(())
            .map_err(|_| stale())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn only_the_current_local_prompt_for_the_same_host_can_be_confirmed() {
        let c = Confirmation::default();
        let a = DeviceId([1; 32]);
        let b = DeviceId([2; 32]);
        let old = c.ask(a);
        let mut current = c.ask(a);
        assert!(c.confirm(a, old.id).is_err());
        drop(old);
        assert!(c.confirm(b, current.id).is_err());
        c.confirm(a, current.id).unwrap();
        assert!(current.reply.try_recv().is_ok());
        assert!(c.confirm(a, current.id).is_err());
        let stale = c.ask(b);
        let id = stale.id;
        drop(stale);
        assert!(c.confirm(b, id).is_err());
    }
}
