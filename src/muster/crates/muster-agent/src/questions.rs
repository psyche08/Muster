//! Read provider question records, validate answers, and leave all writes to AX.
use crate::{Error, model::Provider};
use muster_proto::v1::{InteractionQuestion, PendingInteraction, QuestionAnswer, QuestionOption};
use serde_json::Value;
use std::{
    collections::{HashMap, HashSet},
    io::{Read, Seek, SeekFrom},
    path::Path,
};

#[derive(Clone, Debug, Default)]
pub struct Snapshot {
    /// Includes the complete source history, so absence retires stale UI.
    pub complete: bool,
    pub pending: Option<PendingInteraction>,
    pub additional: Vec<PendingInteraction>,
    pub resolved: HashSet<String>,
    /// No longer current; never treated as a submission acknowledgement.
    pub retired: HashSet<String>,
}
impl Snapshot {
    pub fn find(&self, id: &str) -> Option<&PendingInteraction> {
        self.iter().find(|q| q.id == id)
    }
    pub fn iter(&self) -> impl Iterator<Item = &PendingInteraction> {
        self.pending.iter().chain(&self.additional)
    }
}
pub type Source = std::sync::Arc<dyn Fn() -> Option<Snapshot> + Send + Sync>;

pub fn read_snapshot(path: &Path, provider: Provider, session: &str) -> Option<Snapshot> {
    // CatDesk question cards are not read yet; its store is not JSONL.
    if !provider.file_transcripts() {
        return None;
    }
    if provider == Provider::Claude {
        let mut tracker = crate::claude_cards::Tracker::new(session);
        crate::claude_cards::records(path, |r| tracker.record(r)).ok()?;
        return Some(tracker.snapshot());
    }
    let mut file = std::fs::File::open(path).ok()?;
    let length = file.metadata().ok()?.len();
    let offset = length.saturating_sub(2 * 1024 * 1024);
    file.seek(SeekFrom::Start(offset)).ok()?;
    let mut bytes = Vec::new();
    file.take(2 * 1024 * 1024).read_to_end(&mut bytes).ok()?;
    let start = if offset > 0 {
        bytes.iter().position(|b| *b == b'\n')? + 1
    } else {
        0
    };
    let end = bytes.iter().rposition(|b| *b == b'\n')?;
    if start > end {
        return None;
    }
    Some(snapshot(&bytes[start..end], provider, session))
}

