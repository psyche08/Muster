//! Stable turn presentation assembled by Rust, including legacy transcripts.
mod diff;

use muster_proto::{
    app::{ChatActivityRow, ChatBlock},
    v1::AgentMessage,
};

/// Orca-style presentation keeps prose in the reading flow and collapses only
/// the evidence-heavy rows. The native shell never reconstructs turn membership.
pub fn for_session(messages: &[AgentMessage], running: bool, cli: bool) -> Vec<ChatBlock> {
    if !cli {
        return blocks(messages, running);
    }
    let mut out = Vec::new();
    let mut activity = Vec::new();
    let last_boundary = messages.iter().rposition(|m| m.kind == "turn_status");
    for (index, message) in messages.iter().enumerate() {
        let completed = !running || last_boundary.is_some_and(|end| index <= end);
        if matches!(message.kind.as_str(), "tool_call" | "tool_result")
            && message.tool_name != "diff"
        {
            activity.push(message.clone());
            continue;
        }
        flush(&mut activity, completed, &mut out);
        if message.text.trim().is_empty() {
            continue;
        }
        let (kind, title) = if message.kind == "turn_status" {
            ("notice", message.text.as_str())
        } else if message.tool_name == "diff" && message.kind == "tool_result" {
            ("diff", "File changes")
        } else if message.phase == "reasoning" {
            ("reasoning", "Reasoning")
        } else if message.role == "system" {
            ("notice", message.text.as_str())
        } else {
            ("message", "")
        };
        out.push(ChatBlock {
            id: message.id.clone(),
            kind: kind.into(),
            title: title.into(),
            message: Some(message.clone()),
            completed,
            diffs: if kind == "diff" {
                diff::files(&message.id, &message.text)
            } else {
                Vec::new()
            },
            ..Default::default()
        });
    }
    flush(&mut activity, !running, &mut out);
    out
}

pub fn blocks(messages: &[AgentMessage], running: bool) -> Vec<ChatBlock> {
    let mut out = Vec::new();
    let mut start = 0;
    for (index, message) in messages.iter().enumerate() {
        if message.kind == "turn_summary" {
            turn(&messages[start..index], true, &mut out);
            out.push(ChatBlock {
                id: message.id.clone(),
                kind: "summary".into(),
                message: Some(message.clone()),
                completed: true,
                ..Default::default()
            });
            start = index + 1;
        } else if message.role == "user"
            && message.kind == "text"
            && !message.text.trim().is_empty()
            && !is_compact_context(message)
        {
            // Legacy transcripts can omit usage footers. A new visible prompt
            // must not pull the previous reply into the next turn's work group.
            turn(&messages[start..index], true, &mut out);
            start = index;
        }
    }
    turn(&messages[start..], !running, &mut out);
    out
}

/// Historical Worked groups describe completed transcript segments. Keep a
/// separate live marker when the worker has not finished, or descendants are
/// active, rather than reopening a completed reply or inventing tool messages.
pub(crate) fn activity_indicator(out: &mut Vec<ChatBlock>, own: &str, descendants: &str) {
    let title = if own == "waiting_approval" {
        "Needs you"
    } else if own == "running" {
        if out
            .iter()
            .any(|b| !b.completed && matches!(b.kind.as_str(), "working" | "activity"))
        {
            return;
        }
        "Working"
    } else {
        match descendants {
            "running" => "Subagents working",
            "waiting_approval" => "Subagents need attention",
            _ => return,
        }
    };
    out.push(ChatBlock {
        id: "session-activity".into(),
        kind: "working".into(),
        title: title.into(),
        ..Default::default()
    });
}

