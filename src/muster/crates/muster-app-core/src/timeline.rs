//! Stable turn presentation assembled by Rust, including legacy transcripts.
mod diff;

use muster_proto::{
    app::{ChatActivityRow, ChatBlock},
    outline::{self, Class},
    v1::AgentMessage,
};

/// Loaded and loading work by work record ID.
pub(crate) type Work = std::collections::HashMap<String, crate::state::Work>;

/// Prefix grouping boundaries, invalidated from the first changed record.
/// Appends inspect only new records; paging does not rescan completed turns.
#[derive(Default)]
pub(crate) struct Index {
    starts: Vec<usize>,
    statuses: Vec<usize>,
    indexed: usize,
    cli: Option<bool>,
}
impl Index {
    pub(crate) fn invalidate(&mut self, from: usize) {
        self.indexed = self.indexed.min(from);
        self.statuses
            .truncate(self.statuses.partition_point(|&i| i < from));
        self.starts
            .truncate(self.starts.partition_point(|&i| i < from));
    }
    pub(crate) fn window(
        &mut self,
        messages: &[AgentMessage],
        start: usize,
        end: usize,
        running: bool,
        cli: bool,
        work: &Work,
    ) -> Vec<ChatBlock> {
        if self.cli != Some(cli) || messages.len() < self.indexed {
            self.invalidate(0);
            self.cli = Some(cli);
        }
        for (i, m) in messages.iter().enumerate().skip(self.indexed) {
            if m.kind == "turn_status" {
                self.statuses.push(i);
            }
            let boundary = if cli {
                !matches!(m.kind.as_str(), "tool_call" | "tool_result") || m.tool_name == "diff"
            } else {
                outline::ends_turn(m) || outline::starts_turn(m)
            };
            if boundary {
                self.starts.push(i);
            }
        }
        self.indexed = messages.len();
        let before = self.starts.partition_point(|&i| i <= start);
        let begin = before.checked_sub(1).map_or(0, |i| self.starts[i]);
        // Include the next boundary for completion/legacy trailing-prose
        // semantics, but never project messages outside the requested window.
        let after = self.starts.partition_point(|&i| i < end);
        let finish = self.starts.get(after).map_or(messages.len(), |&i| i + 1);
        window_range(
            &messages[begin..finish],
            start.saturating_sub(begin),
            end.saturating_sub(begin),
            if cli {
                running && self.statuses.last().is_none_or(|&i| i < finish)
            } else {
                running && finish == messages.len()
            },
            cli,
            work,
        )
    }
}

/// Orca-style presentation keeps prose in the reading flow and collapses only
/// the evidence-heavy rows. The native shell never reconstructs turn membership.
#[cfg(test)]
pub fn for_session(messages: &[AgentMessage], running: bool, cli: bool) -> Vec<ChatBlock> {
    window(messages, 0, running, cli)
}

/// Keep group identities grounded in cached history while emitting only the
/// requested window. Native shells must not infer continuity from partial groups.
#[cfg(test)]
pub(crate) fn window(
    messages: &[AgentMessage],
    start: usize,
    running: bool,
    cli: bool,
) -> Vec<ChatBlock> {
    window_range(messages, start, messages.len(), running, cli, &Work::new())
}