fn snapshot(bytes: &[u8], provider: Provider, session: &str) -> Snapshot {
    if provider == Provider::Claude {
        return crate::claude_cards::snapshot(bytes, session);
    }
    let mut resolved = HashSet::new();
    let mut retired = HashSet::new();
    let mut replied: HashMap<String, HashSet<usize>> = HashMap::new();
    let mut requests: Vec<(String, bool, PendingInteraction)> = Vec::new();
    for line in bytes.split(|b| *b == b'\n') {
        let Ok(r) = serde_json::from_slice::<Value>(line) else {
            continue;
        };
        match provider {
            // Current desktop builds retain asynchronous controls after completion.
            // A later turn still retires previous optional questions; completion
            // alone is neither an answer receipt nor proof of absent controls.
            Provider::Codex
                if r["type"] == "event_msg" && r["payload"]["type"] == "task_started" =>
            {
                retired.extend(
                    requests
                        .iter()
                        .filter(|(_, asynchronous, _)| *asynchronous)
                        .map(|(_, _, q)| q.id.clone()),
                );
            }
            Provider::Codex if r["type"] == "response_item" => {
                let p = &r["payload"];
                let name = p["name"].as_str().unwrap_or("");
                if matches!(
                    p["type"].as_str(),
                    Some("function_call" | "custom_tool_call")
                ) && matches!(
                    name.rsplit('.').next(),
                    Some("request_user_input" | "request_user_input_async")
                ) {
                    let raw = p["arguments"]
                        .as_str()
                        .or_else(|| p["input"].as_str())
                        .unwrap_or("");
                    if let Ok(args) = serde_json::from_str(raw) {
                        if let Some(mut q) =
                            parse(&args, p["call_id"].as_str().unwrap_or(""), session)
                        {
                            q.optional = name.ends_with("_async");
                            requests.push((
                                p["call_id"].as_str().unwrap_or("").into(),
                                name.ends_with("_async"),
                                q,
                            ));
                        }
                    }
                } else if matches!(
                    p["type"].as_str(),
                    Some("function_call_output" | "custom_tool_call_output")
                ) {
                    if let Some(id) = p["call_id"].as_str() {
                        if requests
                            .iter()
                            .any(|(key, asynchronous, _)| key == id && !asynchronous)
                        {
                            resolved.insert(format!("question:{id}"));
                        }
                    }
                    requests.retain(|(id, asynchronous, _)| {
                        *asynchronous || Some(id.as_str()) != p["call_id"].as_str()
                    });
                } else if p["type"] == "message" && p["role"] == "user" {
                    let reply = p["content"].as_array().into_iter().flatten().any(|b| {
                        b["text"]
                            .as_str()
                            .is_some_and(|t| t.contains("<send_user_message_question_reply>"))
                    });
                    if !reply {
                        retired.extend(
                            requests
                                .iter()
                                .filter(|(_, asynchronous, _)| *asynchronous)
                                .map(|(_, _, q)| q.id.clone()),
                        );
                    }
                    for b in p["content"].as_array().into_iter().flatten() {
                        let text = b["text"].as_str().unwrap_or("");
                        if let Some(body) = text
                            .split("<send_user_message_question_reply>")
                            .nth(1)
                            .and_then(|s| s.split("</send_user_message_question_reply>").next())
                        {
                            if let Ok(Value::Array(replies)) =
                                serde_json::from_str::<Value>(body.trim())
                            {
                                for reply in replies {
                                    if let Some(key) = reply["questionItemId"]
                                        .as_str()
                                        .and_then(|s| serde_json::from_str::<Vec<Value>>(s).ok())
                                    {
                                        if let (Some(id), Some(index)) = (
                                            key.get(1).and_then(Value::as_str),
                                            key.get(2).and_then(Value::as_u64),
                                        ) {
                                            if let Ok(index) = usize::try_from(index) {
                                                replied.entry(id.into()).or_default().insert(index);
                                            }
                                        }
                                    }
                                }
                            }
                        }
                    }
                }
            }
            _ => {}
        }
    }
    for (id, asynchronous, question) in &mut requests {
        if *asynchronous {
            if let Some(indices) = replied.get(id) {
                question.questions = question
                    .questions
                    .iter()
                    .enumerate()
                    .filter(|(index, _)| !indices.contains(index))
                    .map(|(_, q)| q.clone())
                    .collect();
                if question.questions.is_empty() {
                    resolved.insert(question.id.clone());
                }
            }
        }
    }
    requests.retain(|(_, _, q)| !q.questions.is_empty());
    Snapshot {
        complete: false,
        additional: Vec::new(),
        pending: requests
            .into_iter()
            .rev()
            .find(|(_, _, q)| !retired.contains(&q.id))
            .map(|(_, _, q)| q),
        resolved,
        retired,
    }
}

/// A provider may acknowledge individual asynchronous questions. Continue only
/// when every remaining question is an unchanged member of the original call.
pub fn remaining_matches(expected: &PendingInteraction, current: &PendingInteraction) -> bool {
    if expected == current {
        return true;
    }
    expected.optional
        && current.optional
        && expected.id == current.id
        && expected.session == current.session
        && expected.kind == current.kind
        && expected.prompt == current.prompt
        && expected.options == current.options
        && !current.questions.is_empty()
        && current.questions.len() < expected.questions.len()
        && current
            .questions
            .iter()
            .all(|q| expected.questions.contains(q))
}

/// Optional suggestion cards use immediate-response controls: choosing
/// an option advances the page or sends the answer, rather than toggling a
/// radio.
pub fn immediate_response(
    cards: &crate::profile::Questions,
    pending: &PendingInteraction,
    question: &InteractionQuestion,
) -> bool {
    (cards.immediate_suggestions && pending.optional || crate::claude_cards::immediate(question))
        && !question.multi_select
}

