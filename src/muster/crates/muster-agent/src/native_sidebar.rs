//! Exact title matching for native Agent sidebar controls. A match only
//! selects a candidate; the controller must still verify the opened session ID.

/// A complete sidebar can omit a known older Claude Code session. This route
/// only requests navigation: callers must guard the OS action and resolve the
/// resulting surface to the exact catalog ID before any mutation.
pub fn absent_session_route(
    provider: &str,
    session: &str,
    navigation_areas: usize,
    scan_complete: bool,
    candidates: usize,
) -> Option<String> {
    if provider != "claude" || navigation_areas == 0 || !scan_complete || candidates != 0 {
        return None;
    }
    let canonical_uuid = session.len() == 36
        && session.bytes().enumerate().all(|(i, b)| {
            if matches!(i, 8 | 13 | 18 | 23) {
                b == b'-'
            } else {
                b.is_ascii_digit() || (b'a'..=b'f').contains(&b)
            }
        });
    canonical_uuid.then(|| format!("claude://code/continue?session={session}"))
}

/// OS adapters expose only the attributes needed to read a button's label.
/// AXPress is deliberately not a boundary: Electron exposes it on groups
/// that merely wrap a static title. Nested controls remain boundaries by role.
pub trait LabelNode: Clone + PartialEq {
    fn role(&self) -> String;
    fn text(&self) -> Option<String>;
    fn children(&self) -> Vec<Self>;
}

pub fn read_label<N: LabelNode>(control: &N) -> (Option<Vec<String>>, usize, usize, &'static str) {
    let started = std::time::Instant::now();
    let mut pending = control.children();
    let mut seen = Vec::new();
    let mut labels = Vec::new();
    let mut skipped = 0;
    while let Some(node) = pending.pop() {
        if seen.len() >= 64 || started.elapsed() >= std::time::Duration::from_millis(100) {
            return (None, seen.len(), skipped, "budget");
        }
        if seen.contains(&node) {
            continue;
        }
        seen.push(node.clone());
        match node.role().as_str() {
            "AXStaticText" => {
                let Some(value) = node.text() else {
                    return (None, seen.len(), skipped, "unreadable_text");
                };
                if !value.trim().is_empty() {
                    labels.push(value);
                }
            }
            "AXGroup" => pending.extend(node.children()),
            "AXButton" | "AXLink" | "AXRow" | "AXPopUpButton" | "AXMenu" => skipped += 1,
            _ => {}
        }
    }
    (Some(labels), seen.len(), skipped, "complete")
}

/// Whether a sidebar row names `expected`. With `standalone_title` (see
/// [`crate::profile::SessionList`]), a row whose name carries a status also
/// matches through one exact title plus recognized status/provenance text.
pub fn title_matches(
    standalone_title: bool,
    expected: &str,
    unique: bool,
    title: &str,
    description: &str,
    standalone_text: Option<&[String]>,
) -> bool {
    if !unique || expected.trim().is_empty() {
        return false;
    }
    if title == expected || description == expected {
        return true;
    }
    // Only passive children of this row may supply the exact title. A
    // recognized badge is not authority: the catalog must be unique and the
    // controller still resolves the opened session ID before any mutation.
    if !standalone_title {
        return false;
    }
    let Some(labels) = standalone_text else {
        return false;
    };
    if labels
        .iter()
        .filter(|label| label.as_str() == expected)
        .count()
        != 1
    {
        return false;
    }
    let mut composite = expected.to_owned();
    let mut has_metadata = false;
    // read_label traverses siblings in reverse visual order. Reconstruct
    // the complete name, not a prefix/suffix guess, for rows with metadata.
    for label in labels
        .iter()
        .rev()
        .filter(|label| label.as_str() != expected)
    {
        if matches!(
            label.as_str(),
            "Idle" | "Working" | "Running" | "Awaiting input" | "Unread response"
        ) {
            continue;
        }
        if !metadata_label(label) {
            return false;
        }
        has_metadata = true;
        composite.push(' ');
        composite.push_str(label);
    }
    !has_metadata || title == composite
}

