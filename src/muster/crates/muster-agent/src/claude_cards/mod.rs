//! Claude Desktop card semantics. Transcripts declare content; native controls
//! remain the only authority to perform an action. Never execute action names.
use muster_proto::v1::{InteractionQuestion, PendingInteraction, QuestionOption};
use serde_json::Value;

pub const WRAP: &str = "mcp__ccd_turn__wrap_up";
pub const PLAN: &str = "mcp__ccd_turn__update_plan";
pub const START: &str = "mcp__ccd_session__start_session";
pub const SPAWN: &str = "mcp__ccd_session__spawn_task";
pub const DISMISS: &str = "mcp__ccd_session__dismiss_task";
pub const CHAPTER: &str = "mcp__ccd_session__mark_chapter";
pub const WRAP_QUESTION: &str = "claude:wrap_up";
pub const PROPOSAL_QUESTION: &str = "claude:proposal";
pub const OFFER_QUESTION: &str = "claude:offer";

pub fn is_wrap(p: &PendingInteraction) -> bool {
    p.questions.len() == 1 && p.questions[0].id == WRAP_QUESTION
}
pub fn uses_composer(p: &PendingInteraction) -> bool {
    is_wrap(p) || p.questions.len() == 1 && p.questions[0].id == OFFER_QUESTION
}
pub fn immediate(q: &InteractionQuestion) -> bool {
    matches!(q.id.as_str(), WRAP_QUESTION | OFFER_QUESTION) || q.id.starts_with(PROPOSAL_QUESTION)
}
pub fn action_labels(action: &str) -> &'static [&'static str] {
    match action {
        "commit" => &["Commit", "Commit changes", "提交", "提交更改"],
        "create_pr" => &[
            "Create PR",
            "Create pull request",
            "创建 PR",
            "创建拉取请求",
        ],
        "merge" => &["Merge", "Merge PR", "合并", "合并 PR"],
        "fix_ci" => &["Fix CI", "修复 CI"],
        "address_comments" => &["Address comments", "处理评论"],
        "resolve_conflicts" => &["Resolve conflicts", "解决冲突"],
        "verify" => &["Verify", "验证"],
        "archive" => &["Archive", "归档"],
        "start_task" => &["Start task", "Start", "New session", "开始任务", "新建会话"],
        "here" => &["Do it here", "在此执行", "在这里做"],
        "dismiss" => &["Dismiss", "Dismiss task", "Not now", "忽略", "暂不"],
        _ => &[],
    }
}
pub fn option_labels(option: &QuestionOption) -> Vec<&str> {
    if let Some(action) = option.id.strip_prefix("action:") {
        action_labels(action).to_vec()
    } else {
        vec![option.label.as_str()]
    }
}

fn truncate_utf16(s: &str, max: usize) -> String {
    let mut count = 0;
    s.chars()
        .take_while(|c| {
            count += c.len_utf16();
            count <= max
        })
        .collect()
}
fn text(v: &Value, max: usize) -> Option<String> {
    let s = v.as_str()?.trim();
    (!s.is_empty() && s.len() <= 65536).then(|| truncate_utf16(s, max))
}
fn normalized_reply(s: &str) -> String {
    use unicode_normalization::UnicodeNormalization;
    // Preserve joiners for the separate invisible-code-point rejection, as the
    // native app does. Other control/format separators are removed first.
    static REMOVED: std::sync::LazyLock<regex::Regex> = std::sync::LazyLock::new(|| {
        regex::Regex::new(r"[[\p{Cc}\p{Cf}\p{Zl}\p{Zp}\x{115f}\x{1160}\x{3164}\x{ffa0}\x{2800}\x{fffc}\x{1d159}\x{13441}\x{13442}\x{16910}]--[\x{200c}\x{200d}]]").unwrap()
    });
    REMOVED
        .replace_all(&s.nfc().collect::<String>(), "")
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
}
fn reply_text(v: &str) -> bool {
    static INVISIBLE: std::sync::LazyLock<regex::Regex> = std::sync::LazyLock::new(|| {
        regex::Regex::new(r"\p{Default_Ignorable_Code_Point}").unwrap()
    });
    let v = normalized_reply(v);
    !v.contains('@') && !v.starts_with(['/', '!']) && !INVISIBLE.is_match(&v)
}
fn reply(v: &Value, max: usize) -> Option<String> {
    let s = v.as_str()?;
    if s.len() > 65536 || !reply_text(s) {
        return None;
    }
    let s = normalized_reply(s);
    (!s.is_empty()).then(|| truncate_utf16(&s, max))
}
fn ask(args: &Value) -> Option<String> {
    ["ask", "next_ask", "next_brief"]
        .iter()
        .find_map(|k| args.get(k).filter(|v| !v.is_null()))
        .and_then(|v| text(v, 1200))
}

