//! Normalize Orca's journal into Muster's existing native timeline. No HTML,
//! JavaScript renderer or terminal screen scraping crosses the App ABI.
use super::*;

pub(super) fn interaction_id(item: &Value) -> String {
    format!(
        "orca-prompt:{}:{}",
        hex::encode(item["itemId"].as_str().unwrap_or_default()),
        item["revision"].as_u64().unwrap_or(0)
    )
}
pub(super) fn project(
    session: &str,
    items: &[Value],
) -> Result<(Vec<AgentMessage>, Vec<PendingInteraction>), Error> {
    let mut messages = Vec::new();
    let mut pending = Vec::new();
    for item in items {
        let key = string(item, "itemId")?;
        let body = &item["body"];
        let mut row = AgentMessage {
            id: key.clone(),
            ts_unix: item["observedAt"].as_u64().unwrap_or(0) / 1000,
            role: "system".into(),
            kind: "text".into(),
            ..Default::default()
        };
        match body["kind"].as_str() {
            Some("message") => {
                let role = body["role"].as_str().unwrap_or("system");
                row.role = match role {
                    "user" | "assistant" | "tool" | "system" => role,
                    "reasoning" => "assistant",
                    _ => "system",
                }
                .into();
                if role == "reasoning" {
                    row.phase = "reasoning".into();
                }
                for (index, block) in body["blocks"]
                    .as_array()
                    .ok_or_else(invalid)?
                    .iter()
                    .enumerate()
                {
                    let mut part = row.clone();
                    part.id = format!("{}:{index}", hex::encode(&key));
                    match block["type"].as_str() {
                        Some("text") => {
                            part.text = block["text"].as_str().unwrap_or_default().into()
                        }
                        Some("tool-call") => {
                            part.kind = "tool_call".into();
                            part.tool_name = block["name"].as_str().unwrap_or("tool").into();
                            part.tool_call_id = block["id"].as_str().unwrap_or(&key).into();
                            part.text = block["input"].to_string();
                        }
                        Some("tool-result") => {
                            part.kind = "tool_result".into();
                            part.role = "tool".into();
                            part.tool_call_id = block["toolUseId"].as_str().unwrap_or(&key).into();
                            part.text = block["text"].as_str().unwrap_or("Tool result").into();
                            part.tool_status = tool_status(&block["state"]).into();
                        }
                        Some("image-ref") => {
                            part.text = format!(
                                "[Image: {}]",
                                block["alt"].as_str().unwrap_or("not rendered")
                            )
                        }
                        _ => part.text = "[Additional session content not rendered]".into(),
                    }
                    if !part.text.is_empty() || part.kind == "tool_result" {
                        messages.push(part);
                    }
                }
                continue;
            }
            Some("tool-call") => {
                row.role = "assistant".into();
                row.kind = "tool_call".into();
                row.tool_name = body["name"].as_str().unwrap_or("tool").into();
                row.tool_call_id = body["callId"].as_str().unwrap_or(&key).into();
                row.text = body["input"].to_string();
                row.tool_status = tool_status(&body["state"]).into();
                messages.push(row.clone());
                if body["output"].is_object()
                    && (!body["output"]["head"]
                        .as_str()
                        .unwrap_or_default()
                        .is_empty()
                        || matches!(
                            row.tool_status.as_str(),
                            "completed" | "failed" | "interrupted"
                        ))
                {
                    row.id = format!("{key}:output");
                    row.kind = "tool_result".into();
                    row.role = "tool".into();
                    row.text = bounded(&body["output"]);
                    messages.push(row);
                }
                continue;
            }
            Some("diff") => {
                row.kind = "tool_result".into();
                row.tool_name = "diff".into();
                // The patch already carries file headers. A display label before
                // `diff --git` would become a spurious zero-change file card.
                row.text = bounded(&body["patch"]);
                if row.text.is_empty() {
                    row.text = body["path"].as_str().unwrap_or_default().into();
                }
            }
            Some("approval" | "question") => {
                row.text = [
                    "title",
                    "description",
                    "detail",
                    "question",
                    "decisionReason",
                    "blockedPath",
                ]
                .iter()
                .filter_map(|k| body[k].as_str())
                .collect::<Vec<_>>()
                .join("\n");
                if body["subject"]["kind"] == "plan" {
                    row.text.push_str(&format!(
                        "\n{}",
                        body["subject"]["text"].as_str().unwrap_or_default()
                    ));
                }
                // Only the exact offered approval labels are actionable. Unknown
                // subject kinds and structured question forms remain visibly unavailable.
                if body["kind"] == "approval"
                    && body["resolution"]["state"] == "pending"
                    && (body["subject"].is_null() || body["subject"]["kind"] == "plan")
                {
                    let options = body["options"]
                        .as_array()
                        .ok_or_else(invalid)?
                        .iter()
                        .map(|o| string(o, "label"))
                        .collect::<Result<Vec<_>, _>>()?;
                    let unique = options
                        .iter()
                        .collect::<std::collections::HashSet<_>>()
                        .len()
                        == options.len();
                    if unique && !options.is_empty() {
                        pending.push(PendingInteraction {
                            id: interaction_id(item),
                            session: session.into(),
                            kind: "approval".into(),
                            prompt: row.text.clone(),
                            options,
                            ..Default::default()
                        });
                    }
                } else if body["kind"] == "question"
                    && body["resolution"]["state"] == "pending"
                    && body["questions"].is_array()
                {
                    let questions = body["questions"]
                        .as_array()
                        .unwrap()
                        .iter()
                        .map(|q| {
                            Ok(muster_proto::v1::InteractionQuestion {
                                id: string(q, "id")?,
                                header: q["header"].as_str().unwrap_or_default().into(),
                                prompt: string(q, "question")?,
                                multi_select: q["multiSelect"] == true,
                                allow_text: q["freeTextQuestionId"].is_string(),
                                options: q["options"]
                                    .as_array()
                                    .ok_or_else(invalid)?
                                    .iter()
                                    .map(|o| {
                                        Ok(muster_proto::v1::QuestionOption {
                                            id: string(o, "id")?,
                                            label: string(o, "label")?,
                                            description: o["description"]
                                                .as_str()
                                                .unwrap_or_default()
                                                .into(),
                                        })
                                    })
                                    .collect::<Result<Vec<_>, Error>>()?,
                            })
                        })
                        .collect::<Result<Vec<_>, Error>>()?;
                    pending.push(PendingInteraction {
                        id: interaction_id(item),
                        session: session.into(),
                        kind: "question".into(),
                        prompt: row.text.clone(),
                        questions,
                        ..Default::default()
                    });
                } else if body["resolution"]["state"] == "pending" {
                    row.text.push_str(
                        "\nThis prompt shape is unsupported. Interrupt the turn to continue.",
                    );
                }
            }
            Some("status") => row.text = body["text"].as_str().unwrap_or_default().into(),
            Some("turn") => {
                row.kind = "turn_status".into();
                row.text = format!("Turn: {}", body["state"].as_str().unwrap_or("unknown"));
                if let Some(detail) = diagnostic::error_text(&body["error"]) {
                    row.text.push('\n');
                    row.text.push_str(&detail);
                }
            }
            _ => row.text = "[Unsupported Orca journal item]".into(),
        }
        if let Some(agent) = item["agentId"].as_str() {
            row.text = format!("[Subagent {agent}]\n{}", row.text);
        }
        messages.push(row);
    }
    Ok((messages, pending))
}
fn bounded(value: &Value) -> String {
    let mut text = value["head"].as_str().unwrap_or_default().to_string();
    if value["truncated"] == true {
        text.push_str("\n[Output truncated by Orca]");
    }
    text
}