fn turn(messages: &[AgentMessage], completed: bool, out: &mut Vec<ChatBlock>) {
    let mut work = Vec::new();
    let mut activity = Vec::new();
    // Older transcripts do not label prose phases. Keep the complete trailing
    // reply (possibly several text blocks) outside Worked. Empty text records
    // do not hide it; a later tool call still means this prose was work in progress.
    let trailing_start = if completed {
        messages
            .iter()
            .rposition(|m| {
                !(m.kind == "text"
                    && (m.text.trim().is_empty() || (m.role == "assistant" && m.phase.is_empty())))
            })
            .map_or(0, |index| index + 1)
    } else {
        messages.len()
    };
    for (index, message) in messages.iter().enumerate() {
        if message.kind == "tool_call"
            && messages
                .iter()
                .any(|m| m.kind == "provider_card" && m.tool_call_id == message.tool_call_id)
        {
            continue;
        }
        // Incremental plan updates replace the prior snapshot within this turn.
        if message.kind == "provider_card"
            && message.tool_name == "mcp__ccd_turn__update_plan"
            && messages[index + 1..]
                .iter()
                .any(|m| m.kind == "provider_card" && m.tool_name == message.tool_name)
        {
            continue;
        }
        if matches!(message.kind.as_str(), "tool_call" | "tool_result") {
            activity.push(message.clone());
            continue;
        }
        if message.kind == "text" && message.text.trim().is_empty() {
            continue;
        }
        flush(&mut activity, completed, &mut work);
        let compact = is_compact_context(message);
        let boundary = compact
            || message.role == "user"
            || message.kind == "interaction"
            || message.kind == "provider_card"
            || message.phase == "final_answer"
            || index >= trailing_start;
        if boundary {
            flush_work(&mut work, completed || message.role == "user", out);
            out.push(ChatBlock {
                id: message.id.clone(),
                title: if message.kind == "provider_card" {
                    muster_agent::claude_cards::title(&message.tool_name).into()
                } else {
                    String::new()
                },
                kind: if message.kind == "provider_card" {
                    "provider_card"
                } else if compact {
                    "compact_context"
                } else {
                    "message"
                }
                .into(),
                message: Some(message.clone()),
                completed,
                ..Default::default()
            });
        } else {
            work.push(ChatBlock {
                id: message.id.clone(),
                kind: "message".into(),
                message: Some(message.clone()),
                completed,
                ..Default::default()
            });
        }
    }
    flush(&mut activity, completed, &mut work);
    flush_work(&mut work, completed, out);
}

fn flush_work(work: &mut Vec<ChatBlock>, completed: bool, out: &mut Vec<ChatBlock>) {
    if let Some(first) = work.first() {
        out.push(ChatBlock {
            id: format!("working:{}", first.id),
            kind: "working".into(),
            title: if completed { "Worked" } else { "Working" }.into(),
            children: std::mem::take(work),
            completed,
            default_expanded: !completed,
            ..Default::default()
        });
    }
}

fn is_compact_context(message: &AgentMessage) -> bool {
    if message.kind == "compact_context" || message.phase == "summary" {
        return true;
    }
    // Older viewers exposed provider continuation summaries as plain user text.
    // Match the provider preamble, never a casual mention of compaction.
    message.kind == "text"
        && message.role == "user"
        && [
            "This session is being continued from a previous conversation that ran out of context.",
            "Another language model started to solve this problem and produced a summary of its thinking process.",
            "Another language model began solving this problem and produced a summary of its thinking process.",
        ]
        .iter()
        .any(|prefix| message.text.trim_start().starts_with(prefix))
}
fn flush(activity: &mut Vec<AgentMessage>, completed: bool, out: &mut Vec<ChatBlock>) {
    if let Some(first) = activity.first() {
        out.push(ChatBlock {
            id: format!("activity:{}", first.id),
            kind: "activity".into(),
            activity_rows: activity_rows(activity, completed),
            title: activity_title(activity),
            activity: std::mem::take(activity),
            completed,
            ..Default::default()
        });
    }
}

fn tool_category(name: &str) -> &'static str {
    match name
        .rsplit('.')
        .next()
        .unwrap_or(name)
        .to_ascii_lowercase()
        .as_str()
    {
        "exec" | "exec_command" | "shell" | "bash" => "shell",
        "read" | "read_file" => "read",
        "grep" | "glob" | "search" | "web_search" => "search",
        _ => "tool",
    }
}

