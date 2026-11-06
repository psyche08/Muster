//! The daemon's task-domain boundary, shared by local and remote callers.
use crate::{Daemon, Error};
use muster_store::multica::NativeSession;

pub(crate) fn ordinary(daemon: &Daemon, session: &str) -> Result<(), Error> {
    let identities = daemon
        .agent()
        .map(|agent| agent.native_lineage(session))
        .transpose()?
        .unwrap_or_default();
    let device = daemon.device_id().to_string();
    let denied = || Error::Unauthorized("Task session is available only through Multica".into());
    // An undiscovered session cannot use a cached receipt to bypass ownership.
    // This fallback only denies: it never binds an unproven store to a task.
    if identities.is_empty() && daemon.store().multica_has_session(&device, session)? {
        return Err(denied());
    }
    for identity in identities {
        let native = NativeSession {
            device: device.clone(),
            provider: identity.provider,
            store: identity.store,
            session: identity.session,
        };
        if daemon.store().multica_owner(&native)?.is_some() {
            return Err(denied());
        }
    }
    Ok(())
}
