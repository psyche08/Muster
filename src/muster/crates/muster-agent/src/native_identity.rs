//! Provider facts for ownership checks; no task or permission decisions here.
use crate::{AgentService, Error, model::Provider};

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct NativeIdentity {
    pub provider: String,
    pub store: String,
    pub session: String,
}

impl AgentService {
    pub fn deepseek_task_store(&self) -> Result<String, Error> {
        self.config
            .deepseek_home
            .as_ref()
            .and_then(|p| p.canonicalize().ok())
            .and_then(|p| p.to_str().map(str::to_owned))
            .ok_or_else(|| Error::Unavailable("Original DSH App store is unavailable".into()))
    }

    /// Exact configured native store plus provider-declared parent identities.
    /// A missing parent transcript does not erase the declared parent identity.
    pub fn native_lineage(&self, id: &str) -> Result<Vec<NativeIdentity>, Error> {
        let inner = self.inner.lock().unwrap();
        let Some(found) = inner.catalog.get(id) else {
            return Ok(Vec::new()); // independent/cloud sources have no local store
        };
        let root = match found.provider {
            Provider::Codex => &self.config.codex_home,
            Provider::Claude => &self.config.claude_home,
            Provider::Catdesk => &self.config.catdesk_home,
            Provider::Deepseek => &self.config.deepseek_home,
        }
        .as_ref()
        .ok_or_else(|| Error::Unavailable("Native provider store unavailable".into()))?
        .canonicalize()
        .map_err(|_| Error::Unavailable("Native provider store unavailable".into()))?;
        let store = root
            .to_str()
            .ok_or_else(|| Error::Unavailable("Native provider store is not UTF-8".into()))?
            .to_owned();
        let provider = found.provider;
        let mut next = Some(id.to_owned());
        let mut result: Vec<NativeIdentity> = Vec::new();
        while let Some(session) = next.take() {
            if result.len() == 64 || result.iter().any(|i| i.session == session) {
                return Err(Error::Unavailable(
                    "Native parent identity is cyclic or too deep".into(),
                ));
            }
            next = match inner.catalog.get(&session) {
                Some(row) if row.provider == provider => row.parent_id.clone(),
                Some(_) => return Err(Error::Unavailable("Native parent provider differs".into())),
                None => None,
            };
            result.push(NativeIdentity {
                provider: provider.as_str().into(),
                store: store.clone(),
                session,
            });
        }
        Ok(result)
    }
}