fn activity_title(messages: &[AgentMessage]) -> String {
    let calls: Vec<_> = messages.iter().filter(|m| m.kind == "tool_call").collect();
    if calls.is_empty() {
        return format!("{} tool outputs", messages.len());
    }
    let reads = calls
        .iter()
        .filter(|m| tool_category(&m.tool_name) == "read")
        .count();
    let searches = calls
        .iter()
        .filter(|m| tool_category(&m.tool_name) == "search")
        .count();
    let commands = calls
        .iter()
        .filter(|m| tool_category(&m.tool_name) == "shell")
        .count();
    let others = calls.len() - reads - searches - commands;
    let mut parts = Vec::new();
    for (count, singular, plural) in [
        (reads, "file read", "file reads"),
        (searches, "search", "searches"),
        (commands, "command", "commands"),
        (others, "tool call", "tool calls"),
    ] {
        if count > 0 {
            parts.push(format!(
                "{count} {}",
                if count == 1 { singular } else { plural }
            ));
        }
    }
    parts.join(" · ")
}

fn activity_rows(messages: &[AgentMessage], completed: bool) -> Vec<ChatActivityRow> {
    let mut rows: Vec<ChatActivityRow> = Vec::new();
    let mut calls = std::collections::HashMap::new();
    let mut counts = std::collections::HashMap::new();
    for message in messages
        .iter()
        .filter(|m| m.kind == "tool_call" && !m.tool_call_id.is_empty())
    {
        *counts
            .entry(message.tool_call_id.as_str())
            .or_insert(0usize) += 1;
    }
    for message in messages {
        if message.kind == "tool_result" && !message.tool_call_id.is_empty() {
            if let Some(index) = calls.remove(&message.tool_call_id) {
                let row: &mut ChatActivityRow = &mut rows[index];
                row.output = message.text.clone();
                row.is_result = true;
                row.status = presentation_status(message, completed).into();
                continue;
            }
        }
        // Ambiguous/reused IDs and legacy records remain separate. Adjacency
        // and matching tool names are never sufficient evidence of a pair.
        if message.kind == "tool_call"
            && !message.tool_call_id.is_empty()
            && counts.get(message.tool_call_id.as_str()) == Some(&1)
        {
            calls.insert(message.tool_call_id.clone(), rows.len());
        }
        let mut row = activity_row(message);
        row.status = presentation_status(message, completed).into();
        rows.push(row);
    }
    rows
}

fn presentation_status(message: &AgentMessage, completed: bool) -> &'static str {
    match message.tool_status.as_str() {
        "running" if !completed => "running",
        "completed" => "completed",
        "failed" => "failed",
        "interrupted" => "interrupted",
        _ => "unknown",
    }
}