fn metadata_label(label: &str) -> bool {
    if label.chars().any(char::is_control) {
        return false;
    }
    if label == "SSH"
        || label
            .strip_prefix("Started by Claude from ")
            .is_some_and(|origin| !origin.trim().is_empty())
        || label
            .strip_suffix(" · Offline")
            .is_some_and(|workspace| !workspace.trim().is_empty())
    {
        return true;
    }
    let Some(count) = label
        .strip_prefix("Open · ")
        .or_else(|| label.strip_prefix("Merged · "))
    else {
        return false;
    };
    let Some((number, noun)) = count.split_once(' ') else {
        return false;
    };
    let Ok(number) = number.parse::<u32>() else {
        return false;
    };
    number > 0
        && count == format!("{number} {noun}")
        && noun
            == if number == 1 {
                "pull request"
            } else {
                "pull requests"
            }
}

#[cfg(test)]
mod tests {
    use super::*;

    const TITLE: &str = "示例应用导航任务续";

    #[test]
    fn delegated_claude_row_retains_its_exact_title_with_status_and_provenance() {
        let title = "mac-c 配置示例服务并连接 model.example";
        let origin = "Started by Claude from 示例应用功能改进方案";
        let name = format!("{title} {origin}");
        let labels = ["Awaiting input".into(), origin.into(), title.into()];
        assert!(title_matches(true, title, true, &name, "", Some(&labels)));
        assert!(!title_matches(true, title, false, &name, "", Some(&labels)));
        assert!(!title_matches(
            true,
            title,
            true,
            "Another task",
            "",
            Some(&labels)
        ));
        assert!(!title_matches(true, title, true, &name, "", None));
        assert!(title_matches(
            true,
            title,
            true,
            &format!("Unread response {title}"),
            "",
            Some(&["Unread response".into(), title.into()])
        ));
        assert!(!title_matches(
            true,
            title,
            true,
            &name,
            "",
            Some(&[title.into(), title.into(), origin.into()])
        ));
    }

    #[derive(Clone, PartialEq)]
    struct Node(&'static str, Option<&'static str>, bool, Vec<Node>);
    impl LabelNode for Node {
        fn role(&self) -> String {
            self.0.into()
        }
        fn text(&self) -> Option<String> {
            self.1.map(str::to_owned)
        }
        fn children(&self) -> Vec<Self> {
            self.3.clone()
        }
    }

    #[test]
    fn captured_pr_row_is_selected_without_substring_matching() {
        let title = "Muster 团队协作集成";
        let row = Node(
            "AXButton",
            None,
            true,
            vec![Node(
                "AXGroup",
                None,
                true,
                vec![
                    Node("AXStaticText", Some(title), false, vec![]),
                    Node("AXStaticText", Some("Awaiting input"), false, vec![]),
                    Node(
                        "AXStaticText",
                        Some("Open · 3 pull requests"),
                        false,
                        vec![],
                    ),
                ],
            )],
        );
        let (labels, _, _, stop) = read_label(&row);
        assert_eq!(stop, "complete");
        let name = format!("{title} Open · 3 pull requests");
        assert!(title_matches(
            true,
            title,
            true,
            &name,
            "",
            labels.as_deref()
        ));
        assert!(!title_matches(
            true,
            title,
            false,
            &name,
            "",
            labels.as_deref()
        ));
        assert!(!title_matches(
            false,
            title,
            true,
            &name,
            "",
            labels.as_deref()
        ));
        assert!(!title_matches(
            true,
            "Muster",
            true,
            &name,
            "",
            labels.as_deref()
        ));
    }

    #[test]
    fn merged_pr_row_retains_exact_identity_after_provider_status_changes() {
        for badge in ["Merged · 1 pull request", "Merged · 5 pull requests"] {
            let row = Node(
                "AXButton",
                None,
                true,
                vec![Node(
                    "AXGroup",
                    None,
                    true,
                    vec![
                        Node("AXStaticText", Some(TITLE), false, vec![]),
                        Node("AXStaticText", Some("Awaiting input"), false, vec![]),
                        Node("AXStaticText", Some(badge), false, vec![]),
                    ],
                )],
            );
            let (labels, _, _, stop) = read_label(&row);
            assert_eq!(stop, "complete");
            let name = format!("{TITLE} {badge}");
            assert!(title_matches(
                true,
                TITLE,
                true,
                &name,
                "",
                labels.as_deref()
            ));
            assert!(!title_matches(
                true,
                TITLE,
                false,
                &name,
                "",
                labels.as_deref()
            ));
            assert!(!title_matches(
                false,
                TITLE,
                true,
                &name,
                "",
                labels.as_deref()
            ));
            assert!(!title_matches(
                true,
                "示例应用",
                true,
                &name,
                "",
                labels.as_deref()
            ));
            assert!(!title_matches(
                true,
                TITLE,
                true,
                "Unrelated row",
                "",
                labels.as_deref()
            ));
        }
        for badge in [
            "Merged · 0 pull requests",
            "Merged · 01 pull request",
            "Merged · +1 pull request",
            "Merged · 1 pull requests",
            "Merged · 2 pull request",
            "Merged · -1 pull request",
            "Merged · 4294967296 pull requests",
            "Merged · 1 pull request extra",
            "Merged · 1 pull request\n",
            "Unrecognized · 5 pull requests",
        ] {
            let name = format!("{TITLE} {badge}");
            assert!(
                !title_matches(
                    true,
                    TITLE,
                    true,
                    &name,
                    "",
                    Some(&[badge.into(), "Awaiting input".into(), TITLE.into()])
                ),
                "{badge}"
            );
        }
    }

