//! Local App-plugin contract. This is not a device RPC or a CLI controller.
//! Rust owns capability, binding and receipt decisions; host projections are
//! facts to validate, never permission to execute or replay an operation.
use std::collections::HashSet;

use serde_json::{Value, json};
use sha2::{Digest, Sha256};

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Contract {
    pub epoch: String,
    capabilities: HashSet<String>,
}

fn bounded(value: &Value, max: usize) -> Option<&str> {
    value.as_str().filter(|s| !s.is_empty() && s.len() <= max)
}

impl Contract {
    /// Absence means the installed legacy local protocol. A present but invalid
    /// contract must fail closed, not silently downgrade to that protocol.
    pub fn parse(state: &Value, provider: &str) -> Result<Option<Self>, crate::Error> {
        let Some(value) = state.get("control") else {
            return Ok(None);
        };
        let invalid = || crate::Error::Unavailable("Unsupported App plugin contract".into());
        if value["version"] != 1 || value["domain"] != "app" || value["provider"] != provider {
            return Err(invalid());
        }
        let epoch = bounded(&value["epoch"], 128)
            .ok_or_else(invalid)?
            .to_owned();
        let entries = value["capabilities"].as_array().ok_or_else(invalid)?;
        if entries.len() > 64 {
            return Err(invalid());
        }
        let mut capabilities = HashSet::new();
        for entry in entries {
            if !capabilities.insert(bounded(entry, 64).ok_or_else(invalid)?.to_owned()) {
                return Err(invalid());
            }
        }
        Ok(Some(Self {
            epoch,
            capabilities,
        }))
    }

    pub fn offers(&self, capability: &str) -> bool {
        self.capabilities.contains(capability)
    }

    pub fn bind(
        &self,
        session: &str,
        operation: &str,
        request: &mut Value,
    ) -> Result<(), crate::Error> {
        if !request["op"].as_str().is_some_and(|op| self.offers(op)) {
            return Err(crate::Error::Unavailable(
                "App plugin does not offer this operation".into(),
            ));
        }
        if session.is_empty()
            || session.len() > 128
            || operation.is_empty()
            || operation.len() > 256
        {
            return Err(crate::Error::Invalid(
                "Invalid App plugin operation binding".into(),
            ));
        }
        request["control"] = json!({"version":1,"domain":"app","epoch":self.epoch,
            "sessionId":session,"operationId":operation});
        Ok(())
    }
}

/// Reconstructable after daemon restart, but scoped to the original ledger
/// operation and App session. The ledger must validate the original request body
/// before allowing a lookup. Never use this ID to replay an unknown prompt.
pub fn prompt_id(session: &str, operation: &str) -> String {
    uuid(
        b"MUSTER-APP-PLUGIN-PROMPT-V1\0",
        &[session, operation],
        0x50,
    )
}

/// The App session a native creation names, derived from its ledger
/// operation (ADR 0167): a retry of the same operation can only name the same
/// session, and its first message's receipt is exact. It carries the
/// version-4 form that the plugin's task methods accept; the plugin creates
/// it only where no session of that name exists.
pub fn created_session(operation: &str) -> String {
    format!(
        "session-{}",
        uuid(b"MUSTER-APP-PLUGIN-CREATE-V1\0", &[operation], 0x40)
    )
}

/// A UUID from SHA-256 over `domain` and length-prefixed `fields`, with
/// `version` in its version nibble and the RFC 4122 variant.
fn uuid(domain: &[u8], fields: &[&str], version: u8) -> String {
    let mut digest = Sha256::new();
    digest.update(domain);
    for field in fields {
        digest.update((field.len() as u64).to_be_bytes());
        digest.update(field.as_bytes());
    }
    let mut bytes: [u8; 32] = digest.finalize().into();
    bytes[6] = (bytes[6] & 0x0f) | version;
    bytes[8] = (bytes[8] & 0x3f) | 0x80;
    let h = hex::encode(&bytes[..16]);
    format!(
        "{}-{}-{}-{}-{}",
        &h[..8],
        &h[8..12],
        &h[12..16],
        &h[16..20],
        &h[20..]
    )
}