fn window_range(
    messages: &[AgentMessage],
    start: usize,
    end: usize,
    running: bool,
    cli: bool,
    work: &Work,
) -> Vec<ChatBlock> {
    if !cli {
        return window_blocks(messages, start, end, running, work);
    }
    let mut out = Vec::new();
    let mut activity = Vec::new();
    let mut activity_first = None;
    let last_boundary = messages.iter().rposition(|m| m.kind == "turn_status");
    for (index, message) in messages.iter().enumerate() {
        let completed = !running || last_boundary.is_some_and(|end| index <= end);
        if matches!(message.kind.as_str(), "tool_call" | "tool_result")
            && message.tool_name != "diff"
        {
            activity_first.get_or_insert_with(|| message.id.clone());
            if (start..end).contains(&index) {
                activity.push(message.clone());
            }
            continue;
        }
        flush(&mut activity, activity_first.take(), completed, &mut out);
        if !(start..end).contains(&index) || message.text.trim().is_empty() {
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
    flush(&mut activity, activity_first.take(), !running, &mut out);
    out
}

#[cfg(test)]
pub fn blocks(messages: &[AgentMessage], running: bool) -> Vec<ChatBlock> {
    window_blocks(messages, 0, messages.len(), running, &Work::new())
}

fn window_blocks(
    messages: &[AgentMessage],
    visible: usize,
    end: usize,
    running: bool,
    work: &Work,
) -> Vec<ChatBlock> {
    let mut out = Vec::new();
    let mut start = 0;
    for (index, message) in messages.iter().enumerate() {
        if message.kind == "turn_summary" {
            if index > visible {
                turn(
                    &messages[start..index],
                    true,
                    &mut out,
                    visible.saturating_sub(start)..end.saturating_sub(start),
                    work,
                );
            }
            if (visible..end).contains(&index) {
                out.push(ChatBlock {
                    id: message.id.clone(),
                    kind: "summary".into(),
                    message: Some(message.clone()),
                    completed: true,
                    ..Default::default()
                });
            }
            start = index + 1;
        } else if outline::starts_turn(message) {
            // Legacy transcripts can omit usage footers. A new visible prompt
            // must not pull the previous reply into the next turn's work group.
            if index > visible {
                turn(
                    &messages[start..index],
                    true,
                    &mut out,
                    visible.saturating_sub(start)..end.saturating_sub(start),
                    work,
                );
            }
            start = index;
        }
    }
    if messages.len() > visible {
        turn(
            &messages[start..],
            !running,
            &mut out,
            visible.saturating_sub(start)..end.saturating_sub(start),
            work,
        );
    }
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

fn turn(
    messages: &[AgentMessage],
    completed: bool,
    out: &mut Vec<ChatBlock>,
    visible: std::ops::Range<usize>,
    loaded: &Work,
) {
    // Loaded work follows its work record at the record's position, so a
    // collapsed turn and a whole one classify and group alike (ADR 0160).
    let mut rows: Vec<(usize, &AgentMessage)> = Vec::with_capacity(messages.len());
    for (index, message) in messages.iter().enumerate() {
        rows.push((index, message));
        if let Some(crate::state::Work::Loaded { records, .. }) = loaded.get(&message.id)
            && message.kind == outline::KIND
        {
            rows.extend(records.iter().map(|record| (index, record)));
        }
    }
    let classes = outline::classify(
        &rows.iter().map(|(_, message)| *message).collect::<Vec<_>>(),
        completed,
    );
    let mut work = Vec::new();
    let mut work_first = None;
    let mut details = None;
    let mut activity = Vec::new();
    let mut activity_first = None;
    for (&(index, message), class) in rows.iter().zip(classes) {
        if class == Class::Hidden {
            continue;
        }
        if matches!(message.kind.as_str(), "tool_call" | "tool_result") {
            activity_first.get_or_insert_with(|| message.id.clone());
            if visible.contains(&index) {
                activity.push(message.clone());
            }
            continue;
        }
        flush_into_work(
            &mut activity,
            &mut activity_first,
            completed,
            &mut work,
            &mut work_first,
        );
        if let Some(span) = message
            .work
            .as_ref()
            .filter(|_| message.kind == outline::KIND)
        {
            // The group keeps its ID whether or not its records are loaded.
            work_first.get_or_insert_with(|| {
                outline::group_id(span)
                    .trim_start_matches("working:")
                    .to_owned()
            });
            if !visible.contains(&index) {
                continue;
            }
            details = Some(match loaded.get(&message.id) {
                None => ("unloaded", String::new()),
                Some(crate::state::Work::Loading) => ("loading", String::new()),
                Some(crate::state::Work::Failed(error)) => ("failed", error.clone()),
                Some(crate::state::Work::Loaded { omitted, .. }) => {
                    if *omitted > 0 {
                        work.push(ChatBlock {
                            id: format!("{}:omitted", message.id),
                            kind: "notice".into(),
                            title: format!(
                                "{omitted} earlier {} not shown",
                                if *omitted == 1 { "step" } else { "steps" }
                            ),
                            completed: true,
                            ..Default::default()
                        });
                    }
                    ("", String::new())
                }
            });
            continue;
        }
        // How the turn ended, when it did not simply complete, follows its
        // work as a notice.
        if message.kind == "turn_status" {
            flush_work(&mut work, &mut work_first, &mut details, completed, out);
            if visible.contains(&index) {
                out.push(ChatBlock {
                    id: message.id.clone(),
                    kind: "notice".into(),
                    title: message.text.clone(),
                    message: Some(message.clone()),
                    completed,
                    ..Default::default()
                });
            }
            continue;
        }
        if class == Class::Boundary {
            flush_work(
                &mut work,
                &mut work_first,
                &mut details,
                completed || message.role == "user",
                out,
            );
            if !visible.contains(&index) {
                continue;
            }
            out.push(ChatBlock {
                id: message.id.clone(),
                title: if message.kind == "provider_card" {
                    muster_agent::claude_cards::title(&message.tool_name).into()
                } else {
                    String::new()
                },
                kind: if message.kind == "provider_card" {
                    "provider_card"
                } else if outline::is_compact_context(message) {
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
            work_first.get_or_insert_with(|| message.id.clone());
            if !visible.contains(&index) {
                continue;
            }
            // A provider's recorded reasoning is its own collapsed row.
            let reasoning = message.phase == "reasoning";
            work.push(ChatBlock {
                id: message.id.clone(),
                kind: if reasoning { "reasoning" } else { "message" }.into(),
                title: if reasoning { "Reasoning" } else { "" }.into(),
                message: Some(message.clone()),
                completed,
                ..Default::default()
            });
        }
    }
    flush_into_work(
        &mut activity,
        &mut activity_first,
        completed,
        &mut work,
        &mut work_first,
    );
    flush_work(&mut work, &mut work_first, &mut details, completed, out);
}

fn flush_into_work(
    activity: &mut Vec<AgentMessage>,
    first: &mut Option<String>,
    completed: bool,
    work: &mut Vec<ChatBlock>,
    work_first: &mut Option<String>,
) {
    if let Some(id) = first.as_ref() {
        work_first.get_or_insert_with(|| format!("activity:{id}"));
    }
    flush(activity, first.take(), completed, work);
}

fn flush_work(
    work: &mut Vec<ChatBlock>,
    first: &mut Option<String>,
    details: &mut Option<(&'static str, String)>,
    completed: bool,
    out: &mut Vec<ChatBlock>,
) {
    let id = first.take();
    let (details, details_error) = details.take().unwrap_or_default();
    // A work record whose records are not loaded still shows its group.
    if let Some(id) = id.filter(|_| !work.is_empty() || !details.is_empty()) {
        out.push(ChatBlock {
            id: format!("working:{id}"),
            kind: "working".into(),
            title: if completed { "Worked" } else { "Working" }.into(),
            children: std::mem::take(work),
            completed,
            default_expanded: !completed,
            details: details.into(),
            details_error,
            ..Default::default()
        });
    }
}

fn flush(
    activity: &mut Vec<AgentMessage>,
    first: Option<String>,
    completed: bool,
    out: &mut Vec<ChatBlock>,
) {
    if let Some(id) = first.filter(|_| !activity.is_empty()) {
        out.push(ChatBlock {
            id: format!("activity:{id}"),
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
    #[test]
    fn window_keeps_partial_group_ids_and_excludes_hidden_payload() {
        let messages = vec![
            m("prompt", "user", "text"),
            m("thought", "assistant", "reasoning"),
            m("call-a", "assistant", "tool_call"),
            m("result-a", "tool", "tool_result"),
            m("progress", "assistant", "text"),
            m("call-b", "assistant", "tool_call"),
            m("result-b", "tool", "tool_result"),
            m("final", "assistant", "text"),
            m("summary", "assistant", "turn_summary"),
            m("next-prompt", "user", "text"),
            m("next-work", "assistant", "reasoning"),
            m("next-call", "assistant", "tool_call"),
            m("next-result", "tool", "tool_result"),
            AgentMessage {
                phase: "final_answer".into(),
                ..m("explicit-final", "assistant", "text")
            },
            m("compact", "user", "compact_context"),
            AgentMessage {
                tool_call_id: "card-call".into(),
                ..m("shadowed-call", "assistant", "tool_call")
            },
            m("before-card", "assistant", "reasoning"),
            AgentMessage {
                tool_call_id: "card-call".into(),
                tool_name: "mcp__ccd_turn__update_plan".into(),
                ..m("plan", "assistant", "provider_card")
            },
            m("after-card", "assistant", "reasoning"),
            m("after-card-call", "assistant", "tool_call"),
        ];
        fn all<'a>(blocks: &'a [ChatBlock], out: &mut Vec<&'a ChatBlock>) {
            for b in blocks {
                out.push(b);
                all(&b.children, out);
            }
        }
        for cli in [false, true] {
            for running in [false, true] {
                let full = for_session(&messages, running, cli);
                let mut full_nodes = Vec::new();
                all(&full, &mut full_nodes);
                for start in 0..=messages.len() {
                    let partial = window(&messages, start, running, cli);
                    let mut nodes = Vec::new();
                    all(&partial, &mut nodes);
                    for b in nodes {
                        assert!(
                            full_nodes.iter().any(|f| f.id == b.id && f.kind == b.kind),
                            "start={start} cli={cli} running={running} id={}",
                            b.id
                        );
                        for m in b.message.iter().chain(b.activity.iter()) {
                            assert!(
                                messages[start..].iter().any(|v| v.id == m.id),
                                "hidden payload escaped at {start}: {}",
                                m.id
                            );
                        }
                    }
                }
            }
        }
        let full = window(&messages, 0, true, false);
        let partial = window(&messages, 3, true, false);
        assert_ne!(
            for_session(&messages[3..], true, false)[0].id,
            full[1].id,
            "The old sliced projection loses identity at this boundary"
        );
        assert_eq!(partial[0].id, full[1].id);
        assert_eq!(partial[0].children[0].id, full[1].children[1].id);
        assert_eq!(partial[0].children[0].activity.len(), 1);
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
    fn transcript_reasoning_is_a_collapsed_row_inside_worked() {
        let mut reasoning = m("reason", "assistant", "text");
        reasoning.phase = "reasoning".into();
        let mut last = m("last-thought", "assistant", "text");
        last.phase = "reasoning".into();
        let messages = [
            m("u", "user", "text"),
            reasoning,
            m("tool", "assistant", "tool_call"),
            last,
            m("answer", "assistant", "text"),
        ];
        let out = blocks(&messages, false);
        assert_eq!(
            out.iter().map(|b| b.kind.as_str()).collect::<Vec<_>>(),
            ["message", "working", "message"]
        );
        let work = &out[1].children;
        assert_eq!(
            work.iter()
                .map(|b| (b.kind.as_str(), b.title.as_str()))
                .collect::<Vec<_>>(),
            [
                ("reasoning", "Reasoning"),
                ("activity", "1 tool call"),
                ("reasoning", "Reasoning")
            ]
        );
        // Reasoning right before the reply never takes the reply's place.
        assert_eq!(out[2].message.as_ref().unwrap().id, "answer");
    }
    #[test]
    fn a_turn_that_did_not_complete_ends_with_its_notice_after_the_reply() {
        let mut failed = m("end", "system", "turn_status");
        failed.text = "Turn: failed\nMISSING_CREDENTIAL: no key".into();
        failed.tool_status = "failed".into();
        let messages = [
            m("u", "user", "text"),
            m("tool", "assistant", "tool_call"),
            m("partial", "assistant", "text"),
            failed,
            m("next", "user", "text"),
        ];
        let out = blocks(&messages, false);
        assert_eq!(
            out.iter()
                .map(|b| (b.kind.as_str(), b.title.as_str()))
                .collect::<Vec<_>>(),
            [
                ("message", ""),
                ("working", "Worked"),
                ("message", ""),
                ("notice", "Turn: failed\nMISSING_CREDENTIAL: no key"),
                ("message", "")
            ]
        );
        assert_eq!(out[2].message.as_ref().unwrap().id, "partial");
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
        assert!(outline::is_compact_context(&m(
            "typed",
            "system",
            "compact_context"
        )));
        assert!(!outline::is_compact_context(&m(
            "tool",
            "tool",
            "tool_result"
        )));
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

#[cfg(test)]
mod indexed_window_tests {
    use super::*;
    fn history() -> Vec<AgentMessage> {
        (0..1000)
            .map(|i| AgentMessage {
                id: format!("m{i}"),
                text: format!("message {i}"),
                role: if i % 10 == 0 { "user" } else { "assistant" }.into(),
                kind: match i % 10 {
                    0 | 8 => "text",
                    9 => "turn_summary",
                    _ => "tool_call",
                }
                .into(),
                tool_name: "read_file".into(),
                ..Default::default()
            })
            .collect()
    }
    #[test]
    fn indexed_windows_preserve_group_identity_and_only_project_the_window() {
        let mut messages = history();
        for cli in [false, true] {
            let mut index = Index::default();
            for start in [0, 3, 8, 9, 199, 400, 793, 800] {
                // The unindexed bounded algorithm is the semantic reference;
                // index slicing must retain the full group context.
                let actual = index.window(&messages, start, start + 200, true, cli, &Work::new());
                assert!(
                    actual == window_range(&messages, start, start + 200, true, cli, &Work::new()),
                    "start={start} cli={cli}"
                );
                fn count(blocks: &[ChatBlock]) -> usize {
                    blocks
                        .iter()
                        .map(|b| {
                            usize::from(b.message.is_some()) + b.activity.len() + count(&b.children)
                        })
                        .sum()
                }
                assert!(count(&actual) <= 200);
            }
            messages[900].role = "assistant".into();
            index.invalidate(900);
            assert_eq!(
                index.window(&messages, 903, 1000, true, cli, &Work::new()),
                window_range(&messages, 903, 1000, true, cli, &Work::new())
            );
        }
    }
}

#[cfg(test)]
mod outline_tests {
    use super::*;
    fn m(id: &str, role: &str, kind: &str) -> AgentMessage {
        AgentMessage {
            id: id.into(),
            role: role.into(),
            kind: kind.into(),
            text: id.into(),
            ..Default::default()
        }
    }

    /// Turns with cards, reasoning, interim prose, hidden records and a
    /// summary, then a running turn (ADR 0160).
    fn outline_fixture() -> Vec<AgentMessage> {
        let text = |id: &str, role: &str, phase: &str| AgentMessage {
            phase: phase.into(),
            ..m(id, role, "text")
        };
        let call = |id: &str, call: &str| AgentMessage {
            tool_name: "Bash".into(),
            tool_call_id: call.into(),
            ..m(id, "assistant", "tool_call")
        };
        let result = |id: &str, call: &str| AgentMessage {
            tool_name: "Bash".into(),
            tool_call_id: call.into(),
            ..m(id, "assistant", "tool_result")
        };
        let card = AgentMessage {
            tool_name: "Task".into(),
            tool_call_id: "c3".into(),
            ..m("card", "assistant", "provider_card")
        };
        let empty = AgentMessage {
            text: String::new(),
            ..m("empty", "assistant", "text")
        };
        vec![
            m("u1", "user", "text"),
            text("think1", "assistant", "reasoning"),
            call("t1", "c1"),
            result("r1", "c1"),
            text("interim1", "assistant", ""),
            empty,
            call("t2", "c2"),
            result("r2", "c2"),
            text("answer1", "assistant", ""),
            m("summary1", "system", "turn_summary"),
            m("u2", "user", "text"),
            call("t3", "c3"),
            card,
            call("t4", "c4"),
            result("r4", "c4"),
            text("answer2", "assistant", "final_answer"),
            m("u3", "user", "text"),
            call("t5", "c5"),
            text("interim3", "assistant", ""),
        ]
    }

    /// The fixture's outline, and its work as a client holds it once loaded.
    fn outline_of(records: &[AgentMessage]) -> (Vec<AgentMessage>, Work) {
        let page = muster_proto::outline::Outline::default().page(
            records,
            &muster_proto::v1::AgentMessagesRequest {
                turns: 100,
                tail: 2000,
                ..Default::default()
            },
            Clone::clone,
        );
        let mut loaded = Work::new();
        for stub in page.messages.iter().filter(|m| m.kind == outline::KIND) {
            let span = stub.work.as_ref().unwrap();
            let first = records.iter().position(|m| m.id == span.first_id).unwrap();
            let span = records[first..first + span.count as usize].to_vec();
            loaded.insert(
                stub.id.clone(),
                crate::state::Work::Loaded {
                    records: span,
                    omitted: 0,
                },
            );
        }
        (page.messages, loaded)
    }

    #[test]
    fn a_loaded_outline_renders_the_same_blocks_as_its_records() {
        let records = outline_fixture();
        let (outline, loaded) = outline_of(&records);
        assert!(outline.len() < records.len());
        assert_eq!(loaded.len(), 2, "one Worked group in each completed turn");
        for running in [true, false] {
            assert_eq!(
                window_blocks(&outline, 0, outline.len(), running, &loaded),
                window_blocks(&records, 0, records.len(), running, &Work::new()),
            );
        }
    }

    #[test]
    fn unloaded_work_keeps_its_group_and_says_what_it_is_waiting_for() {
        let records = outline_fixture();
        let (outline, loaded) = outline_of(&records);
        let full = window_blocks(&records, 0, records.len(), true, &Work::new());
        let mut work = Work::new();
        let mut ids = loaded.keys().cloned().collect::<Vec<_>>();
        ids.sort();
        work.insert(ids[0].clone(), crate::state::Work::Loading);
        work.insert(ids[1].clone(), crate::state::Work::Failed("offline".into()));
        let view = window_blocks(&outline, 0, outline.len(), true, &work);
        assert_eq!(
            view.iter().map(|b| (&b.id, &b.kind)).collect::<Vec<_>>(),
            full.iter().map(|b| (&b.id, &b.kind)).collect::<Vec<_>>()
        );
        let details = view
            .iter()
            .filter(|b| b.kind == "working")
            .map(|b| {
                (
                    b.details.as_str(),
                    b.details_error.as_str(),
                    b.children.len(),
                )
            })
            .collect::<Vec<_>>();
        let state = |id: &str| match work.get(id) {
            Some(crate::state::Work::Loading) => ("loading", ""),
            Some(crate::state::Work::Failed(_)) => ("failed", "offline"),
            _ => ("unloaded", ""),
        };
        let mut expected = outline
            .iter()
            .filter(|m| m.kind == outline::KIND)
            .map(|m| {
                let (details, error) = state(&m.id);
                (details, error, 0)
            })
            .collect::<Vec<_>>();
        // The running turn keeps its records, so its group is complete.
        expected.push(("", "", 2));
        assert_eq!(details, expected);
    }

    #[test]
    fn a_long_span_says_how_many_steps_it_leaves_out() {
        let records = outline_fixture();
        let (outline, mut loaded) = outline_of(&records);
        let stub = outline.iter().find(|m| m.kind == outline::KIND).unwrap();
        let Some(crate::state::Work::Loaded { records, omitted }) = loaded.get_mut(&stub.id) else {
            unreachable!()
        };
        records.remove(0);
        *omitted = 1;
        let view = window_blocks(&outline, 0, outline.len(), false, &loaded);
        let group = view
            .iter()
            .find(|b| b.id == muster_proto::outline::group_id(stub.work.as_ref().unwrap()))
            .unwrap();
        assert_eq!(group.details, "");
        assert_eq!(group.children[0].kind, "notice");
        assert_eq!(group.children[0].title, "1 earlier step not shown");
    }
}