    #[test]
    fn metadata_combinations_require_the_exact_composite_name() {
        for (suffix, reverse_labels) in [
            (
                "Open · 1 pull request",
                vec!["Open · 1 pull request", "Awaiting input"],
            ),
            ("Open · 12 pull requests", vec!["Open · 12 pull requests"]),
            ("workspace · Offline", vec!["workspace · Offline"]),
            (
                "Started by Claude from Parent SSH",
                vec!["SSH", "Running", "Started by Claude from Parent"],
            ),
            (
                "Started by Claude from Parent",
                vec!["Running", "Started by Claude from Parent"],
            ),
        ] {
            let name = format!("{TITLE} {suffix}");
            let mut labels: Vec<String> = reverse_labels.into_iter().map(str::to_owned).collect();
            labels.push(TITLE.into());
            assert!(
                title_matches(true, TITLE, true, &name, "", Some(&labels)),
                "{name}"
            );
            assert!(!title_matches(
                true,
                TITLE,
                true,
                "Other row",
                "",
                Some(&labels)
            ));
            labels.push(TITLE.into());
            assert!(!title_matches(true, TITLE, true, &name, "", Some(&labels)));
        }
    }

    #[test]
    fn unrecognized_or_malformed_badges_remain_ambiguous() {
        for badge in [
            "Open · 0 pull requests",
            "Open · 01 pull request",
            "Open · 1 pull requests",
            "Open · 2 pull request",
            "Open · -3 pull requests",
            "Open · 3 pull requests extra",
            "Open · 99999999999999999999999 pull requests",
            "Other session",
            "· Offline",
            "workspace\nother · Offline",
            "Started by Claude from ",
            "Unknown state",
        ] {
            let name = format!("{TITLE} {badge}");
            assert!(
                !title_matches(
                    true,
                    TITLE,
                    true,
                    &name,
                    "",
                    Some(&[badge.into(), TITLE.into()])
                ),
                "{badge}"
            );
        }
        let labels = [
            "SSH".into(),
            "Started by Claude from Parent".into(),
            TITLE.into(),
        ];
        assert!(!title_matches(
            true,
            TITLE,
            true,
            &format!("{TITLE} SSH Started by Claude from Parent"),
            "",
            Some(&labels)
        ));
        assert!(!title_matches(
            true,
            TITLE,
            true,
            &format!("{TITLE} Open · 3 pull requests"),
            "",
            None
        ));
    }

    #[test]
    fn captured_claude_group_with_press_still_exposes_its_title() {
        let text = Node("AXStaticText", Some(TITLE), false, vec![]);
        let group = Node("AXGroup", None, true, vec![text]);
        let button = Node(
            "AXButton",
            None,
            true,
            vec![Node("AXImage", Some("Idle"), false, vec![]), group],
        );
        let (labels, nodes, skipped, stop) = read_label(&button);
        assert_eq!(labels.as_deref(), Some(&[TITLE.to_owned()][..]));
        assert_eq!((nodes, skipped, stop), (3, 0, "complete"));
        assert!(title_matches(
            true,
            TITLE,
            true,
            &format!("Idle {TITLE}"),
            "",
            labels.as_deref()
        ));
    }