/// The host's inbox/session event must name this exact native request. Empty,
/// missing, malformed or unrelated evidence is unknown, never "not sent".
pub fn confirms_prompt(value: &Value, session: &str, request: &str) -> bool {
    if value["sessionId"] != session || value["requestId"] != request {
        return false;
    }
    let Some(records) = value["records"].as_array().filter(|r| r.len() <= 16) else {
        return false;
    };
    records.iter().any(|record| {
        record["source"]["kind"] == "user"
            && record["source"]["rpcId"] == request
            && matches!(
                record["location"].as_str(),
                Some("next-turn" | "next-step" | "session-event")
            )
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn state() -> Value {
        json!({"control":{"version":1,"domain":"app","provider":"deepseek",
            "epoch":"launch-a","capabilities":["prompt","prompt.lookup"]}})
    }

    #[test]
    fn app_contract_cannot_be_cli_or_an_invalid_downgrade() {
        assert!(
            Contract::parse(&json!({"protocol":1}), "deepseek")
                .unwrap()
                .is_none()
        );
        for (key, bad) in [
            ("domain", json!("cli")),
            ("provider", json!("codex")),
            ("version", json!(2)),
            ("epoch", json!("")),
            ("capabilities", json!(["prompt", "prompt"])),
        ] {
            let mut s = state();
            s["control"][key] = bad;
            assert!(Contract::parse(&s, "deepseek").is_err());
        }
        assert!(Contract::parse(&json!({"control":null}), "deepseek").is_err());
        let c = Contract::parse(&state(), "deepseek").unwrap().unwrap();
        let mut request = json!({"op":"prompt"});
        c.bind("session-a", "operation-a", &mut request).unwrap();
        assert_eq!(request["control"]["domain"], "app");
        assert_eq!(request["control"]["epoch"], "launch-a");
        assert!(
            c.bind("session-a", "operation-a", &mut json!({"op":"set-model"}))
                .is_err()
        );
    }

    #[test]
    fn ids_and_receipts_are_session_and_operation_bound() {
        let id = prompt_id("session-a", "op-1");
        assert_eq!(id, prompt_id("session-a", "op-1"));
        assert_ne!(id, prompt_id("session-b", "op-1"));
        assert_ne!(id, prompt_id("session-a", "op-2"));
        assert_ne!(prompt_id("ab", "c"), prompt_id("a", "bc"));
        let created = created_session("op-1");
        assert_eq!(created, created_session("op-1"));
        assert_ne!(created, created_session("op-2"));
        // The form the plugin's task methods accept (index.js, taskSession).
        let uuid = created.strip_prefix("session-").unwrap();
        let groups: Vec<&str> = uuid.split('-').collect();
        assert_eq!(
            groups.iter().map(|g| g.len()).collect::<Vec<_>>(),
            [8, 4, 4, 4, 12]
        );
        assert!(
            uuid.bytes()
                .all(|b| matches!(b, b'-' | b'0'..=b'9' | b'a'..=b'f'))
        );
        assert!(groups[2].starts_with('4') && "89ab".contains(&groups[3][..1]));
        assert!(!created.ends_with(&prompt_id("", "op-1")));
        assert_eq!(&prompt_id("session-a", "op-1")[14..15], "5");
        let receipt = json!({"sessionId":"session-a","requestId":id,"records":[
            {"source":{"kind":"user","rpcId":id},"location":"session-event"}]});
        assert!(confirms_prompt(&receipt, "session-a", &id));
        assert!(!confirms_prompt(&receipt, "session-b", &id));
        assert!(!confirms_prompt(&receipt, "session-a", "another"));
        let mut bad = receipt.clone();
        bad["records"][0]["source"]["kind"] = json!("model");
        assert!(!confirms_prompt(&bad, "session-a", &id));
        assert!(!confirms_prompt(
            &json!({"accepted":true}),
            "session-a",
            &id
        ));
    }
}