fn activity_row(message: &AgentMessage) -> ChatActivityRow {
    let name = message
        .tool_name
        .rsplit('.')
        .next()
        .filter(|s| !s.is_empty())
        .unwrap_or("Tool");
    let category = tool_category(name);
    let is_result = message.kind == "tool_result";
    let arguments = (!is_result)
        .then(|| {
            serde_json::from_str::<serde_json::Value>(
                message
                    .text
                    .strip_prefix(&format!("{} ", message.tool_name))
                    .unwrap_or(&message.text),
            )
            .ok()
        })
        .flatten();
    let excerpt = arguments.as_ref().and_then(|value| {
        if let Some(command) = value.as_str() {
            return Some(command);
        }
        [
            "cmd",
            "command",
            "file_path",
            "path",
            "pattern",
            "query",
            "code",
        ]
        .iter()
        .find_map(|key| value.get(key).and_then(|v| v.as_str()))
    });
    // Preserve the complete bounded source in the disclosure. Never infer
    // success, a pending operation, or call/result pairing from adjacency.
    let title = if is_result {
        format!("{name} output")
    } else if let Some(excerpt) = excerpt {
        format!(
            "{name} · {}",
            excerpt
                .split_whitespace()
                .collect::<Vec<_>>()
                .join(" ")
                .chars()
                .take(180)
                .collect::<String>()
        )
    } else {
        name.to_owned()
    };
    ChatActivityRow {
        id: message.id.clone(),
        title,
        detail: message.text.clone(),
        category: category.into(),
        tool_name: name.into(),
        is_result,
        output: String::new(),
        status: String::new(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn cli_prose_and_reasoning_keep_identity_through_streaming_and_completion() {
        let mut reasoning = m("reason", "assistant", "text");
        reasoning.phase = "reasoning".into();
        let messages = [
            m("u", "user", "text"),
            reasoning,
            m("tool", "assistant", "tool_call"),
            m("answer", "assistant", "text"),
        ];
        let live = for_session(&messages, true, true);
        let done = for_session(&messages, false, true);
        assert_eq!(
            live.iter().map(|b| b.kind.as_str()).collect::<Vec<_>>(),
            ["message", "reasoning", "activity", "message"]
        );
        assert_eq!(
            live.iter().map(|b| &b.id).collect::<Vec<_>>(),
            done.iter().map(|b| &b.id).collect::<Vec<_>>()
        );
        assert!(!live[1].default_expanded);
        assert_eq!(done[3].message.as_ref().unwrap().id, "answer");
    }
    #[test]
    fn cli_turn_boundaries_preserve_old_answers_and_stop_stale_spinners() {
        let mut tool = m("tool", "assistant", "tool_call");
        tool.tool_status = "running".into();
        let messages = [
            tool.clone(),
            m("answer", "assistant", "text"),
            m("end", "system", "turn_status"),
            m("u2", "user", "text"),
            AgentMessage {
                id: "newtool".into(),
                ..tool
            },
        ];
        let view = for_session(&messages, true, true);
        assert!(view[0].completed);
        assert_eq!(view[0].activity_rows[0].status, "unknown");
        assert_eq!(view[1].kind, "message");
        assert_eq!(view[2].kind, "notice");
        assert_eq!(view[4].activity_rows[0].status, "running");
    }
    #[test]
    fn cli_diff_and_failed_result_are_typed_without_hiding_the_answer() {
        let mut call = m("call", "assistant", "tool_call");
        call.tool_name = "shell".into();
        call.text = r#""cargo test""#.into();
        call.tool_call_id = "c".into();
        call.tool_status = "running".into();
        let mut result = m("result", "tool", "tool_result");
        result.tool_call_id = "c".into();
        result.tool_status = "failed".into();
        let mut diff = m("diff", "tool", "tool_result");
        diff.tool_name = "diff".into();
        diff.text = "--- a/x\n+++ b/x\n@@ -1 +1 @@\n-old\n+new\n".into();
        let view = for_session(
            &[call, result, diff, m("answer", "assistant", "text")],
            false,
            true,
        );
        assert_eq!(view[0].activity_rows[0].status, "failed");
        assert_eq!(view[0].activity_rows[0].title, "shell · cargo test");
        assert_eq!(view[1].kind, "diff");
        assert_eq!(view[1].diffs[0].added, 1);
        assert_eq!(view[2].kind, "message");
    }
    fn m(id: &str, role: &str, kind: &str) -> AgentMessage {
        AgentMessage {
            id: id.into(),
            role: role.into(),
            kind: kind.into(),
            text: id.into(),
            ..Default::default()
        }
    }
    #[test]
    fn compact_context_is_separate_and_does_not_replace_the_final_reply() {
        let mut summary = m("compact", "assistant", "text");
        summary.phase = "summary".into();
        summary.text = "## Context\nKeep this complete.".into();
        let mut legacy = m("legacy", "user", "text");
        legacy.text = "This session is being continued from a previous conversation that ran out of context.\nSummary".into();
        let mut ordinary = m("u", "user", "text");
        ordinary.text = "Please compact context by default".into();
        let mut reply = m("reply", "assistant", "text");
        reply.phase = "final_answer".into();
        let view = blocks(&[ordinary, reply, summary, legacy], false);
        assert_eq!(
            view.iter().map(|b| b.kind.as_str()).collect::<Vec<_>>(),
            ["message", "message", "compact_context", "compact_context"]
        );
        assert_eq!(view[1].message.as_ref().unwrap().id, "reply");
        assert_eq!(
            view[2].message.as_ref().unwrap().text,
            "## Context\nKeep this complete."
        );
        assert!(is_compact_context(&m("typed", "system", "compact_context")));
        assert!(!is_compact_context(&m("tool", "tool", "tool_result")));
    }
    #[test]
    fn claude_prose_stays_visible_while_only_tools_collapse_during_work() {
        let view = blocks(
            &[
                m("a", "assistant", "text"),
                m("t", "assistant", "tool_call"),
                m("r", "tool", "tool_result"),
                m("b", "assistant", "text"),
            ],
            true,
        );
        assert_eq!(
            view.iter().map(|b| b.kind.as_str()).collect::<Vec<_>>(),
            ["working"]
        );
        assert!(view[0].default_expanded);
        assert_eq!(view[0].title, "Working");
        assert_eq!(
            view[0]
                .children
                .iter()
                .map(|b| b.kind.as_str())
                .collect::<Vec<_>>(),
            ["message", "activity", "message"]
        );
        assert_eq!(view[0].children[1].activity.len(), 2);
    }
    #[test]
    fn completion_keeps_reply_visible_and_groups_work_without_merging_turns() {
        let mut commentary = m("c", "assistant", "text");
        commentary.phase = "commentary".into();
        let messages = vec![
            m("u", "user", "text"),
            commentary,
            m("t", "assistant", "tool_call"),
            m("r", "tool", "tool_result"),
            m("f", "assistant", "text"),
            m("s", "system", "turn_summary"),
            m("u2", "user", "text"),
        ];
        let view = blocks(&messages, true);
        assert_eq!(
            view.iter().map(|b| b.kind.as_str()).collect::<Vec<_>>(),
            ["message", "working", "message", "summary", "message"]
        );
        assert_eq!(view[1].children[1].activity.len(), 2);
        assert_eq!(view[1].children[0].message.as_ref().unwrap().id, "c");
        assert_eq!(view[2].message.as_ref().unwrap().id, "f");
        assert!(view[1].completed);
        assert!(!view[1].default_expanded);
        let live = blocks(&messages[..4], true);
        assert_eq!(live[1].id, view[1].id);
        assert!(!live[1].completed);
        assert!(live[1].default_expanded);
    }

    #[test]
    fn every_historical_reply_survives_missing_footers_and_a_running_next_turn() {
        let view = blocks(
            &[
                m("u1", "user", "text"),
                m("t1", "assistant", "tool_call"),
                m("f1", "assistant", "text"),
                m("u2", "user", "text"),
                m("t2", "assistant", "tool_call"),
                m("f2a", "assistant", "text"),
                m("f2b", "assistant", "text"),
                AgentMessage {
                    text: "  ".into(),
                    ..m("blank", "assistant", "text")
                },
                m("s2", "system", "turn_summary"),
                m("u3", "user", "text"),
                m("live", "assistant", "text"),
            ],
            true,
        );
        let visible: Vec<_> = view
            .iter()
            .filter_map(|b| b.message.as_ref())
            .filter(|m| m.role == "assistant")
            .map(|m| m.id.as_str())
            .collect();
        assert_eq!(visible, ["f1", "f2a", "f2b"]);
        let groups: Vec<_> = view.iter().filter(|b| b.kind == "working").collect();
        assert_eq!(groups.len(), 3);
        assert!(
            groups[..2]
                .iter()
                .all(|b| b.completed && !b.default_expanded)
        );
        assert!(!groups[2].completed);
        assert!(groups[2].default_expanded);
    }

    #[test]
    fn provider_final_stays_visible_after_later_activity_and_during_next_turn() {
        let mut final_reply = m("result", "assistant", "text");
        final_reply.phase = "final_answer".into();
        let view = blocks(
            &[
                m("progress", "assistant", "text"),
                m("tool", "assistant", "tool_call"),
                final_reply,
                m("late", "tool", "tool_result"),
                m("summary", "system", "turn_summary"),
                m("next", "user", "text"),
            ],
            true,
        );
        assert_eq!(view[0].kind, "working");
        assert_eq!(view[0].children[0].message.as_ref().unwrap().id, "progress");
        assert_eq!(view[1].message.as_ref().unwrap().id, "result");
        assert_eq!(view[2].kind, "working");
        assert_eq!(view[2].children[0].activity[0].id, "late");
    }

    #[test]
    fn tool_rows_expose_arguments_without_inventing_results() {
        let mut call = m("c", "assistant", "tool_call");
        call.tool_name = "functions.exec_command".into();
        call.text = r#"{"cmd":"pwd && rg --files"}"#.into();
        let mut result = m("r", "tool", "tool_result");
        result.tool_name = call.tool_name.clone();
        result.text = "exit code 1".into();
        let view = blocks(&[call.clone(), result.clone()], true);
        let rows = &view[0].children[0].activity_rows;
        assert_eq!(rows[0].title, "exec_command · pwd && rg --files");
        assert_eq!(rows[0].category, "shell");
        assert_eq!(rows[0].detail, call.text);
        assert_eq!(rows[1].detail, result.text);
        assert!(rows[1].is_result);
        assert_eq!(rows[1].title, "exec_command output");
        call.text = "invalid JSON".into();
        assert_eq!(activity_row(&call).title, "exec_command");
        assert_eq!(activity_row(&call).detail, "invalid JSON");
    }
    #[test]
    fn consecutive_calls_pair_by_provider_id_with_reversed_outputs() {
        let mut a = m("a", "assistant", "tool_call");
        a.tool_call_id = "call-a".into();
        a.tool_name = "exec_command".into();
        a.text = r#"exec_command {"cmd":"pwd"}"#.into();
        let mut b = a.clone();
        b.id = "b".into();
        b.tool_call_id = "call-b".into();
        let mut rb = m("rb", "user", "tool_result");
        rb.tool_call_id = "call-b".into();
        rb.text = "output b".into();
        let mut ra = rb.clone();
        ra.id = "ra".into();
        ra.tool_call_id = "call-a".into();
        ra.text = "output a".into();
        let view = blocks(&[a, b, rb, ra], true);
        let groups = &view[0].children;
        assert_eq!(groups.len(), 1);
        assert_eq!(groups[0].title, "2 commands");
        assert_eq!(groups[0].activity_rows.len(), 2);
        assert_eq!(groups[0].activity_rows[0].title, "exec_command · pwd");
        assert_eq!(groups[0].activity_rows[0].output, "output a");
        assert_eq!(groups[0].activity_rows[1].output, "output b");
    }

    #[test]
    fn prose_separates_batches_and_ambiguous_call_ids_do_not_pair() {
        let mut call = m("a", "assistant", "tool_call");
        call.tool_call_id = "same".into();
        let mut other = call.clone();
        other.id = "b".into();
        let mut result = m("r", "tool", "tool_result");
        result.tool_call_id = "same".into();
        let view = blocks(
            &[
                call.clone(),
                other,
                result.clone(),
                m("note", "assistant", "text"),
                call,
                result,
            ],
            true,
        );
        let groups = &view[0].children;
        assert_eq!(groups.len(), 3);
        assert_eq!(groups[0].activity_rows.len(), 3);
        assert!(groups[0].activity_rows.iter().all(|r| r.output.is_empty()));
        assert_eq!(groups[2].activity_rows.len(), 1);
    }
    #[test]
    fn provider_cards_are_visible_boundaries_and_latest_plan_replaces_incremental_snapshots() {
        let card = |id: &str, name: &str, call: &str| AgentMessage {
            tool_name: name.into(),
            tool_call_id: call.into(),
            ..m(id, "assistant", "provider_card")
        };
        let messages = vec![
            AgentMessage {
                tool_call_id: "p1".into(),
                ..m("call1", "assistant", "tool_call")
            },
            card("plan1", muster_agent::claude_cards::PLAN, "p1"),
            m("work", "assistant", "text"),
            card("plan2", muster_agent::claude_cards::PLAN, "p2"),
            card("chapter", muster_agent::claude_cards::CHAPTER, "c"),
            card("file", "SendUserFile", "f"),
            card("wrap", muster_agent::claude_cards::WRAP, "w"),
        ];
        let blocks = blocks(&messages, true);
        let cards: Vec<_> = blocks
            .iter()
            .filter(|b| b.kind == "provider_card")
            .collect();
        assert_eq!(
            cards.iter().map(|b| b.id.as_str()).collect::<Vec<_>>(),
            ["plan2", "chapter", "file", "wrap"]
        );
        assert_eq!(cards[0].title, "计划");
        assert_eq!(cards[2].title, "文件");
        assert!(
            blocks
                .iter()
                .flat_map(|b| &b.children)
                .all(|c| c.kind != "provider_card")
        );
        assert!(
            blocks
                .iter()
                .flat_map(|b| &b.children)
                .flat_map(|b| &b.activity)
                .all(|m| m.id != "call1")
        );
    }
}
