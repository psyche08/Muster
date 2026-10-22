//! Text rules shared by the viewers.

/// Clips to `n` characters, appending an ellipsis when it cut.
pub fn clip(s: &str, n: usize) -> String {
    let mut out: String = s.chars().take(n).collect();
    if s.chars().count() > n {
        out.push('…');
    }
    out
}

/// A JSON value as one line of text: strings as they are, anything else
/// serialized.
pub fn compact(v: &serde_json::Value) -> String {
    match v {
        serde_json::Value::String(s) => s.clone(),
        serde_json::Value::Null => String::new(),
        other => other.to_string(),
    }
}

/// Provider bootstrap text (`<user_instructions>`, `<environment_context>`,
/// `<command-name>` wrappers…) starts with a tag; users never type that.
pub fn is_wrapper(text: &str) -> bool {
    text.trim_start().starts_with('<')
}

/// Removes `<system-reminder>…</system-reminder>` blocks and Codex memory
/// citation blocks, which are for the model, not the reader.
pub fn strip_hidden_blocks(text: &str) -> String {
    let mut out = strip_tagged(text, "<system-reminder>", "</system-reminder>");
    out = strip_tagged(&out, "<oai-mem-citation>", "</oai-mem-citation>");
    out.trim().to_string()
}

fn strip_tagged(text: &str, open: &str, close: &str) -> String {
    let mut out = String::with_capacity(text.len());
    let mut rest = text;
    while let Some(start) = rest.find(open) {
        out.push_str(rest[..start].trim_end());
        rest = &rest[start + open.len()..];
        match rest.find(close) {
            Some(end) => rest = &rest[end + close.len()..],
            None => {
                rest = "";
                break;
            }
        }
    }
    out.push_str(rest);
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn strips_reminders_and_citations() {
        assert_eq!(
            strip_hidden_blocks("hi <system-reminder>x</system-reminder> there"),
            "hi there"
        );
        assert_eq!(
            strip_hidden_blocks("done `\n<oai-mem-citation>private</oai-mem-citation>`"),
            "done ``"
        );
        assert_eq!(
            strip_hidden_blocks("open <system-reminder>never closed"),
            "open"
        );
        assert!(is_wrapper("  <environment_context>"));
        assert!(!is_wrapper("fix the bug"));
        assert_eq!(clip("abcdef", 3), "abc…");
    }
}