pub fn wrap_up(args: &Value, call: &str, session: &str) -> Option<PendingInteraction> {
    if call.is_empty() {
        return None;
    }
    let recap = text(&args["recap"], 1200).or_else(|| text(&args["outcome"], 1200))?;
    let blocked = args["blocked"] == true
        || args["blocked"] == "true"
        || text(&args["needs_user"], 1200).is_some();
    let explicit_ask = ask(args);
    let has_ask = explicit_ask.is_some();
    let ask = explicit_ask.or_else(|| {
        blocked.then(|| text(&args["needs_user"], 1200).unwrap_or_else(|| recap.clone()))
    })?;
    let actions = args
        .get("actions")
        .filter(|v| !v.is_null())
        .or_else(|| args.get("next_actions"))
        .and_then(Value::as_array);
    let primary = actions.and_then(|a| a.first());
    let primary_valid = primary.is_some_and(|a| {
        if let Some(action) = a["action"].as_str() {
            matches!(
                action,
                "commit"
                    | "create_pr"
                    | "merge"
                    | "fix_ci"
                    | "address_comments"
                    | "resolve_conflicts"
                    | "verify"
                    | "archive"
            )
        } else {
            reply(&a["label"], 24).is_some() && a["message"].as_str().is_none_or(reply_text)
        }
    });
    if !blocked && (!has_ask || !primary_valid) {
        return None;
    }
    let actions = if has_ask && primary_valid {
        actions
    } else {
        None
    };
    let mut options = Vec::new();
    // The installed desktop displays only the first two valid candidates.
    for (i, a) in actions
        .into_iter()
        .flatten()
        .filter(|a| {
            a["action"].as_str().is_some_and(|x| {
                matches!(
                    x,
                    "commit"
                        | "create_pr"
                        | "merge"
                        | "fix_ci"
                        | "address_comments"
                        | "resolve_conflicts"
                        | "verify"
                        | "archive"
                        | "new_session"
                )
            }) || a["action"].is_null() && text(&a["label"], 24).is_some()
        })
        .take(2)
        .enumerate()
    {
        let option = if let Some(action) = a["action"].as_str() {
            action_labels(action).first().map(|label| QuestionOption {
                id: format!("action:{action}"),
                label: (*label).into(),
                description: "在 Claude 中执行此操作".into(),
            })
        } else {
            reply(&a["label"], 24).and_then(|label| {
                if a["message"].as_str().is_some_and(|m| !reply_text(m)) {
                    return None;
                }
                let message = reply(&a["message"], 300).unwrap_or_else(|| label.clone());
                (reply_text(&label) && reply_text(&message)).then(|| QuestionOption {
                    id: i.to_string(),
                    label,
                    description: message,
                })
            })
        };
        if i == 0 && option.is_none() {
            // A dropped recommendation also drops the alternatives. A blocked
            // question still needs a reply through the ordinary composer.
            if !blocked {
                return None;
            }
            break;
        }
        if let Some(option) = option {
            options.push(option);
        }
    }
    let mut seen = std::collections::HashSet::new();
    if options
        .iter()
        .any(|o| !seen.insert(o.label.split_whitespace().collect::<Vec<_>>().join(" ")))
    {
        options.clear(); // Ambiguous buttons are not actionable; free text remains.
    }
    if options.is_empty() && !blocked {
        return None;
    }
    Some(PendingInteraction {
        id: format!("question:{call}"),
        session: session.into(),
        kind: "question".into(),
        prompt: recap,
        optional: !blocked,
        options: Vec::new(),
        questions: vec![InteractionQuestion {
            id: WRAP_QUESTION.into(),
            prompt: ask,
            header: "下一步".into(),
            options,
            multi_select: false,
            allow_text: true,
        }],
    })
}

/// Proposals remain pending across turns; tool results only acknowledge that
/// the chip was created. Only an explicit start/dismiss retires that proposal.
pub fn proposal(
    args: &Value,
    call: &str,
    session: &str,
    legacy: bool,
) -> Option<PendingInteraction> {
    let offer = args["initiation"] == "offer";
    if !legacy && !offer && args["initiation"] != "own_initiative" {
        return None;
    }
    let title = text(&args["title"], 300)?;
    let prompt = text(&args["prompt"], 16384).unwrap_or_default();
    let actions = if offer {
        vec!["start_task", "here"]
    } else {
        vec!["start_task", "dismiss"]
    };
    Some(PendingInteraction {
        id: format!("question:{call}"),
        session: session.into(),
        kind: "question".into(),
        prompt,
        optional: !offer,
        options: Vec::new(),
        questions: vec![InteractionQuestion {
            id: if offer {
                OFFER_QUESTION
            } else {
                PROPOSAL_QUESTION
            }
            .into(),
            prompt: title,
            header: "任务提议".into(),
            multi_select: false,
            allow_text: offer,
            options: actions
                .into_iter()
                .map(|a| QuestionOption {
                    id: format!("action:{a}"),
                    label: action_labels(a)[0].into(),
                    description: String::new(),
                })
                .collect(),
        }],
    })
}
fn output(v: &Value) -> String {
    if let Some(s) = v.as_str() {
        return s.into();
    }
    v.as_array()
        .into_iter()
        .flatten()
        .filter_map(|b| b["text"].as_str())
        .collect::<Vec<_>>()
        .join("\n")
}
fn task_id(v: &Value) -> Option<String> {
    if let Some(id) = v["task_id"].as_str() {
        return valid_task(id);
    }
    let s = output(v);
    let id = s
        .split("task_id:")
        .nth(1)?
        .trim_start()
        .split(|c: char| !(c.is_ascii_alphanumeric() || c == '_' || c == '-'))
        .next()?;
    valid_task(id)
}
fn valid_task(s: &str) -> Option<String> {
    (s.starts_with("task_")
        && s.len() <= 160
        && s.chars()
            .all(|c| c.is_ascii_alphanumeric() || c == '_' || c == '-'))
    .then(|| s.into())
}

mod presentation;
mod state;
pub use presentation::{Presentation, title};
pub use state::{
    TrackedFile, Tracker, delivered_file, delivered_key, desktop_tasks, records, snapshot,
};

#[cfg(test)]
mod tests;
