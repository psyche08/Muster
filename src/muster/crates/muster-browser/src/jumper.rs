//! One local operator-configured terminal probe. The extension only receives
//! fixed browser effects on the serialized native channel for this invocation.
use anyhow::{Context, Result, ensure};
use serde::Deserialize;
use serde_json::{Value, json};
use std::{path::Path, time::Duration};
use tokio::{io::AsyncWrite, sync::mpsc::Receiver, time::Instant};

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Binding {
    site: String,
    host: String,
}

impl Binding {
    fn load(dir: &Path) -> Result<Self> {
        let data = std::fs::read(dir.join("jumper.json")).context("missing browser/jumper.json")?;
        ensure!(data.len() <= 512, "invalid terminal binding");
        let binding: Self = serde_json::from_slice(&data)?;
        for name in [&binding.site, &binding.host] {
            ensure!(
                name.len() <= 253
                    && name.split('.').all(|label| {
                        !label.is_empty()
                            && label.len() <= 63
                            && label
                                .starts_with(|c: char| c.is_ascii_lowercase() || c.is_ascii_digit())
                            && !label.ends_with('-')
                            && label
                                .bytes()
                                .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == b'-')
                    }),
                "expected a literal DNS name"
            );
        }
        ensure!(
            binding.site.contains('.') && binding.host != "jumper",
            "invalid terminal binding"
        );
        Ok(binding)
    }
}

struct Channel<'a, W> {
    out: &'a mut W,
    rx: &'a mut Receiver<Result<Value>>,
    id: &'a mut u64,
}

// Rows contain only known prompts/commands, masked unrelated output, and a
// possible single account line. Chrome crops each write to its fresh echo.
fn evidence(screen: &Value, stage: &str, host: &str) -> Option<Value> {
    let rows = screen["rows"].as_array()?;
    let gateway = json!(["Jumper", ""]);
    let target = json!([host, ""]);
    let last = rows.last()?;
    match stage {
        "firstHop" if *last == gateway => Some(json!({"state":"gateway"})),
        "connect" if *last == target => Some(json!({"state":"target"})),
        "whoami" if rows.len() == 3 && rows[0] == json!([host, "whoami"]) && *last == target => {
            Some(json!({"state":"output","account":rows[1].as_str()?}))
        }
        "exit" if rows.first()? == &json!([host, "exit"]) && *last == gateway => {
            Some(json!({"state":"gatewayReturned"}))
        }
        _ => None,
    }
}

impl<W: AsyncWrite + Unpin> Channel<'_, W> {
    async fn effect(&mut self, command: Value, stage: &str) -> Result<Value> {
        *self.id += 1;
        let reply = super::exchange(self.out, self.rx, command, *self.id, super::DEADLINE)
            .await
            .with_context(|| format!("{stage}: native channel lost; outcome unknown"))?;
        // Never return exception text or a URL from the page.
        Ok(reply
            .get("result")
            .cloned()
            .unwrap_or_else(|| json!({"state":"adapterFailed"})))
    }

    async fn observe(&mut self, stage: &str, host: &str) -> Result<Value> {
        let until = Instant::now() + Duration::from_secs(23);
        loop {
            let observed = self.effect(json!({"op":"jumperObserve"}), stage).await?;
            if observed["state"] == "screen" {
                if let Some(value) = evidence(&observed, stage, host) {
                    return Ok(value);
                }
            } else {
                match observed["state"].as_str() {
                    Some("waiting" | "unsupportedRenderer") => (),
                    Some("authenticationRequired" | "documentChanged") => {
                        return Ok(observed);
                    }
                    _ => return Ok(json!({"state":"adapterFailed"})),
                }
            }
            if Instant::now() >= until {
                return Ok(if observed["state"] == "unsupportedRenderer" {
                    observed
                } else {
                    json!({"state":"timeout"})
                });
            }
            tokio::time::sleep(Duration::from_millis(225)).await;
        }
    }

    async fn run(&mut self, binding: &Binding) -> Result<Value> {
        let opened = self
            .effect(
                json!({"op":"jumperOpen","site":binding.site,"host":binding.host}),
                "open",
            )
            .await?;
        if opened["state"] != "opened" {
            return Ok(json!({"stage":"open","status":"failed"}));
        }
        let mut output = None;
        for (stage, expected, action) in [
            ("firstHop", "gateway", Some("connect")),
            ("connect", "target", Some("whoami")),
            ("whoami", "output", Some("exit")),
            ("exit", "gatewayReturned", None),
        ] {
            let observed = self.observe(stage, &binding.host).await?;
            if observed["state"] != expected {
                let status = match observed["state"].as_str() {
                    Some("authenticationRequired") => "authenticationRequired",
                    Some("timeout") => "timeout",
                    Some("documentChanged") => "documentChanged",
                    Some("unsupportedRenderer") => "unsupportedRenderer",
                    _ => "adapterFailed",
                };
                return Ok(
                    json!({"stage":stage,"status":status,"outcomeUnknown":stage != "firstHop"}),
                );
            }
            if stage == "whoami" {
                let name = observed["account"].as_str().unwrap_or_default();
                if name == "whoami"
                    || name.is_empty()
                    || name.len() > 64
                    || !name
                        .bytes()
                        .all(|c| c.is_ascii_alphanumeric() || b"_.-".contains(&c))
                {
                    return Ok(
                        json!({"stage":stage,"status":"ambiguousOutput","outcomeUnknown":true}),
                    );
                }
                output = Some(name.to_owned());
            }
            if let Some(action) = action {
                let receipt = self
                    .effect(json!({"op":"jumperInput","action":action}), action)
                    .await?;
                if receipt["state"] == "authenticationRequired" {
                    return Ok(
                        json!({"stage":action,"status":"authenticationRequired","outcomeUnknown":false}),
                    );
                }
                if receipt["state"] != "sent" {
                    return Ok(
                        json!({"stage":action,"status":"inputUnconfirmed","outcomeUnknown":true}),
                    );
                }
            }
        }
        Ok(
            json!({"status":"closed","host":binding.host,"whoami":output,"closeEvidence":"exit echo followed by first-hop prompt"}),
        )
    }
}

pub async fn probe<W: AsyncWrite + Unpin>(
    dir: &Path,
    out: &mut W,
    rx: &mut Receiver<Result<Value>>,
    id: &mut u64,
) -> Result<Value> {
    let binding = match Binding::load(dir) {
        Ok(binding) => binding,
        Err(e) => return Ok(json!({"error":format!("terminal configuration: {e}")})),
    };
    let mut channel = Channel { out, rx, id };
    // Transport errors are fatal to the host. Chrome releases its owned tab
    // and debugger on disconnect; no command is sent again on the new port.
    let mut result = channel.run(&binding).await?;
    let retain = result["status"] == "authenticationRequired";
    let released = channel
        .effect(json!({"op":"jumperRelease","retain":retain}), "release")
        .await?;
    if released["state"] != "released" {
        return Ok(json!({"error":"release: terminal cleanup unconfirmed"}));
    }
    result["pageRetained"] = json!(released["pageRetained"].as_bool().unwrap_or(retain));
    Ok(json!({"result":result}))
}