pub fn immediate_option(
    cards: &crate::profile::Questions,
    pending: &PendingInteraction,
    plan: &AnswerPlan,
) -> bool {
    immediate_response(cards, pending, &plan.question)
        && plan.labels.len() == 1
        && plan.text.is_empty()
}

#[cfg(test)]
fn parse_claude_wrap_up(args: &Value, call_id: &str, session: &str) -> Option<PendingInteraction> {
    crate::claude_cards::wrap_up(args, call_id, session)
}

pub(crate) fn parse(args: &Value, call_id: &str, session: &str) -> Option<PendingInteraction> {
    if call_id.is_empty() {
        return None;
    }
    let raw = args["questions"].as_array()?;
    if raw.is_empty() || raw.len() > 16 {
        return None;
    }
    let mut questions = Vec::new();
    let mut ids = HashSet::new();
    for (i, q) in raw.iter().enumerate() {
        let id = q["id"]
            .as_str()
            .map(str::to_owned)
            .unwrap_or_else(|| i.to_string());
        let prompt = q["question"]
            .as_str()
            .or_else(|| q["title"].as_str())?
            .to_string();
        if id.is_empty() || !ids.insert(id.clone()) || prompt.is_empty() || prompt.len() > 16384 {
            return None;
        }
        let mut options = Vec::new();
        for (j, o) in q["options"].as_array().into_iter().flatten().enumerate() {
            let label = o.as_str().or_else(|| o["label"].as_str())?;
            if label.is_empty() || label.len() > 4096 || j >= 32 {
                return None;
            }
            options.push(QuestionOption {
                id: j.to_string(),
                label: label.into(),
                description: o["description"]
                    .as_str()
                    .unwrap_or("")
                    .chars()
                    .take(8192)
                    .collect(),
            });
        }
        questions.push(InteractionQuestion {
            id,
            header: q["header"].as_str().unwrap_or("").into(),
            prompt,
            options,
            multi_select: q["multiSelect"] == true,
            allow_text: true,
        });
    }
    Some(PendingInteraction {
        id: format!("question:{call_id}"),
        session: session.into(),
        kind: "question".into(),
        optional: false,
        prompt: String::new(),
        options: Vec::new(),
        questions,
    })
}

#[cfg(test)]
mod focused_submit_tests {
    use super::*;

    #[test]
    fn missing_or_conflicting_focus_never_admits_a_submit_key() {
        for enabled in [None, Some(false), Some(true)] {
            for focused in [None, Some(false), Some(true)] {
                for application in [
                    SubmitApplicationFocus::Matching,
                    SubmitApplicationFocus::Different,
                    SubmitApplicationFocus::NotSupplied,
                    SubmitApplicationFocus::Failed,
                ] {
                    for same_window in [false, true] {
                        assert_eq!(
                            admit_focused_submit(enabled, focused, application, same_window)
                                .is_ok(),
                            enabled == Some(true)
                                && focused == Some(true)
                                && (application == SubmitApplicationFocus::Matching
                                    || application == SubmitApplicationFocus::NotSupplied
                                        && same_window)
                        );
                    }
                }
            }
        }
    }

    #[test]
    fn delayed_focus_waits_without_admitting_a_key_and_has_a_hard_bound() {
        use SubmitApplicationFocus::NotSupplied;
        use std::time::Duration;
        assert!(
            !poll_focused_submit(
                Some(true),
                Some(false),
                NotSupplied,
                true,
                0,
                Duration::ZERO
            )
            .unwrap()
        );
        assert!(
            poll_focused_submit(
                Some(true),
                Some(true),
                NotSupplied,
                true,
                1,
                Duration::from_millis(25)
            )
            .unwrap()
        );
        assert!(
            poll_focused_submit(
                Some(true),
                Some(false),
                NotSupplied,
                true,
                5,
                Duration::from_millis(125)
            )
            .is_err()
        );
        assert!(
            poll_focused_submit(
                Some(true),
                Some(true),
                NotSupplied,
                true,
                6,
                Duration::from_millis(150)
            )
            .is_err()
        );
        assert!(
            poll_focused_submit(
                Some(true),
                Some(true),
                NotSupplied,
                true,
                1,
                Duration::from_millis(250)
            )
            .is_err()
        );
    }