// A transcript label is evidence, not a success inference from receiving output.
fn tool_status(value: &Value) -> &'static str {
    match value.as_str() {
        Some("running" | "inProgress") => "running",
        Some("completed") => "completed",
        Some("failed" | "declined") => "failed",
        Some("interrupted" | "cancelled") => "interrupted",
        _ => "",
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn failed_turn_displays_provider_message() {
        let item = json!({"itemId":"end","body":{"kind":"turn","state":"failed","error":{"message":"Please upgrade Codex."}}});
        let (messages, _) = project("s", &[item]).unwrap();
        assert_eq!(messages[0].kind, "turn_status");
        assert_eq!(messages[0].text, "Turn: failed\nPlease upgrade Codex.");
    }
    #[test]
    fn diff_preserves_patch_without_a_synthetic_file_prelude() {
        let patch = "diff --git a/readme.txt b/readme.txt\n--- a/readme.txt\n+++ b/readme.txt\n@@ -1 +1 @@\n-before\n+after\n";
        let item = json!({"itemId":"diff","body":{"kind":"diff","path":"Turn changes","patch":{"head":patch}}});
        let (messages, _) = project("s", &[item]).unwrap();
        assert_eq!(messages.len(), 1);
        assert_eq!(messages[0].tool_name, "diff");
        assert_eq!(messages[0].text, patch);
    }

    #[test]
    fn reasoning_and_turn_boundaries_are_not_plain_prose() {
        let items = vec![
            json!({"itemId":"think","body":{"kind":"message","role":"reasoning","blocks":[{"type":"text","text":"Checking"}]}}),
            json!({"itemId":"end","body":{"kind":"turn","state":"interrupted"}}),
        ];
        let (messages, _) = project("s", &items).unwrap();
        assert_eq!(messages[0].phase, "reasoning");
        assert_eq!(messages[1].kind, "turn_status");
        assert_eq!(messages[1].text, "Turn: interrupted");
    }
    #[test]
    fn tool_receipt_and_status_remain_independent() {
        let item = |state: &str, output: &str| json!({"itemId":"call","body":{"kind":"tool-call","callId":"c","name":"shell","input":"pwd","state":state,"output":{"head":output}}});
        let (running, _) = project("s", &[item("inProgress", "")]).unwrap();
        assert_eq!(running.len(), 1);
        assert_eq!(running[0].tool_status, "running");
        let (partial, _) = project("s", &[item("inProgress", "partial")]).unwrap();
        assert_eq!(partial.len(), 2);
        assert_eq!(partial[1].tool_status, "running");
        let (failed, _) = project("s", &[item("failed", "")]).unwrap();
        assert_eq!(failed.len(), 2);
        assert_eq!(failed[1].tool_status, "failed");
        let (unknown, _) = project("s", &[item("futureState", "output")]).unwrap();
        assert!(unknown[1].tool_status.is_empty());
    }
}
