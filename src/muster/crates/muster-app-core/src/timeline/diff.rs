//! Read-only unified patch presentation. Never apply patches or invent ranges.
use muster_proto::app::{ChatDiffFile, ChatDiffLine};

const DISPLAY_LINES: usize = 1200;

pub(super) fn files(id: &str, patch: &str) -> Vec<ChatDiffFile> {
    let mut files = Vec::new();
    let mut file = ChatDiffFile::default();
    let mut range: Option<(u32, u32, u32, u32)> = None;
    for source in patch.split_inclusive('\n') {
        let line = source.trim_end_matches(['\r', '\n']);
        if line.starts_with("diff --git ") && !file.patch.is_empty() {
            finish(id, &mut file, &mut files);
            range = None;
        }
        file.patch.push_str(source);
        let mut row = ChatDiffLine {
            kind: "meta".into(),
            text: line.into(),
            ..Default::default()
        };
        if line.starts_with("@@ ") {
            range = hunk(line);
        } else if let Some((old, new, old_left, new_left)) = range.as_mut() {
            match line.as_bytes().first() {
                Some(b'+') if *new_left > 0 => {
                    row.kind = "add".into();
                    row.new_line = *new;
                    *new = new.saturating_add(1);
                    *new_left -= 1;
                    file.added += 1;
                }
                Some(b'-') if *old_left > 0 => {
                    row.kind = "delete".into();
                    row.old_line = *old;
                    *old = old.saturating_add(1);
                    *old_left -= 1;
                    file.removed += 1;
                }
                Some(b' ') if *old_left > 0 && *new_left > 0 => {
                    row.kind = "context".into();
                    row.old_line = *old;
                    row.new_line = *new;
                    *old = old.saturating_add(1);
                    *new = new.saturating_add(1);
                    *old_left -= 1;
                    *new_left -= 1;
                }
                _ if line == "\\ No newline at end of file" => {}
                _ => range = None,
            }
            if row.kind != "meta" {
                row.text = line[1..].into();
            }
        }
        if row.kind == "meta" {
            if let Some(path) = line.strip_prefix("--- ") {
                if file.path.is_empty() {
                    file.path = path.strip_prefix("a/").unwrap_or(path).into();
                }
            } else if let Some(path) = line.strip_prefix("+++ ")
                && path != "/dev/null"
            {
                file.path = path.strip_prefix("b/").unwrap_or(path).into();
            }
        }
        if range.is_some_and(|(_, _, old, new)| old == 0 && new == 0) {
            range = None;
        }
        if file.lines.len() < DISPLAY_LINES {
            file.lines.push(row);
        } else {
            file.truncated = true;
        }
    }
    finish(id, &mut file, &mut files);
    files
}

fn finish(id: &str, file: &mut ChatDiffFile, out: &mut Vec<ChatDiffFile>) {
    if file.patch.is_empty() {
        return;
    }
    if file.path.is_empty() {
        file.path = "Changes".into();
    }
    file.id = format!("{id}:{}", out.len());
    out.push(std::mem::take(file));
}

fn hunk(line: &str) -> Option<(u32, u32, u32, u32)> {
    let mut parts = line.strip_prefix("@@ ")?.split_whitespace();
    let (old, old_count) = span(parts.next()?.strip_prefix('-')?)?;
    let (new, new_count) = span(parts.next()?.strip_prefix('+')?)?;
    (parts.next()? == "@@").then_some((old, new, old_count, new_count))
}
fn span(s: &str) -> Option<(u32, u32)> {
    let (start, count) = s.split_once(',').unwrap_or((s, "1"));
    let start: u32 = start.parse().ok()?;
    let count: u32 = count.parse().ok()?;
    start.checked_add(count)?;
    (start > 0 || count == 0).then_some((start, count))
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn no_newline_markers_preserve_ranges_and_exact_copy_bytes() {
        let patch = "@@ -1 +1 @@\n-before\n\\ No newline at end of file\n+after\n\\ No newline at end of file";
        let parsed = files("diff", patch);
        assert_eq!(parsed[0].patch, patch);
        assert_eq!((parsed[0].added, parsed[0].removed), (1, 1));
        assert_eq!(parsed[0].lines[3].new_line, 1);
    }
    #[test]
    fn multiple_files_keep_real_ranges_counts_and_copy_source() {
        let patch = "diff --git a/old.rs b/old.rs\n--- a/old.rs\n+++ b/old.rs\n@@ -41,2 +41,2 @@\n keep\n-before\n+after\ndiff --git a/new.rs b/new.rs\n--- /dev/null\n+++ b/new.rs\n@@ -0,0 +1 @@\n+你好\n";
        let parsed = files("turn", patch);
        assert_eq!(parsed.len(), 2);
        assert_eq!(parsed[0].path, "old.rs");
        assert_eq!((parsed[0].added, parsed[0].removed), (1, 1));
        assert_eq!(parsed[0].lines[5].old_line, 42);
        assert_eq!(parsed[0].lines[6].new_line, 42);
        assert_eq!(parsed[1].lines[4].new_line, 1);
        assert_eq!(
            parsed.iter().map(|f| f.patch.as_str()).collect::<String>(),
            patch
        );
    }
    #[test]
    fn malformed_or_exhausted_ranges_do_not_invent_counts_or_line_numbers() {
        let p = files(
            "a",
            "--- a/x\n+++ b/x\n@@ -bad +1 @@\n+not a known addition\n@@ -1 +1 @@\n-a\n+b\n+outside range\n",
        );
        assert_eq!((p[0].added, p[0].removed), (1, 1));
        assert_eq!(p[0].lines[3].kind, "meta");
        assert_eq!(p[0].lines[7].new_line, 0);
        assert!(hunk("@@ -4294967295,2 +1 @@").is_none());
    }
    #[test]
    fn long_patches_are_bounded_for_display_but_copy_and_counts_are_complete() {
        let patch = format!("@@ -0,0 +1,1500 @@\n{}", "+line\n".repeat(1500));
        let p = files("a", &patch);
        assert_eq!(p[0].lines.len(), DISPLAY_LINES);
        assert!(p[0].truncated);
        assert_eq!(p[0].added, 1500);
        assert_eq!(p[0].patch, patch);
    }
}