    #[test]
    fn focus_wait_never_recovers_a_conflicting_or_missing_destination() {
        use std::time::Duration;
        for application in [
            SubmitApplicationFocus::Failed,
            SubmitApplicationFocus::Different,
            SubmitApplicationFocus::NotSupplied,
        ] {
            for focused in [None, Some(false), Some(true)] {
                assert!(
                    poll_focused_submit(Some(true), focused, application, false, 0, Duration::ZERO)
                        .is_err()
                );
            }
        }
        for enabled in [None, Some(false)] {
            assert!(
                poll_focused_submit(
                    enabled,
                    Some(false),
                    SubmitApplicationFocus::Matching,
                    true,
                    0,
                    Duration::ZERO
                )
                .is_err()
            );
        }
    }

    #[test]
    fn only_claude_separate_submit_uses_keyboard_activation() {
        use crate::profile::{CLAUDE, CODEX};
        assert!(CLAUDE.questions.as_ref().unwrap().submit_via_focused_space);
        assert!(!CODEX.questions.as_ref().unwrap().submit_via_focused_space);
    }
}

#[derive(Clone, Debug)]
pub struct AnswerPlan {
    pub question: InteractionQuestion,
    pub labels: Vec<String>,
    pub text: String,
}

/// Independent observation of the application's current focused element.
/// NotSupplied requires an explicit native absence/unsupported result; IPC
/// errors, malformed values and stale proxies are Failed, never NotSupplied.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SubmitApplicationFocus {
    Matching,
    Different,
    NotSupplied,
    Failed,
}

/// Admission immediately before a single app-directed keyboard activation.
/// An absent application-level element requires independent window identity;
/// neither absent control focus nor contradictory evidence can admit a key.
pub fn admit_focused_submit(
    enabled: Option<bool>,
    focused: Option<bool>,
    application: SubmitApplicationFocus,
    same_focused_window: bool,
) -> Result<(), Error> {
    let target = application == SubmitApplicationFocus::Matching
        || application == SubmitApplicationFocus::NotSupplied && same_focused_window;
    if enabled != Some(true) || focused != Some(true) || !target {
        return Err(Error::Conflict(
            "The question submit focus changed; no activation key sent".into(),
        ));
    }
    Ok(())
}

/// Bounded readiness for an asynchronous native focus write. Wait only when
/// the destination proof is already valid and control focus has not settled.
/// The caller must recheck authorization/input before every fresh OS sample.
pub fn poll_focused_submit(
    enabled: Option<bool>,
    focused: Option<bool>,
    application: SubmitApplicationFocus,
    same_focused_window: bool,
    sample: usize,
    elapsed: std::time::Duration,
) -> Result<bool, Error> {
    if sample >= 6 || elapsed >= std::time::Duration::from_millis(250) {
        return Err(Error::Conflict(
            "Question Submit focus did not settle in time; no activation key sent".into(),
        ));
    }
    let admission = admit_focused_submit(enabled, focused, application, same_focused_window);
    if admission.is_ok() {
        return Ok(true);
    }
    if sample < 5
        && enabled == Some(true)
        && focused == Some(false)
        && (application == SubmitApplicationFocus::Matching
            || application == SubmitApplicationFocus::NotSupplied && same_focused_window)
    {
        return Ok(false);
    }
    admission.map(|()| true)
}

