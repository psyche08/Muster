use super::state::{desktop_tasks_at, reconcile_tasks};
use super::*;
use serde_json::json;
fn call(id: &str, name: &str, args: Value) -> Value {
    json!({"type":"assistant","message":{"content":[{"type":"tool_use","id":id,"name":name,"input":args}]}})
}
fn result(id: &str, content: &str, error: bool) -> Value {
    json!({"type":"user","message":{"content":[{"type":"tool_result","tool_use_id":id,"content":content,"is_error":error}]}})
}
fn args() -> Value {
    json!({"recap":"Checks passed.","ask":"Continue?","actions":[{"label":"Continue"},{"label":"Wait","message":"Wait here"}]})
}
#[test]
fn wrap_variants_and_blocking_are_independent_of_immediate_buttons() {
    for blocked in [json!(true), json!("true")] {
        let mut a = args();
        a["blocked"] = blocked;
        let p = wrap_up(&a, "w", "s").unwrap();
        assert!(!p.optional);
        assert!(immediate(&p.questions[0]));
        assert!(uses_composer(&p));
        assert_eq!(p.questions[0].options[0].description, "Continue");
    }
    let legacy = json!({"outcome":"Checks passed.","next_ask":"Continue?","next_actions":[{"label":"Continue"}],"needs_user":"Decision"});
    let q = wrap_up(&legacy, "w", "s").unwrap();
    assert!(!q.optional);
    assert_eq!(q.questions[0].prompt, "Continue?");
    let mut a = args();
    a["next_brief"] = a["ask"].take();
    assert_eq!(
        wrap_up(&a, "w", "s").unwrap().questions[0].prompt,
        "Continue?"
    );
    for a in [
        json!({"recap":"Blocked","blocked":true}),
        json!({"outcome":"Blocked","needs_user":"Reply"}),
    ] {
        let p = wrap_up(&a, "w", "s").unwrap();
        assert!(!p.optional);
        assert!(p.questions[0].allow_text);
        assert!(p.questions[0].options.is_empty());
    }
    assert!(wrap_up(&json!({"recap":"Finished"}), "w", "s").is_none());
}
#[test]
fn app_actions_are_controls_never_shell_commands() {
    for action in [
        "commit",
        "create_pr",
        "merge",
        "fix_ci",
        "address_comments",
        "resolve_conflicts",
        "verify",
        "archive",
    ] {
        let mut a = args();
        a["actions"] = json!([{"action":action}]);
        let p = wrap_up(&a, "w", "s").unwrap();
        assert_eq!(p.questions[0].options[0].id, format!("action:{action}"));
        assert!(!option_labels(&p.questions[0].options[0]).is_empty());
    }
    for action in ["new_session", "unknown", "start_task", "rm -rf"] {
        let mut a = args();
        a["actions"] = json!([{"action":action},{"label":"Wait"}]);
        assert!(wrap_up(&a, "w", "s").is_none());
    }
}
#[test]
fn invalid_recommendation_drops_choices_but_preserves_blocked_text() {
    for bad in [
        "@file",
        "/command",
        "!exec",
        "hidden\u{200d}",
        "emoji\u{fe0f}",
    ] {
        let mut a = args();
        a["actions"][0]["message"] = json!(bad);
        assert!(wrap_up(&a, "w", "s").is_none());
        a["blocked"] = json!(true);
        let p = wrap_up(&a, "w", "s").unwrap();
        assert!(!p.optional);
        assert!(p.questions[0].options.is_empty());
    }
    let mut a = args();
    a["actions"] = json!([{"label":"Continue"},{"action":"new_session"},{"label":"Third"}]);
    assert_eq!(wrap_up(&a, "w", "s").unwrap().questions[0].options.len(), 1);
}
#[test]
fn blocked_display_is_not_answer_receipt_and_internal_text_does_not_retire() {
    let mut t = Tracker::new("s");
    let mut a = args();
    a["blocked"] = json!(true);
    t.record(&call("w", WRAP, a));
    t.record(&result("w", "Shown in the next-step box.", false));
    assert!(!t.snapshot().pending.unwrap().optional);
    assert!(t.snapshot().resolved.is_empty());
    for flag in ["isMeta", "isSidechain", "isSyntheticResult"] {
        let mut r = json!({"type":"user","message":{"content":"Internal"}});
        r[flag] = json!(true);
        t.record(&r);
        assert!(t.snapshot().pending.is_some());
    }
    t.record(&json!({"type":"user","message":{"content":"A custom answer"}}));
    assert!(t.snapshot().pending.is_none());
    assert!(t.snapshot().retired.contains("question:w"));
    assert!(t.snapshot().resolved.is_empty());
}
#[test]
fn unsuccessful_or_absent_native_box_never_leaves_clickable_choices() {
    for (content, error) in [
        ("Unavailable", true),
        ("client shows no next-step box", false),
        ("The app draws no box for this call", false),
    ] {
        let mut t = Tracker::new("s");
        t.record(&call("w", WRAP, args()));
        t.record(&result("w", content, error));
        assert!(t.snapshot().pending.is_none());
        assert!(t.snapshot().resolved.is_empty());
    }
}
#[test]
fn proposals_survive_user_turns_coexist_and_retire_by_task_id() {
    let mut t = Tracker::new("s");
    for (id, tool) in [("a", START), ("b", SPAWN)] {
        t.record(&call(
            id,
            tool,
            json!({"initiation":"own_initiative","title":"Task","prompt":"Work"}),
        ));
        t.record(&result(
            id,
            &format!("Proposed (task_id: task_{id})."),
            false,
        ));
    }
    t.record(&json!({"type":"user","message":{"content":"Continue this task"}}));
    t.record(&call("w", WRAP, args()));
    let s = t.snapshot();
    assert_eq!(s.iter().count(), 3);
    assert_eq!(s.pending.unwrap().id, "question:w");
    t.record(&call("d", DISMISS, json!({"task_id":"task_a"})));
    t.record(&result("d", "withdrawn", false));
    assert!(t.snapshot().find("question:a").is_none());
    assert!(t.snapshot().find("question:b").is_some());
    t.record(&call("start", START, json!({"proposal_id":"task_b"})));
    t.record(&result("start", "started", true));
    assert!(t.snapshot().find("question:b").is_some());
    t.record(&call("start2", START, json!({"proposal_id":"task_b"})));
    t.record(&result("start2", "started", false));
    assert!(t.snapshot().find("question:b").is_none());
}
#[test]
fn offer_waits_for_result_and_direct_start_is_not_a_proposal() {
    let mut t = Tracker::new("s");
    t.record(&call(
        "u",
        START,
        json!({"initiation":"user_asked","title":"Direct","prompt":"Work"}),
    ));
    assert!(t.snapshot().pending.is_none());
    t.record(&call(
        "o",
        START,
        json!({"initiation":"offer","title":"Where?"}),
    ));
    assert!(!t.snapshot().pending.unwrap().optional);
    t.record(&result("o", "Do it here", false));
    assert!(t.snapshot().pending.is_none());
    assert!(t.snapshot().resolved.contains("question:o"));
}
#[test]
fn proposals_expire_and_desktop_registry_resolves_out_of_band_clicks() {
    let mut t = Tracker::new("s");
    let mut c = call(
        "a",
        START,
        json!({"initiation":"own_initiative","title":"Task"}),
    );
    c["timestamp"] = json!("2026-10-15T00:00:00Z");
    let ts = crate::model::parse_ts(c["timestamp"].as_str().unwrap());
    t.record(&c);
    t.record(&result("a", "task_id: task_a", false));
    assert!(t.snapshot_at(ts + 7 * 86400 - 1).pending.is_some());
    assert!(t.snapshot_at(ts + 7 * 86400).pending.is_none());
    for status in ["started", "started_notified", "dismissed", "expired"] {
        let mut s = t.snapshot_at(ts);
        reconcile_tasks(&mut s, &json!({"resolved":{"task_a":status}}));
        assert!(s.pending.is_none());
        assert!(s.resolved.is_empty());
    }
}
fn present(p: &mut Presentation, id: &str, name: &str, args: Value) -> String {
    p.call(id, name, &args);
    p.result(id, &json!({"content":"ok"}), &Value::Null)
        .unwrap()
}
#[test]
fn plan_replay_supports_full_incremental_and_completion_without_accepting_failure() {
    let mut p = Presentation::default();
    present(
        &mut p,
        "1",
        PLAN,
        json!({"scope":"first_task","items":[{"content":"A","status":"in_progress"},{"content":"B","status":"completed"},{"content":"C","status":"pending"}]}),
    );
    let body = present(&mut p, "2", PLAN, json!({"advance":"true"}));
    assert_eq!(body, "- ✓ A\n- ✓ B\n- ◉ C");
    p.call("fail", PLAN, &json!({"items":[]}));
    assert!(
        p.result("fail", &json!({"is_error":true}), &Value::Null)
            .is_none()
    );
    assert_eq!(p.plan_body().unwrap(), body);
    let body = present(&mut p, "3", PLAN, json!({"done":[3],"start":1}));
    assert_eq!(body, "- ◉ A\n- ✓ B\n- ✓ C");
    present(&mut p, "4", WRAP, json!({"recap":"Done"}));
    assert_eq!(p.plan_body().unwrap(), "- ✓ A\n- ✓ B\n- ✓ C");
}
#[test]
fn presentation_preserves_chapter_proposal_and_only_verified_artifact_links() {
    let mut p = Presentation::default();
    assert!(
        present(
            &mut p,
            "c",
            CHAPTER,
            json!({"title":"Chapter","summary":"Summary"})
        )
        .contains("### Chapter")
    );
    assert!(
        present(
            &mut p,
            "s",
            START,
            json!({"initiation":"own_initiative","title":"Task","prompt":"Work"})
        )
        .contains("等待选择")
    );
    assert!(
        present(
            &mut p,
            "s2",
            START,
            json!({"initiation":"user_asked","title":"Task","prompt":"Work"})
        )
        .contains("已启动")
    );
    for url in [
        "file:///etc/passwd",
        "https://evil.test/code/artifact/a",
        "javascript:alert(1)",
    ] {
        p.call("a", "Artifact", &json!({}));
        assert!(p.result("a", &json!({}), &json!({"url":url})).is_none());
    }
    p.call("a", "Artifact", &json!({}));
    assert!(
        p.result(
            "a",
            &json!({}),
            &json!({"url":"https://claude.ai/code/artifact/test","title":"A"})
        )
        .unwrap()
        .contains("[A]")
    );
}
#[test]
fn delivered_files_require_matching_success_validated_path_and_complete_record() {
    use std::io::Write;
    let dir = tempfile::tempdir().unwrap();
    let target = dir.path().join("report.md");
    std::fs::write(&target, "report").unwrap();
    let transcript = dir.path().join("s.jsonl");
    let c = call("file", "SendUserFile", json!({"files":[target]}));
    let mut r = result("file", "delivered", false);
    r["toolUseResult"] =
        json!({"attachments":[{"path":target,"pathValidated":true}],"caption":"Report"});
    let key = "muster-delivered:file:0";
    let mut p = Presentation::default();
    p.call("file", "SendUserFile", &json!({}));
    let body = p
        .result("file", &r["message"]["content"][0], &r["toolUseResult"])
        .unwrap();
    assert!(body.contains(key));
    std::fs::write(&transcript, format!("{c}\n{r}")).unwrap();
    assert!(delivered_file(&transcript, key).is_err());
    std::fs::OpenOptions::new()
        .append(true)
        .open(&transcript)
        .unwrap()
        .write_all(b"\n")
        .unwrap();
    assert_eq!(delivered_file(&transcript, key).unwrap(), target);
    assert!(delivered_file(&transcript, "muster-delivered:unknown:0").is_err());
    assert!(delivered_file(&transcript, "muster-delivered:file:1").is_err());
    for mutation in ["failure", "unvalidated", "wrong_tool"] {
        let mut c = c.clone();
        let mut r = r.clone();
        match mutation {
            "failure" => r["message"]["content"][0]["is_error"] = json!(true),
            "unvalidated" => r["toolUseResult"]["attachments"][0]["pathValidated"] = json!(false),
            _ => c["message"]["content"][0]["name"] = json!("Read"),
        };
        std::fs::write(&transcript, format!("{c}\n{r}\n")).unwrap();
        assert!(delivered_file(&transcript, key).is_err());
    }
}
#[test]
fn streaming_snapshot_keeps_proposal_before_tail_window() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("s.jsonl");
    let c = call(
        "a",
        START,
        json!({"initiation":"own_initiative","title":"Task"}),
    );
    let r = result("a", "task_id: task_a", false);
    std::fs::write(
        &path,
        format!(
            "{c}\n{r}\n{}\n",
            json!({"type":"assistant","message":{"content":"x".repeat(3*1024*1024)}})
        ),
    )
    .unwrap();
    assert!(
        crate::questions::read_snapshot(&path, crate::model::Provider::Claude, "s")
            .unwrap()
            .find("question:a")
            .is_some()
    );
}
#[test]
fn normalized_replies_match_desktop_unicode_and_utf16_boundaries() {
    let mut a = args();
    a["actions"] = json!([{"label":"Cafe\u{301}","message":"  A\nB  "}]);
    let q = wrap_up(&a, "w", "s").unwrap();
    assert_eq!(q.questions[0].options[0].label, "Café");
    assert_eq!(q.questions[0].options[0].description, "AB");
    assert_eq!(truncate_utf16("😀😀😀", 5), "😀😀");
    a["actions"][0]["label"] = json!(format!("{}@file", "A".repeat(24)));
    assert!(wrap_up(&a, "w", "s").is_none());
}
#[test]
fn jsonl_cards_replace_successful_tools_but_keep_errors_and_reset_plan() {
    let mut reader = crate::jsonl::Reader::new(
        crate::model::Provider::Claude,
        crate::claude::records(),
        &crate::claude::HOOKS,
        crate::model::SessionMeta::default(),
    );
    let data = vec![
        (
            1,
            call(
                "p",
                PLAN,
                json!({"items":[{"content":"Build","status":"in_progress"}],"scope":"first_task"}),
            ),
        ),
        (2, result("p", "Plan shown", false)),
        (3, call("w", WRAP, json!({"recap":"Done"}))),
        (4, result("w", "Nothing is pending", false)),
    ];
    let (messages, _) = reader.read(&data);
    let cards: Vec<_> = messages
        .iter()
        .filter(|m| m.kind == crate::model::MessageKind::ProviderCard)
        .collect();
    assert_eq!(cards.len(), 3);
    assert_eq!(cards[0].text, "- ◉ Build");
    assert_eq!(cards[1].text, "- ✓ Build");
    assert_eq!(cards[2].text, "Done");
    assert_eq!(
        messages
            .iter()
            .map(|m| &m.id)
            .collect::<std::collections::HashSet<_>>()
            .len(),
        messages.len()
    );
    reader.reset();
    let (messages, _) = reader.read(&[
        (10, call("bad", PLAN, json!({"advance":true}))),
        (11, result("bad", "No plan", true)),
    ]);
    assert!(
        messages
            .iter()
            .all(|m| m.kind != crate::model::MessageKind::ProviderCard)
    );
    assert_eq!(
        messages.last().unwrap().kind,
        crate::model::MessageKind::ToolResult
    );
}
#[test]
fn tracked_file_retries_partial_records_and_resets_after_truncation() {
    use std::io::Write;
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("s.jsonl");
    let c = call("w", WRAP, args());
    std::fs::write(&path, format!("{c}")).unwrap();
    let mut t = TrackedFile::new(&path, "fixture-no-registry");
    assert!(
        t.read(&path, "fixture-no-registry")
            .unwrap()
            .pending
            .is_none()
    );
    std::fs::OpenOptions::new()
        .append(true)
        .open(&path)
        .unwrap()
        .write_all(b"\n")
        .unwrap();
    assert!(
        t.read(&path, "fixture-no-registry")
            .unwrap()
            .pending
            .is_some()
    );
    std::fs::write(&path, b"{}\n").unwrap();
    assert!(
        t.read(&path, "fixture-no-registry")
            .unwrap()
            .pending
            .is_none()
    );
}
#[test]
fn registry_is_scoped_to_cli_session_and_refreshes_without_transcript_changes() {
    let dir = tempfile::tempdir().unwrap();
    let user = dir.path().join("org/user");
    std::fs::create_dir_all(&user).unwrap();
    let path = user.join("local-test.json");
    let mut t = Tracker::new("fixture");
    t.record(&call(
        "a",
        START,
        json!({"initiation":"own_initiative","title":"Task"}),
    ));
    t.record(&result("a", "task_id: task_a", false));
    std::fs::write(
        &path,
        json!({"cliSessionId":"other","resolvedBackgroundTaskSuggestions":{"task_a":"started"}})
            .to_string(),
    )
    .unwrap();
    let mut s = t.snapshot();
    desktop_tasks_at(dir.path(), &mut s, "fixture");
    assert!(s.pending.is_some());
    std::fs::write(&path,json!({"cliSessionId":"fixture","resolvedBackgroundTaskSuggestions":{"task_a":"dismissed"}}).to_string()).unwrap();
    desktop_tasks_at(dir.path(), &mut s, "fixture");
    assert!(s.pending.is_none());
    assert!(s.retired.contains("question:a"));
    assert!(s.resolved.is_empty());
}