    #[test]
    fn nested_controls_and_incomplete_or_ambiguous_labels_cannot_supply_a_title() {
        let text = Node("AXStaticText", Some(TITLE), false, vec![]);
        for role in ["AXButton", "AXLink", "AXRow", "AXPopUpButton", "AXMenu"] {
            let root = Node(
                "AXButton",
                None,
                true,
                vec![Node(
                    "AXGroup",
                    None,
                    true,
                    vec![Node(role, None, false, vec![text.clone()])],
                )],
            );
            assert_eq!(read_label(&root).0, Some(vec![]));
        }
        for children in [
            vec![Node("AXStaticText", None, false, vec![])],
            vec![
                text.clone(),
                Node("AXStaticText", Some("second"), false, vec![]),
            ],
            (0..65)
                .map(|n| Node("AXImage", None, false, vec![text.clone(); n]))
                .collect(),
        ] {
            let root = Node("AXButton", None, true, children);
            let labels = read_label(&root).0;
            assert!(!title_matches(
                true,
                TITLE,
                true,
                "Idle",
                "",
                labels.as_deref()
            ));
        }
    }

    #[test]
    fn claude_status_does_not_change_the_independent_session_title() {
        for name in [
            format!("Idle {TITLE}"),
            format!("Unread response {TITLE}"),
            format!("Working {TITLE}"),
        ] {
            assert!(title_matches(
                true,
                TITLE,
                true,
                &name,
                "",
                Some(&[TITLE.into()])
            ));
        }
    }

    #[test]
    fn never_guesses_prefixes_substrings_or_incomplete_labels() {
        let name = format!("Idle {TITLE}");
        for labels in [
            None,
            Some(vec![]),
            Some(vec![format!("{TITLE} 2")]),
            Some(vec![TITLE.into(), "Other session".into()]),
            Some(vec![TITLE.into(), TITLE.into()]),
        ] {
            assert!(!title_matches(
                true,
                TITLE,
                true,
                &name,
                "",
                labels.as_deref()
            ));
        }
        assert!(!title_matches(
            true,
            "Idle",
            true,
            &name,
            "",
            Some(&[TITLE.into()])
        ));
    }

    #[test]
    fn preserves_catalog_uniqueness_and_provider_boundaries() {
        assert!(!title_matches(
            true,
            TITLE,
            false,
            TITLE,
            "",
            Some(&[TITLE.into()])
        ));
        assert!(!title_matches(true, " ", true, " ", "", None));
        assert!(!title_matches(
            false,
            TITLE,
            true,
            &format!("Idle {TITLE}"),
            "",
            Some(&[TITLE.into()])
        ));
        for standalone_title in [false, true] {
            assert!(title_matches(
                standalone_title,
                TITLE,
                true,
                TITLE,
                "",
                None
            ));
            assert!(title_matches(
                standalone_title,
                TITLE,
                true,
                "",
                TITLE,
                None
            ));
        }
    }
}

#[cfg(test)]
mod navigation_tests {
    use super::absent_session_route;
    const SESSION: &str = "12345678-1234-4234-8234-123456789abc";

    #[test]
    fn absent_row_uses_exact_id_without_a_title_or_last_selector() {
        assert_eq!(
            absent_session_route("claude", SESSION, 2, true, 0),
            Some(format!("claude://code/continue?session={SESSION}"))
        );
    }

    #[test]
    fn visible_ambiguous_incomplete_and_other_provider_scans_cannot_fallback() {
        for (provider, areas, complete, candidates) in [
            ("claude", 2, true, 1),
            ("claude", 2, true, 2),
            ("claude", 0, true, 0),
            ("claude", 2, false, 0),
            ("codex", 2, true, 0),
            ("catdesk", 2, true, 0),
        ] {
            assert!(absent_session_route(provider, SESSION, areas, complete, candidates).is_none());
        }
    }

    #[test]
    fn malformed_ids_and_navigation_injection_are_rejected() {
        for id in [
            "",
            "last",
            "../session",
            "12345678-1234-4234-8234-123456789abC",
            "123456781234-4234-8234-123456789abc",
            "12345678-1234-4234-8234-123456789abg",
            "12345678-1234-4234-8234-123456789abc&prompt=send",
            "12345678-1234-4234-8234-123456789abc#fragment",
            "12345678-1234-4234-8234-123456789abé",
        ] {
            assert!(
                absent_session_route("claude", id, 2, true, 0).is_none(),
                "{id}"
            );
        }
    }
}