pub fn validate(
    interaction: &PendingInteraction,
    answers: &[QuestionAnswer],
) -> Result<Vec<AnswerPlan>, Error> {
    if interaction.kind != "question"
        || interaction.questions.is_empty()
        || answers.len() != interaction.questions.len()
    {
        return Err(Error::Invalid(
            "Answer every question before submitting".into(),
        ));
    }
    let by_id: HashMap<_, _> = answers
        .iter()
        .map(|a| (a.question_id.as_str(), a))
        .collect();
    if by_id.len() != answers.len() {
        return Err(Error::Invalid("Duplicate question answer".into()));
    }
    interaction
        .questions
        .iter()
        .map(|q| {
            let a = by_id
                .get(q.id.as_str())
                .ok_or_else(|| Error::Invalid("Missing question answer".into()))?;
            let text = a.text.trim();
            if text.len() > 8192
                || text
                    .chars()
                    .any(|c| c.is_control() && c != '\n' && c != '\t')
                || (!q.allow_text && !text.is_empty())
            {
                return Err(Error::Invalid("Invalid custom answer".into()));
            }
            let ids: HashSet<_> = a.option_ids.iter().collect();
            if ids.len() != a.option_ids.len()
                || (!q.multi_select && a.option_ids.len() + usize::from(!text.is_empty()) > 1)
            {
                return Err(Error::Invalid("Choose one answer for this question".into()));
            }
            if ids.is_empty() && text.is_empty() {
                return Err(Error::Invalid("Select an option or enter an answer".into()));
            }
            let labels = a
                .option_ids
                .iter()
                .map(|id| {
                    q.options
                        .iter()
                        .find(|o| &o.id == id)
                        .map(|o| o.label.clone())
                        .ok_or_else(|| Error::Invalid("This option is no longer offered".into()))
                })
                .collect::<Result<Vec<_>, _>>()?;
            Ok(AnswerPlan {
                question: q.clone(),
                labels,
                text: text.into(),
            })
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    fn pending(bytes: &[u8], provider: Provider, session: &str) -> Option<PendingInteraction> {
        snapshot(bytes, provider, session).pending
    }
    fn fixture() -> Value {
        serde_json::json!({"questions":[{"question":"Which endpoint?","header":"Endpoint","options":[{"label":"RPC","description":"Authenticated"},{"label":"HTTP","description":"Browser"}],"multiSelect":false},{"question":"Which clients?","options":[{"label":"Mac"},{"label":"iPhone"}],"multiSelect":true}]})
    }
    #[test]
    fn validates_complete_answers_and_rejects_stale_or_ambiguous_selections() {
        let q = parse(&fixture(), "call", "session").unwrap();
        let mut answers = vec![
            QuestionAnswer {
                question_id: "0".into(),
                option_ids: vec!["0".into()],
                text: String::new(),
            },
            QuestionAnswer {
                question_id: "1".into(),
                option_ids: vec!["0".into(), "1".into()],
                text: "also iPad".into(),
            },
        ];
        assert_eq!(validate(&q, &answers).unwrap()[1].labels.len(), 2);
        assert!(validate(&q, &answers[..1]).is_err());
        answers[0].text = "custom".into();
        assert!(validate(&q, &answers).is_err());
        answers[0].option_ids.clear();
        assert!(validate(&q, &answers).is_ok());
        answers[1].option_ids = vec!["stale".into()];
        assert!(validate(&q, &answers).is_err());
    }
    #[test]
    fn optional_single_choice_suggestions_submit_immediately() {
        let mut q = parse(&fixture(), "call", "session").unwrap();
        let answers = vec![
            QuestionAnswer {
                question_id: "0".into(),
                option_ids: vec!["0".into()],
                text: String::new(),
            },
            QuestionAnswer {
                question_id: "1".into(),
                option_ids: vec!["0".into()],
                text: String::new(),
            },
        ];
        let plans = validate(&q, &answers).unwrap();
        let codex = crate::profile::CODEX.questions.as_ref().unwrap();
        let claude = crate::profile::CLAUDE.questions.as_ref().unwrap();
        assert!(!immediate_option(codex, &q, &plans[0]));
        q.optional = true;
        assert!(immediate_option(codex, &q, &plans[0]));
        assert!(immediate_option(claude, &q, &plans[0]));
        assert!(!immediate_option(codex, &q, &plans[1]));
        let mut custom = answers;
        custom[0].option_ids.clear();
        custom[0].text = "custom reply".into();
        let plans = validate(&q, &custom).unwrap();
        assert!(immediate_response(codex, &q, &plans[0].question));
        assert!(!immediate_option(codex, &q, &plans[0]));
    }
    fn wrap_up(id: &str) -> Value {
        serde_json::json!({"type":"assistant","message":{"content":[{
            "type":"tool_use","id":id,"name":"mcp__ccd_turn__wrap_up",
            "input":{"recap":"Build checks passed. Not deployed.","ask":"现在部署吗？",
                "actions":[{"label":"部署","message":"构建并部署到 host"},
                    {"label":"先不部署","message":"先不部署"}]}
        }]}})
    }
    fn records(rows: &[Value]) -> Snapshot {
        snapshot(
            rows.iter()
                .map(Value::to_string)
                .collect::<Vec<_>>()
                .join("\n")
                .as_bytes(),
            Provider::Claude,
            "s",
        )
    }
    #[test]
    fn claude_wrap_up_is_offered_after_display_result_and_turn_end() {
        let call = wrap_up("wrap");
        let shown = serde_json::json!({"type":"user","message":{"content":[{
            "type":"tool_result","tool_use_id":"wrap","content":[{"type":"text","text":"Shown in the next-step box."}]
        }]}});
        let ended = serde_json::json!({"type":"assistant","message":{"content":[{"type":"text","text":"等你决定。"}],"stop_reason":"end_turn"}});
        let state = records(&[call, shown, ended]);
        let q = state.pending.unwrap();
        assert!(state.resolved.is_empty());
        assert!(state.retired.is_empty());
        assert_eq!(q.id, "question:wrap");
        assert_eq!(q.session, "s");
        assert!(q.optional);
        assert_eq!(q.prompt, "Build checks passed. Not deployed.");
        assert_eq!(q.questions[0].prompt, "现在部署吗？");
        assert_eq!(q.questions[0].options[0].label, "部署");
        assert_eq!(q.questions[0].options[0].description, "构建并部署到 host");
        assert!(q.questions[0].allow_text);
        let answer = QuestionAnswer {
            question_id: crate::claude_cards::WRAP_QUESTION.into(),
            option_ids: vec!["0".into()],
            text: String::new(),
        };
        let plans = validate(&q, std::slice::from_ref(&answer)).unwrap();
        assert!(immediate_option(
            crate::profile::CLAUDE.questions.as_ref().unwrap(),
            &q,
            &plans[0]
        ));
        assert!(
            validate(
                &q,
                &[QuestionAnswer {
                    option_ids: vec![],
                    text: "custom".into(),
                    ..answer
                }]
            )
            .is_ok()
        );
    }
    #[test]
    fn claude_wrap_up_retires_on_new_prompt_without_claiming_answer_receipt() {
        for content in [
            serde_json::json!("构建并部署到 host"),
            serde_json::json!([{"type":"text","text":"换一个任务"}]),
        ] {
            let state = records(&[
                wrap_up("wrap"),
                serde_json::json!({"type":"user","message":{"content":content}}),
            ]);
            assert!(state.pending.is_none());
            assert!(state.retired.contains("question:wrap"));
            assert!(!state.resolved.contains("question:wrap"));
        }
        for flag in ["isMeta", "isSidechain"] {
            let mut prompt =
                serde_json::json!({"type":"user","message":{"content":"internal prompt"}});
            prompt[flag] = true.into();
            assert!(records(&[wrap_up("wrap"), prompt]).pending.is_some());
            let mut call = wrap_up("ignored");
            call[flag] = true.into();
            assert!(records(&[call]).pending.is_none());
        }
    }
    #[test]
    fn claude_wrap_up_replacement_failure_and_recap_only_retire_old_actions() {
        let state = records(&[wrap_up("old"), wrap_up("new")]);
        assert_eq!(state.pending.unwrap().id, "question:new");
        assert!(state.retired.contains("question:old"));
        let failed = serde_json::json!({"type":"user","message":{"content":[{"type":"tool_result","tool_use_id":"new","is_error":true,"content":"failed"}]}});
        let state = records(&[wrap_up("old"), wrap_up("new"), failed]);
        assert!(state.pending.is_none());
        assert!(state.retired.contains("question:new"));
        assert!(state.resolved.is_empty());
        let mut recap = wrap_up("recap");
        recap["message"]["content"][0]["input"]["actions"] = serde_json::json!([]);
        let state = records(&[wrap_up("old"), recap]);
        assert!(state.pending.is_none());
        assert!(state.retired.contains("question:old"));
    }
    #[test]
    fn claude_wrap_up_rejects_ambiguous_or_malformed_actions() {
        let call = wrap_up("wrap");
        let args = &call["message"]["content"][0]["input"];
        let mut bad = args.clone();
        bad["actions"][0]["label"] = "".into();
        assert!(parse_claude_wrap_up(&bad, "wrap", "s").is_none());
        let mut label_only = args.clone();
        label_only["actions"][0]
            .as_object_mut()
            .unwrap()
            .remove("message");
        assert_eq!(
            parse_claude_wrap_up(&label_only, "wrap", "s")
                .unwrap()
                .questions[0]
                .options[0]
                .description,
            "部署"
        );
        let mut duplicate = args.clone();
        duplicate["actions"][1]["label"] = " 部署 ".into();
        assert!(parse_claude_wrap_up(&duplicate, "wrap", "s").is_none());
        assert!(parse_claude_wrap_up(args, "", "s").is_none());
        let mut many = args.clone();
        many["actions"] = Value::Array(
            (0..33)
                .map(|i| serde_json::json!({"label":i.to_string(),"message":"next"}))
                .collect(),
        );
        assert_eq!(
            parse_claude_wrap_up(&many, "wrap", "s").unwrap().questions[0]
                .options
                .len(),
            2
        );
        let mut unrelated = call;
        unrelated["message"]["content"][0]["name"] = "mcp__other__wrap_up".into();
        assert!(records(&[unrelated]).pending.is_none());
    }
    #[test]
    fn claude_completed_questions_are_not_offered_again() {
        let call = serde_json::json!({"type":"assistant","message":{"content":[{"type":"tool_use","name":"AskUserQuestion","id":"call","input":fixture()}]}});
        assert_eq!(
            pending(call.to_string().as_bytes(), Provider::Claude, "s")
                .unwrap()
                .questions
                .len(),
            2
        );
        let text = format!(
            "{call}\n{}",
            serde_json::json!({"type":"user","message":{"content":[{"type":"tool_result","tool_use_id":"call","content":"answered"}]}})
        );
        assert!(pending(text.as_bytes(), Provider::Claude, "s").is_none());
    }
    #[test]
    fn codex_uses_stable_question_ids_and_waits_for_tool_result() {
        let call = serde_json::json!({"type":"response_item","payload":{"type":"function_call","name":"functions.request_user_input","call_id":"call","arguments":fixture().to_string()}});
        assert!(pending(call.to_string().as_bytes(), Provider::Codex, "s").is_some());
        let text = format!(
            "{call}\n{}",
            serde_json::json!({"type":"response_item","payload":{"type":"function_call_output","call_id":"call","output":"answered"}})
        );
        assert!(pending(text.as_bytes(), Provider::Codex, "s").is_none());
    }
    #[test]
    fn async_codex_questions_require_each_answer_and_ignore_queued_ack() {
        let call = serde_json::json!({"type":"response_item","payload":{"type":"function_call","name":"functions.request_user_input_async","call_id":"call","arguments":fixture().to_string()}});
        let ack = serde_json::json!({"type":"response_item","payload":{"type":"function_call_output","call_id":"call","output":"queued"}});
        let reply = |index| serde_json::json!({"type":"response_item","payload":{"type":"message","role":"user","content":[{"text":format!("<send_user_message_question_reply>{}</send_user_message_question_reply>", serde_json::json!([{"questionItemId":serde_json::json!(["request_user_input_async","call",index]).to_string(),"answer":"custom"}]))}]}});
        let text = format!("{call}\n{ack}\n{}", reply(0));
        let first = snapshot(text.as_bytes(), Provider::Codex, "s");
        assert!(!first.resolved.contains("question:call"));
        assert_eq!(first.pending.unwrap().questions[0].id, "1");
        let text = format!("{text}\n{}", reply(1));
        let done = snapshot(text.as_bytes(), Provider::Codex, "s");
        assert!(done.pending.is_none());
        assert!(done.resolved.contains("question:call"));
    }
    #[test]
    fn completed_turn_keeps_async_card_until_the_provider_records_its_answer() {
        let args = serde_json::json!({"questions":[{"title":"Receipt test?","options":["Received","Not received"]}]});
        let call = serde_json::json!({"type":"response_item","payload":{"type":"function_call","name":"functions.request_user_input_async","call_id":"receipt","arguments":args.to_string()}});
        let ack = serde_json::json!({"type":"response_item","payload":{"type":"function_call_output","call_id":"receipt","output":"{\"accepted\":true}"}});
        let end = serde_json::json!({"type":"event_msg","payload":{"type":"task_complete"}});
        let text = format!("{call}\n{ack}\n{end}");
        let current = snapshot(text.as_bytes(), Provider::Codex, "s");
        let card = current
            .pending
            .expect("native async card stays visible after completion");
        assert!(card.optional);
        assert_eq!(card.questions[0].options[0].label, "Received");
        assert!(!current.retired.contains("question:receipt"));
        let reply = serde_json::json!({"type":"response_item","payload":{"type":"message","role":"user","content":[{"text":format!("<send_user_message_question_reply>{}</send_user_message_question_reply>", serde_json::json!([{"questionItemId":"[\"request_user_input_async\",\"receipt\",0]","answer":"Received"}]))}]}});
        let done = snapshot(format!("{text}\n{reply}").as_bytes(), Provider::Codex, "s");
        assert!(done.pending.is_none());
        assert!(done.resolved.contains("question:receipt"));
    }

    #[test]
    fn previous_turn_optional_questions_retire_without_acknowledgement() {
        let call = serde_json::json!({"type":"response_item","payload":{"type":"function_call","name":"functions.request_user_input_async","call_id":"optional","arguments":fixture().to_string()}});
        let current = snapshot(call.to_string().as_bytes(), Provider::Codex, "s");
        assert!(current.pending.unwrap().optional);
        for boundary in [
            serde_json::json!({"type":"event_msg","payload":{"type":"task_started"}}),
            serde_json::json!({"type":"response_item","payload":{"type":"message","role":"user","content":[{"text":"Continue with the next task"}]}}),
        ] {
            let text = format!("{call}\n{boundary}");
            let old = snapshot(text.as_bytes(), Provider::Codex, "s");
            assert!(old.pending.is_none());
            assert!(old.retired.contains("question:optional"));
            assert!(!old.resolved.contains("question:optional"));
            let required = serde_json::json!({"type":"response_item","payload":{"type":"function_call","name":"functions.request_user_input","call_id":"required","arguments":fixture().to_string()}});
            let text = format!("{text}\n{required}");
            assert!(
                !snapshot(text.as_bytes(), Provider::Codex, "s")
                    .pending
                    .unwrap()
                    .optional
            );
        }
    }
    #[test]
    fn remaining_questions_must_be_unchanged_members_of_the_async_call() {
        let mut original = parse(&fixture(), "call", "session").unwrap();
        original.optional = true;
        let mut partial = original.clone();
        partial.questions.remove(0);
        assert!(remaining_matches(&original, &partial));
        partial.questions[0].options[0].label = "Changed".into();
        assert!(!remaining_matches(&original, &partial));
        partial = original.clone();
        partial.questions.remove(0);
        partial.id = "another-call".into();
        assert!(!remaining_matches(&original, &partial));
        partial = original.clone();
        partial.questions.clear();
        assert!(!remaining_matches(&original, &partial));
        original.optional = false;
        partial = original.clone();
        partial.questions.remove(0);
        assert!(!remaining_matches(&original, &partial));
    }
}
