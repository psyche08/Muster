//! Execution evidence, paired only by unambiguous provider tool IDs.
use muster_proto::{app::TraceSpan, v1::AgentMessage};
use std::collections::HashMap;

pub fn spans(messages: &[AgentMessage], own: &str) -> Vec<TraceSpan> {
    let mut spans: Vec<TraceSpan> = Vec::new();
    let mut counts = HashMap::new();
    let mut pending = HashMap::new();
    for m in messages
        .iter()
        .filter(|m| m.kind == "tool_call" && !m.tool_call_id.is_empty())
    {
        *counts.entry(m.tool_call_id.as_str()).or_insert(0usize) += 1;
    }
    for m in messages {
        let status = match m.tool_status.as_str() {
            "completed" | "failed" | "interrupted" => m.tool_status.as_str(),
            "running" if matches!(own, "running" | "waiting_approval") => "running",
            _ => "unknown",
        };
        if m.kind == "tool_result"
            && !m.tool_call_id.is_empty()
            && let Some(index) = pending.remove(m.tool_call_id.as_str())
        {
            let span: &mut TraceSpan = &mut spans[index];
            span.status = status.into();
            span.ended_unix = m.ts_unix;
            span.duration_known = span.started_unix > 0 && m.ts_unix >= span.started_unix;
            span.duration_seconds = if span.duration_known {
                m.ts_unix - span.started_unix
            } else {
                0
            };
            span.detail.push_str("\n\nOutput\n");
            span.detail.extend(m.text.chars().take(2000));
            continue;
        }
        let (kind, title) = match m.kind.as_str() {
            "tool_call" => (
                "tool",
                if m.tool_name.is_empty() {
                    "Tool"
                } else {
                    &m.tool_name
                },
            ),
            "tool_result" => ("tool", "Unpaired tool output"),
            "turn_summary" => ("turn", "Turn completed"),
            "turn_status" => ("status", "Turn status"),
            _ if m.phase == "reasoning" => ("reasoning", "Reasoning"),
            _ if m.role == "user" && m.kind == "text" => ("turn", "User message"),
            _ => continue,
        };
        if m.kind == "tool_call"
            && !m.tool_call_id.is_empty()
            && counts.get(m.tool_call_id.as_str()) == Some(&1)
        {
            pending.insert(m.tool_call_id.as_str(), spans.len());
        }
        spans.push(TraceSpan {
            id: m.id.clone(),
            kind: kind.into(),
            title: title.into(),
            detail: if m.kind == "turn_summary" {
                m.usage
                    .as_ref()
                    .map(|u| {
                        format!(
                            "{} · input {} · output {}",
                            u.model, u.input_tokens, u.output_tokens
                        )
                    })
                    .unwrap_or_default()
            } else {
                m.text.chars().take(2000).collect()
            },
            status: if m.kind == "turn_summary" {
                "completed".into()
            } else {
                status.into()
            },
            started_unix: m.ts_unix,
            ..Default::default()
        });
    }
    spans
}

#[cfg(test)]
mod tests {
    use super::*;
    fn tool(id: &str, kind: &str, call: &str, ts: u64, status: &str) -> AgentMessage {
        AgentMessage {
            id: id.into(),
            kind: kind.into(),
            tool_call_id: call.into(),
            ts_unix: ts,
            tool_status: status.into(),
            ..Default::default()
        }
    }
    #[test]
    fn parallel_calls_pair_by_id_and_keep_unknown_or_reversed_times_unknown() {
        let m = vec![
            tool("a", "tool_call", "a", 10, "running"),
            tool("b", "tool_call", "b", 11, "running"),
            tool("r-b", "tool_result", "b", 15, "failed"),
            tool("r-a", "tool_result", "a", 13, "completed"),
        ];
        let s = spans(&m, "idle");
        assert_eq!(s.len(), 2);
        assert_eq!(s[0].duration_seconds, 3);
        assert_eq!(s[1].duration_seconds, 4);
        assert_eq!(s[1].status, "failed");
        let m = vec![
            tool("a", "tool_call", "same", 10, "running"),
            tool("b", "tool_call", "same", 11, "running"),
            tool("r", "tool_result", "same", 15, "completed"),
        ];
        assert_eq!(spans(&m, "idle").len(), 3);
        for (start, end) in [(0, 2), (10, 9)] {
            let s = spans(
                &[
                    tool("a", "tool_call", "a", start, "running"),
                    tool("r", "tool_result", "a", end, "completed"),
                ],
                "idle",
            );
            assert!(!s[0].duration_known);
        }
        assert_eq!(
            spans(&[tool("a", "tool_call", "a", 10, "running")], "idle")[0].status,
            "unknown"
        );
    }
}
